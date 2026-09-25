//! Context compaction, one implementation for both of its triggers.
//!
//! A conversation compacts itself when it approaches its window (automatic),
//! and the person can ask for the same thing before that (manual, the app's
//! "Compact now"). Both call [`compact`]; they differ only in how much recent
//! conversation they keep verbatim, which the caller passes as a token budget.
//! Two implementations would drift, and the one that drifted would be the one
//! nobody was watching.
//!
//! The record that replaces the folded history is mechanical, not written by a
//! model: a second generation costs a call, can fail and can invent, while this
//! can only lose detail and says what it kept. What it keeps is what a
//! conversation cannot rebuild by reading the workspace again:
//!
//! - the first request, verbatim (bounded), and every later request as a line;
//! - the actions taken and the paths they touched;
//! - what the deployment said, the last of which is where it left off;
//! - every file the conversation changed, with the content hash it left
//!   (from the checkpoint, so it is the audit's account, not a recollection);
//! - failures not followed by a success of the same action -- the errors still
//!   open;
//! - what the repository's checks said last.
//!
//! The system prompt is never folded. Neither is anything outside the
//! conversation: repository indexes, retrieval indexes and the workspace are
//! not touched, because compaction is about what the model is sent, not about
//! what exists.
//!
//! Token counts here are estimates (characters divided by four) and are
//! labelled so wherever they are reported.

use pwr_domain::{ChatMessage, MessagePurpose};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The audit event both triggers record.
pub const COMPACTED_EVENT: &str = "context.compacted";

/// How every record begins, so a reader -- and the tests -- can find one.
pub const RECORD_HEADER: &str = "Earlier in this conversation, summarised because it no longer fits. \
Re-read anything you need rather than relying on this.";

/// Messages of recent conversation always kept, whatever they cost: the last
/// thing the model was asked and the last thing it saw.
pub const KEEP_AT_LEAST: usize = 2;

/// Bounds on each part of the record, so a long conversation compacted many
/// times does not grow a record that itself needs compacting.
const FIRST_REQUEST_CHARS: usize = 800;
const MAX_REQUESTS: usize = 12;
const MAX_ACTIONS: usize = 24;
const MAX_STATEMENTS: usize = 6;
const MAX_UNRESOLVED: usize = 8;
const MAX_PATHS: usize = 40;
const LINE_CHARS: usize = 200;

/// What asked for the compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// The conversation reached its threshold between two actions.
    Automatic,
    /// The person asked for it ("Compact now").
    Manual,
}

/// What compaction carries that is not in the messages themselves.
#[derive(Debug, Clone, Default)]
pub struct Carry {
    /// Every file the conversation changed, with the content hash it left --
    /// the checkpoint's account, which survives compaction because it lives
    /// outside the messages.
    pub changed_files: BTreeMap<String, String>,
}

impl Carry {
    pub fn from_checkpoint(checkpoint: &crate::conversation::Checkpoint) -> Self {
        Carry {
            changed_files: checkpoint.changed_files.clone(),
        }
    }
}

/// What the record preserved, in structured form for the audit and the app.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preserved {
    pub first_request: Option<String>,
    pub requests: Vec<String>,
    pub actions: Vec<String>,
    pub statements: Vec<String>,
    pub paths: BTreeSet<String>,
    pub changed_files: BTreeMap<String, String>,
    pub unresolved: Vec<String>,
    pub verification: Option<String>,
    pub dropped_results: usize,
    pub dropped_passages: usize,
    pub superseded_ledgers: usize,
}

/// One compaction, as it happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compaction {
    pub trigger: Trigger,
    pub folded_messages: usize,
    /// Requests from the person inside the folded stretch.
    pub requests: usize,
    /// Estimated tokens of the whole message list before and after.
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub preserved: Preserved,
}

impl Compaction {
    /// The line a front end shows.
    pub fn note(&self) -> String {
        format!(
            "summarised {} earlier message(s) covering {} request(s): {} → {} tokens",
            self.folded_messages, self.requests, self.tokens_before, self.tokens_after
        )
    }
}

/// Estimated tokens of a message list: content plus the serialised tool calls,
/// which the backend is sent too.
pub fn estimated_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(message_tokens).sum()
}

fn message_tokens(message: &ChatMessage) -> usize {
    let calls: usize = message
        .tool_calls
        .iter()
        .map(|call| {
            crate::context::estimate_tokens(&call.name)
                + crate::context::estimate_tokens(&call.arguments.to_string())
        })
        .sum();
    let reasoning = message
        .reasoning
        .as_deref()
        .map_or(0, crate::context::estimate_tokens);
    crate::context::estimate_tokens(&message.content) + calls + reasoning
}

/// The verbatim tail a manual compaction keeps: a quarter of what the
/// conversation holds now, and never more than an automatic one would keep at
/// `room`. Folding less than that would not be worth asking for.
pub fn manual_tail_budget(messages: &[ChatMessage], room: usize) -> usize {
    (estimated_tokens(messages) / 4).min(room / 2)
}

/// Replaces the older part of `messages` with a record of it, keeping the
/// system prompt and as much recent conversation as fits in `tail_budget`
/// estimated tokens (never fewer than [`KEEP_AT_LEAST`] messages).
///
/// `None` when there is nothing older than the tail to fold, which is the
/// case where compacting again would not help and the caller has to say so.
pub fn compact(
    messages: &mut Vec<ChatMessage>,
    tail_budget: usize,
    carry: &Carry,
    trigger: Trigger,
) -> Option<Compaction> {
    let system = messages.first().cloned()?;
    // Reasoning handed back between steps is the first thing a compaction
    // gives up: it is a working note for the steps of one exchange, and the
    // record below keeps what those steps established.
    for message in messages.iter_mut() {
        message.reasoning = None;
    }
    let mut spent = 0usize;
    let mut keep_from = messages.len();
    for (index, message) in messages.iter().enumerate().skip(1).rev() {
        let cost = message_tokens(message);
        let kept = messages.len() - index;
        if spent + cost > tail_budget && kept > KEEP_AT_LEAST {
            break;
        }
        spent += cost;
        keep_from = index;
    }
    // Never open the kept window on a tool result whose call was folded away:
    // a result answering nothing is a message no backend can pair up.
    while keep_from < messages.len() && messages[keep_from].role == "tool" {
        keep_from += 1;
    }
    if keep_from >= messages.len() || keep_from <= 1 {
        return None;
    }
    // A record that is all that would be folded is already as small as it
    // gets: re-folding it alone would make nothing smaller.
    if keep_from == 2 && messages[1].purpose == Some(MessagePurpose::CompactedMemory) {
        return None;
    }
    let folded = &messages[1..keep_from];
    let (preserved, requests) = preserve(folded, carry);
    let record = render(&preserved);
    let tokens_before = estimated_tokens(messages);
    let folded_messages = folded.len();
    // In the user role: a chat template requires a user turn to render at
    // all, and a conversation whose tail held none was refused by the backend
    // with "No user query found in messages".
    let mut compacted = vec![
        system,
        ChatMessage {
            role: "user".into(),
            content: record,
            purpose: Some(MessagePurpose::CompactedMemory),
            ..Default::default()
        },
    ];
    compacted.extend_from_slice(&messages[keep_from..]);
    *messages = compacted;
    Some(Compaction {
        trigger,
        folded_messages,
        requests,
        tokens_before,
        tokens_after: estimated_tokens(messages),
        preserved,
    })
}

/// Records a compaction in the audit as `context.compacted`, in the shape the
/// scripted loop's event already has, with the trigger and model added.
pub fn record(
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
    compaction: &Compaction,
    model: &str,
    window: u32,
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            COMPACTED_EVENT,
            serde_json::json!({
                "trigger": compaction.trigger,
                "session_id": conversation_id.to_string(),
                "model": model,
                "window": window,
                "estimated_tokens_before": compaction.tokens_before,
                "estimated_tokens_after": compaction.tokens_after,
                "estimate_basis": "characters divided by 4; not a provider count",
                "folded_messages": compaction.folded_messages,
                "requests": compaction.requests,
                "preserved": {
                    "changed_files": compaction.preserved.changed_files.len(),
                    "paths": compaction.preserved.paths.len(),
                    "unresolved": compaction.preserved.unresolved.len(),
                    "verification": compaction.preserved.verification.is_some(),
                },
            }),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The last compaction a conversation recorded -- trigger, time, and the
/// estimated tokens before and after -- for a front end's context panel.
pub fn last_recorded(
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
) -> Result<Option<serde_json::Value>, String> {
    let events = store
        .events_for_run(conversation_id)
        .map_err(|error| error.to_string())?;
    Ok(events
        .iter()
        .rev()
        .find(|event| event.event_type == COMPACTED_EVENT)
        .map(|event| {
            serde_json::json!({
                "trigger": event.payload.get("trigger").cloned().unwrap_or(serde_json::json!("automatic")),
                "at": event.at,
                "tokensBefore": event.payload.get("estimated_tokens_before"),
                "tokensAfter": event.payload.get("estimated_tokens_after"),
            })
        }))
}

/// Estimated tokens by what they are, for the context panel. Every figure is
/// an estimate; the front end says so.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
    /// The system prompt: PWR's instructions and harness rules.
    pub system: usize,
    /// What the person asked and the model answered, calls included.
    pub conversation: usize,
    /// Files read, searches and retrieved passages.
    pub repository: usize,
    /// Other action results: commands, edits, services.
    pub tool_results: usize,
    /// Ledgers of what the conversation changed.
    pub task_state: usize,
    /// Records left by compaction.
    pub compacted_memory: usize,
    /// The person's profile, their memories and the project's instructions,
    /// carried in the system message (`crate::personal`).
    pub personal: usize,
}

impl Composition {
    pub fn total(&self) -> usize {
        self.system
            + self.conversation
            + self.repository
            + self.tool_results
            + self.task_state
            + self.compacted_memory
            + self.personal
    }
}

/// Capabilities whose results are repository content rather than effects.
const REPOSITORY_READS: [&str; 5] = [
    "read_file",
    "search",
    "list_tree",
    "find_definition",
    "search_dependencies",
];

pub fn composition(messages: &[ChatMessage]) -> Composition {
    let mut parts = Composition::default();
    let mut calls = CallQueue::default();
    for message in messages {
        let tokens = message_tokens(message);
        match (message.role.as_str(), message.purpose) {
            ("system", _) => {
                let (_, person) = crate::personal::split_system(&message.content);
                let person = crate::context::estimate_tokens(person);
                parts.personal += person;
                parts.system += tokens.saturating_sub(person);
            }
            (_, Some(MessagePurpose::CompactedMemory)) => parts.compacted_memory += tokens,
            (_, Some(MessagePurpose::SessionLedger)) => parts.task_state += tokens,
            (_, Some(MessagePurpose::RepositoryExcerpts)) => parts.repository += tokens,
            ("assistant", _) => {
                calls.expect(message);
                parts.conversation += tokens;
            }
            ("tool", _) => match calls.answer(message) {
                Some(call) if REPOSITORY_READS.contains(&call.name.as_str()) => {
                    parts.repository += tokens;
                }
                _ => parts.tool_results += tokens,
            },
            _ => parts.conversation += tokens,
        }
    }
    parts
}

/// Pairs tool results with the calls they answer: by id where the result
/// names one, otherwise in order.
#[derive(Default)]
struct CallQueue {
    pending: Vec<pwr_domain::ToolCall>,
}

impl CallQueue {
    fn expect(&mut self, message: &ChatMessage) {
        self.pending = message.tool_calls.clone();
    }

    fn answer(&mut self, message: &ChatMessage) -> Option<pwr_domain::ToolCall> {
        if self.pending.is_empty() {
            return None;
        }
        let index = message
            .tool_call_id
            .as_ref()
            .and_then(|id| {
                self.pending
                    .iter()
                    .position(|call| call.id.as_ref() == Some(id))
            })
            .unwrap_or(0);
        Some(self.pending.remove(index))
    }
}

fn preserve(folded: &[ChatMessage], carry: &Carry) -> (Preserved, usize) {
    let mut kept = Preserved {
        changed_files: carry.changed_files.clone(),
        ..Preserved::default()
    };
    // Failures by action, cleared by a later success of the same action.
    let mut open: BTreeMap<String, String> = BTreeMap::new();
    let mut calls = CallQueue::default();
    let mut requests = 0usize;
    for message in folded {
        match (message.role.as_str(), message.purpose) {
            (_, Some(MessagePurpose::CompactedMemory)) => {
                merge_earlier(&mut kept, &mut open, &message.content);
            }
            // Evidence, already used by the turn that retrieved it, and
            // rebuildable with `search` -- not a request.
            ("user", Some(MessagePurpose::RepositoryExcerpts)) => kept.dropped_passages += 1,
            // Composed fresh every turn from the files as they are, so an older
            // copy is superseded, and its hashes would now be stale.
            ("user", Some(MessagePurpose::SessionLedger)) => kept.superseded_ledgers += 1,
            ("user", _) => {
                requests += 1;
                if kept.first_request.is_none() {
                    kept.first_request = Some(bounded(message.content.trim(), FIRST_REQUEST_CHARS));
                }
                kept.requests.push(one_line(&message.content));
            }
            ("assistant", _) => {
                calls.expect(message);
                for call in &message.tool_calls {
                    let target = call_target(call);
                    if let Some(path) = call_path(call) {
                        kept.paths.insert(path);
                    }
                    kept.actions.push(match &target {
                        Some(target) => format!("{} {target}", call.name),
                        None => call.name.clone(),
                    });
                }
                if !message.content.trim().is_empty() {
                    kept.statements.push(one_line(&message.content));
                }
            }
            ("tool", _) => {
                if let Some(verdict) = message
                    .content
                    .strip_prefix("The workspace checks were run after your edits: ")
                {
                    kept.verification = Some(one_line(verdict));
                    continue;
                }
                kept.dropped_results += 1;
                let Some(call) = calls.answer(message) else {
                    continue;
                };
                let key = match call_target(&call) {
                    Some(target) => format!("{} {target}", call.name),
                    None => call.name.clone(),
                };
                match failure(&message.content) {
                    Some(problem) => {
                        open.insert(key, problem);
                    }
                    None => {
                        open.remove(&key);
                    }
                }
            }
            _ => {}
        }
    }
    kept.unresolved = open
        .into_iter()
        .map(|(action, problem)| format!("{action}: {problem}"))
        .collect();
    keep_last(&mut kept.requests, MAX_REQUESTS);
    keep_last(&mut kept.actions, MAX_ACTIONS);
    keep_last(&mut kept.statements, MAX_STATEMENTS);
    keep_last(&mut kept.unresolved, MAX_UNRESOLVED);
    while kept.paths.len() > MAX_PATHS {
        kept.paths.pop_first();
    }
    (kept, requests)
}

/// Folds an earlier record into this one, by the prefixes [`render`] writes.
/// Earlier entries come first, so the bounds keep the most recent.
fn merge_earlier(kept: &mut Preserved, open: &mut BTreeMap<String, String>, record: &str) {
    for line in record.lines() {
        if let Some(first) = line.strip_prefix("The first request: ") {
            kept.first_request.get_or_insert_with(|| first.to_owned());
        } else if let Some(asked) = line.strip_prefix("- you were asked: ") {
            kept.requests.push(asked.to_owned());
        } else if let Some(called) = line.strip_prefix("- you called: ") {
            kept.actions.push(called.to_owned());
        } else if let Some(said) = line.strip_prefix("- you said: ") {
            kept.statements.push(said.to_owned());
        } else if let Some(paths) = line.strip_prefix("- paths you worked with: ") {
            kept.paths.extend(
                paths
                    .split(", ")
                    .map(str::to_owned)
                    .filter(|p| !p.is_empty()),
            );
        } else if let Some(unresolved) = line.strip_prefix("- still unresolved: ") {
            if let Some((action, problem)) = unresolved.split_once(": ") {
                open.insert(action.to_owned(), problem.to_owned());
            }
        } else if let Some(verdict) = line.strip_prefix("- the checks last said: ") {
            kept.verification.get_or_insert_with(|| verdict.to_owned());
        }
    }
}

fn render(kept: &Preserved) -> String {
    let mut record = format!("{RECORD_HEADER}\n");
    if let Some(first) = &kept.first_request {
        // One line, so a later compaction can find it again by its prefix.
        record.push_str(&format!(
            "The first request: {}\n",
            first.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    for asked in &kept.requests {
        record.push_str(&format!("- you were asked: {asked}\n"));
    }
    for called in &kept.actions {
        record.push_str(&format!("- you called: {called}\n"));
    }
    for said in &kept.statements {
        record.push_str(&format!("- you said: {said}\n"));
    }
    if !kept.changed_files.is_empty() {
        let files: Vec<String> = kept
            .changed_files
            .iter()
            .map(|(path, hash)| format!("{path} ({})", &hash[..hash.len().min(12)]))
            .collect();
        record.push_str(&format!(
            "- files this conversation changed, with the hash they have now: {}\n",
            files.join(", ")
        ));
    }
    if !kept.paths.is_empty() {
        record.push_str(&format!(
            "- paths you worked with: {}\n",
            kept.paths.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    for unresolved in &kept.unresolved {
        record.push_str(&format!("- still unresolved: {unresolved}\n"));
    }
    if let Some(verdict) = &kept.verification {
        record.push_str(&format!("- the checks last said: {verdict}\n"));
    }
    record.push_str(&format!(
        "- {} tool result(s) from that stretch were dropped.\n",
        kept.dropped_results
    ));
    if kept.dropped_passages > 0 {
        record.push_str(&format!(
            "- {} block(s) of retrieved repository passages were dropped. Search and read again \
             if you need them.\n",
            kept.dropped_passages
        ));
    }
    if kept.superseded_ledgers > 0 {
        record.push_str(&format!(
            "- {} earlier account(s) of what this conversation had changed were dropped. A \
             current one, with the hashes the files have now, is composed every turn.\n",
            kept.superseded_ledgers
        ));
    }
    record
}

/// What went wrong in a tool result, if anything did: a refusal, a tool
/// failure, or a command that exited non-zero.
fn failure(content: &str) -> Option<String> {
    let envelope = crate::tool_result_json(content)?;
    let result = envelope.get("result").unwrap_or(&envelope);
    if let Some(denied) = result.get("denied") {
        return Some(format!("refused: {}", one_line(&value_text(denied))));
    }
    if let Some(failure) = result.get("tool_failure") {
        return Some(one_line(&value_text(failure)));
    }
    match result.get("exit_code").and_then(serde_json::Value::as_i64) {
        Some(0) | None => None,
        Some(code) => {
            // Long output travels as a `<stderr>` block after the envelope,
            // with a placeholder in the field.
            let stderr = verbatim_block(content, "stderr")
                .or_else(|| result.get("stderr").and_then(serde_json::Value::as_str))
                .and_then(|text| text.lines().rev().find(|line| !line.trim().is_empty()))
                .map(|last| format!(" -- {}", one_line(last)))
                .unwrap_or_default();
            Some(format!("exit {code}{stderr}"))
        }
    }
}

fn verbatim_block<'a>(content: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("\n<{tag}>\n");
    let start = content.find(&open)? + open.len();
    let end = content[start..].find(&format!("\n</{tag}>"))?;
    Some(&content[start..start + end])
}

fn value_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn call_path(call: &pwr_domain::ToolCall) -> Option<String> {
    ["path", "from", "to"]
        .iter()
        .find_map(|key| call.arguments.get(*key).and_then(serde_json::Value::as_str))
        .filter(|path| !path.trim().is_empty())
        .map(str::to_owned)
}

fn call_target(call: &pwr_domain::ToolCall) -> Option<String> {
    if let Some(path) = call_path(call) {
        return Some(path);
    }
    if let Some(executable) = call
        .arguments
        .get("executable")
        .and_then(serde_json::Value::as_str)
    {
        let args = call
            .arguments
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        return Some(one_line(format!("{executable} {args}").trim()));
    }
    call.arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .map(|query| format!("{:?}", one_line(query)))
}

fn keep_last(items: &mut Vec<String>, limit: usize) {
    if items.len() > limit {
        items.drain(..items.len() - limit);
    }
}

fn bounded(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars().take(limit - 1).collect::<String>() + "…"
}

pub(crate) fn one_line(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    bounded(&flat, LINE_CHARS)
}
