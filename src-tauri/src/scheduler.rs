use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::Notify;

pub struct SyncTrigger(pub Arc<Notify>);

const DEBOUNCE: Duration = Duration::from_secs(5);
const PERIODIC: Duration = Duration::from_secs(300);

/// Von jedem Mutations-Command aufgerufen. Nicht blockierend.
pub fn trigger_sync(app: &AppHandle) {
    if let Some(t) = app.try_state::<SyncTrigger>() {
        t.0.notify_one();
    }
}

/// Im setup() einmal gestartet.
pub fn spawn(app: AppHandle, notify: Arc<Notify>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = notify.notified() => {
                    // Debounce: kurz warten damit aufeinanderfolgende Edits gebündelt werden
                    tokio::time::sleep(DEBOUNCE).await;
                }
                _ = tokio::time::sleep(PERIODIC) => {}
            }
            run_once(&app).await;
        }
    });
}

pub async fn run_once(app: &AppHandle) {
    if settings_offline(app) {
        return;
    }
    let client = {
        let s = app.state::<crate::WebDavState>();
        let guard = crate::lock_recover(&s.0);
        guard.clone()
    };
    let Some(client) = client else { return };
    let lock = app.state::<crate::SyncLockState>();
    let _g = match lock.0.try_lock() {
        Ok(g) => g,
        Err(_) => {
            // Bereits ein Sync aktiv (z.B. manueller Sync). Trigger erneut scharf schalten,
            // damit dieser Lauf nicht verloren geht und nach dem Debounce erneut versucht wird
            // (sonst erst beim nächsten Periodic-Tick). Begrenzt durch die Sync-Dauer.
            trigger_sync(app);
            return;
        }
    };
    let dev = {
        let state = app.state::<crate::DeviceIdState>();
        match crate::get_or_create_device_id(app, &state) {
            Ok(d) => d,
            Err(_) => return,
        }
    };
    crate::sync_engine::run_sync(&client, app, &dev, crate::TRASH_RETENTION_MS).await;
    let _ = app.emit("notes-synced", ());
}

fn settings_offline(app: &AppHandle) -> bool {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get("offline_mode"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true) // Default: offline (kein Hintergrund-Sync ohne explizite Einstellung)
}
