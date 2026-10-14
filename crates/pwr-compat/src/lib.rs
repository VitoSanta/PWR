//! Gemma 4's reply protocol.
//!
//! The agent loop asks for a canonical thing -- an action -- and Gemma 4
//! answers in its own protocol: calls as `<|tool_call>call:NAME{...}<tool_call|>`
//! inside the answer text, reasoning in a `<|channel>thought ... <channel|>`
//! block. This crate reads that protocol, so that the loop, the tool runtime
//! and the verifier never learn it. PWR runs the Gemma 4 family only
//! (2026-10-14); a backend that parses calls itself (llama.cpp's server) and
//! the scripted models of the tests get the native adapter, which changes
//! nothing.
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
/// A deployment that is not recognisably Gemma 4 gets this one -- a backend
/// that already parsed the calls, a scripted test model. Guessing that an
/// unknown deployment follows Gemma's protocol is how a misread call comes to
/// look like the model's.
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

/// A reader of the Python literal subset: strings (single, double,
/// triple-quoted, with escapes), numbers, `True`/`False`/`None`, lists,
/// tuples and dicts. Gemma 4 sometimes writes an argument as an ordinary
/// quoted string in place of its `<|"|>` delimiters (`GemmaValue`).
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
        // JSON-escaped double-quoted arguments are emitted by history serializers.
        // Preserve the existing Python-like reader for other literal forms.
        if quote == b'"' && !triple {
            let start = self.at;
            let mut end = start + 1;
            let mut escaped = false;
            while let Some(&byte) = self.text.get(end) {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    if let Ok(value) = serde_json::from_slice::<String>(&self.text[start..=end]) {
                        self.at = end + 1;
                        return Some(value);
                    }
                    break;
                }
                end += 1;
            }
        }
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
}

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
            let parenthesized = gemma_call(body)
                .is_none()
                .then(|| gemma_parenthesized_call(body))
                .flatten();
            if parenthesized.is_some() {
                canonical.diagnostics.push(Diagnostic {
                    kind: "gemma_parenthesized_call",
                    detail: "a call written NAME(key=value) in place of NAME{key:value}".into(),
                });
            }
            if let Some((name, arguments)) = gemma_call(body).or(parenthesized) {
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

/// `call:NAME()` or `call:NAME(key=value, ...)`: the call written the Python
/// way, which Gemma 4 26B wrote at the start of two agent runs (2026-10-14,
/// `call:list_tree()` and `call:list_tree(path=".")`). The call is
/// unambiguous; values are read as Python literals, never evaluated.
fn gemma_parenthesized_call(body: &str) -> Option<(String, serde_json::Value)> {
    let body = body.strip_prefix("call:")?;
    let open = body.find('(')?;
    let name = body[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let mut reader = PyArgs::new(&body[open..]);
    if !reader.eat(b'(') {
        return None;
    }
    let mut arguments = serde_json::Map::new();
    while !reader.eat(b')') {
        let key = reader.ident()?;
        if !reader.eat(b'=') {
            return None;
        }
        arguments.insert(key, reader.value()?);
        if !reader.eat(b',') {
            reader.skip();
            if reader.text.get(reader.at) != Some(&b')') {
                return None;
            }
        }
    }
    reader.skip();
    (reader.at == reader.text.len()).then(|| (name.to_owned(), serde_json::Value::Object(arguments)))
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

/// The reply protocols PWR reads: Gemma 4's own, or none (calls already
/// parsed by the backend).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyProtocol {
    Native,
    Gemma4,
}

/// Resolve from architecture metadata first; use the repository name only when
/// metadata is absent. An unknown explicit architecture must stay unknown.
pub fn reply_protocol_for(family: Option<&str>, model_ref: &str) -> ReplyProtocol {
    let evidence = family.unwrap_or(model_ref).to_ascii_lowercase();
    if evidence.contains("gemma4") || evidence.contains("gemma-4") {
        ReplyProtocol::Gemma4
    } else {
        ReplyProtocol::Native
    }
}

pub fn adapter_for(family: Option<&str>, model_ref: &str) -> Box<dyn ModelBehaviorAdapter> {
    match reply_protocol_for(family, model_ref) {
        ReplyProtocol::Native => Box::new(GenericAdapter),
        ReplyProtocol::Gemma4 => Box::new(Gemma4Adapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_wins_over_the_name_and_others_stay_native() {
        assert_eq!(
            reply_protocol_for(Some("gemma4"), "ornith-ai/Ornith-1.5-9B"),
            ReplyProtocol::Gemma4
        );
        assert_eq!(
            reply_protocol_for(Some("gemma4_unified"), "x/y"),
            ReplyProtocol::Gemma4
        );
        assert_eq!(
            reply_protocol_for(None, "mlx-community/gemma-4-26b-a4b-it-4bit"),
            ReplyProtocol::Gemma4
        );
        assert_eq!(
            reply_protocol_for(Some("unknown"), "mlx-community/gemma-4-12B-it-4bit"),
            ReplyProtocol::Native
        );
        for model in ["Qwen/Qwen3.5-9B", "gpt-oss:20b", "fake"] {
            assert_eq!(reply_protocol_for(None, model), ReplyProtocol::Native);
        }
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

    /// The conformance suite of the Gemma 4 protocol, at the reader: what
    /// Google's format allows and what Gemma was seen writing.
    mod gemma_conformance {
        use super::*;

        fn read(content: &str) -> CanonicalReply {
            Gemma4Adapter.normalize(&reply(content))
        }

        fn s(text: &str) -> String {
            format!("<|\"|>{text}<|\"|>")
        }

        #[test]
        fn one_call() {
            let canonical = read(&format!("<|tool_call>call:read_file{{path:{}}}<tool_call|>", s("a.py")));
            assert_eq!(canonical.tool_calls.len(), 1);
            assert_eq!(canonical.tool_calls[0].name, "read_file");
            assert_eq!(canonical.tool_calls[0].arguments, serde_json::json!({"path": "a.py"}));
            assert!(canonical.narrative.is_empty());
        }

        #[test]
        fn several_calls_in_one_reply_in_the_order_written() {
            let canonical = read(&format!(
                "<|tool_call>call:read_file{{path:{}}}<tool_call|><|tool_call>call:search{{query:{}}}<tool_call|>",
                s("a.py"), s("to_cents")
            ));
            let names: Vec<_> = canonical.tool_calls.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, ["read_file", "search"]);
        }

        #[test]
        fn no_arguments() {
            let canonical = read("<|tool_call>call:list_tree{}<tool_call|>");
            assert_eq!(canonical.tool_calls[0].arguments, serde_json::json!({}));
        }

        #[test]
        fn strings_keep_quotes_braces_markup_lines_and_unicode() {
            let text = "if a == \"{x}\" and b < 3:\n    return 'ok' # è → ✓";
            let canonical = read(&format!(
                "<|tool_call>call:replace_text{{find:{},path:{},replace:{}}}<tool_call|>",
                s(text), s("a.py"), s("x, y")
            ));
            assert_eq!(canonical.tool_calls[0].arguments["find"], text);
            assert_eq!(canonical.tool_calls[0].arguments["replace"], "x, y");
        }

        #[test]
        fn arrays_objects_numbers_and_booleans() {
            let canonical = read(&format!(
                "<|tool_call>call:run_command{{args:[{},{}],env:{{CI:{}}},timeout:30,shell:false}}<tool_call|>",
                s("-m"), s("pytest"), s("1")
            ));
            assert_eq!(
                canonical.tool_calls[0].arguments,
                serde_json::json!({"args": ["-m", "pytest"], "env": {"CI": "1"}, "timeout": 30, "shell": false})
            );
        }

        #[test]
        fn text_and_an_empty_thought_before_the_call() {
            let canonical = read(&format!(
                "<|channel>thought\n<channel|>I will read it.<|tool_call>call:read_file{{path:{}}}<tool_call|>",
                s("a.py")
            ));
            assert_eq!(canonical.narrative, "I will read it.");
            assert_eq!(canonical.tool_calls.len(), 1);
        }

        #[test]
        fn a_cut_off_call_is_reported_and_never_guessed() {
            let canonical = read(&format!("<|tool_call>call:write_file{{path:{},content:<|\"|>def a(", s("a.py")));
            assert!(canonical.tool_calls.is_empty());
            assert!(canonical.diagnostics.iter().any(|d| d.kind == "gemma_unterminated_tool_call"));
        }

        #[test]
        fn an_unclosed_string_is_undecodable() {
            let canonical = read("<|tool_call>call:complete{rationale:<|\"|>done}<tool_call|>");
            assert!(canonical.tool_calls.is_empty());
            assert!(canonical.diagnostics.iter().any(|d| d.kind == "gemma_undecodable_tool_call"));
        }

        #[test]
        fn the_parenthesized_form_gemma_wrote_is_read_and_counted() {
            for (written, arguments) in [
                ("<|tool_call>call:list_tree()<tool_call|>", serde_json::json!({})),
                ("<|tool_call>call:list_tree(path=\".\")<tool_call|>", serde_json::json!({"path": "."})),
                ("<|tool_call>call:read_file(path='a.py', start_line=3)<tool_call|>", serde_json::json!({"path": "a.py", "start_line": 3})),
            ] {
                let canonical = read(written);
                assert_eq!(canonical.tool_calls.len(), 1, "{written}");
                assert_eq!(canonical.tool_calls[0].arguments, arguments, "{written}");
                assert!(canonical.diagnostics.iter().any(|d| d.kind == "gemma_parenthesized_call"));
            }
            // Not a call: a name alone, or code that only looks like one.
            assert!(read("<|tool_call>call:list_tree(os.getcwd())<tool_call|>").tool_calls.is_empty());
        }
    }
}
