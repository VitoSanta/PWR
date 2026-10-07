//! What a built web site shows a visitor, read from a browser.
//!
//! A review that reads code against a request cannot see what the page
//! displays. Seen 2026-10-07 (gpt-oss 20B, an Angular site for an architecture
//! studio, built and building): the generator's `Hello, studio-lineare`
//! heading stayed at the foot of every page, the request having said "no
//! placeholder text of the Angular template", and both the model and the
//! reviewer marked that rule met -- the heading was in `app.html`, which the
//! model read as a shell and not as a page. Opened in a browser it is the
//! largest text on the screen.
//!
//! So the built site is served from this machine for the length of the look,
//! each of its routes is opened with [`crate::look_at`], and what a visitor
//! reads there is handed to the review beside the code.

use crate::{Approval, ToolPolicy, look_at};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Routes opened besides the first page.
const ROUTES: usize = 5;
/// Visible text kept for each page.
const PAGE_CHARS: usize = 1_500;
/// Console messages kept for each page.
const CONSOLE_LINES: usize = 5;

/// The folder a build left a site in: the shallowest `index.html` under
/// `dist/`, `build/` or `out/`.
pub fn built_site(root: &Path) -> Option<PathBuf> {
    for base in ["dist", "build", "out"] {
        let mut level = vec![root.join(base)];
        for _ in 0..4 {
            if let Some(found) = level
                .iter()
                .find(|folder| folder.join("index.html").is_file())
            {
                return Some(found.clone());
            }
            let mut next: Vec<PathBuf> = level
                .iter()
                .filter_map(|folder| std::fs::read_dir(folder).ok())
                .flat_map(|entries| entries.flatten())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_dir() && path.file_name().is_some_and(|name| name != "node_modules")
                })
                .collect();
            next.sort();
            level = next;
        }
    }
    None
}

/// The site's own links to its pages, as its source writes them:
/// `routerLink="/progetti"`, `href="/contatti"`, `to="/about"`. The first
/// page is always there and comes first.
pub fn routes(root: &Path) -> Vec<String> {
    let mut found = vec!["/".to_owned()];
    let mut folders = vec![root.join("src")];
    let mut read = 0usize;
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let source = matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("ts" | "tsx" | "js" | "jsx" | "html" | "vue" | "svelte")
            );
            if !source || read >= 300 {
                continue;
            }
            read += 1;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (attribute, bare_is_route) in [
                ("routerLink=\"", true),
                ("href=\"", false),
                ("to=\"", false),
            ] {
                for piece in text.split(attribute).skip(1) {
                    let Some(value) = piece.split('"').next() else {
                        continue;
                    };
                    let plain = !value.is_empty()
                        && value
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "/-_".contains(c))
                        && !value.starts_with("//");
                    if !plain || !(value.starts_with('/') || bare_is_route) {
                        continue;
                    }
                    let route = format!("/{}", value.trim_matches('/'));
                    if !found.contains(&route) {
                        found.push(route);
                    }
                }
            }
        }
    }
    found.truncate(1 + ROUTES);
    found
}

/// Serves `folder` on a port of this machine until dropped: a file where the
/// path names one, the site's `index.html` for every other path, as a site
/// with client-side routes is served.
pub(crate) struct Served {
    pub(crate) port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Served {
    pub(crate) fn start(folder: PathBuf) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                };
                let folder = folder.clone();
                std::thread::spawn(move || answer(stream, &folder));
            }
        });
        Ok(Self {
            port,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn answer(mut stream: std::net::TcpStream, folder: &Path) {
    // An accepted socket inherits the listener's non-blocking mode on macOS.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut request = [0u8; 4096];
    let read = stream.read(&mut request).unwrap_or(0);
    let line = String::from_utf8_lossy(&request[..read]);
    let asked = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_owned();
    let inside = !asked.split('/').any(|part| part == "..");
    let file = Some(folder.join(&asked))
        .filter(|path| inside && !asked.is_empty() && path.is_file())
        .unwrap_or_else(|| folder.join("index.html"));
    let kind = match file.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    let body = std::fs::read(&file).unwrap_or_default();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(&body);
}

/// What each page of the built site shows, as text for a reviewer, or `None`
/// when no build left a site or no browser could show it. Never an error: a
/// review goes on without it.
pub async fn shown(policy: &ToolPolicy) -> Option<String> {
    let folder = built_site(&policy.root)?;
    let served = Served::start(folder).ok()?;
    let mut looking = policy.clone();
    if !looking.approvals.contains(&Approval::LocalService) {
        looking.approvals.push(Approval::LocalService);
    }
    let mut report = String::new();
    for route in routes(&policy.root) {
        let url = format!("http://127.0.0.1:{}{route}", served.port);
        let Ok(page) = look_at(&looking, &url, None, None).await else {
            continue;
        };
        let text: String = page.text.chars().take(PAGE_CHARS).collect();
        report.push_str(&format!(
            "--- page {route} (title: {}) ---\n{}\n",
            page.title.as_deref().unwrap_or("none"),
            text.trim()
        ));
        if !page.console.is_empty() {
            let lines: Vec<&str> = page
                .console
                .iter()
                .take(CONSOLE_LINES)
                .map(String::as_str)
                .collect();
            report.push_str(&format!("console: {}\n", lines.join(" | ")));
        }
    }
    (!report.is_empty()).then_some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_site_is_the_shallowest_index_under_a_build_folder() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(built_site(root.path()), None);
        let site = root.path().join("dist/studio/browser");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::create_dir_all(root.path().join("dist/node_modules/x")).unwrap();
        std::fs::write(root.path().join("dist/node_modules/x/index.html"), "").unwrap();
        std::fs::write(site.join("index.html"), "<html>").unwrap();
        assert_eq!(built_site(root.path()), Some(site));
    }

    #[test]
    fn routes_are_read_from_the_site_s_own_links() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src/app")).unwrap();
        std::fs::write(
            root.path().join("src/app/navbar.ts"),
            r#"<a routerLink="/">Home</a> <a routerLink="progetti">P</a>
               <a href="/contatti">C</a> <a href="https://example.com">out</a>
               <a href="mailto:x">m</a> <a href="/contatti">again</a>"#,
        )
        .unwrap();
        assert_eq!(routes(root.path()), vec!["/", "/progetti", "/contatti"]);
        assert_eq!(routes(&root.path().join("nothing")), vec!["/"]);
    }

    #[tokio::test]
    async fn what_a_built_site_shows_is_read_page_by_page() {
        if crate::browser_executable().is_none() {
            eprintln!("PWR-SKIP what_a_built_site_shows_is_read_page_by_page no browser");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("dist")).unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("dist/index.html"),
            "<!doctype html><title>Studio</title><h1>Hello, studio-lineare</h1>\
             <script>document.body.append(location.pathname)</script>",
        )
        .unwrap();
        std::fs::write(
            root.path().join("src/nav.html"),
            r#"<a routerLink="/progetti">P</a>"#,
        )
        .unwrap();
        let mut policy = crate::PolicyProfile::Safe.build(root.path().to_path_buf());
        policy.timeout = std::time::Duration::from_secs(60);
        let Some(report) = shown(&policy).await else {
            // The host's browser wrote nothing (a runner with no display).
            eprintln!(
                "PWR-SKIP what_a_built_site_shows_is_read_page_by_page the browser showed nothing"
            );
            return;
        };
        assert!(
            report.contains("--- page / (title: Studio) ---"),
            "{report}"
        );
        assert!(report.contains("Hello, studio-lineare"), "{report}");
        assert!(report.contains("--- page /progetti"), "{report}");
        assert!(
            report.contains("/progetti\n") || report.contains("/progetti"),
            "{report}"
        );
    }

    /// Seen 2026-10-07 (Qwen 3.6, a page on a local server): every step
    /// answered "Cannot read properties of null", the page being of another
    /// origin than PWR's own; a field named under `click`, an option of a
    /// list and a question that opens were all asked for and none worked.
    #[tokio::test]
    async fn a_page_on_a_local_server_is_acted_on_as_a_person_would() {
        const NAME: &str = "a_page_on_a_local_server_is_acted_on_as_a_person_would";
        if crate::browser_executable().is_none() {
            eprintln!("PWR-SKIP {NAME} no browser");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("index.html"),
            "<!doctype html><title>Aurora</title><div style=\"height:3000px\">sky</div>\
             <details><summary>A che ora inizia?</summary><p>Alle ventuno.</p></details>\
             <form id=f><label>Nome e Cognome <input name=nome></label>\
             <select name=persone><option value=\"\">Scegli</option><option value=3>3 persone</option></select>\
             <button type=submit>Invia richiesta</button></form><p id=out></p>\
             <button aria-label=\"Cambia tema\"><svg></svg></button>\
             <script>f.addEventListener('submit', (e) => { e.preventDefault(); \
             out.textContent = 'Grazie ' + f.nome.value + ', in ' + f.persone.value; });\
             document.querySelector('[aria-label]').onclick = () => document.body.append(' tema chiaro');</script>",
        )
        .unwrap();
        let served = Served::start(root.path().to_path_buf()).unwrap();
        let mut policy = crate::PolicyProfile::Safe.build(root.path().to_path_buf());
        policy.timeout = std::time::Duration::from_secs(60);
        policy.approvals.push(Approval::LocalService);
        let steps: Vec<crate::PageStep> = serde_json::from_str(
            r#"[{"click": "A che ora inizia?"},
                {"click": "Nome e Cognome", "type": "Mario Rossi"},
                {"click": "3 persone"},
                {"click": "Invia richiesta"},
                {"click": "Cambia tema"},
                {"click": "Non esiste"}]"#,
        )
        .unwrap();
        let url = format!("http://127.0.0.1:{}/", served.port);
        let look = match crate::look_at_after(&policy, &url, None, None, &steps).await {
            Ok(look) => look,
            Err(error) if error.to_string().contains("could not be captured") => {
                eprintln!("PWR-SKIP {NAME} the browser showed nothing");
                return;
            }
            Err(error) => panic!("{error}"),
        };
        for said in [
            "step 1: clicked \"A che ora inizia?\"",
            "step 2: typed into \"Nome e Cognome\"",
            "step 3: chose \"3 persone\"",
            "step 6: nothing to click matches \"Non esiste\". The page has: \"A che ora inizia?\"",
            "Alle ventuno.",
            "Grazie Mario Rossi, in 3",
            "tema chiaro",
        ] {
            assert!(
                look.text.contains(said),
                "missing {said:?} in:\n{}",
                look.text
            );
        }
        assert_eq!(look.title.as_deref(), Some("Aurora"));
    }

    #[test]
    fn a_route_with_no_file_is_answered_with_the_index_and_nothing_above_the_site_is() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("secret.txt"), "outside").unwrap();
        let site = root.path().join("dist");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), "the page").unwrap();
        std::fs::write(site.join("main.js"), "the script").unwrap();
        let served = Served::start(site).unwrap();
        let get = |path: &str| {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", served.port)).unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut answer = String::new();
            stream.read_to_string(&mut answer).unwrap();
            answer
        };
        assert!(get("/main.js").ends_with("the script"));
        assert!(get("/progetti").ends_with("the page"));
        assert!(get("/../secret.txt").ends_with("the page"));
    }
}
