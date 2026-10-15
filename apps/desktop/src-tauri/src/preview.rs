//! Local pages are separate top-level webviews, never framed or given PWR's IPC.
use std::sync::Mutex;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, State, Webview, WebviewUrl};

const LABEL: &str = "workspace-preview";
#[derive(Default)]
pub struct Preview(pub Mutex<String>);

fn local_url(raw: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(raw).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_none()
        || (cfg!(debug_assertions) && url.port() == Some(4200))
    {
        return Err("Preview accepts only local development servers, excluding PWR itself.".into());
    }
    Ok(url)
}

fn owner(caller: &Webview) -> Result<(), String> {
    if caller.label() == "main" {
        Ok(())
    } else {
        Err("Only the PWR interface controls the preview.".into())
    }
}

#[derive(serde::Deserialize)]
pub struct PreviewRequest {
    address: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
}

#[tauri::command]
pub async fn preview_sync(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Preview>,
    request: PreviewRequest,
) -> Result<(), String> {
    let PreviewRequest {
        address,
        x,
        y,
        width,
        height,
        visible,
    } = request;
    owner(&caller)?;
    if !visible {
        if let Some(view) = app.get_webview(LABEL) {
            view.hide().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let url = local_url(&address)?;
    let window = app.get_window("main").ok_or("PWR window is closed")?;
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    if ![x, y, width, height].iter().all(|v| v.is_finite())
        || x < 0.0
        || y < 0.0
        || width < 1.0
        || height < 1.0
        || x + width > size.width + 1.0
        || y + height > size.height + 1.0
    {
        return Err("Preview bounds must stay inside the PWR window.".into());
    }
    let mut current = state.0.lock().map_err(|_| "Preview state unavailable")?;
    let view = if let Some(view) = app.get_webview(LABEL) {
        if *current != address {
            view.navigate(url).map_err(|e| e.to_string())?;
        }
        view
    } else {
        let builder = tauri::webview::WebviewBuilder::new(LABEL, WebviewUrl::External(url))
            .on_navigation(|url| local_url(url.as_str()).is_ok())
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
        window
            .add_child(
                builder,
                LogicalPosition::new(x, y),
                LogicalSize::new(width, height),
            )
            .map_err(|e| e.to_string())?
    };
    *current = address;
    view.set_bounds(tauri::Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width, height).into(),
    })
    .map_err(|e| e.to_string())?;
    view.show().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_close(
    app: AppHandle,
    caller: Webview,
    state: State<'_, Preview>,
) -> Result<(), String> {
    owner(&caller)?;
    if let Some(view) = app.get_webview(LABEL) {
        view.close().map_err(|e| e.to_string())?;
    }
    state
        .0
        .lock()
        .map_err(|_| "Preview state unavailable")?
        .clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_only_navigates_local_http_servers() {
        assert!(local_url("http://localhost:5199/tickets").is_ok());
        assert!(local_url("http://127.0.0.1:8000/").is_ok());
        for address in [
            "https://example.com/",
            "file:///etc/passwd",
            "http://localhost.evil:5199/",
            "http://user:secret@localhost:5199/",
            "http://localhost/",
        ] {
            assert!(local_url(address).is_err(), "{address}");
        }
        if cfg!(debug_assertions) {
            assert!(local_url("http://127.0.0.1:4200/").is_err());
        }
    }
}
