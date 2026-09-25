//! What PWR knows about each workspace it has worked in, and how a
//! conversation anywhere finds it again ("do you remember ebooks?").
//!
//! Per workspace, in `.pwr/wiki/`:
//!
//! - `overview.md`, **computed** from the files, never written by a model:
//!   what the README says it is, its manifests and scripts, its layout. It is
//!   rebuilt after every turn, so it is as current as the workspace.
//! - `log.md`, what was done there, one entry per turn that changed or finished
//!   something: the request, the answer the model gave, the files it wrote.
//!   The answer is the model's own account, and the entry says so.
//!
//! Across workspaces, `~/.pwr/projects.json` lists every workspace with a wiki,
//! by name and path, so a conversation in another folder -- or in chat mode --
//! can ask for one by name ([`recall`]). It reads only the wiki, never the
//! other workspace's files: the policy that confines a conversation to its own
//! workspace is not widened by remembering another.
//!
//! Model-written summaries of modules are not here yet: they are the part
//! that can be wrong, and are to be labelled unverified when they come.

use crate::personal::Home;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The most of a log a recall returns, newest first, in entries.
const RECALLED_ENTRIES: usize = 12;
/// The most entries a log keeps.
const LOG_ENTRIES: usize = 60;
/// The most a recall returns, in characters (about 3,000 tokens).
const RECALL_CHARS: usize = 12_000;
/// The most projects the registry keeps, most recent first.
const PROJECTS: usize = 100;
/// Separates log entries on disk.
const ENTRY: &str = "\n\n### ";

/// One workspace PWR has worked in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub updated_at: String,
    /// One line on what it is, from its README or manifest.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Registry {
    #[serde(default = "first_schema")]
    schema_version: u32,
    #[serde(default)]
    projects: Vec<Project>,
}

fn first_schema() -> u32 {
    1
}

/// What one turn did, for the log.
pub struct Worked<'a> {
    pub request: &'a str,
    pub answer: &'a str,
    pub files: Vec<String>,
}

fn wiki_dir(root: &Path) -> PathBuf {
    root.join(".pwr/wiki")
}

fn name_of(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// Rebuilds the overview, appends `worked` to the log when there is one, and
/// records the workspace in the registry. Best effort: a read-only workspace
/// keeps working without a wiki.
pub fn refresh(home: &Home, root: &Path, worked: Option<Worked<'_>>) -> Result<(), String> {
    let dir = wiki_dir(root);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let (overview, summary) = overview(root);
    write(&dir.join("overview.md"), &overview)?;
    if let Some(worked) = worked {
        append_log(&dir.join("log.md"), &worked)?;
    }
    register(
        home,
        Project {
            name: name_of(root),
            path: root.to_path_buf(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            summary,
        },
    )
}

/// The projects PWR knows, most recently worked on first.
pub fn projects(home: &Home) -> Vec<Project> {
    std::fs::read(home.0.join("projects.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Registry>(&bytes).ok())
        .map(|registry| registry.projects)
        .unwrap_or_default()
}

/// Forgets a project: its entry in the registry, not its folder or its wiki.
pub fn forget(home: &Home, path: &Path) -> Result<(), String> {
    let mut all = projects(home);
    all.retain(|project| project.path != path);
    save_registry(home, all)
}

fn register(home: &Home, project: Project) -> Result<(), String> {
    let mut all = projects(home);
    all.retain(|known| known.path != project.path);
    all.insert(0, project);
    all.truncate(PROJECTS);
    save_registry(home, all)
}

fn save_registry(home: &Home, projects: Vec<Project>) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(&Registry {
        schema_version: 1,
        projects,
    })
    .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&home.0).map_err(|error| error.to_string())?;
    write(
        &home.0.join("projects.json"),
        &String::from_utf8_lossy(&bytes),
    )
}

/// What a conversation is told when it asks about a project: its overview and
/// the latest of its log, or the list of projects when `name` is empty or
/// names none of them.
pub fn recall(home: &Home, name: &str) -> String {
    let all = projects(home);
    let wanted = name.trim().to_lowercase();
    let found = (!wanted.is_empty())
        .then(|| {
            all.iter()
                .find(|project| project.name.to_lowercase() == wanted)
                .or_else(|| {
                    all.iter()
                        .find(|project| project.name.to_lowercase().contains(&wanted))
                })
        })
        .flatten();
    let Some(project) = found else {
        let mut listed = if wanted.is_empty() {
            String::from("Projects PWR has worked on:\n")
        } else {
            format!("No project named `{name}`. Projects PWR has worked on:\n")
        };
        if all.is_empty() {
            listed.push_str("(none yet)\n");
        }
        for project in all.iter().take(40) {
            listed.push_str(&format!(
                "- {} ({}), last worked on {}{}\n",
                project.name,
                project.path.display(),
                project.updated_at.get(..10).unwrap_or_default(),
                if project.summary.is_empty() {
                    String::new()
                } else {
                    format!(": {}", project.summary)
                }
            ));
        }
        return listed;
    };
    let dir = wiki_dir(&project.path);
    let overview = std::fs::read_to_string(dir.join("overview.md")).unwrap_or_else(|_| {
        format!(
            "# {}\n\n(The overview is no longer on disk.)\n",
            project.name
        )
    });
    let log = std::fs::read_to_string(dir.join("log.md")).unwrap_or_default();
    let mut entries: Vec<&str> = log.split(ENTRY).skip(1).collect();
    entries.reverse();
    let mut text = format!(
        "{}\n\n## Work done there, newest first\n\
         Each entry is the request and the answer the model gave at the time -- its own \
         account, not a check. The workspace is `{}`; its files are not readable from here.\n",
        overview.trim_end(),
        project.path.display()
    );
    if entries.is_empty() {
        text.push_str("\n(nothing recorded yet)\n");
    }
    for entry in entries.iter().take(RECALLED_ENTRIES) {
        let next = format!("\n### {}\n", entry.trim_end());
        if text.len() + next.len() > RECALL_CHARS {
            text.push_str("\n(older entries omitted)\n");
            break;
        }
        text.push_str(&next);
    }
    text
}

/// The workspace as its files describe it, and a one-line summary.
fn overview(root: &Path) -> (String, String) {
    let name = name_of(root);
    let mut text = format!(
        "# {name}\n\nComputed by PWR from the workspace's files at {}; nothing here was written \
         by a model.\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
    );
    let mut summary = String::new();
    if let Some(readme) = ["README.md", "README", "readme.md", "README.txt"]
        .iter()
        .find_map(|file| std::fs::read_to_string(root.join(file)).ok())
    {
        let paragraph = first_paragraph(&readme);
        if !paragraph.is_empty() {
            summary = one_line(&paragraph, 160);
            text.push_str(&format!(
                "\n## From the README\n{}\n",
                clip(&paragraph, 800)
            ));
        }
    }
    let manifests = manifests(root);
    if !manifests.is_empty() {
        text.push_str("\n## Manifests\n");
        for (file, lines) in &manifests {
            text.push_str(&format!("- `{file}`\n"));
            for line in lines {
                text.push_str(&format!("  - {line}\n"));
                if summary.is_empty()
                    && let Some(description) = line.strip_prefix("description: ")
                {
                    summary = one_line(description, 160);
                }
            }
        }
    }
    let layout = layout(root);
    if !layout.is_empty() {
        text.push_str("\n## Layout\n");
        for line in layout {
            text.push_str(&format!("- {line}\n"));
        }
    }
    if let Some((path, _)) = crate::personal::project_instructions(root) {
        text.push_str(&format!(
            "\n## Instructions\nThe project's own instructions are in `{path}`.\n"
        ));
    }
    (text, summary)
}

fn manifests(root: &Path) -> Vec<(String, Vec<String>)> {
    let mut found = Vec::new();
    if let Ok(text) = std::fs::read_to_string(root.join("package.json"))
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let mut lines = Vec::new();
        for key in ["name", "description"] {
            if let Some(value) = json[key].as_str().filter(|value| !value.is_empty()) {
                lines.push(format!("{key}: {}", one_line(value, 200)));
            }
        }
        if let Some(scripts) = json["scripts"].as_object() {
            let names: Vec<&str> = scripts.keys().map(String::as_str).take(20).collect();
            lines.push(format!("scripts: {}", names.join(", ")));
        }
        let mut dependencies: Vec<&str> = json["dependencies"]
            .as_object()
            .map(|all| all.keys().map(String::as_str).take(25).collect())
            .unwrap_or_default();
        dependencies.sort_unstable();
        if !dependencies.is_empty() {
            lines.push(format!("dependencies: {}", dependencies.join(", ")));
        }
        found.push(("package.json".to_owned(), lines));
    }
    for (file, keys) in [
        ("Cargo.toml", &["name", "description", "members"][..]),
        ("pyproject.toml", &["name", "description"][..]),
    ] {
        if let Ok(text) = std::fs::read_to_string(root.join(file)) {
            let lines: Vec<String> = text
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once('=')?;
                    let key = key.trim();
                    keys.contains(&key).then(|| {
                        format!("{key}: {}", one_line(value.trim().trim_matches('"'), 200))
                    })
                })
                .take(6)
                .collect();
            found.push((file.to_owned(), lines));
        }
    }
    for file in [
        "go.mod",
        "requirements.txt",
        "Gemfile",
        "pom.xml",
        "build.gradle",
        "Makefile",
    ] {
        if root.join(file).is_file() {
            found.push((file.to_owned(), Vec::new()));
        }
    }
    found
}

/// The top level, with the number of files under each folder.
fn layout(root: &Path) -> Vec<String> {
    const SKIPPED: [&str; 8] = [
        "node_modules",
        "target",
        "dist",
        "build",
        "__pycache__",
        "venv",
        ".venv",
        "vendor",
    ];
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
                return None;
            }
            let path = entry.path();
            Some(if path.is_dir() {
                format!("`{name}/` ({} files)", count_files(&path, 0))
            } else {
                format!("`{name}`")
            })
        })
        .collect();
    lines.sort();
    lines.truncate(40);
    lines
}

fn count_files(dir: &Path, depth: usize) -> usize {
    if depth > 6 {
        return 0;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if entry.file_name().to_string_lossy().starts_with('.') {
                        0
                    } else if path.is_dir() {
                        count_files(&path, depth + 1)
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

fn append_log(path: &Path, worked: &Worked<'_>) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut entries: Vec<String> = existing.split(ENTRY).skip(1).map(str::to_owned).collect();
    let mut entry = format!(
        "{} -- {}\n{}\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
        one_line(worked.request, 140),
        clip(worked.answer.trim(), 900)
    );
    if !worked.files.is_empty() {
        let mut files = worked.files.clone();
        files.sort();
        files.dedup();
        entry.push_str(&format!("Files written: {}\n", files.join(", ")));
    }
    entries.push(entry);
    let skip = entries.len().saturating_sub(LOG_ENTRIES);
    let mut text = String::from(
        "# Work log\n\nWritten by PWR after each turn that changed or finished something: the \
         request, and the answer the model gave -- its own account, not a verification.",
    );
    for entry in entries.iter().skip(skip) {
        text.push_str(ENTRY);
        text.push_str(entry.trim_end());
    }
    text.push('\n');
    write(path, &text)
}

fn first_paragraph(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .find(|paragraph| {
            !paragraph.is_empty()
                && !paragraph.starts_with('#')
                && !paragraph.starts_with('[')
                && !paragraph.starts_with('<')
                && !paragraph.starts_with('!')
        })
        .unwrap_or_default()
        .to_owned()
}

fn one_line(text: &str, limit: usize) -> String {
    clip(
        &text.split_whitespace().collect::<Vec<_>>().join(" "),
        limit,
    )
}

fn clip(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    let staged = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&staged, text).map_err(|error| error.to_string())?;
    std::fs::rename(&staged, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_worked_on_in_one_folder_is_recalled_from_anywhere() {
        let folder = tempfile::tempdir().unwrap();
        let home = Home(folder.path().join("home"));
        let ebooks = folder.path().join("ebooks");
        std::fs::create_dir_all(ebooks.join("src")).unwrap();
        std::fs::write(
            ebooks.join("README.md"),
            "# Ebooks\n\nA small shop for selling ebooks, built with Angular.\n",
        )
        .unwrap();
        std::fs::write(
            ebooks.join("package.json"),
            r#"{"name":"ebooks","scripts":{"build":"ng build","test":"ng test"},"dependencies":{"@angular/core":"^22"}}"#,
        )
        .unwrap();
        std::fs::write(ebooks.join("src/main.ts"), "").unwrap();

        refresh(
            &home,
            &ebooks,
            Some(Worked {
                request: "Build the catalogue page",
                answer: "Added the catalogue page with search and a cart.",
                files: vec!["src/catalogue.ts".into(), "src/main.ts".into()],
            }),
        )
        .unwrap();
        refresh(&home, &ebooks, None).unwrap();

        let known = projects(&home);
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].name, "ebooks");
        assert_eq!(
            known[0].summary,
            "A small shop for selling ebooks, built with Angular."
        );

        let recalled = recall(&home, "Ebooks");
        assert!(recalled.contains("A small shop for selling ebooks"));
        assert!(recalled.contains("scripts: build, test"));
        assert!(recalled.contains("`src/` (1 files)"));
        assert!(recalled.contains("Build the catalogue page"));
        assert!(recalled.contains("Files written: src/catalogue.ts, src/main.ts"));

        // An unknown or empty name lists what there is.
        assert!(recall(&home, "shop-that-never-was").contains("No project named"));
        assert!(recall(&home, "").contains("- ebooks ("));

        forget(&home, &ebooks).unwrap();
        assert!(projects(&home).is_empty());
        // The wiki itself stays with the workspace.
        assert!(ebooks.join(".pwr/wiki/log.md").is_file());
    }
}
