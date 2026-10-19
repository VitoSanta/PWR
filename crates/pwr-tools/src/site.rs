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
    let regular = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    let directory = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    for base in ["dist", "build", "out"] {
        let mut level: Vec<_> = [root.join(base)]
            .into_iter()
            .filter(|p| directory(p))
            .collect();
        for _ in 0..4 {
            if let Some(found) = level
                .iter()
                .find(|folder| regular(&folder.join("index.html")))
            {
                return Some(found.clone());
            }
            let mut next: Vec<PathBuf> = level
                .iter()
                .filter_map(|folder| std::fs::read_dir(folder).ok())
                .flat_map(|entries| entries.flatten())
                .map(|entry| entry.path())
                .filter(|path| {
                    directory(path) && path.file_name().is_some_and(|name| name != "node_modules")
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
        let folder = folder.canonicalize()?;
        if !folder.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "site root is not a directory",
            ));
        }
        #[cfg(unix)]
        let folder = Arc::new(SiteRoot::open(&folder)?);
        #[cfg(not(unix))]
        let folder = Arc::new(SiteRoot(folder));
        Self::with_root(folder)
    }

    fn start_built(root: &Path, folder: &Path) -> std::io::Result<Self> {
        let relative = folder
            .strip_prefix(root)
            .map_err(|_| std::io::ErrorKind::PermissionDenied)?;
        #[cfg(unix)]
        {
            let pinned = SiteRoot::open(&root.canonicalize()?)?.directory(relative)?;
            Self::with_root(Arc::new(pinned))
        }
        #[cfg(not(unix))]
        {
            let _ = relative;
            let canonical = folder.canonicalize()?;
            if !canonical.starts_with(root.canonicalize()?) {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            }
            Self::start(canonical)
        }
    }

    fn with_root(folder: Arc<SiteRoot>) -> std::io::Result<Self> {
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

// Pin the directory and walk each component relative to an open descriptor.
// No check-then-reopen pathname or symlink traversal occurs during a request.
#[cfg(unix)]
struct SiteRoot(std::fs::File);

#[cfg(unix)]
impl SiteRoot {
    fn open(path: &Path) -> std::io::Result<Self> {
        use rustix::fs::{Mode, OFlags, openat};
        let mut directory = std::fs::File::open("/")?;
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                directory = openat(
                    &directory,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?
                .into();
            }
        }
        Ok(Self(directory))
    }

    fn directory(&self, path: &Path) -> std::io::Result<Self> {
        use rustix::fs::{Mode, OFlags, openat};
        let mut directory = self.0.try_clone()?;
        for component in path.components() {
            let std::path::Component::Normal(name) = component else {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            };
            directory = openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?
            .into();
        }
        Ok(Self(directory))
    }

    fn file(&self, path: &Path) -> std::io::Result<std::fs::File> {
        use rustix::fs::{Mode, OFlags, openat};
        let mut directory = self.0.try_clone()?;
        let mut components = path.components().peekable();
        while let Some(component) = components.next() {
            let std::path::Component::Normal(name) = component else {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            };
            let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
            let flags = if components.peek().is_some() {
                flags | OFlags::DIRECTORY
            } else {
                flags
            };
            let file: std::fs::File = openat(&directory, name, flags, Mode::empty())?.into();
            if components.peek().is_none() {
                return if file.metadata()?.is_file() {
                    Ok(file)
                } else {
                    Err(std::io::ErrorKind::PermissionDenied.into())
                };
            }
            directory = file;
        }
        Err(std::io::ErrorKind::NotFound.into())
    }
}

// Windows qualification is tracked separately; retain its existing path checks.
#[cfg(not(unix))]
struct SiteRoot(PathBuf);
#[cfg(not(unix))]
impl SiteRoot {
    fn file(&self, path: &Path) -> std::io::Result<std::fs::File> {
        let file = self.0.join(path).canonicalize()?;
        if !file.starts_with(&self.0) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        std::fs::File::open(file)
    }
}

fn answer(mut stream: std::net::TcpStream, folder: &SiteRoot) {
    // An accepted socket inherits the listener's non-blocking mode on macOS.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
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
    let requested = if inside && !asked.is_empty() {
        Path::new(&asked)
    } else {
        Path::new("index.html")
    };
    let (name, file) = match folder.file(requested) {
        Ok(file) => (requested, Ok(file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            Path::new("index.html"),
            folder.file(Path::new("index.html")),
        ),
        Err(error) => (requested, Err(error)),
    };
    let Ok(file) = file else {
        let _ = stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let kind = match name.extension().and_then(|extension| extension.to_str()) {
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
    let Ok(metadata) = file.metadata() else {
        return;
    };
    let length = metadata.len();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        length
    );
    let _ = std::io::copy(&mut file.take(length), &mut stream);
}

/// What each page of the built site shows, as text for a reviewer, or `None`
/// when no build left a site or no browser could show it. Never an error: a
/// review goes on without it.
pub async fn shown(policy: &ToolPolicy) -> Option<String> {
    let folder = built_site(&policy.root)?;
    let served = Served::start_built(&policy.root, &folder).ok()?;
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

    #[cfg(unix)]
    #[test]
    fn build_discovery_does_not_leave_the_workspace_through_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("index.html"), "synthetic secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dist")).unwrap();
        assert_eq!(built_site(root.path()), None);
        std::fs::remove_file(root.path().join("dist")).unwrap();
        std::fs::create_dir(root.path().join("dist")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("dist/linked")).unwrap();
        assert_eq!(built_site(root.path()), None);
        std::os::unix::fs::symlink(
            outside.path().join("index.html"),
            root.path().join("dist/index.html"),
        )
        .unwrap();
        assert_eq!(built_site(root.path()), None);
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

    #[cfg(unix)]
    #[test]
    fn external_symlinks_are_never_served_including_the_fallback() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "synthetic secret").unwrap();
        std::fs::write(root.path().join("index.html"), "public page").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            root.path().join("file.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("directory")).unwrap();
        let served = Served::start(root.path().to_path_buf()).unwrap();
        let get = |path: &str| {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", served.port)).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };
        for path in ["/file.txt", "/directory/secret.txt"] {
            let response = get(path);
            assert!(response.starts_with("HTTP/1.1 403"), "{response}");
            assert!(!response.contains("synthetic secret"));
        }
        std::fs::remove_file(root.path().join("index.html")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            root.path().join("index.html"),
        )
        .unwrap();
        for path in ["/", "/route", "/index.html"] {
            let response = get(path);
            assert!(response.starts_with("HTTP/1.1 403"), "{response}");
            assert!(!response.contains("synthetic secret"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn replacing_the_served_directory_cannot_redirect_reads() {
        let root = tempfile::tempdir().unwrap();
        let site = root.path().join("dist");
        let outside = root.path().join("outside");
        std::fs::create_dir(&site).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(site.join("index.html"), "original public page").unwrap();
        std::fs::write(outside.join("index.html"), "synthetic secret").unwrap();
        let served = Served::start(site.clone()).unwrap();
        std::fs::rename(&site, root.path().join("original")).unwrap();
        std::os::unix::fs::symlink(&outside, &site).unwrap();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", served.port)).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        write!(stream, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(!response.contains("synthetic secret"), "{response}");
        assert!(response.ends_with("original public page"), "{response}");
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
