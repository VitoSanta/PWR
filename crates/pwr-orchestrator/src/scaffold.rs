//! A project built in a new folder of a workspace that held no project yet,
//! moved into the workspace root.
//!
//! The folder the person opened is the project. Models put a new project in a
//! folder named after it, two ways, both measured on 2026-10-02 (Nemotron 3.5
//! Lightning 30B in the desktop, the owner's Libra workspaces) with the system
//! prompt saying since 2026-10-01 that a new project goes in the root:
//!
//! - a generator: `npm exec npm create next-app@latest libro-ecommerce`.
//!   Pointing one at `.` is often no way out here: create-next-app refuses a
//!   folder that holds `.pwr/`, and one whose name is not a valid npm name
//!   (`Libra`);
//! - by hand, in a fresh empty folder: `make_directory libro-ecommerce`,
//!   `mkdir -p ./libro-ecommerce/src/...`, then every file written under it
//!   and `cd ./libro-ecommerce && npm install` -- no generator involved.
//!
//! So, while the root holds no project (no manifest in it or in a folder
//! directly under it), the first folder that becomes one -- a command leaves a
//! manifest in it ([`move_to_root`]), or a manifest is about to be written in
//! it ([`adopt_for_write`]) -- is taken as the root: what it holds moves up and
//! the folder goes. Not when the person's request names the folder (a
//! subfolder they asked for stays), when two folders became projects at once,
//! or when a name in it is already taken at the root. Paths the model still
//! writes under the old folder are then read as the root's
//! ([`redirect_action`]), in a command's script too: told the files had
//! moved, it could otherwise recreate the folder one write at a time.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use pwr_tools::ActionProposal;

/// Root entries that are PWR's or the operating system's, never the project.
const NOT_THE_PROJECT: [&str; 5] = [
    ".pwr",
    pwr_tools::SCRATCH_DIRECTORY,
    pwr_tools::TOOLCHAINS_DIRECTORY,
    ".DS_Store",
    "Thumbs.db",
];

/// Whether a file name says its folder is a project: a build system's marker,
/// or a manifest no check is derived from.
fn is_manifest(name: &str) -> bool {
    pwr_verify::BUILD_SYSTEMS
        .iter()
        .any(|system| system.marker == name)
        || matches!(name, "package.json" | "deno.json" | "manage.py")
        || [".csproj", ".fsproj", ".sln"]
            .iter()
            .any(|extension| name.ends_with(extension))
}

fn holds_a_manifest(folder: &Path) -> bool {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && is_manifest(&entry.file_name().to_string_lossy())
        })
}

/// The root's entries, when the root holds no project yet; `None` when it
/// does, or cannot be read.
pub fn entries_without_project(root: &Path) -> Option<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().ok()?;
        if !NOT_THE_PROJECT.contains(&name.as_str())
            && ((kind.is_file() && is_manifest(&name))
                || (kind.is_dir() && holds_a_manifest(&entry.path())))
        {
            return None;
        }
        names.insert(name);
    }
    Some(names)
}

/// Whether the person's request names the folder, as a word of its own.
fn named(folder: &str, request: &[String]) -> bool {
    let folder = folder.to_lowercase();
    request.iter().any(|text| {
        text.to_lowercase()
            .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
            .any(|word| word.trim_matches('.') == folder)
    })
}

/// A folder taken as the workspace root, and the entries it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub folder: String,
    pub entries: Vec<String>,
}

impl Moved {
    /// What the model is told, beside the action's own result.
    pub fn notice(&self) -> String {
        let shown: Vec<String> = self
            .entries
            .iter()
            .take(8)
            .map(|entry| format!("`{entry}`"))
            .collect();
        let held = if shown.is_empty() {
            String::new()
        } else {
            format!(
                " It moved what `{}/` held into the root, which now holds {}{}.",
                self.folder,
                shown.join(", "),
                if self.entries.len() > shown.len() {
                    ", ..."
                } else {
                    ""
                },
            )
        };
        format!(
            "This workspace is the project, so PWR treats `{folder}/` as the workspace root: \
             the folder is gone and paths under it are read as the root's.{held} Use paths from \
             the root, without `{folder}/`, and run the project's commands without cwd or `cd`.",
            folder = self.folder,
        )
    }
}

/// Moves what a folder holds into the root and removes it. `Ok(None)` when a
/// name in it is already taken at the root -- a README beside the new one, or
/// the folder's own name (`mysite/mysite`) -- since nothing is overwritten;
/// `Err` when moving failed and was undone.
fn hoist(root: &Path, folder: &str) -> Result<Option<Vec<String>>, String> {
    let source = root.join(folder);
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&source).map_err(|error| error.to_string())? {
        let Ok(entry) = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .into_string()
        else {
            return Ok(None);
        };
        if root.join(&entry).symlink_metadata().is_ok() {
            return Ok(None);
        }
        entries.push(entry);
    }
    entries.sort();
    let mut done: Vec<&String> = Vec::new();
    for entry in &entries {
        if let Err(error) = std::fs::rename(source.join(entry), root.join(entry)) {
            for back in done.iter().rev() {
                let _ = std::fs::rename(root.join(back), source.join(back));
            }
            return Err(format!(
                "PWR tried to move `{folder}/` into the workspace root, where the project \
                 belongs, and could not ({error}); it is still in `{folder}/`."
            ));
        }
        done.push(entry);
    }
    let _ = std::fs::remove_dir(&source);
    Ok(Some(entries))
}

/// After a command that succeeded in a root that held no project before it:
/// the one folder that now holds a manifest, moved into the root. `Ok(None)`
/// when there is none, more than one, the root became the project itself, or
/// the move would not be clean; `Err` when moving failed and was undone, with
/// what to tell the model.
pub fn move_to_root(root: &Path, request: &[String]) -> Result<Option<Moved>, String> {
    let Ok(listing) = std::fs::read_dir(root) else {
        return Ok(None);
    };
    let mut projects = Vec::new();
    for entry in listing.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if NOT_THE_PROJECT.contains(&name.as_str()) {
            continue;
        }
        if kind.is_file() && is_manifest(&name) {
            return Ok(None);
        }
        if kind.is_dir() && holds_a_manifest(&entry.path()) {
            projects.push(name);
        }
    }
    let [folder] = projects.as_slice() else {
        return Ok(None);
    };
    if named(folder, request) {
        return Ok(None);
    }
    Ok(hoist(root, folder)?.map(|entries| Moved {
        folder: folder.clone(),
        entries,
    }))
}

/// A path written relative to the root, whichever way it was written:
/// `./a/b`, or absolute inside the root. `None` for a path elsewhere.
fn root_relative<'a>(root: &Path, path: &'a Path) -> Option<Vec<&'a str>> {
    let relative = if path.is_absolute() {
        let canonical = root.canonicalize().ok();
        path.strip_prefix(root).ok().or_else(|| {
            canonical
                .as_deref()
                .and_then(|root| path.strip_prefix(root).ok())
        })?
    } else {
        path
    };
    relative
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect()
}

/// Before a manifest is written in a folder directly under a root that holds
/// no project (`shop/package.json`), the folder taken as the root: what it
/// already holds moves up, and the write is then read as the root's.
/// `Ok(None)` when the path is anything else or the move would not be clean.
pub fn adopt_for_write(
    root: &Path,
    path: &str,
    request: &[String],
) -> Result<Option<Moved>, String> {
    let Some(parts) = root_relative(root, Path::new(path.trim())) else {
        return Ok(None);
    };
    let [folder, file] = parts.as_slice() else {
        return Ok(None);
    };
    if folder.starts_with('.')
        || NOT_THE_PROJECT.contains(folder)
        || !is_manifest(file)
        || named(folder, request)
        || entries_without_project(root).is_none()
    {
        return Ok(None);
    }
    let place = root.join(folder);
    let entries = match place.symlink_metadata() {
        Err(_) => Vec::new(),
        Ok(kind) if kind.is_dir() => match hoist(root, folder)? {
            Some(entries) => entries,
            None => return Ok(None),
        },
        Ok(_) => return Ok(None),
    };
    Ok(Some(Moved {
        folder: (*folder).to_owned(),
        entries,
    }))
}

/// A path under a folder moved into the root, read as the root's:
/// `shop/app/page.tsx` is `app/page.tsx` once `shop/` is gone, written
/// relative or absolute. `None` when it is not under one, or the folder
/// exists again.
pub fn redirect(root: &Path, moved: &[String], path: &str) -> Option<String> {
    let parts = root_relative(root, Path::new(path.trim()))?;
    let (first, rest) = parts.split_first()?;
    if !moved.iter().any(|folder| folder == first) || root.join(first).symlink_metadata().is_ok() {
        return None;
    }
    let rest: PathBuf = rest.iter().collect();
    Some(if rest.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        rest.to_string_lossy().into_owned()
    })
}

/// Where a word can begin in a shell script.
fn starts_a_word(before: Option<char>) -> bool {
    before.is_none_or(|c| c.is_whitespace() || "'\"=(:;&|".contains(c))
}

/// Where a path component can end in a shell script.
fn ends_a_component(after: Option<char>) -> bool {
    after.is_none_or(|c| c.is_whitespace() || "/'\";&|)".contains(c))
}

/// A shell script's paths under a moved folder, read as the root's:
/// `cd ./shop && npm install` runs `cd . && npm install`, `ls shop/src` runs
/// `ls ./src`, and the folder's absolute path is the root's. A bare word
/// that is not a path -- `"name": "shop"` -- is left alone, except after `cd`.
fn redirect_script(script: &str, root: &Path, moved: &[String]) -> Option<String> {
    let mut text = script.to_owned();
    let bases: Vec<String> = [Some(root.to_path_buf()), root.canonicalize().ok()]
        .into_iter()
        .flatten()
        .map(|base| base.to_string_lossy().into_owned())
        .collect();
    for folder in moved {
        if root.join(folder).symlink_metadata().is_ok() {
            continue;
        }
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        for (at, _) in text.match_indices(folder.as_str()) {
            if at < copied {
                continue;
            }
            let end = at + folder.len();
            if !ends_a_component(text[end..].chars().next()) {
                continue;
            }
            let before = &text[copied..at];
            let previous = |prefix: &str| {
                before.strip_suffix(prefix).and_then(|kept| {
                    let start = copied + kept.len();
                    starts_a_word(text[..start].chars().next_back()).then_some(start)
                })
            };
            let absolute = bases
                .iter()
                .find_map(|base| previous(&format!("{base}/")).map(|start| (start, base.clone())));
            let (start, replacement) = if let Some((start, base)) = absolute {
                (start, base)
            } else if let Some(start) = previous("./") {
                (start, ".".to_owned())
            } else if starts_a_word(text[..at].chars().next_back())
                && (text[end..].starts_with('/')
                    || text[..at]
                        .trim_end()
                        .rsplit(|c: char| c.is_whitespace() || ";&|(".contains(c))
                        .next()
                        == Some("cd"))
            {
                (at, ".".to_owned())
            } else {
                continue;
            };
            out.push_str(&text[copied..start]);
            out.push_str(&replacement);
            copied = end;
        }
        out.push_str(&text[copied..]);
        text = out;
    }
    (text != script).then_some(text)
}

const SHELLS: [&str; 4] = ["sh", "bash", "zsh", "dash"];

/// Rewrites every path of an action still under a moved folder, and says
/// which: each `(as written, as read)`.
pub fn redirect_action(
    action: &mut ActionProposal,
    root: &Path,
    moved: &[String],
) -> Vec<(String, String)> {
    let mut changed = Vec::new();
    if moved.is_empty() {
        return changed;
    }
    let fix = |path: &mut String, changed: &mut Vec<(String, String)>| {
        if let Some(now) = redirect(root, moved, path) {
            changed.push((std::mem::replace(path, now.clone()), now));
        }
    };
    match action {
        ActionProposal::ApplyPatchHunks { path, .. }
        | ActionProposal::MakeDirectory { path }
        | ActionProposal::DeletePath { path, .. }
        | ActionProposal::RestoreFile { path }
        | ActionProposal::ExtractDocument { path }
        | ActionProposal::ReadFile { path, .. }
        | ActionProposal::ApplyReplace { path, .. }
        | ActionProposal::WriteFile { path, .. }
        | ActionProposal::ReplaceText { path, .. }
        | ActionProposal::ListTree {
            path: Some(path), ..
        }
        | ActionProposal::FetchUrl {
            save_as: Some(path),
            ..
        }
        | ActionProposal::Search {
            path_glob: Some(path),
            ..
        }
        | ActionProposal::FindDefinition {
            path_glob: Some(path),
            ..
        } => fix(path, &mut changed),
        ActionProposal::MovePath { from, to } => {
            fix(from, &mut changed);
            fix(to, &mut changed);
        }
        ActionProposal::VcsDiff { paths } => {
            for path in paths {
                fix(path, &mut changed);
            }
        }
        ActionProposal::RunCommand {
            executable,
            args,
            cwd,
            ..
        } => {
            if let Some(cwd) = cwd {
                fix(cwd, &mut changed);
            }
            let program = executable.rsplit('/').next().unwrap_or(executable);
            let script = SHELLS
                .contains(&program)
                .then(|| args.iter().position(|arg| arg == "-c").map(|flag| flag + 1));
            for (index, arg) in args.iter_mut().enumerate() {
                if script == Some(Some(index)) {
                    if let Some(now) = redirect_script(arg, root, moved) {
                        changed.push((std::mem::replace(arg, now.clone()), now));
                    }
                } else if !moved.contains(arg) {
                    // A bare name is not rewritten: `create-next-app shop`
                    // means the folder, and `.` would not do the same.
                    fix(arg, &mut changed);
                }
            }
        }
        _ => {}
    }
    changed
}

/// Paths recorded under a moved folder, keyed as the root's from now on.
pub fn rekey<V>(map: &mut std::collections::BTreeMap<String, V>, folder: &str) {
    let prefix = format!("{folder}/");
    let stale: Vec<String> = map
        .keys()
        .filter(|path| path.starts_with(&prefix))
        .cloned()
        .collect();
    for path in stale {
        if let Some(value) = map.remove(&path) {
            map.insert(path[prefix.len()..].to_owned(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".pwr")).unwrap();
        std::fs::write(dir.path().join(".pwr/chat-config.json"), "{}").unwrap();
        dir
    }

    /// What `create-next-app libro-ecommerce` leaves, in miniature.
    fn generate(root: &Path, folder: &str) {
        let project = root.join(folder);
        std::fs::create_dir_all(project.join("app")).unwrap();
        std::fs::create_dir_all(project.join("node_modules/next")).unwrap();
        std::fs::write(project.join("package.json"), "{\"name\":\"shop\"}").unwrap();
        std::fs::write(project.join("app/page.tsx"), "export default 1;").unwrap();
        std::fs::write(project.join(".gitignore"), "node_modules\n").unwrap();
    }

    #[test]
    fn a_generated_project_moves_into_an_empty_workspace() {
        let dir = workspace();
        assert!(entries_without_project(dir.path()).is_some());
        // The sandbox's scratch folder appears with the command; it is PWR's.
        std::fs::create_dir(dir.path().join(pwr_tools::SCRATCH_DIRECTORY)).unwrap();
        generate(dir.path(), "libro-ecommerce");
        let moved = move_to_root(
            dir.path(),
            &["Voglio creare un e-commerce di libri con Next.js".into()],
        )
        .unwrap()
        .expect("the project was not moved");
        assert_eq!(moved.folder, "libro-ecommerce");
        assert!(dir.path().join("package.json").is_file());
        assert!(dir.path().join("app/page.tsx").is_file());
        assert!(dir.path().join("node_modules/next").is_dir());
        assert!(dir.path().join(".gitignore").is_file());
        assert!(!dir.path().join("libro-ecommerce").exists());
        assert!(dir.path().join(".pwr/chat-config.json").is_file());
        let notice = moved.notice();
        assert!(notice.contains("without `libro-ecommerce/`"), "{notice}");
        assert!(notice.contains("`package.json`"), "{notice}");
    }

    /// The folder was made first, by hand; a later command (`npm init`) made
    /// it a project.
    #[test]
    fn a_folder_a_command_makes_a_project_moves_too() {
        let dir = workspace();
        std::fs::create_dir_all(dir.path().join("shop/src")).unwrap();
        std::fs::write(dir.path().join("shop/src/index.ts"), "1").unwrap();
        assert!(entries_without_project(dir.path()).is_some());
        std::fs::write(dir.path().join("shop/package.json"), "{}").unwrap();
        let moved = move_to_root(dir.path(), &[]).unwrap().expect("not moved");
        assert_eq!(moved.entries, vec!["package.json", "src"]);
        assert!(dir.path().join("src/index.ts").is_file());
    }

    #[test]
    fn a_workspace_that_holds_a_project_keeps_its_new_subfolder() {
        let dir = workspace();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert!(entries_without_project(dir.path()).is_none());
        let dir = workspace();
        std::fs::create_dir(dir.path().join("backend")).unwrap();
        std::fs::write(dir.path().join("backend/go.mod"), "module api\n").unwrap();
        assert!(
            entries_without_project(dir.path()).is_none(),
            "a project in a folder directly under the root is a project"
        );
    }

    #[test]
    fn a_folder_the_request_names_stays() {
        let dir = workspace();
        generate(dir.path(), "frontend");
        let request = ["Crea il frontend in Angular nella cartella frontend.".to_owned()];
        assert_eq!(move_to_root(dir.path(), &request).unwrap(), None);
        assert!(dir.path().join("frontend/package.json").is_file());
        assert_eq!(
            adopt_for_write(dir.path(), "backend/go.mod", &["un backend".into()]).unwrap(),
            None
        );
    }

    #[test]
    fn nothing_is_moved_over_a_name_the_root_already_has() {
        let dir = workspace();
        std::fs::write(dir.path().join(".gitignore"), "mine\n").unwrap();
        generate(dir.path(), "shop");
        assert_eq!(move_to_root(dir.path(), &[]).unwrap(), None);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "mine\n"
        );
        assert!(dir.path().join("shop/package.json").is_file());
    }

    #[test]
    fn a_folder_without_a_manifest_or_two_projects_stay() {
        let dir = workspace();
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join("notes/todo.md"), "-").unwrap();
        assert_eq!(move_to_root(dir.path(), &[]).unwrap(), None);
        generate(dir.path(), "shop");
        generate(dir.path(), "admin");
        assert_eq!(move_to_root(dir.path(), &[]).unwrap(), None, "two projects");
        let dir = workspace();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        generate(dir.path(), "shop");
        assert_eq!(
            move_to_root(dir.path(), &[]).unwrap(),
            None,
            "the root became the project itself"
        );
    }

    /// Measured 2026-10-02 in a fresh empty folder: `make_directory
    /// libro-ecommerce`, `mkdir -p ./libro-ecommerce/src/...`, then
    /// `write_file ./libro-ecommerce/package.json`.
    #[test]
    fn a_manifest_written_in_a_new_folder_makes_it_the_root() {
        let dir = workspace();
        std::fs::create_dir_all(dir.path().join("libro-ecommerce/src/app/api/books")).unwrap();
        std::fs::create_dir_all(dir.path().join("libro-ecommerce/prisma")).unwrap();
        let moved = adopt_for_write(dir.path(), "./libro-ecommerce/package.json", &[])
            .unwrap()
            .expect("not adopted");
        assert_eq!(moved.entries, vec!["prisma", "src"]);
        assert!(dir.path().join("src/app/api/books").is_dir());
        assert!(!dir.path().join("libro-ecommerce").exists());
        // Not made yet at all: only the name is taken as the root.
        let dir = workspace();
        let absolute = dir.path().join("shop/Cargo.toml");
        let moved = adopt_for_write(dir.path(), &absolute.to_string_lossy(), &[])
            .unwrap()
            .expect("not adopted");
        assert_eq!((moved.folder.as_str(), moved.entries.len()), ("shop", 0));
        assert!(
            moved
                .notice()
                .contains("treats `shop/` as the workspace root")
        );
    }

    #[test]
    fn only_a_manifest_directly_in_a_folder_of_an_empty_root_adopts_it() {
        let dir = workspace();
        for path in [
            "shop/src/index.ts",
            "shop/web/package.json",
            "package.json",
            ".config/package.json",
        ] {
            assert_eq!(
                adopt_for_write(dir.path(), path, &[]).unwrap(),
                None,
                "{path}"
            );
        }
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(
            adopt_for_write(dir.path(), "web/package.json", &[]).unwrap(),
            None,
            "the root is a project already"
        );
    }

    #[test]
    fn a_path_under_the_moved_folder_is_read_as_the_roots() {
        let dir = workspace();
        let moved = vec!["libro-ecommerce".to_owned()];
        let root = dir.path();
        assert_eq!(
            redirect(root, &moved, "libro-ecommerce/app/page.tsx").as_deref(),
            Some("app/page.tsx")
        );
        assert_eq!(
            redirect(root, &moved, "./libro-ecommerce").as_deref(),
            Some(".")
        );
        let absolute = root.join("libro-ecommerce/prisma/schema.prisma");
        assert_eq!(
            redirect(root, &moved, &absolute.to_string_lossy()).as_deref(),
            Some("prisma/schema.prisma")
        );
        let canonical = root
            .canonicalize()
            .unwrap()
            .join("libro-ecommerce/components");
        assert_eq!(
            redirect(root, &moved, &canonical.to_string_lossy()).as_deref(),
            Some("components")
        );
        assert_eq!(redirect(root, &moved, "app/page.tsx"), None);
        assert_eq!(redirect(root, &moved, "/elsewhere/libro-ecommerce/x"), None);
        // Made again on purpose: then it is a folder like any other.
        std::fs::create_dir(root.join("libro-ecommerce")).unwrap();
        assert_eq!(redirect(root, &moved, "libro-ecommerce/app/page.tsx"), None);
    }

    #[test]
    fn a_script_runs_in_the_root_where_it_named_the_moved_folder() {
        let dir = workspace();
        let root = dir.path();
        let moved = vec!["libro-ecommerce".to_owned()];
        let read = |script: &str| redirect_script(script, root, &moved);
        assert_eq!(
            read("cd ./libro-ecommerce && npm install next-auth").as_deref(),
            Some("cd . && npm install next-auth")
        );
        assert_eq!(read("cd libro-ecommerce; ls").as_deref(), Some("cd .; ls"));
        assert_eq!(
            read("mkdir -p ./libro-ecommerce/src/app libro-ecommerce/prisma").as_deref(),
            Some("mkdir -p ./src/app ./prisma")
        );
        let absolute = format!("ls -la {}/libro-ecommerce/src", root.display());
        assert_eq!(
            read(&absolute),
            Some(format!("ls -la {}/src", root.display()))
        );
        assert_eq!(read(r#"echo '{"name": "libro-ecommerce"}' > x.json"#), None);
        assert_eq!(read("ls my-libro-ecommerce/x"), None);
        assert_eq!(read("npm install"), None);
    }

    #[test]
    fn an_action_into_the_moved_folder_lands_in_the_root() {
        let dir = workspace();
        let moved = vec!["shop".to_owned()];
        let mut write = ActionProposal::WriteFile {
            path: "shop/app/api/books/route.ts".into(),
            content: "export {}".into(),
        };
        assert_eq!(
            redirect_action(&mut write, dir.path(), &moved),
            vec![(
                "shop/app/api/books/route.ts".to_owned(),
                "app/api/books/route.ts".to_owned()
            )]
        );
        assert!(matches!(
            write,
            ActionProposal::WriteFile { ref path, .. } if path == "app/api/books/route.ts"
        ));
        let mut command = ActionProposal::RunCommand {
            executable: "npm".into(),
            args: vec!["run".into(), "build".into(), "shop".into()],
            stdin: None,
            cwd: Some("shop".into()),
            outside_sandbox: false,
        };
        redirect_action(&mut command, dir.path(), &moved);
        assert!(matches!(
            command,
            ActionProposal::RunCommand { cwd: Some(ref cwd), ref args, .. }
                if cwd == "." && args[2] == "shop"
        ));
        let mut shell = ActionProposal::RunCommand {
            executable: "sh".into(),
            args: vec!["-c".into(), "cd ./shop && npm run build".into()],
            stdin: None,
            cwd: None,
            outside_sandbox: false,
        };
        assert_eq!(redirect_action(&mut shell, dir.path(), &moved).len(), 1);
        assert!(matches!(
            shell,
            ActionProposal::RunCommand { ref args, .. } if args[1] == "cd . && npm run build"
        ));
        let mut rename = ActionProposal::MovePath {
            from: "shop/a.ts".into(),
            to: "shop/b.ts".into(),
        };
        assert_eq!(redirect_action(&mut rename, dir.path(), &moved).len(), 2);
    }

    #[test]
    fn recorded_paths_follow_the_move() {
        let mut changed = std::collections::BTreeMap::from([
            ("shop/app/page.tsx".to_owned(), "h1".to_owned()),
            ("shopping.md".to_owned(), "h2".to_owned()),
        ]);
        rekey(&mut changed, "shop");
        assert_eq!(
            changed.keys().collect::<Vec<_>>(),
            vec!["app/page.tsx", "shopping.md"]
        );
    }
}
