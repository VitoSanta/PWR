//! The workspace as a knowledge graph: what exists, what depends on what,
//! what tests what, what was changed when, and what was decided -- with how
//! sure each edge is.
//!
//! Built from facts, never from a model: the repository index (`pwr_repo`:
//! files, the symbols they define, the imports they write, the source a test
//! file names by convention), the work log and the workspace's memories. Each
//! edge says how it is known ([`Certainty`]): read from a file, resolved by a
//! path rule, or guessed from a naming convention. A model-written summary may
//! be attached to a node; it is marked unverified and never becomes an edge.
//!
//! Stored as `.pwr/wiki/graph.json`, rebuilt after every turn (the index is
//! incremental, so an unchanged workspace is not re-read), and queried by a
//! conversation through `wiki_query`: a node and its neighbours.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// The most symbol nodes a graph keeps; past it, symbols stay as a count on
/// their file. Enough for a small project whole, and bounded for a large one.
const SYMBOL_NODES: usize = 5_000;
/// The most lines a query answers with.
const QUERY_LINES: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Project,
    /// A directory.
    Module,
    File,
    Symbol,
    /// Something imported by name that is not a file here: a dependency.
    Package,
    /// A turn that changed or finished something (the work log).
    Work,
    /// A memory of this workspace: a decision or a fact the person confirmed.
    Decision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Defines,
    Imports,
    Tests,
    /// file → work: the file was written by that turn.
    ChangedIn,
    /// decision → file: the decision names the file.
    About,
}

/// How an edge is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// Read directly: a file is in a folder, defines a name, was written.
    Fact,
    /// An import resolved to a file by a path rule of its language.
    Resolved,
    /// An import left as the name written, not resolved to anything here.
    Named,
    /// A naming convention or a mention, not a proof.
    Guess,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub certainty: Certainty,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub schema_version: u32,
    pub generated_at: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// One entry of the work log, as the graph links it.
#[derive(Debug, Clone)]
pub struct WorkEntry {
    pub when: String,
    pub request: String,
    pub files: Vec<String>,
}

/// A workspace memory, as the graph links it.
#[derive(Debug, Clone)]
pub struct DecisionEntry {
    pub id: String,
    pub text: String,
}

/// A model-written summary of a node, attached as unverified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub text: String,
    /// The hash of what it summarised; a node whose files changed since has a
    /// stale summary, shown as such until it is rewritten.
    pub source_hash: String,
    pub model: String,
    pub at: String,
}

pub fn file_id(path: &str) -> String {
    format!("file:{path}")
}

pub fn module_id(path: &str) -> String {
    if path.is_empty() {
        "project".into()
    } else {
        format!("dir:{path}")
    }
}

/// Builds the graph. `summaries` are keyed by node id.
pub fn build(
    name: &str,
    index: &pwr_repo::RepositoryIndex,
    work: &[WorkEntry],
    decisions: &[DecisionEntry],
    summaries: &BTreeMap<String, Summary>,
) -> Graph {
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    let mut edges: BTreeSet<Edge> = BTreeSet::new();
    add(
        &mut nodes,
        "project".into(),
        NodeKind::Project,
        name.to_owned(),
    );

    let paths: BTreeSet<&str> = index.files.iter().map(|file| file.path.as_str()).collect();
    // Source files by lowercase stem, for tests named after what they test.
    let mut by_stem: HashMap<String, Vec<&str>> = HashMap::new();
    for file in &index.files {
        if file.tests.is_none()
            && let Some(stem) = Path::new(&file.path)
                .file_stem()
                .and_then(|stem| stem.to_str())
        {
            by_stem
                .entry(stem.to_ascii_lowercase())
                .or_default()
                .push(&file.path);
        }
    }
    let mut symbols = 0usize;
    for file in &index.files {
        // Folders, from the project down to the file.
        let mut parent = String::new();
        let parts: Vec<&str> = file.path.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            let child = if parent.is_empty() {
                (*part).to_owned()
            } else {
                format!("{parent}/{part}")
            };
            add(
                &mut nodes,
                module_id(&child),
                NodeKind::Module,
                format!("{child}/"),
            );
            edges.insert(fact(
                module_id(&parent),
                module_id(&child),
                EdgeKind::Contains,
            ));
            parent = child;
        }
        let id = file_id(&file.path);
        add(&mut nodes, id.clone(), NodeKind::File, file.path.clone());
        edges.insert(fact(module_id(&parent), id.clone(), EdgeKind::Contains));
        if let Some(entry) = nodes.get_mut(&id) {
            entry
                .attrs
                .insert("bytes".into(), serde_json::json!(file.bytes));
            entry
                .attrs
                .insert("hash".into(), serde_json::json!(file.content_hash));
            entry
                .attrs
                .insert("symbols".into(), serde_json::json!(file.symbols.len()));
        }
        for symbol in &file.symbols {
            if symbols >= SYMBOL_NODES {
                break;
            }
            symbols += 1;
            let symbol_id = format!("sym:{}#{symbol}", file.path);
            add(
                &mut nodes,
                symbol_id.clone(),
                NodeKind::Symbol,
                symbol.clone(),
            );
            edges.insert(fact(id.clone(), symbol_id, EdgeKind::Defines));
        }
        for import in &file.imports {
            match resolve_import(&file.path, import, &paths) {
                Some(target) => {
                    edges.insert(Edge {
                        from: id.clone(),
                        to: file_id(&target),
                        kind: EdgeKind::Imports,
                        certainty: Certainty::Resolved,
                    });
                }
                None => {
                    let package = package_name(import);
                    if package.is_empty() {
                        continue;
                    }
                    let package_id = format!("pkg:{package}");
                    let builtin = is_builtin(&package);
                    add(&mut nodes, package_id.clone(), NodeKind::Package, package);
                    if builtin && let Some(entry) = nodes.get_mut(&package_id) {
                        entry
                            .attrs
                            .insert("builtin".into(), serde_json::json!(true));
                    }
                    edges.insert(Edge {
                        from: id.clone(),
                        to: package_id,
                        kind: EdgeKind::Imports,
                        certainty: Certainty::Named,
                    });
                }
            }
        }
        if let Some(subject) = &file.tests {
            for target in by_stem
                .get(&subject.to_ascii_lowercase())
                .into_iter()
                .flatten()
            {
                if *target != file.path {
                    edges.insert(Edge {
                        from: id.clone(),
                        to: file_id(target),
                        kind: EdgeKind::Tests,
                        certainty: Certainty::Guess,
                    });
                }
            }
        }
    }
    for (number, entry) in work.iter().enumerate() {
        let id = format!("work:{number}");
        add(
            &mut nodes,
            id.clone(),
            NodeKind::Work,
            entry.request.clone(),
        );
        if let Some(work) = nodes.get_mut(&id) {
            work.attrs
                .insert("when".into(), serde_json::json!(entry.when));
        }
        for path in &entry.files {
            // A file written then deleted is still history.
            add(&mut nodes, file_id(path), NodeKind::File, path.clone());
            edges.insert(fact(file_id(path), id.clone(), EdgeKind::ChangedIn));
        }
    }
    for decision in decisions {
        let id = format!("decision:{}", decision.id);
        add(
            &mut nodes,
            id.clone(),
            NodeKind::Decision,
            decision.text.clone(),
        );
        edges.insert(fact("project".into(), id.clone(), EdgeKind::Contains));
        for path in &paths {
            let file_name = path.rsplit('/').next().unwrap_or(path);
            if file_name.len() > 3 && decision.text.contains(file_name) {
                edges.insert(Edge {
                    from: id.clone(),
                    to: file_id(path),
                    kind: EdgeKind::About,
                    certainty: Certainty::Guess,
                });
            }
        }
    }
    for (id, summary) in summaries {
        if let Some(entry) = nodes.get_mut(id) {
            entry
                .attrs
                .insert("summary".into(), serde_json::json!(summary.text));
            entry
                .attrs
                .insert("summaryModel".into(), serde_json::json!(summary.model));
            // A summary is never verified; it is stale when its files changed.
            let current = source_hash(index, id);
            entry.attrs.insert(
                "summaryStale".into(),
                serde_json::json!(current.as_deref() != Some(summary.source_hash.as_str())),
            );
        }
    }
    Graph {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        nodes: nodes.into_values().collect(),
        edges: edges.into_iter().collect(),
    }
}

fn add(nodes: &mut BTreeMap<String, Node>, id: String, kind: NodeKind, label: String) {
    nodes.entry(id.clone()).or_insert(Node {
        id,
        kind,
        label,
        attrs: BTreeMap::new(),
    });
}

fn fact(from: String, to: String, kind: EdgeKind) -> Edge {
    Edge {
        from,
        to,
        kind,
        certainty: Certainty::Fact,
    }
}

/// The hash a summary of `id` is written against: its file's hash, or for a
/// folder the hash of every file under it.
pub fn source_hash(index: &pwr_repo::RepositoryIndex, id: &str) -> Option<String> {
    if let Some(path) = id.strip_prefix("file:") {
        return index
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.content_hash.clone());
    }
    let prefix = match id {
        "project" => String::new(),
        _ => format!("{}/", id.strip_prefix("dir:")?),
    };
    let hashes: Vec<&str> = index
        .files
        .iter()
        .filter(|file| file.path.starts_with(&prefix))
        .map(|file| file.content_hash.as_str())
        .collect();
    (!hashes.is_empty()).then(|| pwr_domain::hash_bytes(hashes.join("\n")))
}

/// The folders worth a summary: those holding source files directly, up to
/// two levels down, largest first.
pub fn summary_candidates(index: &pwr_repo::RepositoryIndex) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in &index.files {
        if file.symbols.is_empty() {
            continue;
        }
        let parts: Vec<&str> = file.path.split('/').collect();
        let depth = (parts.len() - 1).min(2);
        let folder = parts[..depth].join("/");
        *counts.entry(module_id(&folder)).or_default() += 1;
    }
    let mut all: Vec<(String, usize)> = counts.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    all.into_iter().map(|(id, _)| id).collect()
}

/// The same folders, from a built graph: the modules that define symbols.
pub fn summary_candidates_in(graph: &Graph) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for edge in &graph.edges {
        if edge.kind == EdgeKind::Contains
            && (edge.from.starts_with("dir:") || edge.from == "project")
            && edge.to.starts_with("file:")
        {
            *counts.entry(edge.from.as_str()).or_default() += 1;
        }
    }
    let mut all: Vec<(&str, usize)> = counts.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    all.into_iter().map(|(id, _)| id.to_owned()).collect()
}

/// An import as the file in this workspace it names, by the rules of its
/// language: a relative path (JavaScript, TypeScript), `crate::`/`super::`
/// (Rust), a dotted module (Python). Anything else stays a name.
fn resolve_import(from: &str, import: &str, paths: &BTreeSet<&str>) -> Option<String> {
    let dir = from.rsplit_once('/').map_or("", |(dir, _)| dir);
    let exists = |candidate: &str| paths.contains(candidate).then(|| candidate.to_owned());
    const SCRIPT: [&str; 11] = [
        "",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".mjs",
        ".cjs",
        "/index.ts",
        "/index.tsx",
        "/index.js",
        ".py",
    ];
    if import.starts_with("./") || import.starts_with("../") {
        let joined = normalize(&format!("{dir}/{import}"))?;
        return SCRIPT
            .iter()
            .find_map(|ext| exists(&format!("{joined}{ext}")));
    }
    if let Some(rest) = import
        .strip_prefix("crate::")
        .or_else(|| import.strip_prefix("super::"))
    {
        // The crate's `src/`: the nearest `src` above the file.
        let src = match dir.rfind("src") {
            Some(at) => &dir[..at + 3],
            None => return None,
        };
        let base = if import.starts_with("super::") {
            dir.rsplit_once('/').map_or(dir, |(parent, _)| parent)
        } else {
            src
        };
        let segments: Vec<&str> = rest.split("::").collect();
        // `crate::a::b::Thing` may be the module `a/b` or the item `Thing` in `a`.
        for take in (1..=segments.len()).rev() {
            let module = segments[..take].join("/");
            for candidate in [
                format!("{base}/{module}.rs"),
                format!("{base}/{module}/mod.rs"),
            ] {
                if let Some(found) = exists(&candidate) {
                    return Some(found);
                }
            }
        }
        return None;
    }
    if from.ends_with(".py") && !import.contains('/') {
        // `from .exc import X` and `import exc` alike: a module beside the
        // file first, then from the project's root.
        let dotted = import.trim_start_matches('.');
        let module = dotted.replace('.', "/");
        if module.is_empty() {
            return None;
        }
        let beside = if dir.is_empty() {
            module.clone()
        } else {
            format!("{dir}/{module}")
        };
        for base in [beside, module] {
            for candidate in [format!("{base}.py"), format!("{base}/__init__.py")] {
                if let Some(found) = exists(&candidate) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// A language's own library rather than a dependency: Python's standard
/// library, Node's built-in modules, Rust's `std`/`core`/`alloc`. Kept as
/// nodes, marked, and left out of the outline's packages.
pub fn is_builtin(package: &str) -> bool {
    const BUILTIN: [&str; 110] = [
        "__future__",
        "abc",
        "argparse",
        "array",
        "ast",
        "asyncio",
        "base64",
        "bisect",
        "builtins",
        "calendar",
        "cmath",
        "collections",
        "concurrent",
        "configparser",
        "contextlib",
        "copy",
        "csv",
        "ctypes",
        "dataclasses",
        "datetime",
        "decimal",
        "difflib",
        "doctest",
        "email",
        "enum",
        "errno",
        "fnmatch",
        "fractions",
        "functools",
        "gc",
        "getpass",
        "gettext",
        "glob",
        "gzip",
        "hashlib",
        "heapq",
        "hmac",
        "html",
        "http",
        "importlib",
        "inspect",
        "io",
        "ipaddress",
        "itertools",
        "json",
        "locale",
        "logging",
        "math",
        "mimetypes",
        "multiprocessing",
        "numbers",
        "operator",
        "os",
        "pathlib",
        "pickle",
        "platform",
        "pprint",
        "queue",
        "random",
        "re",
        "secrets",
        "select",
        "shlex",
        "shutil",
        "signal",
        "socket",
        "sqlite3",
        "ssl",
        "stat",
        "statistics",
        "string",
        "struct",
        "subprocess",
        "sys",
        "tarfile",
        "tempfile",
        "textwrap",
        "threading",
        "time",
        "timeit",
        "tkinter",
        "traceback",
        "types",
        "typing",
        "unicodedata",
        "unittest",
        "urllib",
        "uuid",
        "warnings",
        "weakref",
        "xml",
        "zipfile",
        "zlib",
        "zoneinfo",
        "assert",
        "buffer",
        "child_process",
        "crypto",
        "events",
        "fs",
        "net",
        "path",
        "process",
        "readline",
        "stream",
        "url",
        "util",
        "std",
        "core",
        "alloc",
    ];
    let package = package.strip_prefix("node:").unwrap_or(package);
    BUILTIN.contains(&package)
}

fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// What a dependency is called: `@angular/core` for `@angular/core/testing`,
/// `serde` for `serde::Deserialize`, `os` for `os.path`.
fn package_name(import: &str) -> String {
    let import = import.trim();
    if import.starts_with('.')
        || import.starts_with("super::")
        || import.starts_with("self::")
        || import.starts_with("crate::")
        || matches!(import, "crate" | "self" | "super")
    {
        return String::new();
    }
    if let Some(scoped) = import.strip_prefix('@') {
        let mut parts = scoped.splitn(3, '/');
        return match (parts.next(), parts.next()) {
            (Some(scope), Some(name)) => format!("@{scope}/{name}"),
            _ => import.to_owned(),
        };
    }
    import
        .split(['/', ':', '.'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

impl Graph {
    /// A node and its neighbours, as text a model or a person can read.
    /// `query` is a path, a folder, a symbol, a package or words of a request;
    /// the best matches are shown, up to three.
    pub fn neighbourhood(&self, query: &str) -> String {
        let wanted = query.trim().trim_end_matches('/').to_lowercase();
        if wanted.is_empty() {
            return self.outline();
        }
        let score = |node: &Node| -> u8 {
            let label = node.label.trim_end_matches('/').to_lowercase();
            let id = node.id.to_lowercase();
            if label == wanted || id.ends_with(&format!(":{wanted}")) {
                4
            } else if label.rsplit('/').next() == Some(wanted.as_str())
                || Path::new(&label).file_stem().and_then(|s| s.to_str()) == Some(wanted.as_str())
            {
                3
            } else if label.contains(&wanted) {
                if node.kind == NodeKind::Symbol { 1 } else { 2 }
            } else {
                0
            }
        };
        let mut matches: Vec<(&Node, u8)> = self
            .nodes
            .iter()
            .map(|node| (node, score(node)))
            .filter(|(_, score)| *score > 0)
            .collect();
        matches.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
        if matches.is_empty() {
            return format!(
                "Nothing in the graph matches `{query}`.\n\n{}",
                self.outline()
            );
        }
        let labels: HashMap<&str, &Node> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let mut lines = Vec::new();
        for (node, _) in matches.iter().take(3) {
            lines.push(format!("## {:?} {}", node.kind, node.label));
            if let Some(summary) = node.attrs.get("summary").and_then(|value| value.as_str()) {
                let stale = node.attrs.get("summaryStale") == Some(&serde_json::json!(true));
                lines.push(format!(
                    "Summary (written by a model, unverified{}): {summary}",
                    if stale {
                        "; its files changed since"
                    } else {
                        ""
                    }
                ));
            }
            let mut out: BTreeMap<(EdgeKind, Certainty), Vec<&str>> = BTreeMap::new();
            let mut into: BTreeMap<(EdgeKind, Certainty), Vec<&str>> = BTreeMap::new();
            for edge in &self.edges {
                if edge.from == node.id {
                    let label = labels
                        .get(edge.to.as_str())
                        .map_or(edge.to.as_str(), |n| &n.label);
                    out.entry((edge.kind, edge.certainty))
                        .or_default()
                        .push(label);
                } else if edge.to == node.id {
                    let label = labels
                        .get(edge.from.as_str())
                        .map_or(edge.from.as_str(), |n| &n.label);
                    into.entry((edge.kind, edge.certainty))
                        .or_default()
                        .push(label);
                }
            }
            for ((kind, certainty), targets) in out {
                lines.push(describe(kind, certainty, false, &targets));
            }
            for ((kind, certainty), sources) in into {
                lines.push(describe(kind, certainty, true, &sources));
            }
            lines.push(String::new());
        }
        if matches.len() > 3 {
            let others: Vec<&str> = matches[3..]
                .iter()
                .take(12)
                .map(|(node, _)| node.label.as_str())
                .collect();
            lines.push(format!("Also matching: {}", others.join(", ")));
        }
        lines.truncate(QUERY_LINES);
        lines.join("\n")
    }

    /// The graph at a glance: counts, the most imported files, the packages.
    pub fn outline(&self) -> String {
        let mut kinds: BTreeMap<NodeKind, usize> = BTreeMap::new();
        for node in &self.nodes {
            *kinds.entry(node.kind).or_default() += 1;
        }
        let mut imported: BTreeMap<&str, usize> = BTreeMap::new();
        for edge in &self.edges {
            if edge.kind == EdgeKind::Imports {
                *imported.entry(edge.to.as_str()).or_default() += 1;
            }
        }
        let mut hubs: Vec<(&str, usize)> = imported
            .into_iter()
            .filter(|(id, _)| id.starts_with("file:"))
            .collect();
        hubs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let packages: Vec<&str> = self
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Package && node.attrs.get("builtin").is_none())
            .map(|node| node.label.as_str())
            .take(30)
            .collect();
        let mut text = format!(
            "{} files, {} folders, {} symbols, {} packages, {} work entries, {} decisions; {} edges.",
            kinds.get(&NodeKind::File).unwrap_or(&0),
            kinds.get(&NodeKind::Module).unwrap_or(&0),
            kinds.get(&NodeKind::Symbol).unwrap_or(&0),
            kinds.get(&NodeKind::Package).unwrap_or(&0),
            kinds.get(&NodeKind::Work).unwrap_or(&0),
            kinds.get(&NodeKind::Decision).unwrap_or(&0),
            self.edges.len()
        );
        if !hubs.is_empty() {
            text.push_str("\nMost imported files: ");
            text.push_str(
                &hubs
                    .iter()
                    .take(10)
                    .map(|(id, count)| format!("{} ({count})", id.trim_start_matches("file:")))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !packages.is_empty() {
            text.push_str(&format!("\nPackages: {}", packages.join(", ")));
        }
        text
    }
}

fn describe(kind: EdgeKind, certainty: Certainty, incoming: bool, others: &[&str]) -> String {
    let verb = match (kind, incoming) {
        (EdgeKind::Contains, false) => "contains",
        (EdgeKind::Contains, true) => "is in",
        (EdgeKind::Defines, false) => "defines",
        (EdgeKind::Defines, true) => "is defined in",
        (EdgeKind::Imports, false) => "imports",
        (EdgeKind::Imports, true) => "is imported by",
        (EdgeKind::Tests, false) => "tests",
        (EdgeKind::Tests, true) => "is tested by",
        (EdgeKind::ChangedIn, false) => "was changed in",
        (EdgeKind::ChangedIn, true) => "changed",
        (EdgeKind::About, false) => "is about",
        (EdgeKind::About, true) => "is named by the decision",
    };
    let how = match certainty {
        Certainty::Fact => "",
        Certainty::Resolved => " (resolved by path)",
        Certainty::Named => " (by name)",
        Certainty::Guess => " (a guess from naming)",
    };
    let shown: Vec<&str> = others.iter().take(25).copied().collect();
    let more = if others.len() > shown.len() {
        format!(" and {} more", others.len() - shown.len())
    } else {
        String::new()
    };
    format!("- {verb}{how}: {}{more}", shown.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str, symbols: &[&str], imports: &[&str]) -> pwr_repo::FileRecord {
        pwr_repo::FileRecord {
            path: path.into(),
            content_hash: format!("h-{path}"),
            bytes: 10,
            symbols: symbols.iter().map(|s| (*s).to_owned()).collect(),
            imports: imports.iter().map(|s| (*s).to_owned()).collect(),
            tests: pwr_repo::test_subject(path),
            terms: Vec::new(),
            mtime_ns: 0,
        }
    }

    #[test]
    fn a_workspace_becomes_a_graph_of_what_it_is_and_what_was_done_to_it() {
        let index = pwr_repo::RepositoryIndex {
            schema_version: 1,
            root: "/w".into(),
            generated_at: chrono::Utc::now(),
            inventory_hash: String::new(),
            files: vec![
                record("src/app/cart.ts", &["Cart"], &["./price", "@angular/core"]),
                record("src/app/price.ts", &["price"], &[]),
                record("src/app/cart.spec.ts", &[], &["./cart"]),
                record(
                    "crate/src/lib.rs",
                    &["run"],
                    &["crate::parser::Token", "serde::Deserialize"],
                ),
                record("crate/src/parser.rs", &["Token"], &[]),
            ],
            stale: false,
        };
        let work = vec![WorkEntry {
            when: "2026-09-25".into(),
            request: "Add a cart".into(),
            files: vec!["src/app/cart.ts".into()],
        }];
        let decisions = vec![DecisionEntry {
            id: "d1".into(),
            text: "Prices are computed in price.ts, never in the template".into(),
        }];
        let graph = build("ebooks", &index, &work, &decisions, &BTreeMap::new());
        let has = |from: &str, to: &str, kind, certainty| {
            graph.edges.contains(&Edge {
                from: from.into(),
                to: to.into(),
                kind,
                certainty,
            })
        };
        assert!(has(
            "dir:src/app",
            "file:src/app/cart.ts",
            EdgeKind::Contains,
            Certainty::Fact
        ));
        assert!(has(
            "file:src/app/cart.ts",
            "sym:src/app/cart.ts#Cart",
            EdgeKind::Defines,
            Certainty::Fact
        ));
        assert!(has(
            "file:src/app/cart.ts",
            "file:src/app/price.ts",
            EdgeKind::Imports,
            Certainty::Resolved
        ));
        assert!(has(
            "file:src/app/cart.ts",
            "pkg:@angular/core",
            EdgeKind::Imports,
            Certainty::Named
        ));
        assert!(has(
            "file:src/app/cart.spec.ts",
            "file:src/app/cart.ts",
            EdgeKind::Tests,
            Certainty::Guess
        ));
        assert!(has(
            "file:crate/src/lib.rs",
            "file:crate/src/parser.rs",
            EdgeKind::Imports,
            Certainty::Resolved
        ));
        assert!(has(
            "file:crate/src/lib.rs",
            "pkg:serde",
            EdgeKind::Imports,
            Certainty::Named
        ));
        assert!(has(
            "file:src/app/cart.ts",
            "work:0",
            EdgeKind::ChangedIn,
            Certainty::Fact
        ));
        assert!(has(
            "decision:d1",
            "file:src/app/price.ts",
            EdgeKind::About,
            Certainty::Guess
        ));

        let answer = graph.neighbourhood("price.ts");
        assert!(
            answer.contains("is imported by (resolved by path): src/app/cart.ts"),
            "{answer}"
        );
        assert!(answer.contains("is named by the decision (a guess from naming)"));
        let cart = graph.neighbourhood("cart");
        assert!(cart.contains("was changed in: Add a cart"), "{cart}");
        assert!(graph.outline().contains("Most imported files: "));

        // A summary is attached as unverified, and stale once its file changes.
        let summary = Summary {
            text: "The shopping cart.".into(),
            source_hash: "h-src/app/cart.ts".into(),
            model: "m".into(),
            at: "t".into(),
        };
        let with = build(
            "ebooks",
            &index,
            &[],
            &[],
            &BTreeMap::from([("file:src/app/cart.ts".to_owned(), summary)]),
        );
        let text = with.neighbourhood("src/app/cart.ts");
        assert!(text.contains("written by a model, unverified): The shopping cart."));
        assert!(!text.contains("changed since"));
        let python = pwr_repo::RepositoryIndex {
            files: vec![
                record("cf/main.py", &["main"], &["exc", "os", "requests", "crate"]),
                record("cf/exc.py", &["InvalidInput"], &[]),
            ],
            ..index.clone()
        };
        let graph = build("cf", &python, &[], &[], &BTreeMap::new());
        let edge = |to: &str, certainty| Edge {
            from: "file:cf/main.py".into(),
            to: to.into(),
            kind: EdgeKind::Imports,
            certainty,
        };
        assert!(
            graph
                .edges
                .contains(&edge("file:cf/exc.py", Certainty::Resolved))
        );
        assert!(
            graph
                .edges
                .contains(&edge("pkg:requests", Certainty::Named))
        );
        let os = graph.nodes.iter().find(|node| node.id == "pkg:os").unwrap();
        assert_eq!(os.attrs.get("builtin"), Some(&serde_json::json!(true)));
        assert!(graph.nodes.iter().all(|node| node.id != "pkg:crate"));
        assert!(graph.outline().contains("Packages: requests"));

        let candidates = summary_candidates(&index);
        assert!(candidates.contains(&"dir:src/app".to_owned()));
        assert!(candidates.contains(&"dir:crate/src".to_owned()));
    }
}
