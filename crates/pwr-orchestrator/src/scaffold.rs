//! A project a generator built in a new folder of a workspace that held no
//! project yet, moved into the workspace root.
//!
//! The folder the person opened is the project. A generator names a folder
//! after the project -- `create-next-app shop`, `ng new web`, `cargo new api`
//! -- and pointing it at `.` is often no way out in a PWR workspace:
//! create-next-app refuses a folder that holds `.pwr/`, and one whose name is
//! not a valid npm name (`Libra`). Measured 2026-10-02 (Nemotron 3.5
//! Lightning 30B in the desktop, the Libra workspace): with the system prompt
//! saying since 2026-10-01 that a new project goes in the root, the model ran
//! `npm exec npm create next-app@latest libro-ecommerce` and built everything
//! in `libro-ecommerce/`.
//!
//! So a conversation lets the generator run and moves what it made, only
//! when: the root held no project before the command (no manifest in it or in
//! a folder directly under it), the command created exactly one folder and
//! that folder holds a manifest, none of its names is already taken at the
//! root, and the person's request does not name the folder (a subfolder they
//! asked for stays). Paths the model still writes under the old folder are
//! then read as the root's ([`redirect_action`]): told the files had moved,
//! it could otherwise recreate the folder one write at a time.

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

/// The root's entries before a command, when the root holds no project yet;
/// `None` when it does, or cannot be read.
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

/// A project folder moved into the root, and the entries it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub folder: String,
    pub entries: Vec<String>,
}

impl Moved {
    /// What the model is told, beside the command's own result.
    pub fn notice(&self) -> String {
        let shown: Vec<String> = self
            .entries
            .iter()
            .take(8)
            .map(|entry| format!("`{entry}`"))
            .collect();
        format!(
            "This workspace is the project, so PWR moved everything the command created in \
             `{folder}/` into the workspace root and removed the empty folder. The root now \
             holds {shown}{more}. Use paths from the root, without `{folder}/`, and run the \
             project's commands without cwd.",
            folder = self.folder,
            shown = shown.join(", "),
            more = if self.entries.len() > shown.len() {
                ", ..."
            } else {
                ""
            },
        )
    }
}

/// After a command that succeeded, the one project folder it created at a root
/// that held no project, moved into the root. `Ok(None)` when there is none or
/// the move would not be clean; `Err` when moving failed and was undone, with
/// what to tell the model.
pub fn move_to_root(
    root: &Path,
    before: &BTreeSet<String>,
    request: &[String],
) -> Result<Option<Moved>, String> {
    let Ok(listing) = std::fs::read_dir(root) else {
        return Ok(None);
    };
    let created: Vec<std::fs::DirEntry> = listing
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            !before.contains(&name) && !NOT_THE_PROJECT.contains(&name.as_str())
        })
        .collect();
    let [folder] = created.as_slice() else {
        return Ok(None);
    };
    let Ok(name) = folder.file_name().into_string() else {
        return Ok(None);
    };
    let source = folder.path();
    if !folder.file_type().is_ok_and(|kind| kind.is_dir())
        || !holds_a_manifest(&source)
        || named(&name, request)
    {
        return Ok(None);
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&source).map_err(|error| error.to_string())? {
        let Ok(entry) = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .into_string()
        else {
            return Ok(None);
        };
        // A name the root already has -- a README beside the new one, or
        // the folder's own name (`mysite/mysite`) -- is not overwritten.
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
                "PWR tried to move `{name}/` into the workspace root, where the project belongs, \
                 and could not ({error}); it is still in `{name}/`."
            ));
        }
        done.push(entry);
    }
    let _ = std::fs::remove_dir(&source);
    Ok(Some(Moved {
        folder: name,
        entries,
    }))
}

/// A path under a folder moved into the root, read as the root's:
/// `shop/app/page.tsx` is `app/page.tsx` once `shop/` is gone, written
/// relative or absolute. `None` when it is not under one, or the folder
/// exists again.
pub fn redirect(root: &Path, moved: &[String], path: &str) -> Option<String> {
    let path = Path::new(path.trim());
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
    let mut components = relative
        .components()
        .filter(|component| !matches!(component, Component::CurDir));
    let Some(Component::Normal(first)) = components.next() else {
        return None;
    };
    let first = first.to_str()?;
    if !moved.iter().any(|folder| folder == first) || root.join(first).symlink_metadata().is_ok() {
        return None;
    }
    let rest: PathBuf = components.collect();
    Some(if rest.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        rest.to_string_lossy().into_owned()
    })
}

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
    let mut fix = |path: &mut String| {
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
        | ActionProposal::RunCommand {
            cwd: Some(path), ..
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
        } => fix(path),
        ActionProposal::MovePath { from, to } => {
            fix(from);
            fix(to);
        }
        ActionProposal::VcsDiff { paths } => paths.iter_mut().for_each(fix),
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
        let before = entries_without_project(dir.path()).expect("an empty workspace");
        // The sandbox's scratch folder appears with the command; it is PWR's.
        std::fs::create_dir(dir.path().join(pwr_tools::SCRATCH_DIRECTORY)).unwrap();
        generate(dir.path(), "libro-ecommerce");
        let moved = move_to_root(
            dir.path(),
            &before,
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
        let before = entries_without_project(dir.path()).unwrap();
        generate(dir.path(), "frontend");
        let request = ["Crea il frontend in Angular nella cartella frontend.".to_owned()];
        assert_eq!(move_to_root(dir.path(), &before, &request).unwrap(), None);
        assert!(dir.path().join("frontend/package.json").is_file());
    }

    #[test]
    fn nothing_is_moved_over_a_name_the_root_already_has() {
        let dir = workspace();
        std::fs::write(dir.path().join(".gitignore"), "mine\n").unwrap();
        let before = entries_without_project(dir.path()).unwrap();
        generate(dir.path(), "shop");
        assert_eq!(move_to_root(dir.path(), &before, &[]).unwrap(), None);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "mine\n"
        );
        assert!(dir.path().join("shop/package.json").is_file());
    }

    #[test]
    fn a_folder_without_a_manifest_or_two_new_folders_stay() {
        let dir = workspace();
        let before = entries_without_project(dir.path()).unwrap();
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join("notes/todo.md"), "-").unwrap();
        assert_eq!(move_to_root(dir.path(), &before, &[]).unwrap(), None);
        generate(dir.path(), "shop");
        assert_eq!(
            move_to_root(dir.path(), &before, &[]).unwrap(),
            None,
            "two new entries"
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
            args: vec!["run".into(), "build".into()],
            stdin: None,
            cwd: Some("shop".into()),
            outside_sandbox: false,
        };
        redirect_action(&mut command, dir.path(), &moved);
        assert!(matches!(
            command,
            ActionProposal::RunCommand { cwd: Some(ref cwd), .. } if cwd == "."
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
