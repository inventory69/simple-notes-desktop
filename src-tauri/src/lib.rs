mod error;
mod markdown;
mod models;
mod storage;
mod webdav;

use error::{AppError, Result};
use models::{ConflictInfo, ConflictResolution, Note, NoteMetadata};
use std::collections::HashMap;
use std::sync::Mutex;
use storage::{Credentials, Settings, SyncBaseEntry};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;
use webdav::WebDavClient;

/// Global WebDAV Client State
struct WebDavState(Mutex<Option<WebDavClient>>);

/// Global Device ID
struct DeviceIdState(Mutex<Option<String>>);

/// Global Sync Base State (speichert den letzten bekannten Server-Zustand für Konflikt-Erkennung)
struct SyncBaseState(Mutex<Option<HashMap<String, SyncBaseEntry>>>);

/// Lädt die Sync-Base aus dem Store
fn load_sync_base(app: &AppHandle) -> Result<HashMap<String, SyncBaseEntry>> {
    let store = app
        .store("sync_base.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    if let Some(value) = store.get("entries") {
        let entries: HashMap<String, SyncBaseEntry> = serde_json::from_value(value)
            .unwrap_or_else(|_| HashMap::new());
        Ok(entries)
    } else {
        Ok(HashMap::new())
    }
}

/// Speichert die Sync-Base im Store
fn save_sync_base(app: &AppHandle, entries: &HashMap<String, SyncBaseEntry>) -> Result<()> {
    let store = app
        .store("sync_base.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    store.set("entries", serde_json::to_value(entries).unwrap());
    store
        .save()
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    Ok(())
}

/// Aktualisiert einen Sync-Base-Eintrag nach erfolgreichem Sync
fn update_sync_base_entry(
    app: &AppHandle,
    state: &State<SyncBaseState>,
    note: &Note,
) -> Result<()> {
    let mut base_lock = state.0.lock().unwrap();

    if base_lock.is_none() {
        *base_lock = Some(load_sync_base(app)?);
    }

    let entries = base_lock.as_mut().unwrap();

    let entry = SyncBaseEntry {
        note_id: note.id.clone(),
        content_hash: note.compute_content_hash(),
        synced_at: chrono::Utc::now().timestamp_millis(),
        server_updated_at: note.updated_at,
    };

    entries.insert(note.id.clone(), entry);
    save_sync_base(app, entries)?;

    Ok(())
}

/// Holt einen Sync-Base-Eintrag
fn get_sync_base_entry(
    app: &AppHandle,
    state: &State<SyncBaseState>,
    note_id: &str,
) -> Result<Option<SyncBaseEntry>> {
    let mut base_lock = state.0.lock().unwrap();

    if base_lock.is_none() {
        *base_lock = Some(load_sync_base(app)?);
    }

    let entries = base_lock.as_ref().unwrap();
    Ok(entries.get(note_id).cloned())
}

/// Prüft auf Konflikt
/// Konflikt tritt auf, wenn:
/// 1. Lokale Notiz hat Änderungen seit dem letzten Sync
/// 2. Remote-Notiz hat auch Änderungen seit dem letzten Sync
fn detect_conflict(
    local_note: &Note,
    remote_note: &Note,
    base_entry: &Option<SyncBaseEntry>,
) -> bool {
    let local_hash = local_note.compute_content_hash();
    let remote_hash = remote_note.compute_content_hash();

    // Wenn Hashes gleich sind, kein Konflikt
    if local_hash == remote_hash {
        return false;
    }

    // Wenn es keine Base-Eintrag gibt (neue Notiz oder erster Sync),
    // prüfe ob beide Versionen existieren und unterschiedlich sind
    let base_entry = match base_entry {
        Some(entry) => entry,
        None => {
            // Kein Basiseintrag: Konflikt wenn beide Versionen existieren und unterschiedlich sind
            return local_note.created_at > 0 && remote_note.created_at > 0;
        }
    };

    // Prüfe: Lokale Änderungen UND Remote-Änderungen
    let local_changed = local_hash != base_entry.content_hash;
    let remote_changed = remote_hash != base_entry.content_hash;

    local_changed && remote_changed
}

/// Generiert oder lädt Device ID
fn get_or_create_device_id(app: &AppHandle, state: &State<DeviceIdState>) -> Result<String> {
    let mut device_id_lock = state.0.lock().unwrap();

    if let Some(id) = &*device_id_lock {
        return Ok(id.clone());
    }

    // Versuche aus Store zu laden
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    if let Some(stored_id) = store.get("device_id") {
        if let Some(id_str) = stored_id.as_str() {
            *device_id_lock = Some(id_str.to_string());
            return Ok(id_str.to_string());
        }
    }

    // Neu generieren
    let uuid = Uuid::new_v4().simple().to_string();
    let new_id = format!("tauri-{}", &uuid[..16]);

    store.set("device_id", serde_json::json!(new_id.clone()));
    let _ = store.save();

    *device_id_lock = Some(new_id.clone());
    Ok(new_id)
}

// ============ TAURI COMMANDS ============

#[tauri::command]
async fn connect(
    url: String,
    username: String,
    password: String,
    sync_folder: Option<String>,
    state: State<'_, WebDavState>,
) -> Result<bool> {
    let folder = sync_folder.unwrap_or_else(|| "notes".to_string());
    let client = WebDavClient::new(&url, &username, &password, &folder)?;
    let success = client.test_connection().await?;

    if success {
        let mut client_lock = state.0.lock().unwrap();
        *client_lock = Some(client);
    }

    Ok(success)
}

#[tauri::command]
async fn list_notes(state: State<'_, WebDavState>) -> Result<Vec<NoteMetadata>> {
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    let ids = client.list_json_files().await?;

    let mut notes = Vec::new();
    for id in ids {
        if let Ok(note) = client.get_note(&id).await {
            notes.push(NoteMetadata::from(&note));
        }
    }

    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

#[tauri::command]
async fn get_note(id: String, state: State<'_, WebDavState>) -> Result<Note> {
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    client.get_note(&id).await
}

#[tauri::command]
async fn save_note(
    mut note: Note,
    state: State<'_, WebDavState>,
    sync_base_state: State<'_, SyncBaseState>,
    app: AppHandle,
) -> Result<Note> {
    // Update timestamp to NOW before saving
    note.updated_at = chrono::Utc::now().timestamp_millis();

    #[cfg(debug_assertions)]
    eprintln!("[Save] Saving note '{}' (ID: {})", note.title, note.id);
    #[cfg(debug_assertions)]
    eprintln!("[Save] Updated timestamp: {}", note.updated_at);

    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;

    // ========== Konflikt-Erkennung ==========
    // 1. Holen der aktuellen Remote-Version (falls existiert)
    let remote_note_result = client.get_note(&note.id).await;

    if let Ok(remote_note) = remote_note_result {
        // 2. Holen des Sync-Base-Eintrags
        let base_entry = get_sync_base_entry(&app, &sync_base_state, &note.id)?;

        // 3. Prüfe auf Konflikt
        if detect_conflict(&note, &remote_note, &base_entry) {
            #[cfg(debug_assertions)]
            eprintln!("[Save] Conflict detected for note: {}", note.id);

            return Err(AppError::SyncConflict(note.id.clone()));
        }
    }
    // ========== Ende Konflikt-Erkennung ==========

    // Speichern der Notiz
    client.save_note(&note).await?;

    #[cfg(debug_assertions)]
    eprintln!("[Save] Note saved successfully to WebDAV");

    // Aktualisiere Sync-Base nach erfolgreichem Speichern
    update_sync_base_entry(&app, &sync_base_state, &note)?;

    // Return the updated note with new timestamp
    Ok(note)
}

#[tauri::command]
async fn create_note(
    title: String,
    note_type: String,
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
) -> Result<Note> {
    let device_id = get_or_create_device_id(&app, &device_id_state)?;

    let note = match note_type.as_str() {
        "CHECKLIST" => Note::new_checklist(title, device_id),
        _ => Note::new(title, device_id),
    };

    Ok(note)
}

#[tauri::command]
async fn delete_note(id: String, state: State<'_, WebDavState>) -> Result<()> {
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    let note = client.get_note(&id).await?;
    client.delete_note(&note).await
}

// ========== Konflikt-Handling Commands ==========

/// Holt die Konflikt-Informationen für eine Notiz
/// Gibt lokale und Remote-Version sowie eine Beschreibung der Unterschiede zurück
#[tauri::command]
async fn get_conflict_info(
    note_id: String,
    local_note: Note,
    state: State<'_, WebDavState>,
    sync_base_state: State<'_, SyncBaseState>,
    app: AppHandle,
) -> Result<ConflictInfo> {
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;

    // Holen der Remote-Version
    let remote_note = client.get_note(&note_id).await?;

    // Holen des Sync-Base-Eintrags
    let base_entry = get_sync_base_entry(&app, &sync_base_state, &note_id)?;

    // Berechne Diff-Zusammenfassung
    let diff_summary = ConflictInfo::compute_diff_summary(&local_note, &remote_note);

    Ok(ConflictInfo {
        note_id: note_id.clone(),
        local_note: local_note.clone(),
        remote_note: remote_note.clone(),
        base_hash: base_entry.map(|e| e.content_hash),
        local_modified_at: local_note.updated_at,
        remote_modified_at: remote_note.updated_at,
        diff_summary,
    })
}

/// Löst einen Konflikt basierend auf der Benutzerauswahl
#[tauri::command]
async fn resolve_conflict(
    note_id: String,
    local_note: Note,
    resolution: ConflictResolution,
    state: State<'_, WebDavState>,
    sync_base_state: State<'_, SyncBaseState>,
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
) -> Result<Note> {
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;

    // Holen der Remote-Version
    let remote_note = client.get_note(&note_id).await?;

    match resolution {
        ConflictResolution::KeepLocal => {
            // Lokale Version behalten: Überschreibe Remote
            let mut note_to_save = local_note.clone();
            note_to_save.updated_at = chrono::Utc::now().timestamp_millis();

            client.save_note(&note_to_save).await?;
            update_sync_base_entry(&app, &sync_base_state, &note_to_save)?;

            Ok(note_to_save)
        }
        ConflictResolution::KeepRemote => {
            // Remote-Version behalten: Aktualisiere lokalen Sync-Base
            update_sync_base_entry(&app, &sync_base_state, &remote_note)?;

            Ok(remote_note)
        }
        ConflictResolution::KeepBoth => {
            // Beide Versionen behalten:
            // 1. Remote-Version bleibt als Original
            // 2. Lokale Version wird als neue Notiz mit "(Conflict)" im Titel gespeichert

            let device_id = get_or_create_device_id(&app, &device_id_state)?;

            // Erstelle eine neue Notiz aus der lokalen Version
            let mut new_note = if local_note.note_type == models::NoteType::Checklist {
                Note::new_checklist(
                    format!("{} (Conflict Copy)", local_note.title),
                    device_id,
                )
            } else {
                Note::new(
                    format!("{} (Conflict Copy)", local_note.title),
                    device_id,
                )
            };

            // Kopiere Inhalt
            new_note.content = local_note.content.clone();
            new_note.checklist_items = local_note.checklist_items.clone();
            new_note.checklist_sort_option = local_note.checklist_sort_option.clone();
            new_note.updated_at = chrono::Utc::now().timestamp_millis();

            // Speichere die neue Kopie
            client.save_note(&new_note).await?;
            update_sync_base_entry(&app, &sync_base_state, &new_note)?;

            // Aktualisiere Sync-Base für die Original-Notiz (Remote-Version)
            update_sync_base_entry(&app, &sync_base_state, &remote_note)?;

            // Gib die neue Kopie zurück (die der Benutzer bearbeiten kann)
            Ok(new_note)
        }
    }
}

/// Aktualisiert den Sync-Base nach dem Laden einer Notiz (z.B. nach dem Öffnen)
/// Dies sollte aufgerufen werden, wenn der Benutzer eine Notiz vom Server lädt
#[tauri::command]
async fn update_sync_base_after_load(
    note: Note,
    sync_base_state: State<'_, SyncBaseState>,
    app: AppHandle,
) -> Result<()> {
    update_sync_base_entry(&app, &sync_base_state, &note)
}

#[tauri::command]
async fn get_credentials(app: AppHandle) -> Result<Option<Credentials>> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    let url = store
        .get("server_url")
        .and_then(|v| v.as_str().map(String::from));
    let username = store
        .get("username")
        .and_then(|v| v.as_str().map(String::from));
    let password = store
        .get("password")
        .and_then(|v| v.as_str().map(String::from));

    match (url, username, password) {
        (Some(url), Some(username), Some(password)) => Ok(Some(Credentials {
            url,
            username,
            password,
        })),
        _ => Ok(None),
    }
}

#[tauri::command]
async fn save_credentials(credentials: Credentials, app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    store.set("server_url", serde_json::json!(credentials.url));
    store.set("username", serde_json::json!(credentials.username));
    store.set("password", serde_json::json!(credentials.password));

    store
        .save()
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    Ok(())
}

#[tauri::command]
async fn clear_credentials(app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    store.delete("server_url");
    store.delete("username");
    store.delete("password");

    store
        .save()
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    Ok(())
}

#[tauri::command]
async fn get_device_id(app: AppHandle, state: State<'_, DeviceIdState>) -> Result<String> {
    get_or_create_device_id(&app, &state)
}

#[tauri::command]
async fn get_settings(app: AppHandle) -> Result<Settings> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    let theme = store
        .get("theme")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "system".to_string());

    let autosave = store
        .get("autosave")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let minimize_to_tray = store
        .get("minimize_to_tray")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let autostart = store
        .get("autostart")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let sync_folder = store
        .get("sync_folder")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "notes".to_string());

    Ok(Settings {
        theme,
        autosave,
        minimize_to_tray,
        autostart,
        sync_folder,
    })
}

#[tauri::command]
async fn save_settings(settings: Settings, app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    store.set("theme", serde_json::json!(settings.theme));
    store.set("autosave", serde_json::json!(settings.autosave));
    store.set(
        "minimize_to_tray",
        serde_json::json!(settings.minimize_to_tray),
    );
    store.set("autostart", serde_json::json!(settings.autostart));
    store.set("sync_folder", serde_json::json!(settings.sync_folder));

    // Handle autostart toggle
    if settings.autostart {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }

    store
        .save()
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    Ok(())
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn get_desktop_environment() -> Option<String> {
    // Try to detect desktop environment from environment variables
    if let Ok(session) = std::env::var("XDG_CURRENT_DESKTOP") {
        return Some(session.to_lowercase());
    }
    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        return Some(session.to_lowercase());
    }
    if std::env::var("KDE_FULL_SESSION").is_ok() {
        return Some("kde".to_string());
    }
    if std::env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return Some("gnome".to_string());
    }
    None
}

/// Update the minimize-to-tray runtime setting without restarting
#[tauri::command]
async fn update_tray_setting(enabled: bool, state: State<'_, TraySettings>) -> Result<()> {
    let mut lock = state.0.lock().unwrap();
    *lock = enabled;
    Ok(())
}

/// State to track minimize-to-tray setting at runtime
struct TraySettings(Mutex<bool>);

/// Restore a hidden or minimized window safely on all platforms.
///
/// On Linux/Wayland (KDE Plasma), tao's `set_focus()` silently drops the
/// focus request because the GTK widget isn't visible yet when checked
/// synchronously after the async `show()`. The GTK-CSD titlebar also
/// corrupts its internal input state during hide/show cycles on Wayland.
///
/// Fix: Use `gtk_window().present()` which handles show + focus in a single
/// call, correctly re-negotiating the compositor focus and fixing frozen
/// titlebar buttons after tray restore.
fn restore_window(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;

        // present() both shows and focuses the window, bypassing tao's
        // set_focus() race condition. It also re-negotiates compositor
        // focus, fixing frozen titlebar buttons after hide/show cycles.
        if let Ok(gtk_window) = window.gtk_window() {
            gtk_window.present();
        }

        // Unminimize separately in case the window was minimized (not hidden)
        if let Ok(true) = window.is_minimized() {
            let _ = window.unminimize();
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Ok(false) = window.is_visible() {
            let _ = window.show();
        }
        if let Ok(true) = window.is_minimized() {
            let _ = window.unminimize();
        }
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // F8: Callback wird aufgerufen wenn eine zweite Instanz gestartet wird.
            // Die zweite Instanz beendet sich automatisch.
            // Hier bringen wir das Fenster der ersten Instanz in den Vordergrund.
            #[cfg(debug_assertions)]
            eprintln!(
                "[SingleInstance] Second instance detected with args: {:?}",
                args
            );
            if let Some(window) = app.get_webview_window("main") {
                restore_window(&window);
            }
        }))
        .manage(WebDavState(Mutex::new(None)))
        .manage(DeviceIdState(Mutex::new(None)))
        .manage(SyncBaseState(Mutex::new(None)))
        .manage(TraySettings(Mutex::new(false)))
        .setup(|app| {
            // Load minimize_to_tray setting
            if let Ok(store) = app.store("settings.json") {
                let minimize = store
                    .get("minimize_to_tray")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let state = app.state::<TraySettings>();
                *state.0.lock().unwrap() = minimize;

                // Sync autostart state
                let autostart = store
                    .get("autostart")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if autostart {
                    let _ = app.autolaunch().enable();
                }
            }

            // Build tray menu
            let show_item = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // Load tray icon embedded at compile time
            let icon_bytes = include_bytes!("../icons/32x32.png");
            let icon = tauri::image::Image::from_bytes(icon_bytes)
                .expect("failed to load tray icon")
                .to_owned();

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&tray_menu)
                .tooltip("Simple Notes Desktop")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            restore_window(&window);
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            restore_window(&window);
                        }
                    }
                })
                .build(app)?;

            // Set window icon for taskbar (KDE Plasma, etc.)
            let window_icon_bytes = include_bytes!("../icons/128x128.png");
            let window_icon = tauri::image::Image::from_bytes(window_icon_bytes)
                .expect("failed to load window icon")
                .to_owned();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(window_icon);

                // Bug 1 Fix: Remove GTK Client-Side Decorations (CSD) so the
                // compositor (KWin) renders Server-Side Decorations (SSD).
                // Without this, tao PR #979 forces GTK-drawn GNOME-style
                // titlebars on Wayland, even under KDE Plasma.
                #[cfg(target_os = "linux")]
                {
                    use gtk::prelude::GtkWindowExt;
                    if let Ok(gtk_window) = window.gtk_window() {
                        gtk_window.set_titlebar(Option::<&gtk::Widget>::None);
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let tray_settings = app.state::<TraySettings>();
                let minimize = *tray_settings.0.lock().unwrap();

                if minimize {
                    // Prevent window from closing, hide instead
                    api.prevent_close();
                    let _ = window.hide();
                }
                // If minimize_to_tray is false, window closes normally (app quits)
            }
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            list_notes,
            get_note,
            save_note,
            create_note,
            delete_note,
            get_credentials,
            save_credentials,
            clear_credentials,
            get_device_id,
            get_settings,
            save_settings,
            get_app_version,
            get_desktop_environment,
            update_tray_setting,
            // Konflikt-Handling Commands
            get_conflict_info,
            resolve_conflict,
            update_sync_base_after_load,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
