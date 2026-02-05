mod error;
mod markdown;
mod models;
mod storage;
mod webdav;

use error::{AppError, Result};
use models::{Note, NoteMetadata};
use storage::{Credentials, Settings};
use std::sync::Mutex;
use tauri::{
    AppHandle, Manager, State,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tauri_plugin_store::StoreExt;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use uuid::Uuid;
use webdav::WebDavClient;

/// Global WebDAV Client State
struct WebDavState(Mutex<Option<WebDavClient>>);

/// Global Device ID
struct DeviceIdState(Mutex<Option<String>>);

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
    state: State<'_, WebDavState>,
) -> Result<bool> {
    let client = WebDavClient::new(&url, &username, &password)?;
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
async fn save_note(mut note: Note, state: State<'_, WebDavState>) -> Result<Note> {
    // Update timestamp to NOW before saving
    note.updated_at = chrono::Utc::now().timestamp_millis();
    
    println!("[Save] Saving note '{}' (ID: {})", note.title, note.id);
    println!("[Save] Updated timestamp: {}", note.updated_at);
    
    let client = {
        let client_lock = state.0.lock().unwrap();
        client_lock.clone()
    };
    
    let client = client.ok_or(AppError::NotConnected)?;
    client.save_note(&note).await?;
    
    println!("[Save] Note saved successfully to WebDAV");
    
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

#[tauri::command]
async fn get_credentials(app: AppHandle) -> Result<Option<Credentials>> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    
    let url = store.get("server_url").and_then(|v| v.as_str().map(String::from));
    let username = store.get("username").and_then(|v| v.as_str().map(String::from));
    let password = store.get("password").and_then(|v| v.as_str().map(String::from));
    
    match (url, username, password) {
        (Some(url), Some(username), Some(password)) => {
            Ok(Some(Credentials { url, username, password }))
        }
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
async fn get_device_id(
    app: AppHandle,
    state: State<'_, DeviceIdState>,
) -> Result<String> {
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
    
    Ok(Settings { theme, autosave, minimize_to_tray, autostart })
}

#[tauri::command]
async fn save_settings(settings: Settings, app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    
    store.set("theme", serde_json::json!(settings.theme));
    store.set("autosave", serde_json::json!(settings.autosave));
    store.set("minimize_to_tray", serde_json::json!(settings.minimize_to_tray));
    store.set("autostart", serde_json::json!(settings.autostart));
    
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
async fn update_tray_setting(
    enabled: bool,
    state: State<'_, TraySettings>,
) -> Result<()> {
    let mut lock = state.0.lock().unwrap();
    *lock = enabled;
    Ok(())
}

/// State to track minimize-to-tray setting at runtime
struct TraySettings(Mutex<bool>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(WebDavState(Mutex::new(None)))
        .manage(DeviceIdState(Mutex::new(None)))
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
            let show_item = MenuItemBuilder::with_id("show", "Show Window")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;
            
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
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

