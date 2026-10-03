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
        // v3: and that form without its `<tool_call>` opening, as Qwen3-Coder
        // arrives from the MLX engine.
        "qwen-v4"
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
            if let Some((renamed, diagnostic)) = tools_tag_as_call(&canonical.narrative) {
                canonical.narrative = renamed;
                canonical.diagnostics.push(diagnostic);
            }
            if let Some((prose, calls, diagnostic)) = fenced_calls(&canonical.narrative) {
                canonical.narrative = prose;
                canonical.tool_calls = calls;
                canonical.diagnostics.push(diagnostic);
                return canonical;
            }
            let (wrapped, added) = wrap_bare_xml_calls(&canonical.narrative);
            let (narrative, calls, mut diagnostics) = extract_tool_calls(&wrapped);
            canonical.narrative = narrative;
            if added && !calls.is_empty() {
                diagnostics.push(Diagnostic {
                    kind: "qwen_bare_function_call",
                    detail: "read a <function=...> call written without its <tool_call> opening"
                        .into(),
                });
            }
            canonical.tool_calls = calls;
            canonical.diagnostics.extend(diagnostics);
        }
        canonical
    }
}

/// Qwen3-Coder, as the MLX engine returns it, writes
/// `<function=read_file>…</function></tool_call>`: the opening `<tool_call>` is
/// a token the engine does not hand back. A `<function=` that no `<tool_call>`
/// opens gets the opening (and the closing, when it is missing too) so the one
/// reader of the form reads it; a reply that wrote both is left alone.
/// Measured 2026-10-01: Quick Calibration called the model "no tool call was
/// made" and refused it agent tasks. Returns the text and whether it changed.
// Stop at the explicit call envelope or the next function opening. Within
// one candidate, keep every function closing tag so the parser can reject
// ambiguous delimiters instead of accepting the first truncated prefix.
fn xml_function_end(text: &str) -> Option<usize> {
    let bound = [
        text.find(CLOSE_TOOL),
        text[OPEN_FUNCTION.len()..]
            .find(OPEN_FUNCTION)
            .map(|at| at + OPEN_FUNCTION.len()),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(text.len());
    text[..bound]
        .rfind(CLOSE_FUNCTION)
        .map(|at| at + CLOSE_FUNCTION.len())
}

fn wrap_bare_xml_calls(content: &str) -> (String, bool) {
    if !content.contains(OPEN_FUNCTION) {
        return (content.to_owned(), false);
    }
    let mut out = String::new();
    let mut changed = false;
    let mut rest = content;
    while let Some(at) = rest.find(OPEN_FUNCTION) {
        let before = &rest[..at];
        if before.trim_end().ends_with(OPEN_TOOL) {
            // Opened properly: copy through the block's end and go on after it.
            // A call whose `</function>` is there but whose `</tool_call>` the
            // reply never wrote is whole all the same (Qwen3-Coder ended nine
            // replies of one run so, each refused as "stopped inside an
            // unfinished tool call", 2026-10-01): closed here.
            let closed = rest[at..].find(CLOSE_TOOL).map(|close| at + close);
            let function_end = xml_function_end(&rest[at..]).map(|end| at + end);
            let end = match (closed, function_end) {
                (Some(close), _) => close + CLOSE_TOOL.len(),
                (None, Some(function)) => {
                    out.push_str(&rest[..function]);
                    out.push('\n');
                    out.push_str(CLOSE_TOOL);
                    changed = true;
                    rest = &rest[function..];
                    continue;
                }
                (None, None) => rest.len(),
            };
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        out.push_str(before);
        out.push_str(OPEN_TOOL);
        out.push('\n');
        changed = true;
        let Some(function_end) = xml_function_end(&rest[at..]) else {
            // Cut off inside the call: the unterminated-call path names it.
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        let end = at + function_end;
        out.push_str(&rest[at..end]);
        if !rest[end..].trim_start().starts_with(CLOSE_TOOL) {
            out.push('\n');
            out.push_str(CLOSE_TOOL);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out, changed)
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

/// Qwen2.5-Coder writes its call inside `<tools>` -- the tag its template
/// lists the available tools in -- rather than `<tool_call>`. Measured
/// 2026-09-29 in Quick Calibration: asked to read a file, it wrote
/// `<tools>{"name": "read_file", "arguments": {"path": "src/parser.rs"}}</tools>`,
/// a well-formed call, and failed tool selection for it. A `<tools>` block
/// whose whole body is one call is read as one; any other is left as text.
fn tools_tag_as_call(content: &str) -> Option<(String, Diagnostic)> {
    const OPEN: &str = "<tools>";
    const CLOSE: &str = "</tools>";
    if content.contains(OPEN_TOOL) || !content.contains(OPEN) {
        return None;
    }
    let mut renamed = String::new();
    let mut rest = content;
    let mut found = 0;
    while let Some(open) = rest.find(OPEN) {
        let body_start = open + OPEN.len();
        let Some(close) = rest[body_start..].find(CLOSE) else {
            break;
        };
        let body = &rest[body_start..body_start + close];
        renamed.push_str(&rest[..open]);
        if parse_call(body).is_some() {
            renamed.push_str(OPEN_TOOL);
            renamed.push_str(body);
            renamed.push_str(CLOSE_TOOL);
            found += 1;
        } else {
            renamed.push_str(&rest[open..body_start + close + CLOSE.len()]);
        }
        rest = &rest[body_start + close + CLOSE.len()..];
    }
    renamed.push_str(rest);
    (found > 0).then(|| {
        (
            renamed,
            Diagnostic {
                kind: "qwen_tools_tag_tool_call",
                detail: format!("read {found} call(s) written inside <tools> as <tool_call>"),
            },
        )
    })
}

/// Qwen2.5-Coder's other form: fenced JSON blocks each holding one call,
/// alone or after a sentence about them. Measured 2026-09-29: in the
/// capability probe it answered an edit with only ```json {"name":
/// "apply_replace", ...} ```, and in suite A3 with "Let's read the file
/// first." followed by one such block -- or two, to read two files -- and
/// every task ended after three turns refused as calling no tool.
///
/// Prose between and after the blocks is kept as narrative: measured the same
/// day building small projects from scratch, it writes "### Creating
/// `index.html`" before one block, "Now let's check the files." before the
/// next, and a closing sentence after the last -- and lost all four tasks
/// when only blocks ending the reply were read. Read only when every block in
/// the reply is a call: one code example, or any fenced text that is not a
/// call, and nothing is read.
fn fenced_calls(content: &str) -> Option<(String, Vec<ToolCall>, Diagnostic)> {
    const FENCE: &str = "```";
    let parts: Vec<&str> = content.split(FENCE).collect();
    // prose, body, prose, body, ..., body, prose -- an odd count.
    if parts.len() < 3 || parts.len().is_multiple_of(2) {
        return None;
    }
    // Fenced JSON has no call channel. Recover only a reply consisting wholly
    // of call fences; prose can describe an example rather than request effects.
    if parts.iter().step_by(2).any(|part| !part.trim().is_empty()) {
        return None;
    }
    let mut calls = Vec::new();
    let mut prose = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if index % 2 == 0 {
            if !part.trim().is_empty() {
                prose.push(part.trim());
            }
            continue;
        }
        let body = part.strip_prefix("json").unwrap_or(part);
        calls.push(parse_call(body)?);
    }
    let names: Vec<&str> = calls.iter().map(|call| call.name.as_str()).collect();
    let diagnostic = Diagnostic {
        kind: "qwen_fenced_tool_call",
        detail: format!("read {} from fenced JSON blocks", names.join(", ")),
    };
    Some((prose.join("\n\n"), calls, diagnostic))
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
    let value = serde_json::from_str(body)
        .ok()
        .or_else(|| serde_json::from_str(&python_quote_escapes(body)).ok())
        .or_else(|| {
            let closed = close_unterminated(&python_quote_escapes(body))?;
            serde_json::from_str(&closed).ok()
        })?;
    parse_call_value(&value)
}

/// A call whose block the model closed without closing the JSON inside it:
/// the last string and the objects around it left open. Measured 2026-09-29:
/// Qwen2.5-Coder ended a `write_file` block right after the file's last line,
/// with no `"`, `}` or `}` -- the whole reply, a write, a check and a
/// completion, went unread. Only called for a block whose fence or tag was
/// closed, so a reply cut off by the length limit is never completed this
/// way; and only where the text ends inside what it opened, so a stray or
/// unbalanced closer is left to fail as it is.
fn close_unterminated(body: &str) -> Option<String> {
    let mut open = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in body.chars() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => open.push('}'),
            '[' => open.push(']'),
            '}' | ']' if open.pop() != Some(c) => return None,
            _ => {}
        }
    }
    if !in_string && open.is_empty() {
        return None;
    }
    let mut closed = body.trim_end().to_owned();
    if in_string {
        closed.push('"');
    }
    closed.extend(open.iter().rev());
    Some(closed)
}

/// JSON with Python's `\'` read as the apostrophe it means. `\'` is never
/// valid JSON, so there is one reading; `\\'` -- a backslash, then a quote --
/// is left as it is. Measured 2026-09-29: Qwen2.5-Coder wrote a replace_text
/// whose code held `\'Bag\'`, and the whole call went unread.
fn python_quote_escapes(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\'') => out.push('\''),
            Some(next) => {
                out.push('\\');
                out.push(next);
            }
            None => out.push('\\'),
        }
    }
    out
}

const OPEN_FUNCTION: &str = "<function=";
const OPEN_PARAMETER: &str = "<parameter=";
const CLOSE_PARAMETER: &str = "</parameter>";
const CLOSE_FUNCTION: &str = "</function>";

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
    // A literal function delimiter in a value is ambiguous in this raw
    // protocol. Never execute a guessed prefix of a file or command.
    if body.matches(CLOSE_FUNCTION).count() != 1 || !body.trim_end().ends_with(CLOSE_FUNCTION) {
        return None;
    }
    let after = &body[OPEN_FUNCTION.len()..];
    let end = after.find('>')?;
    let name = after[..end].trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let mut rest = &after[end + 1..];
    let mut arguments = serde_json::Map::new();
    while let Some(open) = rest.find(OPEN_PARAMETER) {
        if !rest[..open].trim().is_empty() {
            return None;
        }
        let from = &rest[open + OPEN_PARAMETER.len()..];
        let key_end = from.find('>')?;
        let key = from[..key_end].trim().to_owned();
        // A name that is not a name is an opening tag that never closed
        // (`<parameter=rationale` and then `</parameter>`): not a call to guess at.
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return None;
        }
        let value_from = &from[key_end + 1..];
        // Where this parameter's value can end at the latest: the next
        // parameter or the end of the function. Within that, the value is
        // closed by `</parameter>` -- or, as Qwen 3.6 sometimes writes it, by
        // a closing tag named for the parameter (`<parameter=content>` ...
        // `</content>`), or not closed at all. Measured 2026-09-26 on the
        // stack matrix: a `write_file` closed with `</content>` was dropped as
        // undecodable, the reply counted as an answer, and the model --
        // seeing its file never written -- wrote it the same way again
        // sixty times. All three have one reading.
        let boundary = [
            value_from.find(&format!("\n{OPEN_PARAMETER}")),
            value_from.find(CLOSE_FUNCTION),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(value_from.len());
        let segment = &value_from[..boundary];
        if value_from[boundary..].starts_with(CLOSE_FUNCTION)
            && value_from[boundary + CLOSE_FUNCTION.len()..].contains(CLOSE_PARAMETER)
        {
            return None;
        }
        let named_close = format!("</{key}>");
        let trimmed = segment.trim_end();
        let (raw, consumed) = if let Some(value) = trimmed.strip_suffix(CLOSE_PARAMETER) {
            (value, boundary)
        } else if let Some(value) = trimmed.strip_suffix(named_close.as_str()) {
            (value, boundary)
        } else if let Some(close) = segment.find(CLOSE_PARAMETER) {
            (&segment[..close], close + CLOSE_PARAMETER.len())
        } else if trimmed
            .rfind("</")
            .is_some_and(|tag| !trimmed[tag..].contains('>'))
        {
            // Closed by half a tag (`3750\n</`): the value's end was lost,
            // and what is left of it is not the value.
            return None;
        } else {
            (trimmed, boundary)
        };
        let raw = raw.strip_prefix('\n').unwrap_or(raw);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let value = match serde_json::from_str::<serde_json::Value>(raw.trim()) {
            Ok(parsed) if !parsed.is_string() => parsed,
            // A list or object written the Python way (`['-m', 'unittest']`):
            // Qwen3-Coder writes `args` so, six calls of one run refused as
            // "written as JSON but not valid JSON" (2026-10-01). It has one
            // reading, and only a value that is wholly such a literal is read.
            _ => python_literal(raw.trim())
                .unwrap_or_else(|| serde_json::Value::String(raw.to_owned())),
        };
        if arguments.insert(key, value).is_some() {
            return None;
        }
        rest = &value_from[consumed..];
    }
    if rest.trim() != CLOSE_FUNCTION {
        return None;
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

/// Mistral's conventions (Mistral Small, Devstral, Magistral, Ministral): a call
/// is `[TOOL_CALLS]name[ARGS]{"path": "src/a.rs"}`, several in a row each with
/// its own marker; the older template writes `[TOOL_CALLS][{"name": …,
/// "arguments": {…}}]`. Measured 2026-10-01: Devstral-Small-2 was refused agent
/// tasks (Limited, "no tool call was made") because nothing read this form.
pub struct MistralFamilyAdapter;

const MISTRAL_CALLS: &str = "[TOOL_CALLS]";
const MISTRAL_ARGS: &str = "[ARGS]";

/// The first JSON value in `text` and what follows it, or `None` when it is
/// not whole.
fn first_json_value(text: &str) -> Option<(serde_json::Value, &str)> {
    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<serde_json::Value>();
    let value = stream.next()?.ok()?;
    Some((value, &text[stream.byte_offset()..]))
}

fn mistral_call(segment: &str) -> Option<Vec<ToolCall>> {
    let segment = segment.trim();
    if segment.starts_with('[') {
        // The older form: one JSON array of calls.
        let (value, _) = first_json_value(segment)?;
        let calls: Vec<ToolCall> = value
            .as_array()?
            .iter()
            .map(parse_call_value)
            .collect::<Option<_>>()?;
        return (!calls.is_empty()).then_some(calls);
    }
    let (name, rest) = segment.split_once(MISTRAL_ARGS)?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let (arguments, _) = first_json_value(rest.trim_start())?;
    arguments.is_object().then(|| {
        vec![ToolCall {
            name: name.to_owned(),
            arguments,
            id: None,
        }]
    })
}

impl ModelBehaviorAdapter for MistralFamilyAdapter {
    fn id(&self) -> &'static str {
        "mistral"
    }

    fn version(&self) -> &'static str {
        "mistral-v2"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        if !canonical.tool_calls.is_empty() || !canonical.narrative.contains(MISTRAL_CALLS) {
            return canonical;
        }
        let text = canonical.narrative.clone();
        let mut parts = text.split(MISTRAL_CALLS);
        let narrative = parts.next().unwrap_or_default().trim().to_owned();
        let mut calls = Vec::new();
        let mut undecodable = false;
        for segment in parts {
            match mistral_call(segment) {
                Some(found) => calls.extend(found),
                None => undecodable = true,
            }
        }
        if undecodable {
            // Kept whole: a call that did not decode is evidence about the
            // template, and nothing is guessed at.
            canonical.diagnostics.push(Diagnostic {
                kind: "mistral_unterminated_tool_call",
                detail: "a [TOOL_CALLS] block did not hold a whole name and arguments".into(),
            });
            return canonical;
        }
        canonical.narrative = narrative;
        canonical.diagnostics.push(Diagnostic {
            kind: "mistral_tool_call",
            detail: format!(
                "read {} from [TOOL_CALLS]",
                calls
                    .iter()
                    .map(|call| call.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
        canonical.tool_calls = calls;
        canonical
    }
}

/// Liquid's LFM2 family: a call is a Python-style list between
/// `<|tool_call_start|>` and `<|tool_call_end|>` --
/// `[read_file(path="src/a.rs", max_lines=40)]` -- and reasoning is `<think>`.
/// The larger LFM2 also writes `<function_call>{"name": …, "arguments": …}`
/// blocks. Measured 2026-10-01: both were refused agent tasks (Limited, "no
/// tool call was made") because no adapter read either.
pub struct LiquidFamilyAdapter;

const LFM_START: &str = "<|tool_call_start|>";
const LFM_END: &str = "<|tool_call_end|>";
const FUNCTION_CALL_OPEN: &str = "<function_call>";
const FUNCTION_CALL_CLOSE: &str = "</function_call>";

/// A whole list or dict written as a Python literal, or `None`.
fn python_literal(text: &str) -> Option<serde_json::Value> {
    if !(text.starts_with('[') || text.starts_with('{')) {
        return None;
    }
    let mut reader = PyArgs::new(text);
    let value = reader.value()?;
    reader.skip();
    (reader.at == reader.text.len()).then_some(value)
}

/// A reader of the Python literal subset a call's arguments use: strings
/// (single, double, triple-quoted, with escapes), numbers, `True`/`False`/
/// `None`, lists, tuples and dicts. Anything else is not a call.
struct PyArgs<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> PyArgs<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            at: 0,
        }
    }
    fn skip(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        self.skip();
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn ident(&mut self) -> Option<String> {
        self.skip();
        let start = self.at;
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
        {
            self.at += 1;
        }
        (self.at > start).then(|| String::from_utf8_lossy(&self.text[start..self.at]).into_owned())
    }
    fn string(&mut self) -> Option<String> {
        self.skip();
        let quote = *self.text.get(self.at)?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let triple = self.text.get(self.at..self.at + 3) == Some(&[quote, quote, quote]);
        self.at += if triple { 3 } else { 1 };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let byte = *self.text.get(self.at)?;
            if triple {
                if self.text.get(self.at..self.at + 3) == Some(&[quote, quote, quote]) {
                    self.at += 3;
                    break;
                }
            } else if byte == quote {
                self.at += 1;
                break;
            }
            if byte == b'\\' {
                self.at += 1;
                let escaped = *self.text.get(self.at)?;
                match escaped {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'0' => out.push(0),
                    b'\\' | b'\'' | b'"' => out.push(escaped),
                    b'\n' => {}
                    other => {
                        out.push(b'\\');
                        out.push(other);
                    }
                }
            } else {
                out.push(byte);
            }
            self.at += 1;
        }
        String::from_utf8(out).ok()
    }
    fn value(&mut self) -> Option<serde_json::Value> {
        self.skip();
        match *self.text.get(self.at)? {
            b'"' | b'\'' => self.string().map(serde_json::Value::String),
            b'[' | b'(' => {
                let close = if self.text[self.at] == b'[' {
                    b']'
                } else {
                    b')'
                };
                self.at += 1;
                let mut items = Vec::new();
                while !self.eat(close) {
                    items.push(self.value()?);
                    if !self.eat(b',') && self.text.get(self.at) != Some(&close) {
                        self.skip();
                        if self.text.get(self.at) != Some(&close) {
                            return None;
                        }
                    }
                }
                Some(serde_json::Value::Array(items))
            }
            b'{' => {
                self.at += 1;
                let mut map = serde_json::Map::new();
                while !self.eat(b'}') {
                    let key = match self.value()? {
                        serde_json::Value::String(key) => key,
                        other => other.to_string(),
                    };
                    if !self.eat(b':') {
                        return None;
                    }
                    map.insert(key, self.value()?);
                    if !self.eat(b',') {
                        self.skip();
                        if self.text.get(self.at) != Some(&b'}') {
                            return None;
                        }
                    }
                }
                Some(serde_json::Value::Object(map))
            }
            b'-' | b'0'..=b'9' => {
                let start = self.at;
                self.at += 1;
                while self.text.get(self.at).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(*b, b'.' | b'e' | b'E' | b'+' | b'-')
                }) {
                    self.at += 1;
                }
                let number = std::str::from_utf8(&self.text[start..self.at]).ok()?;
                serde_json::from_str(number).ok()
            }
            _ => match self.ident()?.as_str() {
                "True" | "true" => Some(serde_json::Value::Bool(true)),
                "False" | "false" => Some(serde_json::Value::Bool(false)),
                "None" | "null" => Some(serde_json::Value::Null),
                _ => None,
            },
        }
    }
    /// `name(key=value, …)`
    fn call(&mut self) -> Option<ToolCall> {
        let name = self.ident()?;
        if !self.eat(b'(') {
            return None;
        }
        let mut arguments = serde_json::Map::new();
        while !self.eat(b')') {
            let key = self.ident()?;
            if !self.eat(b'=') {
                return None;
            }
            arguments.insert(key, self.value()?);
            if !self.eat(b',') {
                self.skip();
                if self.text.get(self.at) != Some(&b')') {
                    return None;
                }
            }
        }
        Some(ToolCall {
            name,
            arguments: serde_json::Value::Object(arguments),
            id: None,
        })
    }
    /// `[call, call]`, or one bare call.
    fn calls(&mut self) -> Option<Vec<ToolCall>> {
        let listed = self.eat(b'[');
        let mut calls = Vec::new();
        loop {
            self.skip();
            if listed && self.eat(b']') {
                break;
            }
            calls.push(self.call()?);
            if listed {
                if self.eat(b',') {
                    continue;
                }
                if self.eat(b']') {
                    break;
                }
                return None;
            }
            break;
        }
        self.skip();
        (self.at == self.text.len() && !calls.is_empty()).then_some(calls)
    }
}

impl ModelBehaviorAdapter for LiquidFamilyAdapter {
    fn id(&self) -> &'static str {
        "liquid"
    }

    fn version(&self) -> &'static str {
        "liquid-v1"
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
        if !canonical.tool_calls.is_empty() {
            return canonical;
        }
        let text = canonical.narrative.clone();
        let mut prose = String::new();
        let mut calls = Vec::new();
        let mut rest = text.as_str();
        loop {
            let python = rest.find(LFM_START);
            let json = rest.find(FUNCTION_CALL_OPEN);
            let (at, open, close) = match (python, json) {
                (Some(p), Some(j)) if p < j => (p, LFM_START, LFM_END),
                (Some(_), Some(j)) => (j, FUNCTION_CALL_OPEN, FUNCTION_CALL_CLOSE),
                (Some(p), None) => (p, LFM_START, LFM_END),
                (None, Some(j)) => (j, FUNCTION_CALL_OPEN, FUNCTION_CALL_CLOSE),
                (None, None) => break,
            };
            prose.push_str(&rest[..at]);
            let body_from = at + open.len();
            let Some(end) = rest[body_from..].find(close) else {
                canonical.diagnostics.push(Diagnostic {
                    kind: "liquid_unterminated_tool_call",
                    detail: "a tool call block was not closed before the reply ended".into(),
                });
                canonical.narrative = text.trim().to_owned();
                return canonical;
            };
            let body = rest[body_from..body_from + end].trim();
            let found = if open == LFM_START {
                PyArgs::new(body).calls()
            } else {
                serde_json::from_str::<serde_json::Value>(body)
                    .ok()
                    .as_ref()
                    .and_then(parse_call_value)
                    .map(|call| vec![call])
            };
            match found {
                Some(found) => calls.extend(found),
                None => {
                    // Not a call: kept, tag and all, for whoever reads the log.
                    prose.push_str(&rest[at..body_from + end + close.len()]);
                    canonical.diagnostics.push(Diagnostic {
                        kind: "liquid_undecodable_tool_call",
                        detail: "a tool call block did not hold a name and arguments".into(),
                    });
                }
            }
            rest = &rest[body_from + end + close.len()..];
        }
        prose.push_str(rest);
        if !calls.is_empty() {
            canonical.diagnostics.push(Diagnostic {
                kind: "liquid_tool_call",
                detail: format!(
                    "read {} from the reply text",
                    calls
                        .iter()
                        .map(|call| call.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
        canonical.narrative = prose.trim().to_owned();
        canonical.tool_calls = calls;
        canonical
    }
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
        "granite-v2"
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
        // Granite 4.1's template writes `<tool_call>{"name": …, "arguments":
        // …}</tool_call>`, the block Qwen's adapter reads (Quick Calibration
        // 2026-10-01: Limited, "no tool call was made").
        if canonical.tool_calls.is_empty() {
            let (narrative, calls, diagnostics) = extract_tool_calls(&canonical.narrative);
            if !calls.is_empty() {
                canonical.narrative = narrative;
                canonical.tool_calls = calls;
            }
            canonical.diagnostics.extend(diagnostics);
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

/// Gemma 4's chat template writes calls as
/// `<|tool_call>call:name{key:<|"|>value<|"|>}<tool_call|>`.
pub struct Gemma4Adapter;

impl ModelBehaviorAdapter for Gemma4Adapter {
    fn id(&self) -> &'static str {
        "gemma4"
    }
    fn version(&self) -> &'static str {
        "gemma4-v3"
    }

    fn normalize(&self, reply: &ModelReply) -> CanonicalReply {
        let mut canonical = CanonicalReply::verbatim(reply);
        let backend_parsed_calls = !canonical.tool_calls.is_empty();
        let mut content = reply.content.clone();
        while let Some(start) = content.find("<|channel>thought") {
            let after = start + "<|channel>thought".len();
            let Some(end) = content[after..].find("<channel|>") else {
                canonical.diagnostics.push(Diagnostic {
                    kind: "gemma_unterminated_thought",
                    detail: "a thought channel had no closing marker".into(),
                });
                content.truncate(start);
                break;
            };
            let end = after + end;
            if !canonical.thinking.is_empty() {
                canonical.thinking.push('\n');
            }
            canonical.thinking.push_str(content[after..end].trim());
            content.replace_range(start..end + "<channel|>".len(), "");
        }
        // Some Gemma 4 continuations start inside the thought channel. In
        // that case the opening token is in the prompt, not the generated
        // reply, but the closing marker still separates thought from answer.
        if let Some(end) = content
            .rfind("<channel|>")
            .filter(|end| !content[..*end].contains("<|tool_call>"))
        {
            let thought = content[..end].trim();
            if !thought.is_empty() {
                if !canonical.thinking.is_empty() {
                    canonical.thinking.push('\n');
                }
                canonical.thinking.push_str(thought);
            }
            content = content[end + "<channel|>".len()..].to_owned();
            canonical.diagnostics.push(Diagnostic {
                kind: "gemma_implicit_thought",
                detail: "a thought channel began in the prompt and ended in the reply".into(),
            });
        }
        let mut rest = content.as_str();
        let mut narrative = String::new();
        while let Some(start) = rest.find("<|tool_call>") {
            narrative.push_str(&rest[..start]);
            let after = &rest[start + "<|tool_call>".len()..];
            let Some(end) = after.find("<tool_call|>") else {
                canonical.diagnostics.push(Diagnostic {
                    kind: "gemma_unterminated_tool_call",
                    detail: "a tool call had no closing marker".into(),
                });
                canonical.narrative = narrative;
                return canonical;
            };
            let body = after[..end].trim();
            if let Some((name, arguments)) = gemma_call(body) {
                if !backend_parsed_calls {
                    canonical.tool_calls.push(ToolCall {
                        name,
                        arguments,
                        id: None,
                    });
                }
            } else {
                canonical.diagnostics.push(Diagnostic {
                    kind: "gemma_undecodable_tool_call",
                    detail: "a tool call could not be parsed".into(),
                });
            }
            rest = &after[end + "<tool_call|>".len()..];
        }
        narrative.push_str(rest);
        canonical.narrative = narrative
            .replace("<turn|>", "")
            .replace("<|channel>final\n", "")
            .trim()
            .to_owned();
        canonical
    }
}

fn gemma_call(body: &str) -> Option<(String, serde_json::Value)> {
    let body = body.strip_prefix("call:")?;
    let brace = body.find('{')?;
    let name = body[..brace].trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let mut parser = GemmaValue {
        rest: &body[brace..],
    };
    let arguments = parser.value()?;
    (parser.rest.trim().is_empty() && arguments.is_object()).then(|| (name.to_owned(), arguments))
}

struct GemmaValue<'a> {
    rest: &'a str,
}

impl GemmaValue<'_> {
    fn value(&mut self) -> Option<serde_json::Value> {
        self.rest = self.rest.trim_start();
        if let Some(after) = self.rest.strip_prefix("<|\"|>") {
            let end = after.find("<|\"|>")?;
            self.rest = &after[end + "<|\"|>".len()..];
            return Some(serde_json::Value::String(after[..end].to_owned()));
        }
        // Some replies use ordinary quoted strings in the native call body.
        // Parse only a complete string literal; never split or execute it.
        if let Some(quote @ ('\'' | '"')) = self.rest.chars().next() {
            let mut escaped = false;
            for (index, ch) in self.rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    let end = index + ch.len_utf8();
                    let mut reader = PyArgs::new(&self.rest[..end]);
                    let value = reader.value()?;
                    if reader.at != end {
                        return None;
                    }
                    if !value.is_string() {
                        return None;
                    }
                    self.rest = &self.rest[end..];
                    return Some(value);
                }
            }
            return None;
        }
        if let Some(after) = self.rest.strip_prefix('{') {
            self.rest = after;
            let mut map = serde_json::Map::new();
            loop {
                self.rest = self.rest.trim_start();
                if let Some(after) = self.rest.strip_prefix('}') {
                    self.rest = after;
                    return Some(serde_json::Value::Object(map));
                }
                let colon = self.rest.find(':')?;
                let key = self.rest[..colon].trim();
                if key.is_empty() {
                    return None;
                }
                self.rest = &self.rest[colon + 1..];
                map.insert(key.to_owned(), self.value()?);
                self.rest = self.rest.trim_start();
                if let Some(after) = self.rest.strip_prefix(',') {
                    self.rest = after;
                } else if !self.rest.starts_with('}') {
                    return None;
                }
            }
        }
        if let Some(after) = self.rest.strip_prefix('[') {
            self.rest = after;
            let mut values = Vec::new();
            loop {
                self.rest = self.rest.trim_start();
                if let Some(after) = self.rest.strip_prefix(']') {
                    self.rest = after;
                    return Some(serde_json::Value::Array(values));
                }
                values.push(self.value()?);
                self.rest = self.rest.trim_start();
                if let Some(after) = self.rest.strip_prefix(',') {
                    self.rest = after;
                } else if !self.rest.starts_with(']') {
                    return None;
                }
            }
        }
        let end = self
            .rest
            .find([',', '}', ']', ' ', '\n'])
            .unwrap_or(self.rest.len());
        let value = serde_json::from_str(&self.rest[..end]).ok()?;
        self.rest = &self.rest[end..];
        Some(value)
    }
}

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
                    let name = target.strip_prefix("functions.").unwrap_or(&target);
                    // The first JSON value is the arguments; anything the model
                    // wrote after it is not.
                    let parsed = serde_json::Deserializer::from_str(body.trim())
                        .into_iter::<serde_json::Value>()
                        .next()
                        .and_then(Result::ok);
                    // gpt-oss can end a complete JSON call with EOS instead
                    // of a Harmony marker. Only an incomplete body is unsafe.
                    if end.is_none() && parsed.is_none() {
                        canonical.diagnostics.push(Diagnostic {
                            kind: "harmony_unterminated_tool_call",
                            detail: "a tool-call message ended with incomplete JSON".into(),
                        });
                    }
                    let arguments =
                        parsed.unwrap_or_else(|| serde_json::Value::String(body.trim().to_owned()));
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

/// Reply syntax is independent of parameter count, expert routing and backend.
/// Multiple model families can share it without sharing rendering or sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyProtocol {
    Native,
    Qwen,
    Glm,
    Seed,
    Harmony,
    Gemma4,
    Granite,
    Liquid,
    Mistral,
}

/// Resolve from architecture metadata first; use the repository name only when
/// metadata is absent. An unknown explicit architecture must stay unknown.
/// This is reply normalization, not a claim that the artifact is certified.
pub fn reply_protocol_for(family: Option<&str>, model_ref: &str) -> ReplyProtocol {
    let evidence = family.unwrap_or(model_ref).to_ascii_lowercase();
    // Ornith 1.5 9B and 35B-A3B publish Qwen XML templates. Do not map all
    // Ornith releases: the brand also includes models derived from Gemma.
    let ornith_qwen = family.is_none()
        && ["ornith-1.5-9b", "ornith-1.5-35b-a3b"].iter().any(|name| {
            evidence.split('/').next_back().is_some_and(|artifact| {
                artifact == *name
                    || artifact
                        .strip_prefix(name)
                        .is_some_and(|suffix| suffix.starts_with('-'))
            })
        });
    if evidence.contains("qwen") || evidence.contains("nemotron") || ornith_qwen {
        return ReplyProtocol::Qwen;
    }
    if evidence.contains("glm") {
        return ReplyProtocol::Glm;
    }
    if evidence.contains("seed") {
        return ReplyProtocol::Seed;
    }
    if evidence.contains("gpt_oss") || evidence.contains("gpt-oss") {
        return ReplyProtocol::Harmony;
    }
    if evidence.contains("gemma4") || evidence.contains("gemma-4") {
        return ReplyProtocol::Gemma4;
    }
    if evidence.contains("granite") {
        return ReplyProtocol::Granite;
    }
    if evidence.contains("lfm") || evidence.contains("liquid") {
        return ReplyProtocol::Liquid;
    }
    if evidence.contains("mistral")
        || evidence.contains("devstral")
        || evidence.contains("magistral")
        || evidence.contains("ministral")
    {
        return ReplyProtocol::Mistral;
    }
    ReplyProtocol::Native
}

pub fn adapter_for(family: Option<&str>, model_ref: &str) -> Box<dyn ModelBehaviorAdapter> {
    match reply_protocol_for(family, model_ref) {
        ReplyProtocol::Native => Box::new(GenericAdapter),
        ReplyProtocol::Qwen => Box::new(QwenFamilyAdapter),
        ReplyProtocol::Glm => Box::new(GlmFamilyAdapter),
        ReplyProtocol::Seed => Box::new(SeedFamilyAdapter),
        ReplyProtocol::Harmony => Box::new(HarmonyAdapter),
        ReplyProtocol::Gemma4 => Box::new(Gemma4Adapter),
        ReplyProtocol::Granite => Box::new(GraniteFamilyAdapter),
        ReplyProtocol::Liquid => Box::new(LiquidFamilyAdapter),
        ReplyProtocol::Mistral => Box::new(MistralFamilyAdapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_ornith_releases_share_qwen_calls_across_dense_and_moe() {
        for model in [
            "ornith-ai/Ornith-1.5-9B",
            "mlx-community/Ornith-1.5-35B-A3B-4bit",
        ] {
            let adapter = adapter_for(None, model);
            assert_eq!(adapter.id(), "qwen");
            let canonical = adapter.normalize(&reply("<tool_call><function=read_file><parameter=path>src/main.rs</parameter></function></tool_call>"));
            assert_eq!(canonical.tool_calls.len(), 1);
            assert_eq!(canonical.tool_calls[0].arguments["path"], "src/main.rs");
        }
    }

    #[test]
    fn metadata_wins_over_brand_and_unverified_releases_stay_native() {
        assert_eq!(
            reply_protocol_for(Some("gemma4"), "ornith-ai/Ornith-1.5-9B"),
            ReplyProtocol::Gemma4
        );
        assert_eq!(
            reply_protocol_for(Some("unknown"), "Qwen/Qwen3.5-9B"),
            ReplyProtocol::Native
        );
        for model in [
            "ornith-ai/Ornith-1.0-26B-A4B",
            "ornith-ai/Ornith-1.5-9Billion",
            "ornith-ai/Ornith-2.0-9B",
            "gpt-4.1",
        ] {
            assert_eq!(reply_protocol_for(None, model), ReplyProtocol::Native);
        }
        assert_eq!(
            reply_protocol_for(Some("qwen3_5_moe"), "ornith-ai/custom"),
            ReplyProtocol::Qwen
        );
    }

    /// Qwen2.5-Coder's edit, as it wrote it in the capability probe.
    #[test]
    fn a_list_written_the_python_way_in_an_xml_parameter_is_a_list() {
        let found = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=run_command>\n<parameter=args>\n['python3', '-m', 'unittest', \"it's\"]\n</parameter>\n<parameter=executable>\nbash\n</parameter>\n</function>\n</tool_call>",
        ));
        assert_eq!(
            found.tool_calls[0].arguments["args"],
            serde_json::json!(["python3", "-m", "unittest", "it's"])
        );
        assert_eq!(found.tool_calls[0].arguments["executable"], "bash");
        // Prose that merely starts with a bracket stays a string.
        let prose = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=write_file>\n<parameter=path>\nn.md\n</parameter>\n<parameter=content>\n[a note, not a list] and more\n</parameter>\n</function>\n</tool_call>",
        ));
        assert_eq!(
            prose.tool_calls[0].arguments["content"],
            "[a note, not a list] and more"
        );
    }

    #[test]
    fn granite_reads_the_tool_call_block_its_template_writes() {
        let adapter = adapter_for(None, "mlx-community/granite-4.1-8b-4bit");
        let found = adapter.normalize(&reply(
            "<tool_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"src/parser.rs\"}}\n</tool_call>",
        ));
        assert_eq!(found.tool_calls.len(), 1);
        assert_eq!(found.tool_calls[0].arguments["path"], "src/parser.rs");
        // The bare array it was first measured writing still reads.
        let array = adapter.normalize(&reply(
            "[{\"name\": \"read_file\", \"arguments\": {\"path\": \"a\"}}]",
        ));
        assert_eq!(array.tool_calls.len(), 1);
    }

    #[test]
    fn liquid_calls_are_read_in_both_of_its_forms() {
        let adapter = adapter_for(None, "mlx-community/LFM2.5-8B-A1B-MLX-4bit");
        assert_eq!(adapter.id(), "liquid");
        let python = adapter.normalize(&reply(
            "<think>I should read it.</think><|tool_call_start|>[read_file(path=\"src/parser.rs\", max_lines=40, deep=True)]<|tool_call_end|>",
        ));
        assert_eq!(python.tool_calls.len(), 1);
        assert_eq!(python.tool_calls[0].arguments["path"], "src/parser.rs");
        assert_eq!(python.tool_calls[0].arguments["max_lines"], 40);
        assert_eq!(python.tool_calls[0].arguments["deep"], true);
        assert_eq!(python.thinking, "I should read it.");
        // A file written through it: escapes, quotes of the other kind, a comma.
        let write = adapter.normalize(&reply(
            "<|tool_call_start|>[write_file(path='a.txt', content=\"line one\\nsay \\\"hi\\\", ok\\n\")]<|tool_call_end|>",
        ));
        assert_eq!(
            write.tool_calls[0].arguments["content"],
            "line one\nsay \"hi\", ok\n"
        );
        let two = adapter.normalize(&reply(
            "Both.<|tool_call_start|>[read_file(path=\"a\"), read_file(path=\"b\")]<|tool_call_end|>",
        ));
        assert_eq!(two.tool_calls.len(), 2);
        assert_eq!(two.narrative, "Both.");
        let json = adapter.normalize(&reply(
            "I will read it.\n\n<function_call>\n{\"name\": \"read_file\", \"arguments\": {\"path\": \"src/parser.rs\"}}\n</function_call>",
        ));
        assert_eq!(json.tool_calls.len(), 1);
        assert_eq!(json.narrative, "I will read it.");
        let cut = adapter.normalize(&reply(
            "<|tool_call_start|>[write_file(path=\"a\", content=\"x",
        ));
        assert!(cut.tool_calls.is_empty());
        assert!(
            cut.diagnostics
                .iter()
                .any(|d| d.kind == "liquid_unterminated_tool_call")
        );
        let bad = adapter.normalize(&reply("<|tool_call_start|>[not a call]<|tool_call_end|>"));
        assert!(bad.tool_calls.is_empty());
        assert!(bad.narrative.contains("not a call"));
    }

    #[test]
    fn mistral_calls_are_read_in_both_of_its_forms() {
        let adapter = adapter_for(
            None,
            "mlx-community/Devstral-Small-2-24B-Instruct-2512-4bit",
        );
        assert_eq!(adapter.id(), "mistral");
        let one = adapter.normalize(&reply(
            "[TOOL_CALLS]read_file[ARGS]{\"path\": \"src/parser.rs\"}",
        ));
        assert_eq!(one.tool_calls.len(), 1);
        assert_eq!(one.tool_calls[0].name, "read_file");
        assert_eq!(one.tool_calls[0].arguments["path"], "src/parser.rs");
        assert_eq!(one.narrative, "");
        let two = adapter.normalize(&reply(
            "Reading both.[TOOL_CALLS]read_file[ARGS]{\"path\": \"a.rs\"}[TOOL_CALLS]read_file[ARGS]{\"path\": \"b {}.rs\"}",
        ));
        assert_eq!(two.tool_calls.len(), 2);
        assert_eq!(two.tool_calls[1].arguments["path"], "b {}.rs");
        assert_eq!(two.narrative, "Reading both.");
        let old = adapter.normalize(&reply(
            "[TOOL_CALLS] [{\"name\": \"read_file\", \"arguments\": {\"path\": \"a.rs\"}}]",
        ));
        assert_eq!(old.tool_calls.len(), 1);
        // Cut off in the arguments: nothing is run, and it says so.
        let cut = adapter.normalize(&reply(
            "[TOOL_CALLS]write_file[ARGS]{\"path\": \"a.rs\", \"content\": \"fn main",
        ));
        assert!(cut.tool_calls.is_empty());
        assert!(
            cut.diagnostics
                .iter()
                .any(|d| d.kind == "mistral_unterminated_tool_call")
        );
        // A backend that already decoded the call is left alone.
        let mut decoded = reply("[TOOL_CALLS]read_file[ARGS]{\"path\": \"x\"}");
        decoded.tool_calls.push(ToolCall {
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "x"}),
            id: None,
        });
        assert_eq!(adapter.normalize(&decoded).tool_calls.len(), 1);
    }

    #[test]
    fn a_reply_that_is_one_fenced_call_is_read_as_one() {
        let text = "```json\n{\n  \"name\": \"apply_replace\",\n  \"arguments\": {\n    \"expected_hash\": \"236e\",\n    \"path\": \"probe.rs\",\n    \"replacement\": \"pub fn value() -> i32 {\\n    2\\n}\"\n  }\n}\n```";
        let canonical = QwenFamilyAdapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].name, "apply_replace");
        assert_eq!(canonical.tool_calls[0].arguments["path"], "probe.rs");
        // After a sentence about it, as it writes calls in a run.
        let after =
            QwenFamilyAdapter.normalize(&reply(&format!("Let's read the file first.\n\n{text}")));
        assert!(after.tool_calls.is_empty());
        assert!(after.narrative.contains("Let's read the file first."));
        // Not after a fence that is not a call.
        let two = format!("```python\nprint(1)\n```\n{text}");
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(&two))
                .tool_calls
                .is_empty()
        );
        // Two reads, one block each, as it asked for two files.
        let read = |path: &str| {
            format!(
                "```json\n{{\"name\": \"read_file\", \"arguments\": {{\"path\": \"{path}\"}}}}\n```"
            )
        };
        let both = QwenFamilyAdapter.normalize(&reply(&format!(
            "Reading both.\n\n{}\n\n{}",
            read("money.py"),
            read("invoice.py")
        )));
        assert!(both.tool_calls.is_empty());
        // Python's \' inside a string, as it wrote one replace_text.
        let quoted = "```json\n{\"name\": \"replace_text\", \"arguments\": {\"path\": \"bag.py\", \"replace\": \"raise TypeError(\\'Bag\\')\"}}\n```";
        let read_back = QwenFamilyAdapter.normalize(&reply(quoted));
        assert_eq!(
            read_back.tool_calls[0].arguments["replace"],
            "raise TypeError('Bag')"
        );
        // Prose between and after the blocks, as it writes them building a
        // project: both calls read, the prose kept in order.
        let interleaved = QwenFamilyAdapter.normalize(&reply(&format!(
            "### Creating `money.py`\n\n{}\n\nNow the second one.\n\n{}\n\nThen we can run the checks.",
            read("money.py"),
            read("invoice.py")
        )));
        assert!(interleaved.tool_calls.is_empty());
        assert!(
            interleaved
                .narrative
                .contains("Then we can run the checks.")
        );
        // A code example anywhere in the reply and nothing is read.
        let example = format!(
            "{}\n\nFor instance:\n```js\nrequire('x')\n```",
            read("money.py")
        );
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(&example))
                .tool_calls
                .is_empty()
        );
        // A write whose block ends without closing the content or the call,
        // then a sentence and a second call, as Qwen2.5-Coder wrote one.
        let unclosed = QwenFamilyAdapter.normalize(&reply(&format!(
            "Creating it.\n\n```json\n{{\n  \"name\": \"write_file\",\n  \"arguments\": {{\n    \"path\": \"server.js\",\n    \"content\": \"listen();\\n\n```\n\nNow check it.\n\n{}",
            read("server.js")
        )));
        assert!(unclosed.tool_calls.is_empty());
        // Nor is a fenced object that is not a call.
        let data = "```json\n{\"file\": \"src/parser.rs\", \"line\": 7}\n```";
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(data))
                .tool_calls
                .is_empty()
        );
    }

    /// Qwen2.5-Coder's call, as it wrote it in Quick Calibration.
    #[test]
    fn a_call_written_inside_tools_is_read_as_one() {
        let text = "<tools>\n{\n  \"name\": \"read_file\",\n  \"arguments\": {\n    \"path\": \"src/parser.rs\"\n  }\n}\n</tools>";
        let canonical = QwenFamilyAdapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1);
        assert_eq!(canonical.tool_calls[0].name, "read_file");
        assert_eq!(canonical.tool_calls[0].arguments["path"], "src/parser.rs");
        assert!(canonical.narrative.is_empty(), "{}", canonical.narrative);
        assert!(
            canonical
                .diagnostics
                .iter()
                .any(|d| d.kind == "qwen_tools_tag_tool_call")
        );
        // A <tools> block that is not a call stays text; nothing is invented.
        let prose = QwenFamilyAdapter.normalize(&reply("Use <tools>the read tool</tools> next."));
        assert!(prose.tool_calls.is_empty());
        assert!(prose.narrative.contains("<tools>the read tool</tools>"));
    }
    /// The two readings suite A1 records as unreadable (2026-09-18 traces):
    /// a value closed by half a tag, and an opening tag that never closed.
    #[test]
    fn half_a_tag_is_not_a_call_to_guess_at() {
        for body in [
            "<tool_call>\n<function=read_file>\n<parameter=first_line>\n3750\n</\n</function>\n</tool_call>",
            "<tool_call>\n<function=complete>\n<parameter=rationale\n</parameter>\n</function>\n</tool_call>",
        ] {
            let canonical = QwenFamilyAdapter.normalize(&reply(body));
            assert!(canonical.tool_calls.is_empty(), "{body}");
            assert_eq!(canonical.diagnostics[0].kind, "qwen_undecodable_tool_call");
        }
        // A value that ends in a whole tag is a value.
        let canonical = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=write_file>\n<parameter=path>\na.html\n</parameter>\n<parameter=content>\n<p>hi</p>\n</function>\n</tool_call>",
        ));
        assert_eq!(canonical.tool_calls[0].arguments["content"], "<p>hi</p>");
    }

    #[test]
    fn a_parameter_closed_by_its_own_name_or_not_at_all_still_reads() {
        let adapter = QwenFamilyAdapter;
        let text = "Creating it.\n<tool_call>\n<function=write_file>\n<parameter=path>\nsrc/Api/Api.csproj\n</parameter>\n<parameter=content>\n<Project Sdk=\"Microsoft.NET.Sdk.Web\">\n  <PropertyGroup>\n    <Nullable>enable</Nullable>\n  </PropertyGroup>\n</Project>\n\n</content>\n</function>\n</tool_call>";
        let canonical = adapter.normalize(&reply(text));
        assert_eq!(canonical.tool_calls.len(), 1, "{:?}", canonical.diagnostics);
        let arguments = &canonical.tool_calls[0].arguments;
        assert_eq!(arguments["path"], "src/Api/Api.csproj");
        assert!(
            arguments["content"]
                .as_str()
                .unwrap()
                .ends_with("</Project>\n"),
            "{arguments}"
        );
        assert!(
            !arguments["content"]
                .as_str()
                .unwrap()
                .contains("</content>")
        );

        // Qwen3-Coder from the MLX engine: no opening tag, a closing one.
        let bare = QwenFamilyAdapter.normalize(&reply(
            "<function=read_file>\n<parameter=path>\nsrc/parser.rs\n</parameter>\n</function>\n</tool_call>",
        ));
        assert_eq!(bare.tool_calls.len(), 1);
        assert_eq!(bare.tool_calls[0].name, "read_file");
        assert_eq!(bare.tool_calls[0].arguments["path"], "src/parser.rs");
        assert!(
            bare.diagnostics
                .iter()
                .any(|d| d.kind == "qwen_bare_function_call")
        );
        // Neither tag, prose before it, and two calls.
        let two = QwenFamilyAdapter.normalize(&reply(
            "Reading both.\n<function=read_file>\n<parameter=path>\na.rs\n</parameter>\n</function>\n<function=read_file>\n<parameter=path>\nb.rs\n</parameter>\n</function>",
        ));
        assert_eq!(two.tool_calls.len(), 2, "{:?}", two.diagnostics);
        assert_eq!(two.narrative, "Reading both.");
        // The call is whole but its closing tag never came: read, not refused.
        let open_end = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=read_file>\n<parameter=path>\na.rs\n</parameter>\n</function>\n",
        ));
        assert_eq!(open_end.tool_calls.len(), 1, "{:?}", open_end.diagnostics);
        // A call that is not whole still is not.
        let half = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=read_file>\n<parameter=path>\na.rs",
        ));
        assert!(half.tool_calls.is_empty());
        // A well-formed call is read once, and says nothing about a missing tag.
        let whole = QwenFamilyAdapter.normalize(&reply(
            "<tool_call>\n<function=read_file>\n<parameter=path>\na.rs\n</parameter>\n</function>\n</tool_call>",
        ));
        assert_eq!(whole.tool_calls.len(), 1);
        assert!(
            !whole
                .diagnostics
                .iter()
                .any(|d| d.kind == "qwen_bare_function_call")
        );
        // Cut off inside a bare call: unfinished, not guessed at.
        let cut = QwenFamilyAdapter.normalize(&reply("<function=write_file>\n<parameter=path>\na.rs\n</parameter>\n<parameter=content>\nfn main() {"));
        assert!(cut.tool_calls.is_empty());
        assert!(
            cut.diagnostics
                .iter()
                .any(|d| d.kind == "qwen_unterminated_tool_call")
        );
        let unclosed = "<tool_call>\n<function=read_file>\n<parameter=path>\nsrc/lib.rs\n</function>\n</tool_call>";
        let canonical = adapter.normalize(&reply(unclosed));
        assert_eq!(canonical.tool_calls[0].arguments["path"], "src/lib.rs");

        // A value that itself contains the closing tag of the format keeps it.
        let quoted = "<tool_call>\n<function=write_file>\n<parameter=path>\ndocs/format.md\n</parameter>\n<parameter=content>\nEnd a value with </parameter> on its own line.\n</parameter>\n</function>\n</tool_call>";
        let canonical = adapter.normalize(&reply(quoted));
        assert_eq!(
            canonical.tool_calls[0].arguments["content"],
            "End a value with </parameter> on its own line."
        );
    }

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
        assert!(
            canonical
                .diagnostics
                .iter()
                .all(|d| d.kind != "harmony_unterminated_tool_call")
        );
    }

    #[test]
    fn harmony_rejects_an_incomplete_json_call_without_a_closing_marker() {
        let canonical = HarmonyAdapter.normalize(&reply(
            "<|channel|>commentary to=functions.read_file<|message|>{\"path\":\"src/a",
        ));
        assert!(
            canonical
                .diagnostics
                .iter()
                .any(|d| d.kind == "harmony_unterminated_tool_call")
        );
    }

    #[test]
    fn gemma_quoted_strings_preserve_argument_boundaries() {
        let (name, args) =
            gemma_call("call:run_command{args:['test', 'a b,c'],executable:'npm'}").unwrap();
        assert_eq!(name, "run_command");
        assert_eq!(
            args,
            serde_json::json!({"args":["test","a b,c"],"executable":"npm"})
        );
        assert!(gemma_call("call:run_command{args:['test],executable:'npm'}").is_none());
    }

    #[test]
    fn gemma4_reads_its_tool_call_and_nested_arguments() {
        let adapter = adapter_for(Some("gemma4"), "lmstudio-community/gemma-4-31B-it-MLX-6bit");
        let canonical = adapter.normalize(&reply(
            "Reading. <|tool_call>call:read_file{path:<|\"|>src/parser.rs<|\"|>,lines:[1,2],options:{all:true}}<tool_call|>",
        ));
        assert_eq!(canonical.narrative, "Reading.");
        assert_eq!(canonical.tool_calls[0].name, "read_file");
        assert_eq!(canonical.tool_calls[0].arguments["path"], "src/parser.rs");
        assert_eq!(
            canonical.tool_calls[0].arguments["lines"],
            serde_json::json!([1, 2])
        );
        assert_eq!(canonical.tool_calls[0].arguments["options"]["all"], true);
        let answer = adapter.normalize(&reply(
            "<|channel>thought\nCheck the file.<channel|>READY<turn|>",
        ));
        assert_eq!(answer.thinking, "Check the file.");
        assert_eq!(answer.narrative, "READY");
        let continuation = adapter.normalize(&reply(
            "The file contains CHECK=40. I should respond with the exact content. <channel|>CHECK=40<turn|>",
        ));
        assert_eq!(
            continuation.thinking,
            "The file contains CHECK=40. I should respond with the exact content."
        );
        assert_eq!(continuation.narrative, "CHECK=40");
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
    #[test]
    fn core_audit_ambiguous_xml_never_truncates_file_content() {
        let text = "<tool_call><function=write_file>\n<parameter=path>x.txt</parameter>\n<parameter=content>Before </function> after</parameter>\n</function></tool_call>";
        let result = QwenFamilyAdapter.normalize(&reply(text));
        assert!(result.tool_calls.is_empty());
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(text.trim_start_matches("<tool_call>")))
                .tool_calls
                .is_empty()
        );
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(text.trim_end_matches("</tool_call>")))
                .tool_calls
                .is_empty()
        );
    }

    #[test]
    fn core_audit_prose_examples_are_not_executable_fences() {
        let text = "Here is an example; do not execute it:\n```json\n{\"name\":\"run_command\",\"arguments\":{\"executable\":\"sh\"}}\n```";
        assert!(
            QwenFamilyAdapter
                .normalize(&reply(text))
                .tool_calls
                .is_empty()
        );
    }

    #[test]
    fn core_audit_mistral_batch_is_all_or_nothing() {
        let text = r#"[TOOL_CALLS][{"name":"read_file","arguments":{"path":"x"}},{"arguments":{"path":"y"}}]"#;
        assert!(
            MistralFamilyAdapter
                .normalize(&reply(text))
                .tool_calls
                .is_empty()
        );
    }
}
