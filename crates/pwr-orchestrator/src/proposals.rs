//! Verified proposals: the mechanical half of asking a model for one file at a
//! time and keeping it only when the owner's tests say it helped.
//!
//! Measured outside the product first (pwr-evidence, 2026-10-05/06, Ornith 1.5
//! 9B and Qwen3.5 9B, MLX 4-bit). On a repair task with seventeen tests the
//! tool-driven agent left 5 passing; asking for whole files, checking each and
//! keeping only improvements left 7-8; adding the governor below left about 13
//! (ten and nine runs). On two create-from-scratch tasks the gain was in
//! steadiness more than in the best case, and on one of them there was none.
//! Three later changes to what the model is shown did nothing measurable, so
//! none of them is here.
//!
//! What this module decides is what the harness may decide: which files the
//! owner's tests name, where in a stream an answer starts and ends, whether a
//! failing set shrank. What goes in the file is the model's.
//!
//! Nothing here touches a workspace or a model; the executor does, through its
//! host.

use std::collections::BTreeSet;
use std::path::Path;

/// The fence tags a language's file is accepted under.
pub fn language(path: &str) -> Option<(&'static str, &'static [&'static str])> {
    let tags: (&str, &[&str]) = match Path::new(path).extension()?.to_str()? {
        "ts" | "mts" | "cts" | "tsx" => ("typescript", &["typescript", "ts", "tsx"]),
        "js" | "mjs" | "cjs" | "jsx" => ("javascript", &["javascript", "js", "mjs", "jsx"]),
        "py" => ("python", &["python", "py"]),
        "rs" => ("rust", &["rust", "rs"]),
        "go" => ("go", &["go", "golang"]),
        _ => return None,
    };
    Some(tags)
}

/// The complete fenced blocks of `tags` in `text`, in order. A block that is
/// still open is not in the list: half a file is never an answer.
fn blocks<'a>(text: &'a str, tags: &[&str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut open: Option<usize> = None;
    let mut wanted = false;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let Some(tag) = line.trim_end().strip_prefix("```") else {
            continue;
        };
        match open {
            None => {
                wanted = tags.contains(&tag.trim());
                open = Some(offset);
            }
            // Only a bare fence closes: "```python" inside a block is text.
            Some(body) if tag.is_empty() => {
                if wanted {
                    found.push(&text[body..start]);
                }
                open = None;
            }
            Some(_) => {}
        }
    }
    found
}

/// Whether `text` ends inside a fenced block: code is being written.
fn writing(text: &str) -> bool {
    text.lines().filter(|line| line.starts_with("```")).count() % 2 == 1
}

/// The file a reply proposes: its last complete block in the file's language.
/// Prose around it is ignored and nothing is repaired.
pub fn extract(reply: &str, path: &str) -> Option<String> {
    let (_, tags) = language(path)?;
    blocks(reply, tags)
        .into_iter()
        .rev()
        .find(|block| !block.trim().is_empty())
        .map(str::to_owned)
}

/// Source files the owner's tests name and the workspace does not have: what a
/// repository built from nothing must contain for its tests to run at all.
/// `tests` is each test file's path and text; `exists` answers for a path
/// relative to the workspace.
pub fn missing_targets(tests: &[(String, String)], exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (_, text) in tests {
        for word in text.split(|ch: char| !(ch.is_alphanumeric() || "._-/".contains(ch))) {
            // A bare file name, not a path: `../src/money.ts` is an import of
            // something that should already be there, and is a repair target
            // only if its test fails.
            let name = word.trim_matches('.');
            if name.contains('/')
                || name.starts_with("test")
                || language(name).is_none()
                || name.split('.').count() != 2
                || found.iter().any(|seen| seen == name)
                || exists(name)
            {
                continue;
            }
            found.push(name.to_owned());
        }
    }
    found
}

/// A file to ask for, and what the model is shown with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    /// Read-only files: the tests that use it and the targets it imports.
    pub context: Vec<(String, String)>,
    /// The test files that use it, and every test file of the workspace: what
    /// [`relevant`] needs to tell this file's failures from the others'.
    pub tests: Vec<String>,
    pub all_tests: Vec<String>,
}

/// The part of the checks' output that is about the test files `mine`, when
/// the output says which test file each failure belongs to; `None` when it
/// does not.
///
/// One check often runs every test of a project. Shown all of it under "the
/// output on money.ts", Ornith 1.5 put CSV export and invoice printing into
/// money.ts to make the other files' tests pass: 196 lines for a file of 28
/// (product path, 2026-10-06). A failure belongs to a test file when the line
/// that opens it names the file, by path or by its name without extension
/// (`test at test/money.test.ts:5:1`, `FAIL: test_add (test_todo.Case.test_add)`).
pub fn relevant(evidence: &str, all_tests: &[String], mine: &[String]) -> Option<String> {
    fn stem(path: &str) -> &str {
        let name = path.rsplit('/').next().unwrap_or(path);
        name.rsplit_once('.').map_or(name, |(stem, _)| stem)
    }
    let owner = |line: &str| {
        all_tests
            .iter()
            .find(|path| line.contains(path.as_str()) || line.contains(stem(path)))
            .map(|path| mine.contains(path))
    };
    let mut kept = String::new();
    let mut keeping = false;
    for line in evidence.lines() {
        if let Some(is_mine) = owner(line) {
            keeping = is_mine;
        }
        if keeping {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    (!kept.trim().is_empty()).then_some(kept)
}

/// The checks' output as a request for `target` shows it: its own tests'
/// part, or all of it said to be all of it.
pub fn shown(evidence: &str, target: &Target) -> String {
    relevant(evidence, &target.all_tests, &target.tests).unwrap_or_else(|| {
        format!(
            "(Every test of the project, not only those of {}: the output does not say which \
             is which. Other files are corrected separately.)\n{evidence}",
            target.path
        )
    })
}

/// Directories a survey never enters.
const SKIPPED: &[&str] = &[
    ".git",
    ".pwr",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "dist",
    "build",
];
/// A test file larger than this is not shown to the model whole.
const MAX_SHOWN: u64 = 48 * 1024;

fn test_files(root: &Path) -> Vec<(String, String)> {
    fn walk(
        root: &Path,
        folder: &Path,
        depth: usize,
        inside: bool,
        found: &mut Vec<(String, String)>,
    ) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        for path in entries {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if depth < 3 && !SKIPPED.contains(&name) && !path.is_symlink() {
                    let tests = inside || matches!(name, "test" | "tests" | "__tests__" | "spec");
                    walk(root, &path, depth + 1, tests, found);
                }
            } else if (inside
                || name.contains(".test.")
                || name.contains(".spec.")
                || name.starts_with("test_"))
                && language(name).is_some()
                && path.metadata().is_ok_and(|meta| meta.len() <= MAX_SHOWN)
                && found.len() < 24
                && let (Ok(relative), Ok(text)) =
                    (path.strip_prefix(root), std::fs::read_to_string(&path))
            {
                found.push((relative.to_string_lossy().replace('\\', "/"), text));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, 0, false, &mut found);
    found
}

/// The source files a test file imports by relative path, there or not: one
/// that is missing is a file the tests need written.
fn imported(root: &Path, test: &str, text: &str) -> Vec<String> {
    let folder = Path::new(test).parent().unwrap_or(Path::new(""));
    let mut found = Vec::new();
    for quoted in text.split(['\'', '"', '`']) {
        if !(quoted.starts_with("./") || quoted.starts_with("../")) || language(quoted).is_none() {
            continue;
        }
        let mut parts: Vec<&str> = Vec::new();
        let mut inside = true;
        let joined = folder.join(quoted);
        for part in joined.iter().filter_map(|part| part.to_str()) {
            match part {
                "." => {}
                ".." => inside &= parts.pop().is_some(),
                part => parts.push(part),
            }
        }
        let path = parts.join("/");
        if inside && !root.join(&path).is_dir() && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// The files to ask for, in the order to ask: what the owner's tests name and
/// the workspace lacks, then the source that failing test files import.
///
/// A test file counts as failing when the checks' output names it; when the
/// output names none of them, all do. A file other targets mention comes
/// before them, so a module is settled before what is built on it.
pub fn survey(root: &Path, evidence: &str) -> Vec<Target> {
    let tests = test_files(root);
    let named: Vec<&(String, String)> = tests
        .iter()
        .filter(|(path, _)| evidence.contains(path.as_str()))
        .collect();
    let failing: Vec<&(String, String)> = if named.is_empty() {
        tests.iter().collect()
    } else {
        named
    };
    let mut paths = missing_targets(&tests, |path| root.join(path).exists());
    for (test, text) in &failing {
        for path in imported(root, test, text) {
            if !paths.contains(&path) && !tests.iter().any(|(test, _)| *test == path) {
                paths.push(path);
            }
        }
    }
    let texts: Vec<String> = paths
        .iter()
        .map(|path| std::fs::read_to_string(root.join(path)).unwrap_or_default())
        .collect();
    let name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();
    let mentions = |index: usize| {
        let name = name(&paths[index]);
        texts
            .iter()
            .enumerate()
            .filter(|(other, text)| *other != index && text.contains(&name))
            .count()
    };
    let mut order: Vec<usize> = (0..paths.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(mentions(*index)));
    order
        .into_iter()
        .map(|index| {
            let file = name(&paths[index]);
            let mut context: Vec<(String, String)> = tests
                .iter()
                .filter(|(_, text)| text.contains(&file))
                .cloned()
                .collect();
            let using = context.iter().map(|(path, _)| path.clone()).collect();
            for (other, text) in paths.iter().zip(&texts) {
                if *other != paths[index] && !text.is_empty() && texts[index].contains(&name(other))
                {
                    context.push((other.clone(), text.clone()));
                }
            }
            Target {
                path: paths[index].clone(),
                context,
                tests: using,
                all_tests: tests.iter().map(|(path, _)| path.clone()).collect(),
            }
        })
        .collect()
}

/// What a goal is told about where its failing checks point: the files the
/// failing tests use, read from the tests. Mechanical, and said to be: finding
/// a file in a test is the harness's work, deciding what is wrong in it is not.
///
/// A weak model spends a goal finding where to work. Measured outside PWR
/// (arXiv 2609.20804, a 30B model on SWE-Bench Verified): 58 % of its runs
/// ended while still locating the problem and 69 % without an edit. Seen here
/// on the product path, 2026-10-06: Ornith 1.5 9B read every file of a
/// four-module project in its first twelve actions, ten runs in ten, and
/// changed none.
pub fn pointers(targets: &[Target], exists: impl Fn(&str) -> bool) -> Option<String> {
    const SHOWN: usize = 8;
    if targets.is_empty() {
        return None;
    }
    let mut note = String::from(
        "Where the failing checks point. PWR read this from the tests; it is where to look, \
         not what is wrong:",
    );
    for target in targets.iter().take(SHOWN) {
        let tests = if target.tests.is_empty() {
            "the tests".to_owned()
        } else {
            target.tests.join(", ")
        };
        note.push_str(&if exists(&target.path) {
            format!("\n- {}, used by {tests}", target.path)
        } else {
            format!(
                "\n- {}, which {tests} needs and is not there yet",
                target.path
            )
        });
    }
    if targets.len() > SHOWN {
        note.push_str(&format!("\n- and {} more", targets.len() - SHOWN));
    }
    note.push_str("\nBegin with these, and read other files as these lead you to them.");
    Some(note)
}

/// The project's own statement of what it must do, when it has one.
pub fn contract(root: &Path) -> String {
    ["README.md", "README", "readme.md"]
        .iter()
        .find_map(|name| std::fs::read_to_string(root.join(name)).ok())
        .map(|text| text.chars().take(16_000).collect())
        .unwrap_or_default()
}

/// What a set of checks said, as far as a proposal is judged by it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standing {
    /// Any check failed.
    pub failing: bool,
    /// The failing tests by name ([`pwr_verify::failure`]); empty when the
    /// toolchain's output names none, or when nothing ran far enough to.
    pub failures: BTreeSet<String>,
}

/// Whether the workspace is better with a proposal than without: everything
/// passes, or the tests that fail are fewer and none of them is new.
///
/// A proposal that trades one failure for another is refused, and so is one
/// after which no test is named at all while checks still fail -- that is what
/// a file that does not parse looks like, and it must not read as "no
/// failures". Without names on both sides only a full pass counts.
pub fn improves(before: &Standing, after: &Standing) -> bool {
    if !after.failing {
        return before.failing;
    }
    !before.failures.is_empty()
        && !after.failures.is_empty()
        && after.failures.len() < before.failures.len()
        && after.failures.is_subset(&before.failures)
}

/// Why the governor ended a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// A complete file is in the stream; anything after it would be prose.
    Answer,
    /// The reasoning is repeating itself.
    Stalled,
    /// The share of the budget for thinking, or for the whole reply, is spent.
    Share,
}

/// Which part of a reply a piece of text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Reasoning,
    Answer,
}

/// Reads a reply as it is generated and says when to stop it, judged against
/// the file being asked for rather than against a number of reasoning tokens.
///
/// A fixed reasoning cap was tried first (1,024 tokens): Ornith 1.5 went on
/// analysing in its answer instead, 8 kB of prose where a 40-line file was
/// asked for, and the reply was cut at its token limit. Template `<think>`
/// tags did not separate the two phases on either family measured.
pub struct Governor {
    current: String,
    names: Vec<String>,
    tags: &'static [&'static str],
    transport: Transport,
    thinking_share: Option<f64>,
    share: Option<f64>,
    text: String,
    answer: String,
    channel: Option<Channel>,
    judged_words: usize,
    stop: Option<Stop>,
    file: Option<String>,
}

impl Governor {
    /// Words judged together, and how many must exist before repetition is
    /// looked for. Set on the archived streams of one task; a threshold, and
    /// to be treated as one.
    const WINDOW: usize = 120;
    const MIN_WORDS: usize = 260;
    const STEP: usize = 30;
    /// The share of a window's four-word sequences already written earlier.
    const REPEATED: f64 = 0.6;
    /// A new file's answer is a substantial block, not a snippet in passing.
    const NEW_FILE_CHARS: usize = 300;
    /// A correction keeps most of the file: a shorter block quotes a part.
    const KEPT: f64 = 0.6;
    /// An answer being written is not cut at its share; a runaway is, here.
    const RUNAWAY: f64 = 3.0;

    /// `current` is the file as it stands, empty for one that does not exist.
    /// The shares are seconds on the caller's clock; `None` is no limit.
    pub fn new(path: &str, current: &str, thinking_share: Option<f64>, share: Option<f64>) -> Self {
        let (name, tags) = language(path).unwrap_or(("", &[]));
        Self {
            current: current.to_owned(),
            names: if name == "typescript" || name == "javascript" {
                exports(current)
            } else {
                Vec::new()
            },
            tags,
            transport: Transport::Whole,
            thinking_share,
            share,
            text: String::new(),
            answer: String::new(),
            channel: None,
            judged_words: 0,
            stop: None,
            file: None,
        }
    }

    /// Reads the reply as edit blocks instead of a whole file.
    pub fn with_transport(mut self, transport: Transport) -> Self {
        // A file that does not exist has nothing to search in.
        if !self.current.trim().is_empty() {
            self.transport = transport;
        }
        self
    }

    /// Adds a piece of the reply; `elapsed` is the time since the generation
    /// began. Returns why to stop, once there is a reason.
    pub fn feed(&mut self, piece: &str, channel: Channel, elapsed: f64) -> Option<Stop> {
        if self.stop.is_some() {
            return self.stop;
        }
        // The two channels arrive as one text with no line between them.
        if self.channel.is_some_and(|last| last != channel) && !self.text.ends_with('\n') {
            self.text.push('\n');
        }
        self.channel = Some(channel);
        self.text.push_str(piece);
        if channel == Channel::Answer {
            self.answer.push_str(piece);
        }
        if (piece.contains('`') || piece.contains('\n'))
            && let Some(file) = self.candidate()
        {
            self.file = Some(file);
            self.stop = Some(Stop::Answer);
            return self.stop;
        }
        if writing(&self.text) || (self.transport == Transport::Blocks && open_block(&self.answer))
        {
            // Cutting a file half written throws away everything spent on it.
            if self
                .share
                .is_some_and(|share| elapsed >= Self::RUNAWAY * share)
            {
                self.stop = Some(Stop::Share);
            }
            return self.stop;
        }
        if self.share.is_some_and(|share| elapsed >= share)
            || self.thinking_share.is_some_and(|share| elapsed >= share)
        {
            self.stop = Some(Stop::Share);
            return self.stop;
        }
        let words = self.text.split_whitespace().count();
        if words >= self.judged_words + Self::STEP {
            self.judged_words = words;
            if self.repeated() >= Self::REPEATED {
                self.stop = Some(Stop::Stalled);
            }
        }
        self.stop
    }

    /// The file the reply holds, once [`Stop::Answer`] was returned.
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// The model's latest analysis outside code, verbatim, for handing back to
    /// it when it is asked to write the file now.
    pub fn notes(&self, limit: usize) -> String {
        let mut prose = String::new();
        let mut inside = false;
        for line in self.text.lines() {
            if line.starts_with("```") {
                inside = !inside;
            } else if !inside && !matches!(line.trim(), "<think>" | "</think>") {
                prose.push_str(line);
                prose.push('\n');
            }
        }
        let prose = prose.trim();
        let start = prose
            .char_indices()
            .rev()
            .nth(limit.saturating_sub(1))
            .map_or(0, |(index, _)| index);
        prose[start..].to_owned()
    }

    fn candidate(&self) -> Option<String> {
        if self.transport == Transport::Blocks {
            // Blocks do not say how many there will be: the answer is over
            // when something that is not a block follows the last one.
            let (found, followed) = edit_blocks(&self.answer);
            return (followed && !found.is_empty())
                .then(|| apply_blocks(&self.current, &self.answer).ok())
                .flatten();
        }
        if self.current.trim().is_empty() {
            // Nothing to compare with: a substantial block of the answer, not
            // a draft inside the reasoning.
            return blocks(&self.answer, self.tags)
                .into_iter()
                .rev()
                .find(|block| block.trim().len() >= Self::NEW_FILE_CHARS)
                .map(str::to_owned);
        }
        blocks(&self.text, self.tags)
            .into_iter()
            .rev()
            .find(|block| {
                let found = exports(block);
                self.names.iter().all(|name| found.contains(name))
                    // A quote of the file as it is, or of one function of it,
                    // is not an answer.
                    && !block.split_whitespace().eq(self.current.split_whitespace())
                    && block.len() as f64 >= Self::KEPT * self.current.len() as f64
            })
            .map(str::to_owned)
    }

    fn repeated(&self) -> f64 {
        let words: Vec<&str> = self.text.split_whitespace().collect();
        if words.len() < Self::MIN_WORDS {
            return 0.0;
        }
        let split = words.len() - Self::WINDOW;
        let earlier: BTreeSet<&[&str]> = words[..split].windows(4).collect();
        let recent: Vec<&[&str]> = words[split.saturating_sub(3)..].windows(4).collect();
        recent.iter().filter(|gram| earlier.contains(*gram)).count() as f64
            / recent.len().max(1) as f64
    }
}

/// The names a TypeScript or JavaScript file exports at its top level.
fn exports(source: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        let rest = rest.strip_prefix("async ").unwrap_or(rest);
        let Some((kind, rest)) = rest.split_once(' ') else {
            continue;
        };
        if !matches!(
            kind,
            "function" | "const" | "let" | "class" | "interface" | "type" | "enum"
        ) {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if !name.is_empty() {
            names.insert(name);
        }
    }
    names.into_iter().collect()
}

/// What a previous proposal for the same file came to.
pub struct Refused<'a> {
    pub file: &'a str,
    /// The checks' own output on it.
    pub evidence: &'a str,
    /// Tests that passed before it and failed with it.
    pub broke: &'a [String],
}

/// What the model is asked: one file, with the contract, the owner's tests and
/// what the checks actually said. Plain text; the file to correct comes last.
pub fn brief(
    path: &str,
    contract: &str,
    context: &[(String, String)],
    current: &str,
    evidence: &str,
    refused: Option<&Refused<'_>>,
) -> String {
    let tag = language(path).map_or("", |(name, _)| name);
    let new = current.trim().is_empty();
    let mut parts = vec![if new {
        format!(
            "Write {path}: it does not exist yet. It must satisfy the project contract and \
             make the owner's tests pass."
        )
    } else {
        format!(
            "Correct {path} so the failing tests that use it pass. The tests and the contract \
             are right; keep what already passes. Other files are corrected separately: do \
             not move their work into this one."
        )
    }];
    parts.push(format!(
        "Reply with the complete {path} in one ```{tag} block."
    ));
    if !contract.trim().is_empty() {
        parts.push(format!("PROJECT CONTRACT:\n{}", contract.trim()));
    }
    for (name, text) in context {
        let tag = language(name).map_or("", |(name, _)| name);
        parts.push(format!(
            "READ-ONLY FILE {name}:\n```{tag}\n{}\n```",
            text.trim_end()
        ));
    }
    if !new {
        parts.push(format!(
            "REAL OUTPUT OF THE CHECKS ON THE CURRENT {path}:\n{}",
            evidence.trim()
        ));
    }
    if let Some(refused) = refused {
        let mut text = format!(
            "A PREVIOUS {path} WAS REFUSED:\n```{tag}\n{}\n```",
            refused.file.trim_end()
        );
        if !refused.broke.is_empty() {
            text.push_str(&format!("\nIt broke: {}", refused.broke.join(", ")));
        }
        text.push_str(&format!(
            "\nReal output of the checks on it:\n{}\nMake a different change.",
            refused.evidence.trim()
        ));
        parts.push(text);
    }
    if !new {
        parts.push(format!(
            "CURRENT {path} (the file to correct):\n```{tag}\n{}\n```",
            current.trim_end()
        ));
    }
    parts.join("\n\n") + "\n"
}

/// How a proposal for a file that exists is written.
///
/// A whole file is the simplest thing to ask for and to check, and it does
/// not scale: past a few hundred lines the reply does not fit, and every line
/// copied is a chance to copy it wrong (LFM2.5 added a brace to a line it was
/// not changing, three times in three, diagnostics of 2026-10-05). Blocks
/// leave what they do not name byte for byte as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    #[default]
    Whole,
    Blocks,
}

const SEARCH: &str = "<<<<<<< SEARCH";
const DIVIDER: &str = "=======";
const REPLACE: &str = ">>>>>>> REPLACE";

/// The complete edit blocks of a reply as (search, replace), and whether
/// anything but blank lines and fences follows the last of them.
fn edit_blocks(reply: &str) -> (Vec<(String, String)>, bool) {
    let mut found = Vec::new();
    let mut followed = false;
    let mut search: Option<String> = None;
    let mut replace: Option<String> = None;
    for line in reply.split_inclusive('\n') {
        let marker = line.trim_end();
        match (&mut search, &mut replace) {
            (None, _) if marker == SEARCH => {
                search = Some(String::new());
                followed = false;
            }
            (None, _) => {
                followed |= !found.is_empty() && !marker.is_empty() && !marker.starts_with("```");
            }
            (Some(_), None) if marker == DIVIDER => replace = Some(String::new()),
            (Some(text), None) => text.push_str(line),
            (Some(_), Some(_)) if marker == REPLACE => {
                found.push((
                    search.take().unwrap_or_default(),
                    replace.take().unwrap_or_default(),
                ));
            }
            (Some(_), Some(text)) => text.push_str(line),
        }
    }
    (found, followed)
}

/// Whether a reply ends inside an edit block.
fn open_block(reply: &str) -> bool {
    let count = |marker: &str| {
        reply
            .lines()
            .filter(|line| line.trim_end() == marker)
            .count()
    };
    count(SEARCH) > count(REPLACE)
}

/// `current` with a reply's edit blocks applied, or why they cannot be: a
/// block whose search text is not in the file exactly once is refused, never
/// matched loosely -- the harness does not guess which lines were meant.
pub fn apply_blocks(current: &str, reply: &str) -> Result<String, String> {
    let (found, _) = edit_blocks(reply);
    if found.is_empty() {
        return Err("no complete edit block".into());
    }
    let mut file = current.to_owned();
    for (search, replace) in found {
        let shown = search.lines().next().unwrap_or_default().trim().to_owned();
        if search.trim().is_empty() {
            return Err("an edit block searches for nothing".into());
        }
        match file.matches(&search).count() {
            1 => file = file.replacen(&search, &replace, 1),
            0 => {
                return Err(format!(
                    "a SEARCH block is not in the file as written: {shown}"
                ));
            }
            many => return Err(format!("a SEARCH block matches {many} places: {shown}")),
        }
    }
    if file == current {
        return Err("the edit blocks change nothing".into());
    }
    Ok(file)
}

/// [`brief`], asking for edit blocks when the file exists and `transport`
/// says so.
pub fn brief_for(
    transport: Transport,
    path: &str,
    contract: &str,
    context: &[(String, String)],
    current: &str,
    evidence: &str,
    refused: Option<&Refused<'_>>,
) -> String {
    let whole = brief(path, contract, context, current, evidence, refused);
    if transport == Transport::Whole || current.trim().is_empty() {
        return whole;
    }
    let tag = language(path).map_or("", |(name, _)| name);
    whole.replacen(
        &format!("Reply with the complete {path} in one ```{tag} block."),
        &format!(
            "Reply with edit blocks only, one for each place to change in {path}, each exactly \
             in this form:\n{SEARCH}\nlines copied exactly from the current file\n{DIVIDER}\nthe \
             lines that replace them\n{REPLACE}\nEach SEARCH must match the file exactly and in \
             one place only. Do not rewrite lines you are not changing."
        ),
        1,
    )
}

/// [`handoff`] for `transport`; a file that does not exist is always whole.
pub fn handoff_for(
    transport: Transport,
    brief: &str,
    path: &str,
    notes: &str,
    new: bool,
) -> String {
    let asked = handoff(brief, path, notes);
    if transport == Transport::Whole || new {
        return asked;
    }
    let tag = language(path).map_or("", |(name, _)| name);
    asked.replacen(
        &format!("Write the complete {path} now. Start your reply with ```{tag}\n"),
        &format!("Write the edit blocks for {path} now. Start your reply with {SEARCH}\n"),
        1,
    )
}

/// What the model is told when it is asked to stop analysing and write.
pub fn handoff(brief: &str, path: &str, notes: &str) -> String {
    let tag = language(path).map_or("", |(name, _)| name);
    format!(
        "{}\nYOUR ANALYSIS SO FAR (it is enough; do not analyse further):\n{notes}\n\n\
         Write the complete {path} now. Start your reply with ```{tag}\n",
        brief.trim_end()
    )
}

/// Where a proposal's text comes from: a plain generation, no tools.
#[async_trait::async_trait(?Send)]
pub trait Author {
    /// A reply to `prompt` as it is generated; `think` off asks the template
    /// for an answer without a reasoning phase, where it can.
    async fn write(&self, prompt: String, think: bool)
    -> Result<pwr_provider::ModelStream, String>;
}

/// How long the model may think and write, in seconds: each a share of what
/// the goal has left, decided by the caller.
#[derive(Debug, Clone, Copy)]
pub struct Shares {
    pub thinking: f64,
    pub reply: f64,
    /// For the second request, when the first ended without a file.
    pub answer: f64,
}

/// How one generation of a proposal ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Governor(Stop),
    /// The model finished by itself.
    Finished,
    /// The person, or the goal's deadline, stopped it.
    Interrupted,
}

/// A file the model proposed, or that it proposed none, and how it got there.
#[derive(Debug, Default)]
pub struct Proposal {
    pub file: Option<String>,
    pub phases: Vec<Ended>,
}

/// Reads a reply into `governor` until it says stop, the reply ends or the
/// person stops it. Dropping the stream is what cancels the generation.
async fn read(
    mut stream: pwr_provider::ModelStream,
    governor: &mut Governor,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<(Ended, String), String> {
    use futures_util::StreamExt;
    let started = tokio::time::Instant::now();
    let mut answer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok((Ended::Interrupted, answer));
        }
        let elapsed = started.elapsed().as_secs_f64();
        let mut said = None;
        if let Some(thinking) = chunk.thinking.as_deref().filter(|text| !text.is_empty()) {
            said = governor.feed(thinking, Channel::Reasoning, elapsed);
        }
        if said.is_none() && !chunk.content.is_empty() {
            answer.push_str(&chunk.content);
            said = governor.feed(&chunk.content, Channel::Answer, elapsed);
        }
        if let Some(stop) = said {
            return Ok((Ended::Governor(stop), answer));
        }
        if chunk.done {
            break;
        }
    }
    Ok((Ended::Finished, answer))
}

/// Asks for `path` once, and once more with the model's own notes and its
/// reasoning off when the first reply holds no file.
///
/// The second request is what made the difference measured in W2.9: told to
/// write now, with what it had already worked out in front of it, Ornith 1.5
/// produced the file in 7-17 s where it had spent its whole reply analysing.
/// A template that ignores the reasoning switch (LFM2.5) is not helped by it.
pub async fn propose<A: Author + ?Sized>(
    author: &A,
    path: &str,
    current: &str,
    brief: String,
    shares: Shares,
    transport: Transport,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Proposal, String> {
    let mut proposal = Proposal::default();
    let new = current.trim().is_empty();
    let transport = if new { Transport::Whole } else { transport };
    // What a reply that ended by itself holds: a block the governor would not
    // stop on -- a short correction; the checks judge it like any other.
    let finished = |answer: &str| match transport {
        Transport::Whole => extract(answer, path),
        Transport::Blocks => apply_blocks(current, answer).ok(),
    };
    let mut first = Governor::new(path, current, Some(shares.thinking), Some(shares.reply))
        .with_transport(transport);
    let (ended, answer) = read(author.write(brief.clone(), true).await?, &mut first, stop).await?;
    proposal.phases.push(ended);
    proposal.file = first.file().map(str::to_owned).or_else(|| {
        (ended == Ended::Finished)
            .then(|| finished(&answer))
            .flatten()
    });
    if proposal.file.is_some() || ended == Ended::Interrupted {
        return Ok(proposal);
    }
    let mut second =
        Governor::new(path, current, None, Some(shares.answer)).with_transport(transport);
    let asked = handoff_for(transport, &brief, path, &first.notes(1_800), new);
    let (ended, answer) = read(author.write(asked, false).await?, &mut second, stop).await?;
    proposal.phases.push(ended);
    proposal.file = second.file().map(str::to_owned).or_else(|| {
        (ended == Ended::Finished)
            .then(|| finished(&answer))
            .flatten()
    });
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATES: &str = "function parse(iso: string): Date {\n  return new Date(iso);\n}\n\n\
        function isWeekend(date: Date): boolean {\n  const day = date.getUTCDay();\n  \
        return day === 0 || day === 6;\n}\n\nexport function dueDate(issued: string, days: number): string {\n  \
        const date = parse(issued);\n  let left = days;\n  while (left > 0) {\n    \
        if (!isWeekend(date)) left--;\n    date.setUTCDate(date.getUTCDate() + 1);\n  }\n  \
        return date.toISOString().slice(0, 10);\n}\n";

    fn stream(governor: &mut Governor, text: &str, channel: Channel) -> Option<Stop> {
        let chars: Vec<char> = text.chars().collect();
        for piece in chars.chunks(7) {
            let piece: String = piece.iter().collect();
            if let Some(stop) = governor.feed(&piece, channel, 0.0) {
                return Some(stop);
            }
        }
        None
    }

    fn standing(failing: bool, failures: &[&str]) -> Standing {
        Standing {
            failing,
            failures: failures.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn a_proposal_is_kept_only_when_fewer_tests_fail_and_none_is_new() {
        let before = standing(true, &["a", "b", "c"]);
        assert!(improves(&before, &standing(true, &["a", "b"])));
        assert!(improves(&before, &standing(false, &[])));
        // One fixed, one broken: not progress, whatever the count says.
        assert!(!improves(&before, &standing(true, &["a", "d"])));
        assert!(!improves(&before, &standing(true, &["a", "b", "c"])));
        // A file that does not parse fails its checks and names no test.
        assert!(!improves(&before, &standing(true, &[])));
        // A toolchain that names nothing: only a full pass can be judged.
        assert!(!improves(&standing(true, &[]), &standing(true, &["a"])));
        assert!(improves(&standing(true, &[]), &standing(false, &[])));
        assert!(!improves(&standing(false, &[]), &standing(false, &[])));
    }

    #[test]
    fn the_file_is_the_last_complete_block_in_its_language() {
        let reply = "The bug is the order.\n```ts\nconst a = 1;\n```\nSo:\n```typescript\nconst b = \"`\";\n\n```\nDone.";
        assert_eq!(
            extract(reply, "src/dates.ts").as_deref(),
            Some("const b = \"`\";\n\n")
        );
        assert_eq!(
            extract("```python\nx = 1\n```", "todo.py").as_deref(),
            Some("x = 1\n")
        );
        for (reply, path) in [
            ("const a = 1;", "a.ts"),
            ("```typescript\nconst a = 1;", "a.ts"),
            ("```python\nx = 1\n```", "a.ts"),
            ("```typescript\n\n```", "a.ts"),
            ("<tool_call>write_file</tool_call>", "a.ts"),
            ("```\nplain\n```", "a.ts"),
            ("```text\nx\n```", "notes.txt"),
        ] {
            assert_eq!(extract(reply, path), None, "{reply}");
        }
    }

    #[test]
    fn targets_are_the_source_files_the_tests_name_and_the_workspace_lacks() {
        let python = "ROOT=pathlib.Path(__file__).resolve().parents[1]\n\
                      p=subprocess.run([sys.executable,str(ROOT/'todo.py'),'--db',str(self.db)])\n\
                      self.db=pathlib.Path(self.temp.name)/'items.json'";
        let node = "import {fileURLToPath} from 'node:url';\n\
                    const proc=spawn(process.execPath,[path.join(root,'server.mjs')]);\n\
                    import { dueDate } from '../src/dates.ts';";
        let tests = [
            ("tests/test_todo.py".to_owned(), python.to_owned()),
            ("tests/acceptance.mjs".to_owned(), node.to_owned()),
        ];
        assert_eq!(
            missing_targets(&tests, |_| false),
            ["todo.py", "server.mjs"]
        );
        assert_eq!(
            missing_targets(&tests, |path| path == "todo.py"),
            ["server.mjs"]
        );
    }

    #[test]
    fn generation_stops_at_the_complete_file_and_not_at_a_quote_of_it() {
        let fixed = DATES.replace("left--", "left -= 1");
        let mut governor = Governor::new("src/dates.ts", DATES, None, None);
        // The file as it is, then one function of it: neither is an answer.
        let quoted = format!(
            "The file is:\n```typescript\n{DATES}```\nonly\n```typescript\nexport function dueDate(a: string, b: number): string {{ return a; }}\n```\n"
        );
        assert_eq!(stream(&mut governor, &quoted, Channel::Reasoning), None);
        let answer = format!("```typescript\n{fixed}```\nExplanation that must not be generated.");
        assert_eq!(
            stream(&mut governor, &answer, Channel::Answer),
            Some(Stop::Answer)
        );
        assert_eq!(governor.file(), Some(fixed.as_str()));
        assert!(!governor.text.contains("Explanation"));
    }

    #[test]
    fn a_new_file_is_a_substantial_block_of_the_answer_not_a_draft_in_reasoning() {
        let body: String = (0..30)
            .map(|n| format!("def f{n}():\n    return {n}\n"))
            .collect();
        let mut governor = Governor::new("todo.py", "", None, None);
        let draft = format!("Plan:\n```python\n{body}```\n");
        assert_eq!(stream(&mut governor, &draft, Channel::Reasoning), None);
        assert_eq!(
            stream(
                &mut governor,
                "A snippet:\n```python\nimport sys\n```\n",
                Channel::Answer
            ),
            None
        );
        let answer = format!("```python\n{body}```\ntrailing");
        assert_eq!(
            stream(&mut governor, &answer, Channel::Answer),
            Some(Stop::Answer)
        );
        assert_eq!(governor.file(), Some(body.as_str()));
    }

    #[test]
    fn reasoning_that_repeats_itself_is_stalled_and_fresh_reasoning_is_not() {
        let mut governor = Governor::new("src/dates.ts", DATES, None, None);
        let fresh: String = (0..400)
            .map(|n| format!("point{n} about case{} ", n * 7))
            .collect();
        assert_eq!(stream(&mut governor, &fresh, Channel::Reasoning), None);
        let mut governor = Governor::new("src/dates.ts", DATES, None, None);
        let circling = "Let me recompute the weekday for this date again to be sure. ".repeat(60);
        assert_eq!(
            stream(&mut governor, &circling, Channel::Reasoning),
            Some(Stop::Stalled)
        );
    }

    #[test]
    fn thinking_ends_at_its_share_but_a_file_being_written_is_not_cut_by_it() {
        let mut governor = Governor::new("todo.py", "", Some(10.0), Some(20.0));
        assert_eq!(governor.feed("thinking ", Channel::Reasoning, 9.0), None);
        assert_eq!(
            governor.feed("more ", Channel::Reasoning, 10.0),
            Some(Stop::Share)
        );
        let mut governor = Governor::new("todo.py", "", Some(10.0), Some(20.0));
        assert_eq!(governor.feed("```python\n", Channel::Answer, 5.0), None);
        // Past both shares, still writing: let it finish.
        assert_eq!(governor.feed("x = 1\n", Channel::Answer, 45.0), None);
        // Three shares is a runaway.
        assert_eq!(
            governor.feed("y = 2\n", Channel::Answer, 60.0),
            Some(Stop::Share)
        );
    }

    #[test]
    fn the_notes_handed_back_are_the_model_s_own_prose_without_its_code() {
        let mut governor = Governor::new("src/dates.ts", DATES, None, None);
        governor.feed(
            "<think>\nThe bug is the order.\n```typescript\nconst secret = 1;\n```\nSo advance first.",
            Channel::Reasoning,
            0.0,
        );
        let notes = governor.notes(1_800);
        assert!(notes.contains("The bug is the order.") && notes.ends_with("So advance first."));
        assert!(!notes.contains("secret") && !notes.contains("<think>"));
        assert_eq!(governor.notes(8), "e first.");
    }

    #[test]
    fn the_brief_says_what_is_asked_and_puts_the_file_to_correct_last() {
        let tests = [(
            "tests/test_todo.py".to_owned(),
            "import unittest\n".to_owned(),
        )];
        let new = brief("todo.py", "CLI contract", &tests, "", "", None);
        assert!(new.starts_with("Write todo.py: it does not exist yet."));
        assert!(
            new.contains("in one ```python block")
                && new.contains("READ-ONLY FILE tests/test_todo.py:\n```python\n")
        );
        assert!(!new.contains("CURRENT todo.py") && !new.contains("REAL OUTPUT"));
        let broke = ["a weekday".to_owned()];
        let refused = Refused {
            file: "bad\n",
            evidence: "✖ a weekday",
            broke: &broke,
        };
        let repair = brief(
            "src/dates.ts",
            "rules",
            &[],
            DATES,
            "✖ a weekend",
            Some(&refused),
        );
        assert!(repair.starts_with("Correct src/dates.ts so the failing tests that use it pass."));
        assert!(
            repair.contains("REAL OUTPUT OF THE CHECKS ON THE CURRENT src/dates.ts:\n✖ a weekend")
        );
        assert!(
            repair.contains("It broke: a weekday") && repair.contains("Make a different change.")
        );
        assert!(
            repair.trim_end().ends_with("```")
                && repair.contains(
                    "CURRENT src/dates.ts (the file to correct):\n```typescript\nfunction parse"
                )
        );
        let asked = handoff(&repair, "src/dates.ts", "advance first");
        assert!(
            asked.ends_with("Start your reply with ```typescript\n")
                && asked.contains("YOUR ANALYSIS SO FAR")
        );
    }

    /// Replies scripted in advance; records what it was asked.
    struct Scripted {
        replies: std::cell::RefCell<Vec<Vec<(Channel, String)>>>,
        asked: std::cell::RefCell<Vec<(String, bool)>>,
    }

    impl Scripted {
        fn new(replies: Vec<Vec<(Channel, String)>>) -> Self {
            Self {
                replies: std::cell::RefCell::new(replies),
                asked: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl Author for Scripted {
        async fn write(
            &self,
            prompt: String,
            think: bool,
        ) -> Result<pwr_provider::ModelStream, String> {
            self.asked.borrow_mut().push((prompt, think));
            let reply = self.replies.borrow_mut().remove(0);
            let mut chunks: Vec<Result<pwr_domain::ModelChunk, pwr_provider::ProviderError>> =
                reply
                    .into_iter()
                    .flat_map(|(channel, text)| {
                        let chars: Vec<char> = text.chars().collect();
                        chars
                            .chunks(9)
                            .map(|piece| {
                                let piece: String = piece.iter().collect();
                                Ok(pwr_domain::ModelChunk {
                                    content: if channel == Channel::Answer {
                                        piece.clone()
                                    } else {
                                        String::new()
                                    },
                                    thinking: (channel == Channel::Reasoning).then_some(piece),
                                    tool_calls: Vec::new(),
                                    metrics: None,
                                    prefill: None,
                                    done: false,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
            chunks.push(Ok(pwr_domain::ModelChunk {
                content: String::new(),
                thinking: None,
                tool_calls: Vec::new(),
                metrics: None,
                prefill: None,
                done: true,
            }));
            Ok(Box::pin(futures_util::stream::iter(chunks)))
        }
    }

    const SHARES: Shares = Shares {
        thinking: 30.0,
        reply: 60.0,
        answer: 60.0,
    };

    fn run(author: &Scripted, path: &str, current: &str) -> Proposal {
        let stop = std::sync::atomic::AtomicBool::new(false);
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(propose(
                author,
                path,
                current,
                "BRIEF\n".to_owned(),
                SHARES,
                Transport::Whole,
                &stop,
            ))
            .unwrap()
    }

    fn block(search: &str, replace: &str) -> String {
        format!("{SEARCH}\n{search}{DIVIDER}\n{replace}{REPLACE}\n")
    }

    #[test]
    fn edit_blocks_change_what_they_name_and_leave_the_rest_byte_for_byte() {
        let reply = format!(
            "Two places.\n```\n{}```\n{}",
            block(
                "    if (!isWeekend(date)) left--;\n",
                "    if (!isWeekend(date)) left -= 1;\n"
            ),
            block(
                "  const date = parse(issued);\n",
                "  const date = parse(issued); // utc\n"
            ),
        );
        let file = apply_blocks(DATES, &reply).unwrap();
        assert_eq!(
            file,
            DATES
                .replace("left--", "left -= 1")
                .replace("parse(issued);", "parse(issued); // utc")
        );
        // Refused, never matched loosely.
        for (reply, why) in [
            (
                block("  nothing like this\n", "x\n"),
                "is not in the file as written: nothing like this",
            ),
            (block("}\n", "} // end\n"), "places"),
            (
                block("  let left = days;\n", "  let left = days;\n"),
                "change nothing",
            ),
            (block("\n", "x\n"), "searches for nothing"),
            (
                format!("{SEARCH}\n  let left = days;\n{DIVIDER}\n  let left = 0;\n"),
                "no complete edit block",
            ),
            (
                "```typescript\nconst a = 1;\n```".to_owned(),
                "no complete edit block",
            ),
        ] {
            assert!(
                apply_blocks(DATES, &reply).unwrap_err().contains(why),
                "{why}"
            );
        }
    }

    #[test]
    fn generation_of_blocks_stops_when_something_else_follows_the_last_block() {
        let first = block(
            "    if (!isWeekend(date)) left--;\n",
            "    if (!isWeekend(date)) left -= 1;\n",
        );
        let second = block("  let left = days;\n", "  let left = days + 0;\n");
        let mut governor = Governor::new("src/dates.ts", DATES, None, Some(10.0))
            .with_transport(Transport::Blocks);
        // One block, then another: not over yet, and not cut inside a block.
        assert_eq!(stream(&mut governor, &first, Channel::Answer), None);
        assert_eq!(
            governor.feed(
                &format!("{SEARCH}\n  let left = days;\n"),
                Channel::Answer,
                25.0
            ),
            None
        );
        assert_eq!(
            governor.feed(
                &format!("{DIVIDER}\n  let left = days + 0;\n{REPLACE}\n"),
                Channel::Answer,
                5.0
            ),
            None
        );
        assert_eq!(
            governor.feed(
                "\nThat is all: the loop counted the issue date.\n",
                Channel::Answer,
                5.0
            ),
            Some(Stop::Answer)
        );
        assert_eq!(
            governor.file().unwrap(),
            apply_blocks(DATES, &format!("{first}{second}")).unwrap()
        );
        // A file that does not exist is still asked for whole.
        let new = Governor::new("todo.py", "", None, None).with_transport(Transport::Blocks);
        assert_eq!(new.transport, Transport::Whole);
    }

    #[test]
    fn a_proposal_in_blocks_is_asked_for_and_read_as_blocks_through_both_requests() {
        let fix = block(
            "    if (!isWeekend(date)) left--;\n",
            "    if (!isWeekend(date)) left -= 1;\n",
        );
        let author = Scripted::new(vec![
            vec![(
                Channel::Reasoning,
                "Let me recompute the weekday for this date again to be sure. ".repeat(60),
            )],
            vec![(Channel::Answer, fix.clone())],
        ]);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let asked = brief_for(
            Transport::Blocks,
            "src/dates.ts",
            "rules",
            &[],
            DATES,
            "✖ a weekend",
            None,
        );
        assert!(asked.contains("Reply with edit blocks only") && asked.contains(SEARCH));
        assert!(!asked.contains("Reply with the complete"));
        let proposal = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(propose(
                &author,
                "src/dates.ts",
                DATES,
                asked,
                SHARES,
                Transport::Blocks,
                &stop,
            ))
            .unwrap();
        assert_eq!(proposal.file.unwrap(), DATES.replace("left--", "left -= 1"));
        assert_eq!(
            proposal.phases,
            [Ended::Governor(Stop::Stalled), Ended::Finished]
        );
        assert!(
            author.asked.borrow()[1]
                .0
                .ends_with(&format!("Start your reply with {SEARCH}\n"))
        );
        // A new file is asked for whole whatever the transport.
        let whole = brief_for(Transport::Blocks, "todo.py", "c", &[], "", "", None);
        assert!(whole.contains("Reply with the complete todo.py in one ```python block."));
    }

    #[test]
    fn a_reply_with_the_file_is_one_request_and_ends_at_the_file() {
        let fixed = DATES.replace("left--", "left -= 1");
        let author = Scripted::new(vec![vec![
            (Channel::Reasoning, "The order is wrong.".to_owned()),
            (
                Channel::Answer,
                format!("```typescript\n{fixed}```\nand then a long explanation"),
            ),
        ]]);
        let proposal = run(&author, "src/dates.ts", DATES);
        assert_eq!(proposal.file.as_deref(), Some(fixed.as_str()));
        assert_eq!(proposal.phases, [Ended::Governor(Stop::Answer)]);
        assert_eq!(author.asked.borrow().len(), 1);
        assert!(author.asked.borrow()[0].1, "the first request may reason");
    }

    #[test]
    fn a_reply_that_only_analyses_is_asked_once_more_with_its_notes_and_reasoning_off() {
        let fixed = DATES.replace("left--", "left -= 1");
        let author = Scripted::new(vec![
            vec![(
                Channel::Reasoning,
                "Let me recompute the weekday for this date again to be sure. ".repeat(60),
            )],
            vec![(Channel::Answer, format!("```typescript\n{fixed}```"))],
        ]);
        let proposal = run(&author, "src/dates.ts", DATES);
        assert_eq!(proposal.file.as_deref(), Some(fixed.as_str()));
        assert_eq!(
            proposal.phases,
            [
                Ended::Governor(Stop::Stalled),
                Ended::Governor(Stop::Answer)
            ]
        );
        let asked = author.asked.borrow();
        assert!(!asked[1].1, "the second request does not reason");
        assert!(asked[1].0.starts_with("BRIEF\nYOUR ANALYSIS SO FAR"));
        assert!(asked[1].0.contains("recompute the weekday"));
    }

    #[test]
    fn two_replies_without_a_file_are_no_proposal_and_never_a_guess() {
        let author = Scripted::new(vec![
            vec![(
                Channel::Answer,
                "<tool_call>write_file</tool_call>".to_owned(),
            )],
            vec![(
                Channel::Answer,
                "```typescript\nexport function dueDate(".to_owned(),
            )],
        ]);
        let proposal = run(&author, "src/dates.ts", DATES);
        assert_eq!(proposal.file, None);
        assert_eq!(proposal.phases, [Ended::Finished, Ended::Finished]);
    }

    #[test]
    fn a_short_correction_the_model_finished_by_itself_is_still_its_proposal() {
        let author = Scripted::new(vec![vec![(
            Channel::Answer,
            "```typescript\nexport function dueDate(): string { return ''; }\n```".to_owned(),
        )]]);
        let proposal = run(&author, "src/dates.ts", DATES);
        assert_eq!(proposal.phases, [Ended::Finished]);
        assert_eq!(
            proposal.file.as_deref(),
            Some("export function dueDate(): string { return ''; }\n")
        );
    }

    #[test]
    fn a_survey_asks_for_what_failing_tests_import_dependencies_first() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("test")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x/test")).unwrap();
        std::fs::write(root.join("README.md"), "# ledger\nRules.\n").unwrap();
        std::fs::write(
            root.join("src/money.ts"),
            "export function roundCents() {}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/invoice.ts"),
            "import { roundCents } from './money.ts';\n",
        )
        .unwrap();
        std::fs::write(root.join("src/dates.ts"), "export function dueDate() {}\n").unwrap();
        std::fs::write(
            root.join("test/invoice.test.ts"),
            "import { totals } from '../src/invoice.ts';\n",
        )
        .unwrap();
        std::fs::write(
            root.join("test/money.test.ts"),
            "import { roundCents } from '../src/money.ts';\n",
        )
        .unwrap();
        std::fs::write(
            root.join("test/dates.test.ts"),
            "import { dueDate } from '../src/dates.ts';\nimport x from '../../outside.ts';\n",
        )
        .unwrap();
        std::fs::write(
            root.join("node_modules/x/test/a.test.ts"),
            "import '../../../src/dates.ts';\n",
        )
        .unwrap();

        // The output names two test files: only what they import is asked for,
        // and money.ts, which invoice.ts is built on, comes first.
        let targets = survey(
            root,
            "test at test/invoice.test.ts:17:1\ntest at test/money.test.ts:5:1\n",
        );
        assert_eq!(
            targets
                .iter()
                .map(|target| target.path.as_str())
                .collect::<Vec<_>>(),
            ["src/money.ts", "src/invoice.ts"]
        );
        let invoice = &targets[1];
        assert_eq!(
            invoice
                .context
                .iter()
                .map(|(path, _)| path.as_str())
                .collect::<Vec<_>>(),
            ["test/invoice.test.ts", "src/money.ts"]
        );
        // Output that names no test file: every test file counts.
        assert_eq!(survey(root, "exit code 1").len(), 3);
        assert_eq!(contract(root), "# ledger\nRules.\n");
    }

    #[test]
    fn pointers_name_the_files_and_their_tests_and_say_what_they_are() {
        let target = |path: &str, tests: &[&str]| Target {
            path: path.into(),
            context: Vec::new(),
            tests: tests.iter().map(|test| (*test).to_owned()).collect(),
            all_tests: Vec::new(),
        };
        let note = pointers(
            &[
                target("src/money.ts", &["test/money.test.ts"]),
                target("todo.py", &["tests/test_todo.py"]),
            ],
            |path| path == "src/money.ts",
        )
        .unwrap();
        assert!(note.starts_with("Where the failing checks point."));
        assert!(note.contains("it is where to look, not what is wrong"));
        assert!(note.contains("\n- src/money.ts, used by test/money.test.ts"));
        assert!(note.contains("\n- todo.py, which tests/test_todo.py needs and is not there yet"));
        assert_eq!(pointers(&[], |_| true), None);
        let many: Vec<Target> = (0..11)
            .map(|n| target(&format!("src/f{n}.ts"), &[]))
            .collect();
        let note = pointers(&many, |_| true).unwrap();
        assert!(
            note.contains("src/f7.ts")
                && !note.contains("src/f8.ts")
                && note.contains("and 3 more")
        );
    }

    #[test]
    fn a_survey_of_an_empty_repository_asks_for_the_file_its_tests_run() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        std::fs::create_dir_all(root.join("tests")).unwrap();
        std::fs::write(
            root.join("tests/test_todo.py"),
            "ROOT=pathlib.Path(__file__).resolve().parents[1]\nsubprocess.run([sys.executable,str(ROOT/'todo.py')])\n",
        )
        .unwrap();
        let targets = survey(root, "");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, "todo.py");
        assert_eq!(targets[0].context[0].0, "tests/test_todo.py");
        std::fs::write(root.join("todo.py"), "print(1)\n").unwrap();
        assert!(
            survey(root, "").is_empty(),
            "nothing is missing and nothing is imported by path"
        );
        assert_eq!(contract(root), "");
    }

    #[test]
    fn a_request_is_shown_its_own_tests_failures_and_not_the_other_files() {
        let all = [
            "test/csv.test.ts".to_owned(),
            "test/money.test.ts".to_owned(),
        ];
        let node = "✖ rows end with CRLF (1.0ms)\n✖ rounds half away from zero (0.8ms)\nℹ fail 2\n✖ failing tests:\n\
                    test at test/csv.test.ts:5:1\n✖ rows end with CRLF (1.0ms)\n  + 'a\\n'\n  - 'a\\r\\n'\n\
                    test at test/money.test.ts:5:1\n✖ rounds half away from zero (0.8ms)\n  -0 !== -1\n";
        let mine = ["test/money.test.ts".to_owned()];
        assert_eq!(
            relevant(node, &all, &mine).as_deref(),
            Some(
                "test at test/money.test.ts:5:1\n✖ rounds half away from zero (0.8ms)\n  -0 !== -1\n"
            )
        );
        // unittest names the module, not the path.
        let python = "test_add (test_todo.Case.test_add) ... FAIL\nFAIL: test_add (test_todo.Case.test_add)\nAssertionError: 1 != 0\n";
        let tests = ["tests/test_todo.py".to_owned()];
        assert_eq!(relevant(python, &tests, &tests).as_deref(), Some(python));
        // Output that names no test file is shown whole, and said to be whole.
        assert_eq!(relevant("exit code 1\nboom\n", &all, &mine), None);
        let target = Target {
            path: "src/money.ts".into(),
            context: Vec::new(),
            tests: mine.to_vec(),
            all_tests: all.to_vec(),
        };
        let whole = shown("exit code 1\nboom\n", &target);
        assert!(whole.starts_with("(Every test of the project, not only those of src/money.ts"));
        assert!(whole.ends_with("exit code 1\nboom\n"));
        assert!(!shown(node, &target).contains("CRLF"));
    }
}
