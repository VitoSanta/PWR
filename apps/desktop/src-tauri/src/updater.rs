//! Signed updates are owned by the native main window, never remote previews.
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
struct Pending {
    busy: bool,
    update: Option<Update>,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
pub struct Updates {
    pending: Mutex<Pending>,
    pub activity: Mutex<()>,
    installed: AtomicBool,
    installing: AtomicBool,
}
impl Updates {
    pub fn installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }
    pub fn allow_activity(&self, closing: &super::Closing) -> Result<(), String> {
        if closing.0.load(Ordering::SeqCst) {
            Err("PWR is closing; reopen the app before starting another operation.".into())
        } else if self.installed.load(Ordering::SeqCst) {
            Err("Restart PWR to finish the update.".into())
        } else {
            Ok(())
        }
    }
}
fn configuration() -> Result<(&'static str, &'static str), String> {
    let key = Some(
        option_env!("PWR_UPDATER_PUBLIC_KEY")
            .unwrap_or(include_str!("../updater.pub"))
            .trim(),
    )
    .filter(|s| !s.is_empty());
    let endpoint = option_env!("PWR_UPDATER_ENDPOINT").filter(|s| s.starts_with("https://"));
    let channel = option_env!("PWR_UPDATER_CHANNEL");
    if !matches!(channel, Some("stable" | "prerelease")) {
        return Err("Updates are not configured for this build.".into());
    }
    key.zip(endpoint)
        .ok_or_else(|| "Updates are not configured for this build.".into())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    current_version: String,
    channel: &'static str,
}
#[tauri::command]
pub fn update_status(app: AppHandle) -> Status {
    Status {
        configured: configuration().is_ok(),
        current_version: app.package_info().version.to_string(),
        channel: option_env!("PWR_UPDATER_CHANNEL").unwrap_or("prerelease"),
    }
}
#[derive(Serialize)]
pub struct Available {
    version: String,
    notes: String,
}
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, Updates>,
) -> Result<Option<Available>, String> {
    let (key, endpoint) = configuration()?;
    {
        let mut p = state
            .pending
            .lock()
            .map_err(|_| "Update state unavailable")?;
        if p.busy {
            return Err("An update operation is already running.".into());
        }
        state.allow_activity(&app.state::<super::Closing>())?;
        p.busy = true;
        p.update = None;
        p.bytes = None;
    }
    let result = async {
        let url = endpoint
            .parse()
            .map_err(|e| format!("Invalid update endpoint: {e}"))?;
        app.updater_builder()
            .pubkey(key)
            .endpoints(vec![url])
            .map_err(|e| e.to_string())?
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    let mut p = state
        .pending
        .lock()
        .map_err(|_| "Update state unavailable")?;
    p.busy = false;
    let update = result?;
    if update
        .as_ref()
        .is_some_and(|u| u.download_url.scheme() != "https")
    {
        return Err("Update downloads require HTTPS.".into());
    }
    if option_env!("PWR_UPDATER_CHANNEL") == Some("stable")
        && update.as_ref().is_some_and(|u| u.version.contains('-'))
    {
        return Err("The stable channel cannot install a prerelease.".into());
    }
    let info = update.as_ref().map(|u| Available {
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default(),
    });
    p.update = update;
    Ok(info)
}
#[tauri::command]
pub async fn update_download(app: AppHandle, state: State<'_, Updates>) -> Result<(), String> {
    let update = {
        let mut p = state
            .pending
            .lock()
            .map_err(|_| "Update state unavailable")?;
        if p.busy {
            return Err("An update operation is already running.".into());
        }
        let update = p.update.take().ok_or("Check for an update first.")?;
        p.busy = true;
        p.bytes = None;
        update
    };
    let mut received = 0u64;
    // Only these plugin-verified bytes can reach install; no JS byte argument.
    let result = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let _ = app.emit_to(
                    "main",
                    "update-progress",
                    serde_json::json!({"received":received,"total":total}),
                );
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string());
    let mut p = state
        .pending
        .lock()
        .map_err(|_| "Update state unavailable")?;
    p.busy = false;
    p.update = Some(update);
    p.bytes = Some(result?);
    Ok(())
}
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Updates>();
        let _activity = state
            .activity
            .lock()
            .map_err(|_| "Activity state unavailable")?;
        state.allow_activity(&app.state::<super::Closing>())?;
        let core = app.state::<super::Core>();
        let core = core.0.lock().map_err(|_| "Core state unavailable")?;
        if core.is_some() || app.state::<super::terminal::Terminals>().has_sessions() {
            return Err("Stop the workspace and close all terminals before installing.".into());
        }
        let mut p = state
            .pending
            .lock()
            .map_err(|_| "Update state unavailable")?;
        if p.busy {
            return Err("An update operation is already running.".into());
        }
        let update = p.update.as_ref().ok_or("Check for an update first.")?;
        let bytes = p
            .bytes
            .as_ref()
            .ok_or("Download and verify the update first.")?;
        state.installing.store(true, Ordering::SeqCst);
        let result = update.install(bytes).map_err(|e| e.to_string());
        state.installing.store(false, Ordering::SeqCst);
        result?;
        state.installed.store(true, Ordering::SeqCst);
        p.bytes = None;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn update_restart(app: AppHandle, state: State<'_, Updates>) -> Result<(), String> {
    if !state.installed.load(Ordering::SeqCst) {
        return Err("No installed update is waiting for restart.".into());
    }
    app.state::<super::Closing>()
        .0
        .store(true, Ordering::SeqCst);
    app.restart()
}

#[cfg(test)]
mod tests {
    #[test]
    fn unconfigured_packaged_plugin_deserializes_without_weakening_verification() {
        let app: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let config: tauri_plugin_updater::Config =
            serde_json::from_value(app["plugins"]["updater"].clone()).unwrap();
        assert!(config.pubkey.is_empty());
        assert!(config.endpoints.is_empty());
        assert!(!config.dangerous_insecure_transport_protocol);
        assert!(!config.dangerous_accept_invalid_certs);
        assert!(!config.dangerous_accept_invalid_hostnames);
    }
    #[test]
    fn queued_native_activity_cannot_start_after_close_confirmation() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        };
        let updates = Arc::new(super::Updates::default());
        let closing = Arc::new(crate::Closing::default());
        let started = Arc::new(AtomicBool::new(false));
        let activity = updates.activity.lock().unwrap();
        let (queued, waiting) = mpsc::channel();
        let worker = {
            let updates = updates.clone();
            let closing = closing.clone();
            let started = started.clone();
            std::thread::spawn(move || {
                queued.send(()).unwrap();
                let _activity = updates.activity.lock().unwrap();
                if updates.allow_activity(&closing).is_ok() {
                    started.store(true, Ordering::SeqCst);
                }
                closing.0.load(Ordering::SeqCst)
            })
        };
        waiting
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        closing.0.store(true, Ordering::SeqCst);
        drop(activity);
        assert!(worker.join().unwrap());
        assert!(
            !started.load(Ordering::SeqCst),
            "queued operation started after confirmed close"
        );
    }

    #[test]
    fn native_activity_remains_available_until_close_or_installed_update() {
        let updates = super::Updates::default();
        let closing = crate::Closing::default();
        assert!(updates.allow_activity(&closing).is_ok());
        updates
            .installed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(updates
            .allow_activity(&closing)
            .unwrap_err()
            .contains("update"));
        closing.0.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(updates
            .allow_activity(&closing)
            .unwrap_err()
            .contains("closing"));
    }
}
