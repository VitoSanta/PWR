//! Persistent visible browser tabs, isolated from the application's IPC.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Webview, WebviewUrl,
};

#[derive(Default)]
pub struct Browser(pub Arc<Mutex<BrowserData>>);
#[derive(Default)]
pub struct BrowserData {
    tabs: HashMap<String, Tab>,
    pending: HashMap<String, (String, mpsc::Sender<Value>)>,
}
struct Tab {
    url: String,
    visible: bool,
}
impl Browser {
    pub fn close_all(&self, app: &AppHandle) {
        let ids = if let Ok(mut data) = self.0.lock() {
            data.pending.clear();
            data.tabs.drain().map(|(id, _)| id).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for id in ids {
            if let Ok(name) = label(&id) {
                if let Some(view) = app.get_webview(&name) {
                    let _ = view.close();
                }
            }
        }
    }
}
fn label(id: &str) -> Result<String, String> {
    if id.is_empty() || id.len() > 40 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid browser tab.".into());
    }
    Ok(format!("browser-{id}"))
}
fn owner(caller: &Webview) -> Result<(), String> {
    if caller.label() == "main" {
        Ok(())
    } else {
        Err("Only PWR controls browser tabs.".into())
    }
}
pub fn address(raw: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(raw).map_err(|why| why.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || matches!(url.host_str(), Some("tauri.localhost" | "ipc.localhost"))
        || (cfg!(debug_assertions)
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
            && url.port_or_known_default() == Some(4200))
    {
        return Err("Use an HTTP or HTTPS website outside PWR's own interface.".into());
    }
    Ok(url)
}
#[tauri::command]
pub async fn browser_open(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Browser>,
    id: String,
    url: String,
) -> Result<(), String> {
    owner(&caller)?;
    let name = label(&id)?;
    let url = address(&url)?;
    if let Some(view) = app.get_webview(&name) {
        view.navigate(url.clone()).map_err(|why| why.to_string())?;
        if let Some(tab) = state
            .0
            .lock()
            .map_err(|_| "Browser state unavailable")?
            .tabs
            .get_mut(&id)
        {
            tab.url = url.to_string();
        }
        return Ok(());
    }
    if state
        .0
        .lock()
        .map_err(|_| "Browser state unavailable")?
        .tabs
        .len()
        >= 12
    {
        return Err("Close a browser tab before opening another.".into());
    }
    let shared = state.0.clone();
    let tab_id = id.clone();
    let events = app.clone();
    let loaded_id = id.clone();
    let loaded_events = app.clone();
    let loaded_state = state.0.clone();
    let title_id = id.clone();
    let title_events = app.clone();
    let builder = tauri::webview::WebviewBuilder::new(&name, WebviewUrl::External(url.clone()))
        // Without Safari's name, sites such as Google serve their basic, legacy pages.
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15")
        .on_navigation(move |url| {
            if url.scheme() == "pwr-browser" {
                let token = url.path().trim_start_matches('/');
                if let Ok(mut data) = shared.lock() {
                    if data.pending.get(token).is_some_and(|(id,_)| id == &tab_id) {
                        if let Some((_,sender)) = data.pending.remove(token) {
                            if let Some((_,value)) = url.query_pairs().find(|(key,_)|key=="result") {
                                if value.len() <= 160000 { if let Ok(value)=serde_json::from_str::<Value>(&value) { let _=sender.send(value); } }
                            }
                        }
                    }
                }
                return false;
            }
            if address(url.as_str()).is_err() { return false; }
            true
        })
        .on_page_load(move |view, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                if let Ok(url) = view.url() {
                    if address(url.as_str()).is_ok() {
                        if let Ok(mut data) = loaded_state.lock() { if let Some(tab)=data.tabs.get_mut(&loaded_id) { tab.url=url.to_string(); } }
                        let _ = events.emit("browser-navigation", json!({"id":loaded_id,"url":url.as_str()}));
                    }
                }
            }
            let _=loaded_events.emit("browser-loaded",json!({"id":loaded_id,"url":payload.url().as_str(),"finished":matches!(payload.event(),tauri::webview::PageLoadEvent::Finished)}));
        })
        .on_document_title_changed(move |_, title| {
            let title: String = title.chars().take(160).collect();
            let _ = title_events.emit("browser-title", json!({"id": title_id, "title": title}));
        })
        .on_new_window(|_,_| tauri::webview::NewWindowResponse::Deny);
    let window = app.get_window("main").ok_or("PWR window is closed")?;
    let view = window
        .add_child(
            builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(1.0, 1.0),
        )
        .map_err(|why| why.to_string())?;
    view.hide().map_err(|why| why.to_string())?;
    state
        .0
        .lock()
        .map_err(|_| "Browser state unavailable")?
        .tabs
        .insert(
            id,
            Tab {
                url: url.to_string(),
                visible: false,
            },
        );
    Ok(())
}
#[derive(serde::Deserialize)]
pub struct Bounds {
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    #[serde(default)]
    radius: f64,
}

/// Rounds the page's bottom corners to the card's, so it can fill the card edge to edge.
#[cfg(target_os = "macos")]
fn round_bottom(view: &tauri::Webview, radius: f64) {
    let radius = radius.clamp(0.0, 64.0);
    let _ = view.with_webview(move |platform| unsafe {
        use objc2::{
            msg_send,
            runtime::{AnyObject, Bool},
        };
        let web = platform.inner() as *mut AnyObject;
        if web.is_null() {
            return;
        }
        let _: () = msg_send![web, setWantsLayer: Bool::YES];
        let layer: *mut AnyObject = msg_send![web, layer];
        if layer.is_null() {
            return;
        }
        let flipped: Bool = msg_send![layer, isGeometryFlipped];
        // CACornerMask: 1 and 2 are the min-y corners, 4 and 8 the max-y ones; which is the bottom depends on the flip.
        let corners: usize = if flipped.as_bool() { 4 | 8 } else { 1 | 2 };
        let _: () = msg_send![layer, setMaskedCorners: corners];
        let _: () = msg_send![layer, setCornerRadius: radius];
        let _: () = msg_send![layer, setMasksToBounds: Bool::new(radius > 0.0)];
    });
}
#[tauri::command]
pub async fn browser_sync(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Browser>,
    request: Bounds,
) -> Result<(), String> {
    owner(&caller)?;
    let view = app
        .get_webview(&label(&request.id)?)
        .ok_or("Browser tab is closed")?;
    let mut data = state.0.lock().map_err(|_| "Browser state unavailable")?;
    for (id, tab) in data.tabs.iter_mut() {
        if id != &request.id {
            if let Some(other) = app.get_webview(&label(id)?) {
                let _ = other.hide();
            }
            tab.visible = false;
        }
    }
    if !request.visible {
        view.hide().map_err(|why| why.to_string())?;
        if let Some(tab) = data.tabs.get_mut(&request.id) {
            tab.visible = false;
        }
        return Ok(());
    }
    let window = app.get_window("main").ok_or("Window closed")?;
    let size = window
        .inner_size()
        .map_err(|why| why.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|why| why.to_string())?);
    if ![request.x, request.y, request.width, request.height]
        .iter()
        .all(|v| v.is_finite())
        || request.x < 0.0
        || request.y < 0.0
        || request.width < 2.0
        || request.height < 2.0
        || request.x + request.width > size.width + 1.0
        || request.y + request.height > size.height + 1.0
    {
        return Err("Browser bounds must stay inside PWR.".into());
    }
    view.set_bounds(tauri::Rect {
        position: LogicalPosition::new(request.x, request.y).into(),
        size: LogicalSize::new(request.width, request.height).into(),
    })
    .map_err(|why| why.to_string())?;
    view.show().map_err(|why| why.to_string())?;
    #[cfg(target_os = "macos")]
    round_bottom(&view, request.radius);
    if let Some(tab) = data.tabs.get_mut(&request.id) {
        tab.visible = true;
    }
    Ok(())
}
#[tauri::command]
pub async fn browser_action(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Browser>,
    id: String,
    operation: String,
    arguments: Value,
) -> Result<Value, String> {
    owner(&caller)?;
    let view = app
        .get_webview(&label(&id)?)
        .ok_or("Browser tab is closed")?;
    if !state
        .0
        .lock()
        .map_err(|_| "Browser state unavailable")?
        .tabs
        .get(&id)
        .is_some_and(|tab| tab.visible)
    {
        return Err("This browser tab must be visible before interacting with it.".into());
    }
    if ![
        "snapshot", "click", "fill", "key", "scroll", "back", "forward", "reload",
    ]
    .contains(&operation.as_str())
    {
        return Err("Unsupported browser operation.".into());
    }
    let token = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = mpsc::channel();
    state
        .0
        .lock()
        .map_err(|_| "Browser state unavailable")?
        .pending
        .insert(token.clone(), (id, sender));
    let request = json!({"operation":operation,"arguments":arguments,"token":token});
    let script = include_str!("browser.js").replace("__REQUEST__", &request.to_string());
    if let Err(why) = view.eval(script) {
        state
            .0
            .lock()
            .map_err(|_| "Browser state unavailable")?
            .pending
            .remove(&token);
        return Err(why.to_string());
    }
    let received = tauri::async_runtime::spawn_blocking(move || {
        receiver.recv_timeout(std::time::Duration::from_secs(8))
    })
    .await
    .map_err(|why| why.to_string())?;
    state
        .0
        .lock()
        .map_err(|_| "Browser state unavailable")?
        .pending
        .remove(&token);
    let value = received.map_err(|_| {
        "The page navigated or did not answer. Observe its state before retrying an action."
            .to_string()
    })?;
    if let Some(error) = value["error"].as_str() {
        return Err(error.into());
    }
    Ok(value)
}
#[tauri::command]
pub async fn browser_close(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Browser>,
    id: String,
) -> Result<(), String> {
    owner(&caller)?;
    if let Some(view) = app.get_webview(&label(&id)?) {
        view.close().map_err(|why| why.to_string())?;
    }
    let mut data = state.0.lock().map_err(|_| "Browser state unavailable")?;
    data.tabs.remove(&id);
    data.pending.retain(|_, (tab, _)| tab != &id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_websites_and_local_apps_but_never_host_protocols_or_ipc() {
        assert!(address("https://example.com/path?q=1").is_ok());
        assert!(address("http://localhost:3000/").is_ok());
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "tauri://localhost",
            "http://ipc.localhost",
            "http://tauri.localhost",
            "https://u:p@example.com",
        ] {
            assert!(address(url).is_err(), "{url}");
        }
    }
    #[test]
    fn tab_identifiers_cannot_name_other_webviews() {
        assert!(label("tab-123").is_ok());
        assert!(label("../main").is_err());
        assert!(label("").is_err());
        assert_ne!(label("main").unwrap(), "main");
    }
}
