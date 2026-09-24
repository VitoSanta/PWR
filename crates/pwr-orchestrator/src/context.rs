//! Compiling a prompt from typed sections, with the cost of each recorded.
//!
//! A prompt was built by concatenating strings at the call site: the system
//! prompt, a per-model suffix, a session ledger, a block of repository
//! excerpts and the task, glued together and handed over. Two things followed
//! from that shape.
//!
//! The repository excerpts and the task shared one user message, so nothing
//! downstream could tell them apart -- not compaction, which had to keep the
//! whole thing or lose the goal with it, and not a reader asking what a turn
//! actually cost. And the budget was a fraction: retrieval got a share of the
//! context and nobody knew what any section spent, so a prompt that was too
//! large could only be made smaller by guessing which part to cut.
//!
//! Sections are typed here, each carries its own estimated cost and hash, and
//! what was dropped or truncated to make the prompt fit is recorded rather
//! than inferred.

use crate::TaskProfile;
use pwr_domain::{ChatMessage, hash_bytes};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What a section is, which is also what may be done to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// The shared agent instructions. Required.
    System,
    /// A per-deployment addition to them. Required with the system prompt,
    /// because a deployment told to behave one way and then not told is being
    /// given two different agents on two turns.
    ModelSuffix,
    /// The task. Required, and the last thing that would ever be dropped: a
    /// run without its goal is not a cheaper run, it is a different one.
    Task,
    /// What earlier runs of this session established.
    SessionLedger,
    /// Deterministic entrypoints and reachable roots discovered from the
    /// workspace. This is architecture evidence, not lexical retrieval.
    WorkspaceTopology,
    /// A small, deterministic framework-specific work packet. It guides tool
    /// use but never counts as verification evidence.
    FrameworkGuidance,
    /// Passages ranked against the task.
    RepositoryExcerpts,
}

impl SectionKind {
    /// Whether the prompt is still the prompt without it.
    pub fn required(self) -> bool {
        matches!(
            self,
            Self::System | Self::ModelSuffix | Self::WorkspaceTopology | Self::Task
        )
    }

    /// What gets cut first. Higher drops earlier.
    ///
    /// Excerpts before the ledger: excerpts are a starting point the agent can
    /// rebuild with `search` and `read_file`, while the ledger is the only
    /// account of what earlier runs did and cannot be recovered from the
    /// workspace.
    fn eviction_order(self) -> u8 {
        match self {
            Self::RepositoryExcerpts => 0,
            Self::SessionLedger => 1,
            Self::FrameworkGuidance => 2,
            _ => u8::MAX,
        }
    }

    fn role(self) -> &'static str {
        match self {
            Self::System | Self::ModelSuffix => "system",
            _ => "user",
        }
    }
}

/// A section offered to the compiler.
#[derive(Debug, Clone)]
pub struct Section {
    pub kind: SectionKind,
    pub content: String,
}

impl Section {
    pub fn new(kind: SectionKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
        }
    }
}

/// What happened to a section, and what it cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledSection {
    pub kind: SectionKind,
    /// Estimated at four characters per token, and labelled as an estimate
    /// wherever it is read. The backend's reported count is compared against
    /// the total after the turn.
    pub estimated_tokens: usize,
    pub bytes: usize,
    pub content_hash: String,
    /// Cut to fit, with the number of characters that survived.
    pub truncated_to: Option<usize>,
    /// Left out entirely.
    pub dropped: bool,
}

/// A prompt, with the accounting that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPrompt {
    pub sections: Vec<CompiledSection>,
    pub estimated_tokens: usize,
    /// Held back for the reply. A prompt that fills the context leaves the
    /// deployment nowhere to answer, which is a failure that looks like a
    /// refusal.
    pub reserve_tokens: usize,
    pub context_tokens: u32,
    /// True when something had to be cut. Recorded so a run whose retrieval
    /// was thrown away does not look like one that was never offered any.
    pub reduced: bool,
}

/// Characters per token. An estimate, and never reported as a count.
const CHARS_PER_TOKEN: usize = 4;

/// How far the correction may go, so one strange turn cannot shrink the
/// context to nothing.
const MAX_PROMPT_OVERHEAD: usize = 8_192;

/// What a request costs beyond the messages in it, learned from the counts the
/// backend reports.
///
/// D6, 2026-09-16: the budget was enforced on `characters / 4` over the
/// messages, and the backend's own count of the whole request ran a median
/// 1.37x that. The first correction read the gap as a ratio and scaled the
/// estimate by it. That was wrong, and a campaign caught it within three
/// trials: the gap is an offset, not a factor. Measured on
/// `qwen3.6-35b-a3b`, turn by turn, the backend counted 2,859 then 3,193 then
/// 2,881 tokens more than the estimate -- the tool schemas and the chat
/// template, which are in every request and in none of the messages. Read as a
/// ratio that constant becomes 2.4x on a long history and 4.8x just after a
/// compaction, so each compaction shrank the budget further and forced the
/// next: twenty-seven compactions in three trials against two in the campaign
/// it was meant to improve.
///
/// So what is learned is the offset, and the largest one seen is kept: a
/// request cannot cost less than its own fixed parts, and underestimating
/// spends a turn on a reply that had nowhere to go.
#[derive(Debug, Clone, Copy, Default)]
pub struct PromptOverhead {
    tokens: usize,
}

impl PromptOverhead {
    /// Learns from one turn the backend counted.
    pub fn observe(&mut self, estimated: usize, reported: u64) {
        let reported = usize::try_from(reported).unwrap_or(usize::MAX);
        let seen = reported.saturating_sub(estimated);
        self.tokens = self.tokens.max(seen).min(MAX_PROMPT_OVERHEAD);
    }

    /// What the backend will count for a prompt this estimate says is `estimated`.
    pub fn predict(&self, estimated: usize) -> usize {
        estimated.saturating_add(self.tokens)
    }

    /// A budget in real tokens, expressed in the estimate's own units, so a
    /// caller that measures the history in estimates can be given a true one.
    pub fn as_estimate(&self, tokens: usize) -> usize {
        tokens.saturating_sub(self.tokens)
    }

    pub fn tokens(&self) -> usize {
        self.tokens
    }
}

/// The share of a context window a prompt must leave for the reply. A prompt
/// that crosses it is a turn the deployment cannot answer.
pub fn reply_headroom(context_tokens: u32) -> u64 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let reserve = (f64::from(context_tokens) * OUTPUT_RESERVE_SHARE) as u64;
    reserve
}

/// The share of the context held back for the reply.
///
/// A quota, and the honest note is that it is not yet a measured one: the
/// audit asked for measured quotas and this is a starting value, replaced when
/// a campaign has enough reply lengths to derive one. It is here rather than
/// scattered because a number in one place can be measured; five numbers at
/// five call sites cannot.
const OUTPUT_RESERVE_SHARE: f64 = 0.25;

/// A truncated section keeps at least this much, or is dropped instead.
///
/// A section cut to a few hundred characters is not a smaller section, it is a
/// misleading one -- half an excerpt reads like a whole file.
const MIN_USEFUL_CHARS: usize = 2_000;

/// Share of a context budget reserved for repository retrieval. This is a
/// composition decision, not a CLI preference, so every task entry point gets
/// the same retrieval pressure at a given context tier.
pub const RETRIEVAL_TOKEN_SHARE: usize = 6;
pub const RETRIEVAL_MAX_EXCERPTS: usize = 5;

/// Inputs to the common opening-context composer.
pub struct ContextComposition<'a> {
    pub root: &'a Path,
    pub index: &'a pwr_repo::RepositoryIndex,
    pub task: &'a str,
    pub context_tokens: u32,
    pub system_prompt: &'a str,
    pub task_profile: &'a TaskProfile,
    pub session_ledger: Option<&'a str>,
    /// Ranks document sections by meaning as well as by words (backlog C.22).
    /// `None` is the lexical ranking, exactly as before it existed.
    pub ranker: Option<&'a mut dyn pwr_repo::SectionRanker>,
}

/// Composes the opening prompt used for both an interactive run and an eval.
///
/// Repository retrieval is intentionally performed here so section order,
/// retrieval budget and compaction accounting cannot diverge by caller.
pub fn compose(input: ContextComposition<'_>) -> (Vec<ChatMessage>, CompiledPrompt) {
    compile(
        vec![
            Section::new(SectionKind::System, input.system_prompt),
            Section::new(SectionKind::ModelSuffix, &input.task_profile.prompt_suffix),
            Section::new(
                SectionKind::SessionLedger,
                input.session_ledger.unwrap_or_default(),
            ),
            Section::new(
                SectionKind::WorkspaceTopology,
                workspace_topology(input.root),
            ),
            Section::new(
                SectionKind::FrameworkGuidance,
                framework_guidance(input.root),
            ),
            Section::new(
                SectionKind::RepositoryExcerpts,
                retrieved_context(
                    input.ranker,
                    input.root,
                    input.index,
                    input.task,
                    input.context_tokens,
                    input.task_profile.retrieval_excerpts,
                ),
            ),
            Section::new(SectionKind::Task, input.task),
        ],
        input.context_tokens,
    )
}

/// What one conversation turn contributes to the prompt.
///
/// A run composes its whole opening because it has one task for its whole life.
/// A conversation has a request per turn and a history it must keep, so what it
/// composes is the turn: the passages ranked against what was just asked, and
/// the request itself. The same two sections [`compose`] builds for a run's
/// task, through the same retrieval, the same budget share and the same
/// eviction and accounting.
///
/// It exists because the conversation was the one caller that composed nothing.
/// It opened with a bare system string and never retrieved anything, so the
/// loop where the work happens saw less of the repository than the loop the
/// benchmarks run -- and a retrieval result measured in one would not have
/// reached the other. Having one composer only helps if every caller is in it.
pub struct TurnComposition<'a> {
    pub root: &'a Path,
    pub index: &'a pwr_repo::RepositoryIndex,
    /// What the engineer just asked for. Ranked against, and then sent.
    pub request: &'a str,
    /// The room this turn may occupy, which for a conversation is what the
    /// history has left rather than the whole window.
    pub context_tokens: u32,
    pub task_profile: &'a TaskProfile,
    /// What this conversation has already established, re-hashed from disk.
    ///
    /// Composed every turn rather than once, because its whole purpose is to
    /// be true now: a hash recorded ten turns ago and replayed as current is
    /// the refused-edit loop this project spent a campaign removing. Earlier
    /// copies in the history are superseded by construction, which is what
    /// lets compaction drop them.
    pub session_ledger: Option<&'a str>,
    /// As in [`ContextComposition::ranker`].
    pub ranker: Option<&'a mut dyn pwr_repo::SectionRanker>,
}

pub fn compose_turn(input: TurnComposition<'_>) -> (Vec<ChatMessage>, CompiledPrompt) {
    compile(
        vec![
            Section::new(
                SectionKind::SessionLedger,
                input.session_ledger.unwrap_or_default(),
            ),
            Section::new(
                SectionKind::WorkspaceTopology,
                workspace_topology(input.root),
            ),
            Section::new(
                SectionKind::FrameworkGuidance,
                framework_guidance(input.root),
            ),
            Section::new(
                SectionKind::RepositoryExcerpts,
                retrieved_context(
                    input.ranker,
                    input.root,
                    input.index,
                    input.request,
                    input.context_tokens,
                    input.task_profile.retrieval_excerpts,
                ),
            ),
            Section::new(SectionKind::Task, input.request),
        ],
        input.context_tokens,
    )
}

/// Entry-point evidence that does not depend on whether a file happens to
/// share words with the task. The first implementation is deliberately small:
/// it recognises only the on-disk Angular standalone layout, and otherwise
/// returns no guidance rather than pretending to understand an unknown stack.
pub fn workspace_topology(root: &Path) -> String {
    if !root.join("angular.json").is_file()
        && !read_workspace_file(root, "package.json").contains("@angular/core")
    {
        return String::new();
    }

    let main = read_workspace_file(root, "src/main.ts");
    let root_component = module_import_path(&main, "./app/app")
        .filter(|path| root.join(path).is_file())
        .unwrap_or_else(|| "src/app/app.ts".into());
    let component = read_workspace_file(root, &root_component);
    let root_template = quoted_metadata_path(&component, "templateUrl")
        .map(|relative| join_workspace_relative(&root_component, &relative))
        .filter(|path| root.join(path).is_file())
        .unwrap_or_else(|| "src/app/app.html".into());
    let config_path = module_import_path(&main, "./app/app.config")
        .filter(|path| root.join(path).is_file())
        .unwrap_or_else(|| "src/app/app.config.ts".into());
    let config = read_workspace_file(root, &config_path);
    let routes_path = module_import_path(&config, "./app.routes")
        .filter(|path| root.join(path).is_file())
        .unwrap_or_else(|| "src/app/app.routes.ts".into());

    format!(
        "Workspace topology (deterministic preflight, not lexical retrieval):\n\
         - Framework: Angular standalone application.\n\
         - Bootstrap entry: `src/main.ts`.\n\
         - Active root component: `{root_component}`.\n\
         - Active root template: `{root_template}`.\n\
         - Router configuration: `{config_path}` -> `{routes_path}`.\n\
         Before changing a user-visible Angular page, read these active files. Do not assume a \
         separately named `app.component.*` tree is rendered unless the bootstrap or active routes \
         import it. A successful build of a lazy component proves only that it compiles, not that \
         the active home page or navigation reaches it.\n"
    )
}

/// A concise framework work packet. Its trigger is deterministic and its text
/// is deliberately advisory: only repository-owned acceptance checks can prove
/// the outcome. The compiled context records its presence as `framework_guidance`.
pub fn framework_guidance(root: &Path) -> String {
    if workspace_topology(root).is_empty() {
        return String::new();
    }
    "Framework guidance loaded: Angular standalone. Keep one active component architecture: follow `bootstrapApplication` from `src/main.ts`, then edit the root template or components actually reached by the active router. Import `RouterOutlet` in the root and `RouterLink` where templates use router links. Replace/update generated root tests when the product requirement replaces the starter screen. Test the real pages: a root test that renders the home route and follows the main navigation through the router, not one that only finds `<router-outlet>`. You cannot see the rendered page -- no tool renders or screenshots it -- so do not claim visual verification; say which declared checks passed, and reason about layout from the CSS you wrote (mobile and desktop media queries, `prefers-reduced-motion` for every animation).".into()
}

fn read_workspace_file(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_default()
}

fn module_import_path(source: &str, module: &str) -> Option<String> {
    source
        .lines()
        .find(|line| line.contains(module))
        .map(|_| format!("{}.ts", module.trim_start_matches("./")))
}

fn quoted_metadata_path(source: &str, key: &str) -> Option<String> {
    let rest = source.split_once(key)?.1;
    let quote = rest
        .chars()
        .find(|character| matches!(character, '\'' | '\"'))?;
    let after_quote = rest.split_once(quote)?.1;
    Some(after_quote.split_once(quote)?.0.to_string())
}

fn join_workspace_relative(source: &str, relative: &str) -> String {
    let parent = Path::new(source).parent().unwrap_or_else(|| Path::new(""));
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in parent.join(relative).components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_os_string()),
            std::path::Component::ParentDir => {
                let _ = parts.pop();
            }
            std::path::Component::CurDir | std::path::Component::RootDir => {}
            std::path::Component::Prefix(_) => return source.into(),
        }
    }
    Path::new("")
        .join(parts.into_iter().collect::<std::path::PathBuf>())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod topology_tests {
    use super::{framework_guidance, workspace_topology};

    #[test]
    fn angular_standalone_topology_names_the_bootstrap_root_template_and_routes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src/app")).unwrap();
        std::fs::write(root.path().join("angular.json"), "{}").unwrap();
        std::fs::write(
            root.path().join("src/main.ts"),
            "import { App } from './app/app';\nbootstrapApplication(App, appConfig);",
        )
        .unwrap();
        std::fs::write(
            root.path().join("src/app/app.ts"),
            "@Component({ templateUrl: './app.html' }) export class App {}",
        )
        .unwrap();
        std::fs::write(root.path().join("src/app/app.html"), "<router-outlet />").unwrap();
        std::fs::write(
            root.path().join("src/app/app.config.ts"),
            "import { routes } from './app.routes';\nprovideRouter(routes);",
        )
        .unwrap();
        std::fs::write(
            root.path().join("src/app/app.routes.ts"),
            "export const routes = [];",
        )
        .unwrap();

        let topology = workspace_topology(root.path());
        assert!(topology.contains("Bootstrap entry: `src/main.ts`"));
        assert!(topology.contains("Active root component: `src/app/app.ts`"));
        assert!(topology.contains("Active root template: `src/app/app.html`"));
        assert!(
            topology.contains(
                "Router configuration: `src/app/app.config.ts` -> `src/app/app.routes.ts`"
            )
        );
        assert!(framework_guidance(root.path()).contains("Angular standalone"));
    }

    #[test]
    fn unrecognised_workspaces_do_not_receive_framework_guesses() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.cbl"), "IDENTIFICATION DIVISION.").unwrap();
        assert!(workspace_topology(root.path()).is_empty());
        assert!(framework_guidance(root.path()).is_empty());
    }
}

fn retrieved_context(
    ranker: Option<&mut dyn pwr_repo::SectionRanker>,
    root: &Path,
    index: &pwr_repo::RepositoryIndex,
    task: &str,
    context_tokens: u32,
    max_excerpts: Option<usize>,
) -> String {
    let budget = context_tokens as usize / RETRIEVAL_TOKEN_SHARE;
    let excerpts = pwr_repo::retrieve_with(
        root,
        index,
        task,
        max_excerpts.unwrap_or(RETRIEVAL_MAX_EXCERPTS),
        budget,
        ranker,
    )
    .unwrap_or_default();
    if excerpts.is_empty() {
        return String::new();
    }
    let mut block = String::from(
        "Repository passages ranked against this task. They are a starting point, not a complete view: the ranking is lexical, so read more if what you need is not here.\n\n",
    );
    for excerpt in &excerpts {
        block.push_str(&format!(
            "--- {} lines {}-{} (artifact_hash {}, {})\n{}\n\n",
            excerpt.path,
            excerpt.first_line,
            excerpt.last_line,
            excerpt.content_hash,
            excerpt.rationale,
            excerpt.content,
        ));
    }
    block.push_str("--- end of retrieved passages\n\n");
    block
}

pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(CHARS_PER_TOKEN)
}

/// Fits the sections into the context and says what that cost.
///
/// Required sections are never cut. Optional ones are dropped in eviction
/// order until the rest fits, and the last one considered is truncated rather
/// than dropped if what survives is still worth reading.
pub fn compile(sections: Vec<Section>, context_tokens: u32) -> (Vec<ChatMessage>, CompiledPrompt) {
    let reserve_tokens = (f64::from(context_tokens) * OUTPUT_RESERVE_SHARE) as usize;
    let budget = (context_tokens as usize).saturating_sub(reserve_tokens);

    let mut sections = sections;
    sections.retain(|section| !section.content.is_empty());

    let required: usize = sections
        .iter()
        .filter(|section| section.kind.required())
        .map(|section| estimate_tokens(&section.content))
        .sum();

    // Optional sections, worst first, so the one evicted is the one whose
    // absence costs least.
    let mut order: Vec<usize> = (0..sections.len())
        .filter(|index| !sections[*index].kind.required())
        .collect();
    order.sort_by_key(|index| sections[*index].kind.eviction_order());

    let mut kept: std::collections::BTreeMap<usize, Option<usize>> =
        order.iter().map(|index| (*index, None::<usize>)).collect();
    let spend = |kept: &std::collections::BTreeMap<usize, Option<usize>>| -> usize {
        required
            + kept
                .iter()
                .map(|(index, limit)| match limit {
                    Some(chars) => estimate_tokens(&sections[*index].content[..*chars]),
                    None => estimate_tokens(&sections[*index].content),
                })
                .sum::<usize>()
    };

    for index in &order {
        if spend(&kept) <= budget {
            break;
        }
        // Try to keep some of it before giving it up entirely.
        let over = spend(&kept).saturating_sub(budget);
        let content = &sections[*index].content;
        let keep_chars = content.len().saturating_sub(over * CHARS_PER_TOKEN);
        let keep_chars = floor_char_boundary(content, keep_chars);
        if keep_chars >= MIN_USEFUL_CHARS {
            kept.insert(*index, Some(keep_chars));
        } else {
            kept.remove(index);
        }
    }

    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut compiled: Vec<CompiledSection> = Vec::new();
    let mut reduced = false;
    for (index, section) in sections.iter().enumerate() {
        let limit = if section.kind.required() {
            Some(None)
        } else {
            kept.get(&index).copied()
        };
        let Some(limit) = limit else {
            reduced = true;
            compiled.push(CompiledSection {
                kind: section.kind,
                estimated_tokens: 0,
                bytes: 0,
                content_hash: hash_bytes(section.content.as_bytes()),
                truncated_to: None,
                dropped: true,
            });
            continue;
        };
        let content = match limit {
            Some(chars) => {
                reduced = true;
                &section.content[..chars]
            }
            None => section.content.as_str(),
        };
        compiled.push(CompiledSection {
            kind: section.kind,
            estimated_tokens: estimate_tokens(content),
            bytes: content.len(),
            // The hash of what was sent, not of what was offered, so a
            // truncated section is not mistaken for the whole one.
            content_hash: hash_bytes(content.as_bytes()),
            truncated_to: limit,
            dropped: false,
        });
        messages.push(ChatMessage {
            role: section.kind.role().into(),
            content: content.to_string(),
            purpose: match section.kind {
                SectionKind::Task => Some(pwr_domain::MessagePurpose::Task),
                SectionKind::SessionLedger => Some(pwr_domain::MessagePurpose::SessionLedger),
                SectionKind::RepositoryExcerpts => {
                    Some(pwr_domain::MessagePurpose::RepositoryExcerpts)
                }
                _ => None,
            },
            ..Default::default()
        });
    }

    // The system prompt and its per-deployment suffix become one message,
    // because they are one instruction and a backend that expects a single
    // system message should get one.
    //
    // User sections are never merged. Gluing the repository excerpts to the
    // task is exactly the shape this file exists to undo: nothing downstream
    // could tell them apart, so compaction had to keep the whole thing or lose
    // the goal with it.
    let messages = merge_system(messages);
    let estimated_tokens = compiled
        .iter()
        .map(|section| section.estimated_tokens)
        .sum();
    (
        messages,
        CompiledPrompt {
            sections: compiled,
            estimated_tokens,
            reserve_tokens,
            context_tokens,
            reduced,
        },
    )
}

fn merge_system(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut merged: Vec<ChatMessage> = Vec::new();
    for message in messages {
        match merged.last_mut() {
            Some(last) if last.role == "system" && message.role == "system" => {
                last.content.push_str(&message.content);
            }
            _ => merged.push(message),
        }
    }
    merged
}

/// The largest index at or below `at` that is a character boundary.
///
/// Cutting a string mid-code-point panics, and a prompt that panics on a
/// non-ASCII repository is worse than one that keeps four bytes fewer.
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod prompt_overhead_tests {
    use super::*;

    #[test]
    fn the_overhead_is_an_offset_and_not_a_factor() {
        // The numbers are the ones a campaign recorded on qwen3.6-35b-a3b:
        // the backend counted about 2,900 tokens more than the estimate,
        // whatever the size of the history.
        let mut overhead = PromptOverhead::default();
        assert_eq!(overhead.predict(2_036), 2_036);

        overhead.observe(2_036, 4_895);
        assert_eq!(overhead.tokens(), 2_859);
        assert_eq!(overhead.predict(2_036), 4_895);

        // A longer history costs the same fixed parts, so the offset holds and
        // the budget it leaves does not shrink with it.
        overhead.observe(2_202, 5_395);
        assert_eq!(overhead.tokens(), 3_193);
        assert_eq!(overhead.as_estimate(8_192), 8_192 - 3_193);

        // The turn that broke the first correction: right after a compaction
        // the history is small and the fixed parts are not, which read as a
        // ratio of 4.8 and shrank the budget to a fifth. As an offset it is
        // the same 2,881 tokens, and the budget is unchanged.
        let before = overhead.as_estimate(8_192);
        overhead.observe(762, 3_643);
        assert_eq!(
            overhead.tokens(),
            3_193,
            "a small history must not inflate it"
        );
        assert_eq!(overhead.as_estimate(8_192), before);

        // It is bounded, so one strange count cannot leave no room at all.
        overhead.observe(10, 1_000_000);
        assert_eq!(overhead.tokens(), MAX_PROMPT_OVERHEAD);
        // And a count smaller than the estimate is not negative room.
        let mut generous = PromptOverhead::default();
        generous.observe(5_000, 1_000);
        assert_eq!(generous.tokens(), 0);
    }

    #[test]
    fn the_reply_headroom_is_the_reserve_the_compiler_holds_back() {
        assert_eq!(reply_headroom(16_384), 4_096);
        let (_, compiled) = compile(
            vec![Section::new(SectionKind::Task, "do the thing")],
            16_384,
        );
        assert_eq!(
            u64::try_from(compiled.reserve_tokens).unwrap(),
            reply_headroom(16_384)
        );
    }
}
