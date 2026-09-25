//! Quick Calibration: can PWR safely operate this model?
//!
//! Not a benchmark. A fixed set of at most nine small requests, each scored
//! mechanically -- parsed, compared with a fixture, validated against a
//! schema -- never by another model's opinion. Deterministic prompts,
//! temperature 0 and a fixed seed, so a rerun asks exactly the same
//! questions. Bounded per request and in total, and cancellable at any point:
//! cancelling stops the generation in progress, and nothing is recorded.
//!
//! What it does not prove: that the model is good at coding, that it works at
//! long context (never tested here), or that a pass today holds for another
//! quantization, backend or template (see [`crate::profile::compare`]).

use crate::profile::{
    Check, EVIDENCE_SCHEMA, LocalEvidence, ProfileStatus, Provenance, ReasoningObservation,
};
use pwr_domain::{
    ChatMessage, ModelInspection, ModelRequest, ReasoningCapability, ReasoningProfile, ToolCall,
};
use pwr_provider::{Cancel, ModelProvider, ProviderError};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Each request's bound, prefill included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Most requests one calibration makes.
pub const MAX_REQUESTS: usize = 9;
/// Answer cap for the plain checks.
const ANSWER_TOKENS: u64 = 512;
/// The thinking budget a plain check allows a model that cannot switch
/// thinking off, so it reaches its answer within [`ANSWER_TOKENS`] more.
const PLAIN_THINKING: u64 = 256;
/// A generous budget, to see whether thinking ends by itself.
const NATURAL_THINKING: u64 = 1_024;
/// A tiny budget, to see whether an answer follows a forced close.
const FORCED_THINKING: u64 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    Cancelled,
}

impl std::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "calibration cancelled"),
        }
    }
}

/// What the person sees while it runs.
pub trait Progress {
    fn step(&mut self, index: usize, total: usize, name: &str);
}

impl<F: FnMut(usize, usize, &str)> Progress for F {
    fn step(&mut self, index: usize, total: usize, name: &str) {
        self(index, total, name)
    }
}

/// One reply, reduced to what the checks read.
#[derive(Debug, Clone, Default)]
struct Answer {
    text: String,
    thinking_chars: usize,
    reasoning_tokens: Option<u64>,
    budget_reached: bool,
    tool_calls: Vec<ToolCall>,
}

struct Runner<'a, P: ModelProvider + ?Sized> {
    provider: &'a P,
    base: ModelRequest,
    reasoning: &'a ReasoningProfile,
    cancel: &'a Cancel,
    family: Option<String>,
    requests: usize,
}

impl<P: ModelProvider + ?Sized> Runner<'_, P> {
    async fn ask(
        &mut self,
        messages: Vec<ChatMessage>,
        tools: Option<Value>,
        thinking: Thinking,
    ) -> Result<Result<Answer, String>, CalibrationError> {
        if self.cancel.is_cancelled() {
            return Err(CalibrationError::Cancelled);
        }
        self.requests += 1;
        debug_assert!(self.requests <= MAX_REQUESTS);
        let mut request = self.base.clone();
        request.messages = messages;
        request.tools = tools;
        let sampling = &mut request.sampling;
        let enforceable = self.reasoning.budget_enforceable();
        match thinking {
            Thinking::Minimal if self.reasoning.switchable => {
                sampling.insert("think".into(), json!(false));
                sampling.insert("max_tokens".into(), json!(ANSWER_TOKENS));
            }
            Thinking::Minimal if enforceable => {
                sampling.insert("reasoning_budget".into(), json!(PLAIN_THINKING));
                sampling.insert("max_tokens".into(), json!(ANSWER_TOKENS + PLAIN_THINKING));
            }
            Thinking::Minimal => {
                if self.reasoning.capability == ReasoningCapability::TemplateControlled {
                    sampling.insert("reasoning_effort".into(), json!("low"));
                }
                // No way to bound it: room for some reasoning before the answer.
                sampling.insert("max_tokens".into(), json!(ANSWER_TOKENS + NATURAL_THINKING));
            }
            Thinking::Budget(tokens) => {
                if self.reasoning.switchable {
                    sampling.insert("think".into(), json!(true));
                }
                sampling.insert("reasoning_budget".into(), json!(tokens));
                sampling.insert("max_tokens".into(), json!(tokens + ANSWER_TOKENS * 2));
            }
        }
        let family = self.family.clone();
        let model_ref = request.deployment.model_ref.clone();
        // This request's own handle: a timeout ends this generation and the
        // calibration moves on; the person's cancel ends both.
        let this_request = Cancel::new();
        let outcome = tokio::select! {
            outcome = tokio::time::timeout(REQUEST_TIMEOUT, async {
                let stream = self.provider.chat_cancellable(request, this_request.clone()).await?;
                pwr_provider::collect_reply(stream).await
            }) => outcome,
            () = self.cancel.cancelled() => {
                this_request.cancel();
                return Err(CalibrationError::Cancelled);
            }
        };
        let reply = match outcome {
            Err(_) => {
                this_request.cancel();
                return Ok(Err(format!(
                    "no reply within {} s",
                    REQUEST_TIMEOUT.as_secs()
                )));
            }
            Ok(Err(ProviderError::Cancelled)) if self.cancel.is_cancelled() => {
                return Err(CalibrationError::Cancelled);
            }
            Ok(Err(error)) => return Ok(Err(error.to_string())),
            Ok(Ok(reply)) => reply,
        };
        // Read the way a conversation turn reads it, through the family
        // adapter, so a call the turn would find is a call here too.
        let canonical = pwr_compat::adapter_for(family.as_deref(), &model_ref).normalize(&reply);
        let metrics = reply.metrics.clone().unwrap_or_default();
        Ok(Ok(Answer {
            text: canonical.narrative,
            thinking_chars: reply.thinking.len() + canonical.thinking.len(),
            reasoning_tokens: metrics.reasoning_tokens,
            budget_reached: metrics.reasoning_budget_reached == Some(true),
            tool_calls: canonical.tool_calls,
        }))
    }
}

#[derive(Debug, Clone, Copy)]
enum Thinking {
    /// As little reasoning as the model allows.
    Minimal,
    /// Reasoning allowed, closed after this many tokens.
    Budget(u64),
}

/// Trims what a mechanical comparison should not care about: whitespace,
/// one pair of surrounding quotes or backticks, a final period.
fn normalized(text: &str) -> String {
    let mut text = text.trim().trim_end_matches('.').trim_end();
    for (open, close) in [("`", "`"), ("\"", "\""), ("'", "'"), ("**", "**")] {
        if text.len() >= open.len() + close.len() && text.starts_with(open) && text.ends_with(close)
        {
            text = text[open.len()..text.len() - close.len()].trim();
        }
    }
    text.trim_end_matches('.').trim().to_owned()
}

fn check(name: &str, critical: bool, passed: bool, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        passed: Some(passed),
        critical,
        detail: detail.into(),
    }
}

fn not_run(name: &str, critical: bool, why: &str) -> Check {
    Check {
        name: name.into(),
        passed: None,
        critical,
        detail: why.into(),
    }
}

fn exact(name: &str, critical: bool, answer: &Result<Answer, String>, expected: &str) -> Check {
    match answer {
        Ok(answer) => {
            let got = normalized(&answer.text);
            let passed = got == expected;
            check(
                name,
                critical,
                passed,
                if passed {
                    "matched the fixture".to_owned()
                } else {
                    format!("expected {expected:?}, got {:?}", clip(&got))
                },
            )
        }
        Err(error) => check(name, critical, false, format!("no usable reply: {error}")),
    }
}

fn clip(text: &str) -> String {
    let mut clipped: String = text.chars().take(80).collect();
    if clipped.len() < text.len() {
        clipped.push('…');
    }
    clipped
}

/// Strict JSON, with one Markdown fence tolerated.
pub fn score_structured(text: &str) -> (bool, String) {
    let trimmed = text.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    match serde_json::from_str::<Value>(body) {
        Err(error) => (false, format!("not JSON: {error}")),
        Ok(value) => {
            let ok = value.get("file").and_then(Value::as_str) == Some("src/parser.rs")
                && value.get("line").and_then(Value::as_u64) == Some(7)
                && value.as_object().is_some_and(|object| object.len() == 2);
            (
                ok,
                if ok {
                    "valid JSON with exactly the requested fields".into()
                } else {
                    format!("JSON, but not the requested object: {}", clip(body))
                },
            )
        }
    }
}

/// Validates a value against the subset of JSON Schema PWR's tool schemas
/// use: object with properties, required and additionalProperties false;
/// string, integer, boolean, array with items.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let object = value.as_object().ok_or("expected an object")?;
            let properties = schema.get("properties").and_then(Value::as_object);
            for required in schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !object.contains_key(required) {
                    return Err(format!("missing required argument {required:?}"));
                }
            }
            for (key, item) in object {
                match properties.and_then(|properties| properties.get(key)) {
                    Some(property) => {
                        validate(property, item).map_err(|error| format!("{key}: {error}"))?
                    }
                    None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                        return Err(format!("unexpected argument {key:?}"));
                    }
                    None => {}
                }
            }
            Ok(())
        }
        Some("string") => value
            .is_string()
            .then_some(())
            .ok_or("expected a string".into()),
        Some("integer") => value
            .is_i64()
            .then_some(())
            .ok_or("expected an integer".into()),
        Some("boolean") => value
            .is_boolean()
            .then_some(())
            .ok_or("expected a boolean".into()),
        Some("array") => {
            let items = value.as_array().ok_or("expected an array")?;
            if let Some(item_schema) = schema.get("items") {
                for item in items {
                    validate(item_schema, item)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn fixture_tools() -> Value {
    json!([
        {"type": "function", "function": {
            "name": "read_file",
            "description": "Read a file of the repository.",
            "parameters": {"type": "object",
                "properties": {"path": {"type": "string", "description": "Path relative to the repository root."}},
                "required": ["path"], "additionalProperties": false}}},
        {"type": "function", "function": {
            "name": "run_command",
            "description": "Run a program in the repository.",
            "parameters": {"type": "object",
                "properties": {"argv": {"type": "array", "items": {"type": "string"}}},
                "required": ["argv"], "additionalProperties": false}}}
    ])
}

/// Runs Quick Calibration and returns its evidence (not saved: the caller
/// records it). `Err` only when cancelled; every other failure is a result.
pub async fn quick_calibrate<P: ModelProvider + ?Sized>(
    provider: &P,
    inspection: &ModelInspection,
    provenance: Provenance,
    reasoning: &ReasoningProfile,
    cancel: &Cancel,
    progress: &mut dyn Progress,
) -> Result<LocalEvidence, CalibrationError> {
    let started = Instant::now();
    let mut base = ModelRequest {
        deployment: inspection.deployment.clone(),
        messages: Vec::new(),
        context_tokens: 8_192,
        tools: None,
        seed: Some(7),
        sampling: Default::default(),
    };
    base.sampling.insert("temperature".into(), json!(0));
    let mut runner = Runner {
        provider,
        base,
        reasoning,
        cancel,
        family: inspection.definition.family.clone(),
        requests: 0,
    };
    let total = if reasoning.budget_enforceable() { 9 } else { 8 };
    let user = |text: &str| ChatMessage::text("user", text);
    let mut checks = Vec::new();

    progress.step(1, total, "instruction following");
    let ready = runner
        .ask(
            vec![user("Reply with exactly the word READY and nothing else.")],
            None,
            Thinking::Minimal,
        )
        .await?;
    checks.push(check(
        "termination",
        true,
        ready.is_ok(),
        match &ready {
            Ok(_) => "the reply ended by itself".to_owned(),
            Err(error) => format!("the reply did not end cleanly: {error}"),
        },
    ));
    checks.push(exact("instruction_following", false, &ready, "READY"));
    let first_error = ready.as_ref().err().cloned();

    progress.step(2, total, "structured output");
    let structured = runner
        .ask(
            vec![user(
                "Return only a JSON object with two fields: \"file\" set to \"src/parser.rs\" \
                 and \"line\" set to the number 7. No other text.",
            )],
            None,
            Thinking::Minimal,
        )
        .await?;
    checks.push(match &structured {
        Ok(answer) => {
            let (passed, detail) = score_structured(&answer.text);
            check("structured_output", false, passed, detail)
        }
        Err(error) => check("structured_output", false, false, error.clone()),
    });

    progress.step(3, total, "code understanding");
    let code = runner
        .ask(
            vec![user(
                "In Rust, `fn double(x: i32) -> i32 { x * 2 }`. What does `double(21)` return? \
                 Reply with the number only.",
            )],
            None,
            Thinking::Minimal,
        )
        .await?;
    checks.push(exact("code_understanding", false, &code, "42"));

    progress.step(4, total, "repository reasoning");
    let repository = runner
        .ask(
            vec![user(
                "A repository has three files:\n\
                 - src/parser.rs: defines fn parse(input: &str) -> Vec<Token>\n\
                 - src/lexer.rs: defines fn lex(input: &str) -> Vec<char>\n\
                 - docs/guide.md: explains how to call parse()\n\
                 A bug report says parse() returns tokens in the wrong order. Which file must \
                 change to fix it? Reply with its path only.",
            )],
            None,
            Thinking::Minimal,
        )
        .await?;
    checks.push(exact(
        "repository_file_selection",
        false,
        &repository,
        "src/parser.rs",
    ));

    progress.step(5, total, "tool selection");
    let tools = fixture_tools();
    let called = runner
        .ask(
            vec![user(
                "Show me what src/parser.rs contains. Use the tools you have.",
            )],
            Some(tools.clone()),
            Thinking::Minimal,
        )
        .await?;
    match &called {
        Ok(answer) => {
            let call = answer.tool_calls.first();
            let selected =
                answer.tool_calls.len() == 1 && call.is_some_and(|call| call.name == "read_file");
            checks.push(check(
                "tool_selection",
                true,
                selected,
                match call {
                    None => "no tool call was made".to_owned(),
                    Some(_) if answer.tool_calls.len() > 1 => {
                        format!("{} calls instead of one", answer.tool_calls.len())
                    }
                    Some(call) => format!("called {}", call.name),
                },
            ));
            let schema = &tools[0]["function"]["parameters"];
            checks.push(match call.filter(|_| selected) {
                None => not_run("tool_arguments", true, "no read_file call to validate"),
                Some(call) => match validate(schema, &call.arguments) {
                    Err(error) => check("tool_arguments", true, false, error),
                    Ok(()) if call.arguments["path"] == "src/parser.rs" => check(
                        "tool_arguments",
                        true,
                        true,
                        "arguments valid against the schema",
                    ),
                    Ok(()) => check(
                        "tool_arguments",
                        true,
                        false,
                        format!("valid, but asked for {}", call.arguments["path"]),
                    ),
                },
            });
        }
        Err(error) => {
            checks.push(check("tool_selection", true, false, error.clone()));
            checks.push(not_run("tool_arguments", true, "no reply"));
        }
    }

    progress.step(6, total, "tool-result continuation");
    // A fixed transcript, so this is tested even when the call above failed.
    let continued = runner
        .ask(
            vec![
                user("What number does parse() in src/parser.rs return? Use the tools."),
                ChatMessage {
                    role: "assistant".into(),
                    content: String::new(),
                    tool_calls: vec![ToolCall {
                        name: "read_file".into(),
                        arguments: json!({"path": "src/parser.rs"}),
                        id: Some("call_1".into()),
                    }],
                    tool_call_id: None,
                    purpose: None,
                    images: Vec::new(),
                    reasoning: None,
                },
                ChatMessage {
                    role: "tool".into(),
                    content: "fn parse() -> i32 {\n    1337\n}\n".into(),
                    tool_calls: Vec::new(),
                    tool_call_id: Some("call_1".into()),
                    purpose: None,
                    images: Vec::new(),
                    reasoning: None,
                },
            ],
            Some(tools),
            Thinking::Minimal,
        )
        .await?;
    checks.push(match &continued {
        Ok(answer) if answer.text.contains("1337") => check(
            "tool_result_continuation",
            true,
            true,
            "answered from the tool result",
        ),
        Ok(answer) if !answer.tool_calls.is_empty() => check(
            "tool_result_continuation",
            true,
            false,
            "called a tool again instead of using the result",
        ),
        Ok(answer) => check(
            "tool_result_continuation",
            true,
            false,
            format!("did not use the tool result: {:?}", clip(&answer.text)),
        ),
        Err(error) => check("tool_result_continuation", true, false, error.clone()),
    });

    // Reasoning. Nothing aggressive is assumed: a model with no sign of a
    // thinking phase is asked once, with room to think, to see if it does.
    let mut observation = ReasoningObservation::default();
    if reasoning.disabled_by_profile {
        observation.detail = "thinking is turned off by this model's profile; not probed".into();
    } else {
        progress.step(7, total, "reasoning");
        let natural = runner
            .ask(
                vec![user(
                    "What is 17 multiplied by 3? Think it through if you need to, then reply \
                     with the number only.",
                )],
                None,
                Thinking::Budget(NATURAL_THINKING),
            )
            .await?;
        match &natural {
            Ok(answer) => {
                let emitted = answer.reasoning_tokens.unwrap_or(0) > 0 || answer.thinking_chars > 0;
                observation.emitted = Some(emitted);
                observation.separated = emitted.then_some(true);
                observation.terminated_naturally = emitted.then_some(!answer.budget_reached);
                checks.push(exact("answer_after_reasoning", false, &natural, "51"));
            }
            Err(error) => {
                observation.detail = format!("the reasoning probe failed: {error}");
                checks.push(check("answer_after_reasoning", false, false, error.clone()));
            }
        }
        if reasoning.budget_enforceable() {
            progress.step(8, total, "reasoning budget");
            let forced = runner
                .ask(
                    vec![user(
                        "Think carefully, step by step, about which of 91, 97 and 99 is a \
                         prime number. Then reply with that number only.",
                    )],
                    None,
                    Thinking::Budget(FORCED_THINKING),
                )
                .await?;
            observation.finalization_after_budget = match &forced {
                Ok(answer) if answer.budget_reached => {
                    Some(!answer.text.trim().is_empty() || !answer.tool_calls.is_empty())
                }
                // It did not think long enough to reach even this budget.
                Ok(_) => None,
                Err(_) => Some(false),
            };
            if observation.detail.is_empty() {
                observation.detail = match observation.finalization_after_budget {
                    Some(true) => "an answer followed when the engine closed the thinking phase",
                    Some(false) => "no answer followed when the engine closed the thinking phase",
                    None => "the thinking phase never reached the test budget",
                }
                .into();
            }
        }
        // One more request slot is kept free (MAX_REQUESTS): the suite may grow
        // by one check without the bound moving.
    }
    progress.step(total, total, "done");

    let critical_ok = checks
        .iter()
        .filter(|check| check.critical)
        .all(|check| check.passed == Some(true));
    let nothing_generated = [&ready, &structured, &code, &repository, &called, &continued]
        .iter()
        .all(|answer| answer.is_err());
    let (status, reason) = if nothing_generated {
        (
            ProfileStatus::Incompatible,
            Some(format!(
                "No request produced a reply: {}",
                first_error.unwrap_or_else(|| "unknown error".into())
            )),
        )
    } else if critical_ok {
        (ProfileStatus::LocallyCalibrated, None)
    } else {
        let failed: Vec<&str> = checks
            .iter()
            .filter(|check| check.critical && check.passed != Some(true))
            .map(|check| check.name.as_str())
            .collect();
        (
            ProfileStatus::Limited,
            Some(format!(
                "Checks agent mode depends on did not pass: {}.",
                failed.join(", ")
            )),
        )
    };
    Ok(LocalEvidence {
        schema_version: EVIDENCE_SCHEMA,
        status,
        provenance,
        checks,
        reasoning: observation,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_mechanical_and_narrow() {
        assert_eq!(normalized("  `READY`. "), "READY");
        assert_eq!(normalized("\"src/parser.rs\""), "src/parser.rs");
        assert_ne!(normalized("READY and more"), "READY");
    }

    #[test]
    fn structured_output_is_parsed_not_judged() {
        assert!(score_structured(r#"{"file": "src/parser.rs", "line": 7}"#).0);
        assert!(score_structured("```json\n{\"file\": \"src/parser.rs\", \"line\": 7}\n```").0);
        assert!(!score_structured(r#"{"file": "src/parser.rs", "line": "7"}"#).0);
        assert!(!score_structured(r#"{"file": "src/parser.rs", "line": 7, "extra": 1}"#).0);
        assert!(!score_structured("file src/parser.rs line 7").0);
    }

    #[test]
    fn arguments_are_validated_against_the_schema() {
        let tools = fixture_tools();
        let read = &tools[0]["function"]["parameters"];
        assert!(validate(read, &json!({"path": "a"})).is_ok());
        assert!(validate(read, &json!({})).is_err());
        assert!(validate(read, &json!({"path": 3})).is_err());
        assert!(validate(read, &json!({"path": "a", "mode": "r"})).is_err());
        let run = &tools[1]["function"]["parameters"];
        assert!(validate(run, &json!({"argv": ["ls"]})).is_ok());
        assert!(validate(run, &json!({"argv": "ls"})).is_err());
    }
}
