//! Model compatibility layer.
//!
//! The agent loop asks for a canonical thing -- an action -- and deployments
//! answer in the conventions of their family. This crate is where those
//! conventions are absorbed, so that the loop, the tool runtime and the
//! verifier never learn that one family writes its calls inside the answer
//! text and another puts them in a protocol field.
//!
//! It owns response normalization and malformed-output classification only.
//! Selecting files, performing an edit, deciding verification and backend
//! lifecycle stay where they are; an adapter that could do any of those would
//! be a second orchestrator.
//!
//! Rendering a tool catalogue into a wire envelope stays at the backend
//! boundary, where the envelope is a property of the backend's protocol rather
//! than of the model family: Ollama's native-tools shape and LM Studio's
//! OpenAI-compatible shape are the same for every family they serve.

use pwr_domain::ToolCall;
use pwr_provider::ModelReply;

/// A reply in the form the agent loop reasons about, whatever the deployment
/// wrote to produce it.
///
/// `narrative` is what the deployment said to the operator, with any content
/// that turned out to be a call or reasoning removed. `thinking` is diagnostic
/// only and never carries control flow -- a family that omits it is not a
/// family that failed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalReply {
    pub narrative: String,
    pub thinking: String,
    pub tool_calls: Vec<ToolCall>,
    /// What the adapter changed, and why.
    ///
    /// Normalization that leaves no trace is indistinguishable from a
    /// deployment that never needed it, and those call for opposite work: the
    /// first says the family adapter is carrying the run, the second says it
    /// could be retired.
    pub diagnostics: Vec<Diagnostic>,
    pub chunks: usize,
    pub metrics: Option<pwr_domain::GenerationMetrics>,
}

impl CanonicalReply {
    /// The reply exactly as the backend delivered it.
    pub fn verbatim(reply: &ModelReply) -> Self {
        Self {
            narrative: reply.content.clone(),
            thinking: reply.thinking.clone(),
            tool_calls: reply.tool_calls.clone(),
            diagnostics: Vec::new(),
            chunks: reply.chunks,
            metrics: reply.metrics.clone(),
        }
    }
}

/// One normalization the adapter performed, named so a run can be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable kind, countable across runs.
    pub kind: &'static str,
    pub detail: String,
}

/// Translates one model family's conventions into the canonical form.
pub trait ModelBehaviorAdapter: Send + Sync {
    /// Stable identifier recorded with a run, so a result can be attributed to
    /// the adapter that produced it.
    fn id(&self) -> &'static str;

    /// The adapter's own revision, part of a certification scope.
    ///
    /// Changing how a family's replies are read changes what the deployment
    /// was measured doing, so evidence gathered under an earlier revision is
    /// evidence about a different thing. Bumping this demotes it rather than
    /// letting a badge survive the change that invalidated it.
    fn version(&self) -> &'static str;

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply;
}

/// The conservative default: it changes nothing.
///
/// A model whose family is unknown gets this one. Guessing that an unknown
/// deployment follows a convention we happen to have implemented is how an
/// uncertified model comes to look certified.
pub struct GenericAdapter;

impl ModelBehaviorAdapter for GenericAdapter {
    fn id(&self) -> &'static str {
        "generic"
    }

    fn version(&self) -> &'static str {
        "generic-v1"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        CanonicalReply::verbatim(reply)
    }
}

/// Qwen's own conventions, which the family emits regardless of backend.
///
/// Two of them are load-bearing. Qwen's chat template writes a call as a
/// `<tool_call>` block inside the answer text, and every backend that fails to
/// parse that block hands it over as prose -- which the loop then reports as
/// "model output must be one valid typed-action JSON object" about output that
/// contains a perfectly well-formed call. And Qwen marks reasoning with
/// `<think>` inline whenever the transport has no separate channel for it, so
/// the same reasoning that LM Studio delivers in `reasoning_content` arrives
/// mixed into the answer elsewhere.
///
/// Both are recovery, not interpretation: the adapter moves text between
/// channels and never invents a call the deployment did not write.
pub struct QwenFamilyAdapter;

const OPEN_TOOL: &str = "<tool_call>";
const CLOSE_TOOL: &str = "</tool_call>";
const OPEN_THINK: &str = "<think>";
const CLOSE_THINK: &str = "</think>";

impl ModelBehaviorAdapter for QwenFamilyAdapter {
    fn id(&self) -> &'static str {
        "qwen"
    }

    fn version(&self) -> &'static str {
        // v2: also reads the XML parameter form Qwen 3.5/3.6 write.
        "qwen-v2"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        let (narrative, thinking, diagnostic) = split_thinking(&canonical.narrative);
        canonical.narrative = narrative;
        if let Some(diagnostic) = diagnostic {
            if !canonical.thinking.is_empty() && !thinking.is_empty() {
                canonical.thinking.push('\n');
            }
            canonical.thinking.push_str(&thinking);
            canonical.diagnostics.push(diagnostic);
        }
        // Only when the backend's own parser produced nothing. A backend that
        // already decoded the call has the authoritative reading of it, and
        // recovering the same call from the text it was rendered in would
        // report one action as two.
        if canonical.tool_calls.is_empty() {
            let (narrative, calls, diagnostics) = extract_tool_calls(&canonical.narrative);
            canonical.narrative = narrative;
            canonical.tool_calls = calls;
            canonical.diagnostics.extend(diagnostics);
        }
        canonical
    }
}

/// Moves `<think>` spans out of the answer text.
///
/// An unterminated span is reasoning that ran until the deployment was cut
/// off, not malformed prose: everything after the opening tag is reasoning and
/// the answer is what came before it. Read the other way, a turn that spent its
/// whole budget thinking is recorded as a deployment that answered nonsense.
fn split_thinking(content: &str) -> (String, String, Option<Diagnostic>) {
    if !content.contains(OPEN_THINK) {
        return (content.to_owned(), String::new(), None);
    }
    let mut narrative = String::new();
    let mut thinking = String::new();
    let mut rest = content;
    let mut unterminated = false;
    while let Some(open) = rest.find(OPEN_THINK) {
        narrative.push_str(&rest[..open]);
        let after = &rest[open + OPEN_THINK.len()..];
        match after.find(CLOSE_THINK) {
            Some(close) => {
                thinking.push_str(&after[..close]);
                rest = &after[close + CLOSE_THINK.len()..];
            }
            None => {
                thinking.push_str(after);
                unterminated = true;
                rest = "";
                break;
            }
        }
    }
    narrative.push_str(rest);
    let detail = if unterminated {
        "an unterminated <think> span was read as reasoning that was cut off".to_owned()
    } else {
        format!("{} characters of inline <think> reasoning", thinking.len())
    };
    (
        narrative.trim().to_owned(),
        thinking.trim().to_owned(),
        Some(Diagnostic {
            kind: "qwen_inline_thinking",
            detail,
        }),
    )
}

/// Recovers `<tool_call>` blocks the backend left in the answer text.
///
/// A block whose body is not a call is left in the narrative rather than
/// dropped: the deployment wrote something, and text that vanishes between the
/// model and the record cannot be diagnosed later.
fn extract_tool_calls(content: &str) -> (String, Vec<ToolCall>, Vec<Diagnostic>) {
    if !content.contains(OPEN_TOOL) {
        return (content.to_owned(), Vec::new(), Vec::new());
    }
    let mut narrative = String::new();
    let mut calls = Vec::new();
    let mut diagnostics = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find(OPEN_TOOL) {
        let body_start = open + OPEN_TOOL.len();
        let Some(close) = rest[body_start..].find(CLOSE_TOOL) else {
            // An opening tag with no close is a call that was cut off. It is
            // left in the narrative so the turn is reported as truncated
            // output rather than as a call that never arrived.
            diagnostics.push(Diagnostic {
                kind: "qwen_unterminated_tool_call",
                detail: "a <tool_call> block was not closed before the reply ended".into(),
            });
            break;
        };
        let body = &rest[body_start..body_start + close];
        match parse_call(body) {
            Some(call) => {
                narrative.push_str(&rest[..open]);
                diagnostics.push(Diagnostic {
                    kind: "qwen_embedded_tool_call",
                    detail: format!("recovered {} from the answer text", call.name),
                });
                calls.push(call);
            }
            None => {
                // Kept verbatim, tag and all: a block that did not decode is
                // evidence about the template, and the next turn's operator
                // needs to see what was actually written.
                narrative.push_str(&rest[..body_start + close + CLOSE_TOOL.len()]);
                diagnostics.push(Diagnostic {
                    kind: "qwen_undecodable_tool_call",
                    detail: "a <tool_call> block did not contain a name and arguments".into(),
                });
            }
        }
        rest = &rest[body_start + close + CLOSE_TOOL.len()..];
    }
    narrative.push_str(rest);
    (narrative.trim().to_owned(), calls, diagnostics)
}

fn parse_call(body: &str) -> Option<ToolCall> {
    let body = body.trim();
    if body.starts_with(OPEN_FUNCTION) {
        return parse_xml_call(body);
    }
    parse_call_value(&serde_json::from_str(body).ok()?)
}

const OPEN_FUNCTION: &str = "<function=";
const OPEN_PARAMETER: &str = "<parameter=";
const CLOSE_PARAMETER: &str = "</parameter>";

/// Reads the XML form Qwen 3.5 and 3.6 are trained to write:
///
/// ```text
/// <function=read_file>
/// <parameter=path>
/// src/lib.rs
/// </parameter>
/// </function>
/// ```
///
/// LM Studio decodes it before PWR sees it, which is why this was never
/// needed; an engine that returns the model's text unparsed hands it over as
/// written. A value is text unless it reads as JSON that is not a string -- a
/// number, a boolean, an object, an array -- because the template writes every
/// value bare, and `max_lines` of `30` must arrive as the number it was meant
/// to be. The template puts one newline either side of a value; exactly those
/// are removed, so a value's own leading or trailing whitespace survives.
fn parse_xml_call(body: &str) -> Option<ToolCall> {
    let after = &body[OPEN_FUNCTION.len()..];
    let end = after.find('>')?;
    let name = after[..end].trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let mut rest = &after[end + 1..];
    let mut arguments = serde_json::Map::new();
    while let Some(open) = rest.find(OPEN_PARAMETER) {
        let from = &rest[open + OPEN_PARAMETER.len()..];
        let key_end = from.find('>')?;
        let key = from[..key_end].trim().to_owned();
        let value_from = &from[key_end + 1..];
        let value_end = value_from.find(CLOSE_PARAMETER)?;
        let raw = &value_from[..value_end];
        let raw = raw.strip_prefix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let value = match serde_json::from_str::<serde_json::Value>(raw.trim()) {
            Ok(parsed) if !parsed.is_string() => parsed,
            _ => serde_json::Value::String(raw.to_owned()),
        };
        arguments.insert(key, value);
        rest = &value_from[value_end + CLOSE_PARAMETER.len()..];
    }
    Some(ToolCall {
        name,
        arguments: serde_json::Value::Object(arguments),
        id: None,
    })
}

fn parse_call_value(value: &serde_json::Value) -> Option<ToolCall> {
    let name = value.get("name")?.as_str()?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let arguments = match value.get("arguments") {
        // Some templates render the arguments as a JSON string rather than as
        // an object. Both are the same call.
        Some(serde_json::Value::String(encoded)) => serde_json::from_str(encoded).ok()?,
        Some(arguments) => arguments.clone(),
        None => serde_json::json!({}),
    };
    Some(ToolCall {
        name,
        arguments,
        // The call was never assigned a protocol id, and inventing one would
        // let a tool result claim to answer a call the backend never made.
        id: None,
    })
}

/// Granite's conventions, added as a proof that a second family costs a
/// family adapter and nothing else.
///
/// This is the boundary test the migration plan asks for: Granite is served
/// here without one change to the agent loop, the tool runtime, the verifier
/// or the backend adapters. If adding it had required touching any of those,
/// the compatibility layer would not be a boundary.
///
/// Granite's instruct templates mark reasoning with a `<|start_of_role|>`
/// header rather than a tag pair, and render calls as a bare JSON array in the
/// answer text -- IBM's own documented convention. Both are absorbed the same
/// way Qwen's are: text moves between channels, and nothing is invented.
pub struct GraniteFamilyAdapter;

const GRANITE_THOUGHT: &str = "<|start_of_role|>thought<|end_of_role|>";
const GRANITE_RESPONSE: &str = "<|start_of_role|>response<|end_of_role|>";

impl ModelBehaviorAdapter for GraniteFamilyAdapter {
    fn id(&self) -> &'static str {
        "granite"
    }

    fn version(&self) -> &'static str {
        "granite-v1"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        if let Some(thought) = canonical.narrative.find(GRANITE_THOUGHT) {
            let after = &canonical.narrative[thought + GRANITE_THOUGHT.len()..];
            // The response header closes the thought. Without one the whole
            // remainder is reasoning that never reached an answer, which is a
            // turn that ran out of room rather than a bad reply.
            let (thinking, rest) = match after.find(GRANITE_RESPONSE) {
                Some(response) => (
                    &after[..response],
                    &after[response + GRANITE_RESPONSE.len()..],
                ),
                None => (after, ""),
            };
            let mut narrative = canonical.narrative[..thought].to_owned();
            narrative.push_str(rest);
            if !canonical.thinking.is_empty() && !thinking.trim().is_empty() {
                canonical.thinking.push('\n');
            }
            canonical.thinking.push_str(thinking.trim());
            canonical.narrative = narrative.trim().to_owned();
            canonical.diagnostics.push(Diagnostic {
                kind: "granite_role_header_reasoning",
                detail: "reasoning marked by a role header was moved to the reasoning channel"
                    .into(),
            });
        }
        if canonical.tool_calls.is_empty()
            && let Some((narrative, calls)) = granite_calls(&canonical.narrative)
        {
            canonical.diagnostics.push(Diagnostic {
                kind: "granite_embedded_tool_calls",
                detail: format!("recovered {} call(s) from the answer text", calls.len()),
            });
            canonical.narrative = narrative;
            canonical.tool_calls = calls;
        }
        canonical
    }
}

/// Reads a bare JSON array of calls where Granite writes one.
///
/// The whole trimmed answer has to be that array. A model that wrote prose and
/// then an array is doing something this adapter has no evidence about, and
/// guessing which part was meant as the call is how an answer becomes an
/// action nobody asked for.
fn granite_calls(content: &str) -> Option<(String, Vec<ToolCall>)> {
    let trimmed = content.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let serde_json::Value::Array(items) = serde_json::from_str(trimmed).ok()? else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    let calls: Vec<ToolCall> = items.iter().filter_map(parse_call_value).collect();
    // All or nothing: a partially decoded array would drop calls the
    // deployment believes it made.
    (calls.len() == items.len()).then(|| (String::new(), calls))
}

/// Picks the adapter for a deployment from what was observed about it.
///
/// Matched on the family a backend reported, falling back to the model
/// reference. Anything unrecognised gets the generic adapter, which is the
/// point: an unknown family is unknown, not assumed compatible.
/// Seed-OSS (ByteDance): Qwen's XML calls and think spans under its own
/// tags -- `<seed:tool_call>`, `<seed:think>`, and a
/// `<seed:cot_budget_reflect>` note inside the reasoning. Its replies are read
/// by renaming those tags to Qwen's and reading them as Qwen's, so the two
/// families share one parser rather than two that drift.
pub struct SeedFamilyAdapter;

const SEED_TAGS: [(&str, &str); 6] = [
    ("<seed:tool_call>", "<tool_call>"),
    ("</seed:tool_call>", "</tool_call>"),
    ("<seed:think>", "<think>"),
    ("</seed:think>", "</think>"),
    ("<seed:cot_budget_reflect>", ""),
    ("</seed:cot_budget_reflect>", ""),
];

impl ModelBehaviorAdapter for SeedFamilyAdapter {
    fn id(&self) -> &'static str {
        "seed"
    }

    fn version(&self) -> &'static str {
        "seed-v1"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut renamed = reply.clone();
        for (seed, qwen) in SEED_TAGS {
            renamed.content = renamed.content.replace(seed, qwen);
            renamed.thinking = renamed.thinking.replace(seed, qwen);
        }
        QwenFamilyAdapter.normalize(&renamed)
    }
}

/// gpt-oss (OpenAI): the harmony format. A reply is a run of messages, each
/// `<|channel|>NAME[ to=RECIPIENT][ <|constrain|>json]<|message|>BODY` ended
/// by `<|end|>`, `<|call|>` or the end of the reply, the later ones opened by
/// `<|start|>assistant`. `analysis` is reasoning; `commentary to=functions.X`
/// is a call to X with a JSON body; `final`, and commentary addressed to no
/// one, is what the model says. Seen on PWR's MLX engine 2026-09-19:
/// `<|channel|>analysis<|message|>…<|end|><|start|>assistant<|channel|>
/// commentary to=functions.read_file <|constrain|>json<|message|>{"path":…}`.
pub struct HarmonyAdapter;

const HARMONY_CHANNEL: &str = "<|channel|>";
const HARMONY_MESSAGE: &str = "<|message|>";

impl ModelBehaviorAdapter for HarmonyAdapter {
    fn id(&self) -> &'static str {
        "harmony"
    }

    fn version(&self) -> &'static str {
        "harmony-v2"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        if !canonical.tool_calls.is_empty() || !reply.content.contains(HARMONY_CHANNEL) {
            return canonical;
        }
        let mut narrative = String::new();
        let mut thinking = canonical.thinking.clone();
        // A recipient whose header ended without a message, carried to the
        // next: gpt-oss writes `…to=functions.complete<|constrain|>json
        // <|channel|>analysis code<|message|>{…}`, and that body is the call's.
        let mut pending: Option<String> = None;
        for segment in reply.content.split(HARMONY_CHANNEL).skip(1) {
            let Some((header, body)) = segment.split_once(HARMONY_MESSAGE) else {
                if let Some(recipient) = harmony_recipient(segment) {
                    pending = Some(recipient);
                } else {
                    canonical.diagnostics.push(Diagnostic {
                        kind: "harmony_header_without_message",
                        detail: segment.chars().take(120).collect(),
                    });
                }
                continue;
            };
            let end = ["<|end|>", "<|call|>", "<|return|>", "<|start|>"]
                .iter()
                .filter_map(|marker| body.find(marker))
                .min();
            let body = end.map_or(body, |end| &body[..end]);
            let channel = header.split_whitespace().next().unwrap_or_default();
            match (channel, harmony_recipient(header).or(pending.take())) {
                (_, Some(target)) => {
                    if end.is_none() {
                        canonical.diagnostics.push(Diagnostic {
                            kind: "harmony_unterminated_tool_call",
                            detail: "a tool-call message had no closing protocol marker".into(),
                        });
                    }
                    let name = target.strip_prefix("functions.").unwrap_or(&target);
                    // The first JSON value is the arguments; anything the model
                    // wrote after it is not.
                    let arguments = serde_json::Deserializer::from_str(body.trim())
                        .into_iter::<serde_json::Value>()
                        .next()
                        .and_then(Result::ok)
                        .unwrap_or_else(|| serde_json::Value::String(body.trim().to_owned()));
                    canonical.tool_calls.push(ToolCall {
                        name: name.to_owned(),
                        arguments,
                        id: None,
                    });
                    canonical.diagnostics.push(Diagnostic {
                        kind: "harmony_call_read",
                        detail: name.to_owned(),
                    });
                }
                ("analysis", None) => {
                    if !thinking.is_empty() {
                        thinking.push('\n');
                    }
                    thinking.push_str(body.trim());
                }
                _ => {
                    if !narrative.is_empty() {
                        narrative.push('\n');
                    }
                    narrative.push_str(body.trim());
                }
            }
        }
        canonical.narrative = narrative;
        canonical.thinking = thinking;
        canonical
    }
}

/// The recipient a harmony header names, ending where the name does: at
/// whitespace or at the next `<|` token, which gpt-oss writes without a space
/// (`to=functions.apply_patch<|constrain|>json`).
fn harmony_recipient(header: &str) -> Option<String> {
    let after = header.split_once("to=")?.1;
    let end = after
        .find(|c: char| c.is_whitespace())
        .into_iter()
        .chain(after.find("<|"))
        .min()
        .unwrap_or(after.len());
    let name = after[..end].trim();
    (!name.is_empty()).then(|| name.to_owned())
}

pub fn render_tools(catalog: &pwr_domain::ToolCatalog) -> serde_json::Value {
    serde_json::Value::Array(
        catalog
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    }
                })
            })
            .collect(),
    )
}

/// GLM-4.x (Z.ai): `<tool_call>NAME<arg_key>K</arg_key><arg_value>V</arg_value>…
/// </tool_call>`, string values written as they are and the rest as JSON,
/// reasoning in `<think>` spans as Qwen's. Read from GLM-4.7-Flash's chat
/// template, 2026-09-19.
pub struct GlmFamilyAdapter;

impl ModelBehaviorAdapter for GlmFamilyAdapter {
    fn id(&self) -> &'static str {
        "glm"
    }

    fn version(&self) -> &'static str {
        "glm-v1"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        let (narrative, thinking, diagnostic) = split_thinking(&canonical.narrative);
        canonical.narrative = narrative;
        if let Some(diagnostic) = diagnostic {
            canonical.thinking.push_str(&thinking);
            canonical.diagnostics.push(diagnostic);
        }
        // A reply opened after a pre-filled empty think block starts with its
        // closing tag.
        if let Some(rest) = canonical.narrative.trim_start().strip_prefix(CLOSE_THINK) {
            canonical.narrative = rest.to_owned();
        }
        if !canonical.tool_calls.is_empty() {
            return canonical;
        }
        let mut narrative = String::new();
        let mut rest = canonical.narrative.as_str();
        while let Some(open) = rest.find(OPEN_TOOL) {
            narrative.push_str(&rest[..open]);
            let after = &rest[open + OPEN_TOOL.len()..];
            let (body, next) = match after.find(CLOSE_TOOL) {
                Some(close) => (&after[..close], &after[close + CLOSE_TOOL.len()..]),
                None => {
                    canonical.diagnostics.push(Diagnostic {
                        kind: "glm_unterminated_tool_call",
                        detail: "a <tool_call> block was not closed before the reply ended".into(),
                    });
                    (after, "")
                }
            };
            match glm_call(body) {
                Some(call) => {
                    canonical.diagnostics.push(Diagnostic {
                        kind: "glm_call_read",
                        detail: call.name.clone(),
                    });
                    canonical.tool_calls.push(call);
                }
                None => narrative.push_str(&rest[open..open + OPEN_TOOL.len() + body.len()]),
            }
            rest = next;
        }
        narrative.push_str(rest);
        canonical.narrative = narrative.trim().to_owned();
        canonical
    }
}

fn glm_call(body: &str) -> Option<ToolCall> {
    const KEY: &str = "<arg_key>";
    const END_KEY: &str = "</arg_key>";
    const VALUE: &str = "<arg_value>";
    const END_VALUE: &str = "</arg_value>";
    let name_end = body.find(KEY).unwrap_or(body.len());
    let name = body[..name_end].trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let mut arguments = serde_json::Map::new();
    let mut rest = &body[name_end..];
    while let Some(key_at) = rest.find(KEY) {
        let after_key = &rest[key_at + KEY.len()..];
        let key_end = after_key.find(END_KEY)?;
        let key = after_key[..key_end].trim().to_owned();
        let after = &after_key[key_end + END_KEY.len()..];
        let value_at = after.find(VALUE)?;
        let value_from = &after[value_at + VALUE.len()..];
        let value_end = value_from.find(END_VALUE)?;
        let raw = &value_from[..value_end];
        let value = match serde_json::from_str::<serde_json::Value>(raw.trim()) {
            Ok(parsed) if !parsed.is_string() => parsed,
            _ => serde_json::Value::String(raw.to_owned()),
        };
        arguments.insert(key, value);
        rest = &value_from[value_end + END_VALUE.len()..];
    }
    Some(ToolCall {
        name: name.to_owned(),
        arguments: serde_json::Value::Object(arguments),
        id: None,
    })
}

pub fn adapter_for(family: Option<&str>, model_ref: &str) -> Box<dyn ModelBehaviorAdapter> {
    let evidence = family.unwrap_or(model_ref).to_ascii_lowercase();
    // Nemotron 3.x writes Qwen's XML calls and think spans (its chat
    // template, 2026-09-19), so Qwen's adapter reads it.
    if evidence.contains("qwen") || evidence.contains("nemotron") {
        return Box::new(QwenFamilyAdapter);
    }
    if evidence.contains("glm") {
        return Box::new(GlmFamilyAdapter);
    }
    if evidence.contains("seed") {
        return Box::new(SeedFamilyAdapter);
    }
    if evidence.contains("gpt_oss") || evidence.contains("gpt-oss") {
        return Box::new(HarmonyAdapter);
    }
    if evidence.contains("granite") {
        return Box::new(GraniteFamilyAdapter);
    }
    Box::new(GenericAdapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glm_call_is_read_with_typed_values() {
        let text = "</think>I will read it.<tool_call>read_file<arg_key>path</arg_key><arg_value>src/a.py</arg_value><arg_key>first_line</arg_key><arg_value>12</arg_value></tool_call>";
        let adapter = adapter_for(
            Some("glm4_moe_lite"),
            "lmstudio-community/GLM-4.7-Flash-MLX-4bit",
        );
        assert_eq!(adapter.id(), "glm");
        let canonical = adapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1, "{canonical:?}");
        assert_eq!(canonical.tool_calls[0].name, "read_file");
        assert_eq!(canonical.tool_calls[0].arguments["path"], "src/a.py");
        assert_eq!(canonical.tool_calls[0].arguments["first_line"], 12);
        assert_eq!(canonical.narrative, "I will read it.");
    }

    #[test]
    fn nemotron_is_read_by_the_qwen_adapter() {
        let adapter = adapter_for(
            Some("nemotron_h"),
            "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
        );
        assert_eq!(adapter.id(), "qwen");
        let canonical = adapter.normalize(&reply("<tool_call>\n<function=complete>\n<parameter=rationale>\nDone.\n</parameter>\n</function>\n</tool_call>"));
        assert_eq!(canonical.tool_calls[0].name, "complete");
        assert_eq!(canonical.tool_calls[0].arguments["rationale"], "Done.");
    }

    #[test]
    fn a_harmony_call_is_read_with_its_json_arguments() {
        let text = "<|channel|>analysis<|message|>We need to read the file.<|end|><|start|>assistant<|channel|>commentary to=functions.read_file <|constrain|>json<|message|>{\"path\":\"src/app.py\",\"first_line\":10}";
        let adapter = adapter_for(Some("gpt_oss"), "mlx-community/gpt-oss-20b-MXFP4-Q8");
        assert_eq!(adapter.id(), "harmony");
        let canonical = adapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1, "{canonical:?}");
        assert_eq!(canonical.tool_calls[0].name, "read_file");
        assert_eq!(canonical.tool_calls[0].arguments["first_line"], 10);
        assert_eq!(canonical.thinking, "We need to read the file.");
        assert!(canonical.narrative.is_empty());
    }

    #[test]
    fn a_harmony_final_answer_is_the_narrative() {
        let text = "<|channel|>analysis<|message|>Easy.<|end|><|start|>assistant<|channel|>final<|message|>The entry point is main.<|return|>";
        let canonical = HarmonyAdapter.normalize(&reply(text));
        assert!(canonical.tool_calls.is_empty());
        assert_eq!(canonical.narrative, "The entry point is main.");
    }

    #[test]
    fn a_seed_call_is_read_as_the_qwen_call_it_is() {
        let text = "<seed:think>short</seed:think>Reading it.\n<seed:tool_call>\n<function=read_file>\n<parameter=path>src/a.py</parameter>\n<parameter=first_line>12</parameter>\n</function>\n</seed:tool_call>";
        let adapter = adapter_for(
            Some("seed_oss"),
            "lmstudio-community/Seed-OSS-36B-Instruct-MLX-4bit",
        );
        assert_eq!(adapter.id(), "seed");
        let canonical = adapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1, "{canonical:?}");
        let call = &canonical.tool_calls[0];
        assert_eq!(call.name, "read_file");
        assert_eq!(call.arguments["path"], "src/a.py");
        assert_eq!(call.arguments["first_line"], 12);
        assert!(
            !canonical.narrative.contains("seed:"),
            "{}",
            canonical.narrative
        );
        assert_eq!(canonical.thinking, "short");
    }

    fn reply(content: &str) -> ModelReply {
        ModelReply {
            content: content.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_generic_adapter_changes_nothing_it_does_not_understand() {
        let original = reply("<tool_call>{\"name\":\"read_file\"}</tool_call>");
        let canonical = GenericAdapter.normalize(&original);
        assert_eq!(canonical.narrative, original.content);
        assert!(canonical.tool_calls.is_empty());
        assert!(canonical.diagnostics.is_empty());
    }

    #[test]
    fn an_ordinary_reply_survives_normalization_unchanged() {
        // The property that matters most: a family adapter must be invisible
        // on every reply that did not need it.
        let original = ModelReply {
            content: "I read the file and it defines one function.".into(),
            thinking: "checking".into(),
            tool_calls: vec![ToolCall {
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/lib.rs"}),
                id: Some("call_1".into()),
            }],
            chunks: 4,
            metrics: None,
        };
        let canonical = QwenFamilyAdapter.normalize(&original);
        assert_eq!(canonical.narrative, original.content);
        assert_eq!(canonical.thinking, original.thinking);
        assert_eq!(canonical.tool_calls, original.tool_calls);
        assert_eq!(canonical.chunks, 4);
        assert!(canonical.diagnostics.is_empty());
    }

    #[test]
    fn a_call_written_into_the_answer_text_is_recovered_as_a_call() {
        let canonical = QwenFamilyAdapter.normalize(&reply(
            "I will read it.\n<tool_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"src/lib.rs\"}}\n</tool_call>",
        ));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].name, "read_file");
        assert_eq!(canonical.tool_calls[0].arguments["path"], "src/lib.rs");
        // No protocol id was issued, so none is claimed.
        assert!(canonical.tool_calls[0].id.is_none());
        assert_eq!(canonical.narrative, "I will read it.");
        assert_eq!(canonical.diagnostics[0].kind, "qwen_embedded_tool_call");
    }

    #[test]
    fn arguments_rendered_as_a_json_string_are_the_same_call() {
        let canonical = QwenFamilyAdapter.normalize(&reply(
            r#"<tool_call>{"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}</tool_call>"#,
        ));
        assert_eq!(canonical.tool_calls[0].arguments["path"], "a.rs");
    }

    #[test]
    fn a_backend_that_already_parsed_the_call_is_not_second_guessed() {
        let original = ModelReply {
            content:
                "<tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"a\"}}</tool_call>"
                    .into(),
            tool_calls: vec![ToolCall {
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "a"}),
                id: Some("call_1".into()),
            }],
            ..Default::default()
        };
        let canonical = QwenFamilyAdapter.normalize(&original);
        // One action was proposed, so one action is reported.
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].id.as_deref(), Some("call_1"));
    }

    #[test]
    fn several_embedded_calls_all_survive_in_the_order_written() {
        let canonical = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>{\"name\":\"read_file\",\"arguments\":{\"path\":\"a\"}}</tool_call>\
             <tool_call>{\"name\":\"read_file\",\"arguments\":{\"path\":\"b\"}}</tool_call>",
        ));
        let paths: Vec<&str> = canonical
            .tool_calls
            .iter()
            .map(|call| call.arguments["path"].as_str().unwrap())
            .collect();
        // Dropping the second would leave the deployment believing both reads
        // happened, which is the failure the loop's multi-call handling exists
        // to avoid; the adapter must not reintroduce it.
        assert_eq!(paths, vec!["a", "b"]);
    }

    #[test]
    fn inline_reasoning_moves_to_the_reasoning_channel() {
        let canonical =
            QwenFamilyAdapter.normalize(&reply("<think>weighing options</think>The answer is 4."));
        assert_eq!(canonical.narrative, "The answer is 4.");
        assert_eq!(canonical.thinking, "weighing options");
        assert_eq!(canonical.diagnostics[0].kind, "qwen_inline_thinking");
    }

    #[test]
    fn reasoning_cut_off_mid_span_is_reasoning_and_not_a_bad_answer() {
        // The turn spent its budget thinking. Read as prose it looks like a
        // deployment answering nonsense; read as reasoning it is a context
        // that left no room to reply, which is a different fix.
        let canonical = QwenFamilyAdapter.normalize(&reply("<think>step one, step two"));
        assert!(canonical.narrative.is_empty());
        assert_eq!(canonical.thinking, "step one, step two");
        assert_eq!(
            canonical.diagnostics[0].detail,
            "an unterminated <think> span was read as reasoning that was cut off"
        );
    }

    #[test]
    fn a_block_that_is_not_a_call_stays_visible_in_the_answer() {
        let canonical =
            QwenFamilyAdapter.normalize(&reply("<tool_call>not json at all</tool_call>"));
        assert!(canonical.tool_calls.is_empty());
        assert!(canonical.narrative.contains("not json at all"));
        assert_eq!(canonical.diagnostics[0].kind, "qwen_undecodable_tool_call");
    }

    #[test]
    fn an_unclosed_call_block_is_left_as_truncated_output() {
        let canonical =
            QwenFamilyAdapter.normalize(&reply("<tool_call>{\"name\": \"read_file\", \"argum"));
        assert!(canonical.tool_calls.is_empty());
        assert!(canonical.narrative.contains("<tool_call>"));
        assert_eq!(canonical.diagnostics[0].kind, "qwen_unterminated_tool_call");
    }

    #[test]
    fn granite_reasoning_marked_by_a_role_header_leaves_the_answer() {
        let canonical = GraniteFamilyAdapter.normalize(&reply(
            "<|start_of_role|>thought<|end_of_role|>weighing it\
             <|start_of_role|>response<|end_of_role|>The answer is 4.",
        ));
        assert_eq!(canonical.narrative, "The answer is 4.");
        assert!(canonical.thinking.starts_with("weighing it"));
        assert_eq!(
            canonical.diagnostics[0].kind,
            "granite_role_header_reasoning"
        );
    }

    #[test]
    fn granite_calls_written_as_a_bare_array_are_all_recovered_or_none() {
        let canonical = GraniteFamilyAdapter.normalize(&reply(
            r#"[{"name": "read_file", "arguments": {"path": "a"}}, {"name": "read_file", "arguments": {"path": "b"}}]"#,
        ));
        assert_eq!(canonical.tool_calls.len(), 2);
        assert!(canonical.narrative.is_empty());

        // One undecodable entry means the array is not a call list this
        // adapter understands. Taking the half that parsed would drop a call
        // the deployment believes it made.
        let partial = GraniteFamilyAdapter.normalize(&reply(
            r#"[{"name": "read_file", "arguments": {"path": "a"}}, {"note": "and then stop"}]"#,
        ));
        assert!(partial.tool_calls.is_empty());
        assert!(partial.narrative.starts_with('['));
    }

    #[test]
    fn prose_followed_by_an_array_is_not_read_as_an_action() {
        let canonical = GraniteFamilyAdapter.normalize(&reply(
            r#"Here is what I would do: [{"name": "delete_file", "arguments": {"path": "a"}}]"#,
        ));
        assert!(canonical.tool_calls.is_empty());
    }

    /// The point of adding Granite at all: a second family is a second
    /// adapter, and nothing else moves.
    #[test]
    fn a_second_family_reuses_the_same_canonical_form() {
        let qwen = QwenFamilyAdapter.normalize(&reply(
            r#"<tool_call>{"name": "read_file", "arguments": {"path": "a"}}</tool_call>"#,
        ));
        let granite = GraniteFamilyAdapter.normalize(&reply(
            r#"[{"name": "read_file", "arguments": {"path": "a"}}]"#,
        ));
        assert_eq!(qwen.tool_calls, granite.tool_calls);
    }

    #[test]
    fn every_adapter_names_a_revision_a_certification_can_be_scoped_to() {
        let versions = [
            GenericAdapter.version(),
            QwenFamilyAdapter.version(),
            GraniteFamilyAdapter.version(),
        ];
        assert!(versions.iter().all(|version| !version.is_empty()));
        // Distinct, or two adapters would share one another's evidence.
        let unique: std::collections::BTreeSet<_> = versions.iter().collect();
        assert_eq!(unique.len(), versions.len());
    }

    #[test]
    fn an_unknown_family_is_never_assumed_to_follow_a_convention() {
        assert_eq!(adapter_for(Some("qwen3_5"), "qwen/qwen3.5-9b").id(), "qwen");
        assert_eq!(adapter_for(None, "qwen3.8:27b-mlx").id(), "qwen");
        assert_eq!(adapter_for(Some("granite"), "granite:8b").id(), "granite");
        assert_eq!(adapter_for(None, "phi-4-reasoning-plus").id(), "generic");
        assert_eq!(adapter_for(Some("llama"), "llama3:8b").id(), "generic");
    }

    #[test]
    fn qwen_reads_the_xml_parameter_form_its_newer_templates_write() {
        let adapter = QwenFamilyAdapter;
        let text = "I will read it.\n<tool_call>\n<function=read_file>\n<parameter=path>\nsrc/lib.rs\n</parameter>\n<parameter=max_lines>\n30\n</parameter>\n</function>\n</tool_call>";
        let canonical = adapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1);
        let call = &canonical.tool_calls[0];
        assert_eq!(call.name, "read_file");
        assert_eq!(call.arguments["path"], "src/lib.rs");
        assert_eq!(call.arguments["max_lines"], 30);
        assert_eq!(canonical.narrative, "I will read it.");
    }

    #[test]
    fn an_xml_value_keeps_its_own_lines_and_stays_text_unless_it_is_json() {
        let adapter = QwenFamilyAdapter;
        let text = "<tool_call>\n<function=write_file>\n<parameter=path>\n123\n</parameter>\n<parameter=content>\nline one\n  indented\n</parameter>\n<parameter=regex>\nfalse\n</parameter>\n</function>\n</tool_call>";
        let call = &adapter.normalize(&reply(text)).tool_calls[0];
        assert_eq!(call.arguments["content"], "line one\n  indented");
        assert_eq!(call.arguments["regex"], false);
        // A bare number is read as a number; the tool's own schema check is
        // what refuses a path that is not a string.
        assert_eq!(call.arguments["path"], 123);
    }

    #[test]
    fn an_xml_call_with_no_parameters_is_still_a_call() {
        let adapter = QwenFamilyAdapter;
        let text = "<tool_call>\n<function=complete>\n</function>\n</tool_call>";
        let call = &adapter.normalize(&reply(text)).tool_calls[0];
        assert_eq!(call.name, "complete");
        assert_eq!(call.arguments, serde_json::json!({}));
    }
}
