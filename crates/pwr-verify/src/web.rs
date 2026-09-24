//! A deterministic check for a workspace whose product is a page.
//!
//! Measured on the campaign of 2026-09-06: a run wrote `index.html`,
//! `style.css` and `main.js`, and the HTML asked for `script.js`. Every
//! generic signal said the work was sound -- seven sections present, the
//! selected terms from the source all found, `node --check` green, no duplicate
//! ids, no broken internal anchors -- and the delivered page rendered its
//! header and then 6,500 pixels of nothing, because 27 blocks were left at
//! `opacity: 0` by a stylesheet whose script never loaded. On a phone it had no
//! navigation either: the menu is moved off-screen by CSS and only the missing
//! script brings it back.
//!
//! What separates that page from a working one is a single asset reference, and
//! no check in this crate could see it: every verifier here is a command a
//! toolchain provides, and a static site has no toolchain. So the workspace
//! that most needs a check is the one that has none, and the run either invents
//! confidence or cannot complete at all.
//!
//! This is the narrowest check that would have caught it: every local thing a
//! page asks the browser to load must exist. It proves nothing about how the
//! page looks and does not run a browser. It only refuses to call a page
//! finished while it points at a file that is not there.

use std::path::{Path, PathBuf};

/// The reserved name this check is discovered and recorded under.
///
/// Not an executable, and deliberately unspellable as one: it must never reach
/// the command allowlist or a sandboxed process. The colon is what keeps a
/// repository from declaring it by accident and the runner from trying to
/// spawn it.
pub const WEB_ASSETS_CHECK: &str = "pwr:web-assets";

/// Files searched for references. Kept to markup: a stylesheet's `url()` and a
/// script's own fetches are real references too, but resolving them needs a CSS
/// parser and a module graph, and a check that is sometimes wrong about what a
/// page needs is worse than one that is always right about part of it.
const MARKUP_EXTENSIONS: &[&str] = &["html", "htm"];

/// Attributes whose value is a URL the browser will fetch.
const URL_ATTRIBUTES: &[&str] = &["src", "href", "poster", "data-src"];

/// Bound on the walk, so a workspace with a build output directory cannot turn
/// a check into a scan of everything.
const MAX_MARKUP_FILES: usize = 400;

/// One reference that will not load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAsset {
    /// Markup file holding the reference, relative to the workspace root.
    pub source: String,
    /// One-based line the reference is on.
    pub line: usize,
    /// The attribute and value exactly as written.
    pub reference: String,
    /// Why it will not load.
    pub reason: &'static str,
}

/// True when this workspace has markup worth checking.
pub fn has_markup(root: &Path) -> bool {
    !markup_files(root).is_empty()
}

/// Every local reference in the workspace's markup that will not load.
pub fn missing_assets(root: &Path) -> Vec<MissingAsset> {
    let mut findings = Vec::new();
    for file in markup_files(root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let relative = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        let directory = file.parent().unwrap_or(root).to_path_buf();
        for (index, line) in text.lines().enumerate() {
            for (attribute, value) in url_attributes(line) {
                let Some(target) = local_target(&value) else {
                    continue;
                };
                let resolved = if let Some(rooted) = target.strip_prefix('/') {
                    root.join(rooted)
                } else {
                    directory.join(&target)
                };
                let inside = within(root, &resolved);
                // Angular copies configured public assets to the served root.
                // In src/index.html, favicon.svg therefore resolves to
                // public/favicon.svg even though it is absent beside index.html.
                let resolved = if inside && !resolved.exists() {
                    angular_public_target(root, &file, &target)
                        .filter(|path| path.exists())
                        .unwrap_or(resolved)
                } else {
                    resolved
                };
                let reason = if !inside {
                    "resolves outside the workspace"
                } else if !resolved.exists() {
                    "no such file in the workspace"
                } else if std::fs::metadata(&resolved).is_ok_and(|m| m.len() == 0) {
                    // Existing was the whole test, and a zero-byte file exists.
                    // Measured on the ornith-1.5:35b run of 2026-09-07: its last
                    // action emptied `js/main.js`, the page lost every animated
                    // block, and this check stayed green because the reference
                    // still resolved. A page that loads nothing from a file it
                    // asks for is in the same position as one whose file is
                    // missing, so it is reported the same way.
                    "resolves to an empty file"
                } else {
                    continue;
                };
                findings.push(MissingAsset {
                    source: relative.clone(),
                    line: index + 1,
                    reference: format!("{attribute}=\"{value}\""),
                    reason,
                });
            }
        }
    }
    findings.sort_by(|a, b| (&a.source, a.line).cmp(&(&b.source, b.line)));
    findings
}

fn angular_public_target(root: &Path, file: &Path, target: &str) -> Option<PathBuf> {
    let mut project = file.parent()?;
    loop {
        let config = project.join("angular.json");
        if let Ok(text) = std::fs::read_to_string(config) {
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let configured = json["projects"].as_object()?.values().any(|project| {
                project["architect"]["build"]["options"]["assets"]
                    .as_array()
                    .is_some_and(|assets| assets.iter().any(|asset| asset["input"] == "public"))
            });
            if configured && file.starts_with(project.join("src")) {
                let public = project.join("public");
                let path = public.join(target.trim_start_matches('/'));
                return (within(root, &path) && within(&public, &path)).then_some(path);
            }
        }
        if project == root {
            break;
        }
        project = project.parent()?;
        if !project.starts_with(root) {
            break;
        }
    }
    None
}

/// The check's output, in the shape a failing command would have written it.
///
/// One line per finding, located, because recovery reads this text to find the
/// file and line it must edit.
pub fn report(findings: &[MissingAsset]) -> String {
    if findings.is_empty() {
        return "every local reference in the workspace's markup resolves\n".to_string();
    }
    let mut out = String::new();
    for finding in findings {
        out.push_str(&format!(
            "{}:{}: {} {}\n",
            finding.source, finding.line, finding.reference, finding.reason
        ));
    }
    out.push_str(&format!(
        "{} local reference(s) will not load\n",
        findings.len()
    ));
    out
}

/// Markup files below `root`, skipping directories no page is served from.
fn markup_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `symlink_metadata`, so a link into the wider filesystem is a link
            // and not the directory it points at.
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if metadata.is_dir() {
                if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if MARKUP_EXTENSIONS.contains(&extension.as_str()) {
                found.push(path);
            }
            if found.len() >= MAX_MARKUP_FILES {
                found.sort();
                return found;
            }
        }
    }
    found.sort();
    found
}

/// Attribute name and raw value for every URL-bearing attribute on `line`.
fn url_attributes(line: &str) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for attribute in URL_ATTRIBUTES {
        let mut rest = line;
        while let Some(at) = rest.find(attribute) {
            let after = &rest[at + attribute.len()..];
            // The name must be whole and followed by `=`: `data-srcset` is not
            // `data-src`, and `href` inside a word is not an attribute.
            let before_is_boundary = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || c == '<');
            let value = after.strip_prefix('=').and_then(quoted_value);
            if let (true, Some(value)) = (before_is_boundary, value) {
                found.push((*attribute, value));
            }
            rest = &rest[at + attribute.len()..];
        }
    }
    found
}

/// The quoted string at the start of `text`, if there is one.
fn quoted_value(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// The workspace-relative path a reference points at, or `None` when it is not
/// this workspace's problem.
///
/// Everything a browser resolves somewhere else is skipped rather than guessed
/// at: another origin, a fragment on this page, an inline payload, a scheme
/// that opens a mail client. A check that flagged those would be wrong far more
/// often than the defect it exists to catch.
fn local_target(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('#') || value.starts_with("//") {
        return None;
    }
    let lowered = value.to_lowercase();
    if lowered.contains("://")
        || ["data:", "mailto:", "tel:", "javascript:", "blob:", "sms:"]
            .iter()
            .any(|scheme| lowered.starts_with(scheme))
    {
        return None;
    }
    let path = value
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if path.is_empty() {
        return None;
    }
    Some(percent_decoded(&path))
}

/// Decodes `%XX` so a file written with a space is looked for by its real name.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Whether `candidate` stays inside `root` once `..` segments are folded.
///
/// Textual, because the file usually does not exist -- that is the thing being
/// reported -- and `canonicalize` needs it to.
fn within(root: &Path, candidate: &Path) -> bool {
    let mut depth: i64 = 0;
    for component in candidate
        .strip_prefix(root)
        .unwrap_or(candidate)
        .components()
    {
        match component {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::CurDir => {}
            _ => depth += 1,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delivered page from the campaign, reduced to what broke it.
    #[test]
    fn the_defect_the_campaign_shipped_is_caught() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("index.html"),
            "<link rel=\"stylesheet\" href=\"style.css\">\n<script src=\"script.js\" defer></script>\n",
        )
        .unwrap();
        std::fs::write(root.path().join("style.css"), "body{}").unwrap();
        std::fs::write(root.path().join("main.js"), "//").unwrap();

        let findings = missing_assets(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line, 2);
        assert!(findings[0].reference.contains("script.js"));
        assert!(report(&findings).contains("index.html:2"));

        // The fix the verification measured: one rename, and the check is green.
        std::fs::rename(root.path().join("main.js"), root.path().join("script.js")).unwrap();
        assert!(missing_assets(root.path()).is_empty());
    }

    /// What the browser resolves elsewhere is not this check's business. A
    /// check that cried wolf on every external link would be turned off.
    #[test]
    fn references_this_workspace_does_not_own_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("page.html"),
            concat!(
                "<a href=\"https://example.com/x.css\">a</a>\n",
                "<a href=\"//cdn.example.com/x.js\">b</a>\n",
                "<a href=\"#contatti\">c</a>\n",
                "<a href=\"mailto:someone@example.com\">d</a>\n",
                "<img src=\"data:image/png;base64,AAAA\">\n",
                "<a href=\"cv.pdf?v=2#page=1\">e</a>\n",
            ),
        )
        .unwrap();
        std::fs::write(root.path().join("cv.pdf"), "%PDF-1.4").unwrap();
        assert!(missing_assets(root.path()).is_empty());
    }

    #[test]
    fn a_reference_out_of_the_workspace_is_reported_not_followed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("pages")).unwrap();
        std::fs::write(
            root.path().join("pages/a.html"),
            "<img src=\"../../secrets.png\">\n<img src=\"../logo.png\">\n",
        )
        .unwrap();
        std::fs::write(root.path().join("logo.png"), "x").unwrap();
        let findings = missing_assets(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].reason, "resolves outside the workspace");
    }

    #[test]
    fn a_percent_encoded_name_is_looked_for_by_its_real_name() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("i.html"), "<img src=\"my%20photo.png\">").unwrap();
        std::fs::write(root.path().join("my photo.png"), "x").unwrap();
        assert!(missing_assets(root.path()).is_empty());
    }

    /// A file that exists and holds nothing is not a file the page can use.
    #[test]
    fn a_reference_to_an_empty_file_is_reported() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("index.html"),
            "<script src=\"main.js\"></script>",
        )
        .unwrap();
        std::fs::write(root.path().join("main.js"), "").unwrap();
        let findings = missing_assets(root.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].reason, "resolves to an empty file");
    }

    #[test]
    fn a_workspace_without_markup_offers_no_check() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), "fn main() {}").unwrap();
        assert!(!has_markup(root.path()));
    }

    #[test]
    fn angular_public_assets_resolve_from_the_served_root() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("site");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::create_dir_all(project.join("public")).unwrap();
        std::fs::write(
            project.join("angular.json"),
            r#"{"projects":{"site":{"architect":{"build":{"options":{"assets":[{"glob":"**/*","input":"public"}]}}}}}}"#,
        )
        .unwrap();
        std::fs::write(
            project.join("src/index.html"),
            "<link rel=\"icon\" href=\"favicon.svg\">",
        )
        .unwrap();
        std::fs::write(project.join("public/favicon.svg"), "<svg/>").unwrap();
        assert!(missing_assets(root.path()).is_empty());
        std::fs::remove_file(project.join("public/favicon.svg")).unwrap();
        assert_eq!(missing_assets(root.path()).len(), 1);
    }
}
