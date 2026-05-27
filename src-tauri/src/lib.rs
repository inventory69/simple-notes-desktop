mod error;
mod markdown;
mod models;
mod storage;
mod webdav;

use error::{AppError, Result};
use models::{Note, NoteMetadata};
use std::sync::Mutex;
use storage::{Credentials, Settings};
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

/// Recovers from a poisoned mutex by extracting the inner value.
/// A panic while holding a lock is rare in this app (no heavy work inside critical
/// sections) but would otherwise leave the mutex permanently unusable.
fn lock_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Generiert oder lädt Device ID
fn get_or_create_device_id(app: &AppHandle, state: &State<DeviceIdState>) -> Result<String> {
    let mut device_id_lock = lock_recover(&state.0);

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
    store
        .save()
        .map_err(|e| AppError::StorageError(e.to_string()))?;

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
        let mut client_lock = lock_recover(&state.0);
        *client_lock = Some(client);
    }

    Ok(success)
}

#[tauri::command]
async fn list_notes(state: State<'_, WebDavState>) -> Result<Vec<NoteMetadata>> {
    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    let ids = client.list_json_files().await?;

    let mut notes = Vec::new();
    for id in ids {
        match client.get_note(&id).await {
            Ok(note) => notes.push(NoteMetadata::from(&note)),
            Err(e) => eprintln!("[list_notes] failed to load {}: {}", id, e),
        }
    }

    notes.sort_by(|a, b| {
        let a_pin = a.is_pinned.unwrap_or(false);
        let b_pin = b.is_pinned.unwrap_or(false);
        b_pin
            .cmp(&a_pin)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    Ok(notes)
}

#[tauri::command]
async fn get_note(id: String, state: State<'_, WebDavState>) -> Result<Note> {
    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    client.get_note(&id).await
}

#[tauri::command]
async fn save_note(mut note: Note, state: State<'_, WebDavState>) -> Result<Note> {
    // Update timestamp to NOW before saving
    note.updated_at = chrono::Utc::now().timestamp_millis();

    #[cfg(debug_assertions)]
    eprintln!("[Save] Saving note '{}' (ID: {})", note.title, note.id);
    #[cfg(debug_assertions)]
    eprintln!("[Save] Updated timestamp: {}", note.updated_at);

    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    client.save_note(&note).await?;

    #[cfg(debug_assertions)]
    eprintln!("[Save] Note saved successfully to WebDAV");

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
        let client_lock = lock_recover(&state.0);
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

    // Read each Settings key individually so missing keys fall back to Settings::default()
    // rather than failing deserialization. This list MUST stay in sync with the fields
    // of the Settings struct in storage.rs — see test_get_settings_keys_match_settings_struct
    // in storage.rs which enforces this at test time.
    let mut map = serde_json::Map::new();
    for key in [
        "theme",
        "autosave",
        "minimize_to_tray",
        "autostart",
        "sync_folder",
    ] {
        if let Some(val) = store.get(key) {
            map.insert(key.to_string(), val.clone());
        }
    }

    // Missing keys are filled from Settings::default() via #[serde(default)]
    Ok(serde_json::from_value(serde_json::Value::Object(map)).unwrap_or_default())
}

#[tauri::command]
async fn save_settings(settings: Settings, app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    // Derive store keys from the Settings struct itself so adding a new field
    // to the struct automatically persists it without a separate hand-typed entry.
    let value = serde_json::to_value(&settings).map_err(|e| AppError::ParseError(e.to_string()))?;
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            store.set(k, v);
        }
    }

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

/// Aktuelles Betriebssystem zurückgeben (für plattformspezifische UI-Logik im Frontend)
#[tauri::command]
fn get_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}

/// Prüft ob ein In-App-Update verfügbar ist.
/// Gibt die neue Versionsnummer zurück oder None wenn aktuell.
/// Auf Linux ist diese Funktion deaktiviert — Updates laufen über den Paketmanager.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<Option<String>> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater()
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    let update = updater
        .check()
        .await
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    Ok(update.map(|u| u.version))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn check_for_updates(_app: AppHandle) -> Result<Option<String>> {
    // Auf Linux/macOS übernimmt der Paketmanager die Updates
    Ok(None)
}

/// Lädt das Update herunter und installiert es (nur Windows).
/// Nach erfolgreicher Installation wird die App beendet;
/// der Installer startet die neue Version automatisch.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<()> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater()
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    if let Some(update) = updater
        .check()
        .await
        .map_err(|e| AppError::StorageError(e.to_string()))?
    {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| AppError::StorageError(e.to_string()))?;
        // Installer wurde gestartet; App beenden damit der Installer die Binary ersetzen kann
        app.exit(0);
        Ok(())
    } else {
        // Update zwischen Check und Install verschwunden (Race Condition, Release zurückgezogen etc.)
        Err(AppError::StorageError(
            "Update no longer available. Please click 'Check for Updates' again.".to_string(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn install_update(_app: AppHandle) -> Result<()> {
    Err(AppError::StorageError(
        "In-app updates are not available on this platform. Please use your package manager."
            .to_string(),
    ))
}

/// Update the minimize-to-tray runtime setting without restarting
#[tauri::command]
async fn update_tray_setting(enabled: bool, state: State<'_, TraySettings>) -> Result<()> {
    let mut lock = lock_recover(&state.0);
    *lock = enabled;
    Ok(())
}

/// Farbe mehrerer Notizen setzen oder entfernen
#[tauri::command]
async fn color_notes(
    ids: Vec<String>,
    color: Option<String>,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;

    for id in ids {
        match client.get_note(&id).await {
            Ok(mut note) => {
                note.color = color.clone();
                note.updated_at = chrono::Utc::now().timestamp_millis();
                if let Err(e) = client.save_note(&note).await {
                    eprintln!("[color_notes] save failed for {}: {}", id, e);
                }
            }
            Err(e) => eprintln!("[color_notes] get failed for {}: {}", id, e),
        }
    }
    Ok(())
}

/// Mehrere Notizen auf einmal an-/abpinnen
#[tauri::command]
async fn pin_notes(ids: Vec<String>, pinned: bool, state: State<'_, WebDavState>) -> Result<()> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;

    for id in ids {
        match client.get_note(&id).await {
            Ok(mut note) => {
                // None statt Some(false) beim Lösen — Android-kompatibel
                note.is_pinned = if pinned { Some(true) } else { None };
                note.updated_at = chrono::Utc::now().timestamp_millis();
                if let Err(e) = client.save_note(&note).await {
                    eprintln!("[pin_notes] save failed for {}: {}", id, e);
                }
            }
            Err(e) => eprintln!("[pin_notes] get failed for {}: {}", id, e),
        }
    }
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
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Position-Restore deaktiviert: Wayland erlaubt Apps nicht,
                // ihre eigene Position zu setzen; gespeicherte Koordinaten
                // würden vom Compositor ignoriert oder das Fenster off-screen legen.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
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
        }));

    // Windows-only: In-App-Updater — Linux-Nutzer verwenden AUR/deb/rpm
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
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
                *lock_recover(&state.0) = minimize;

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
                    use gtk::prelude::{GtkWindowExt, WidgetExt};
                    if let Ok(gtk_window) = window.gtk_window() {
                        gtk_window.set_titlebar(Option::<&gtk::Widget>::None);

                        // Bug 2 Fix: Wayland-Taskleiste zeigt generisches Icon statt App-Icon.
                        // Tauri setzt die GTK-GApplication-ID auf den Identifier
                        // "com.inventory69.simple-notes-desktop", der direkt als Wayland
                        // xdg_toplevel app_id landet. KDE sucht dann nach
                        // "com.inventory69.simple-notes-desktop.desktop", installiert ist
                        // aber "simple-notes-desktop.desktop" → kein Match → falsches Icon.
                        // gdk_wayland_window_set_application_id() (GTK ≥ 3.24.22) setzt die
                        // app_id direkt am GDK-Surface und hat Vorrang vor der GApplication-ID.
                        // Beide Pfade: Fenster bereits realized (GTK-Warning oben) oder noch nicht.
                        if std::env::var("WAYLAND_DISPLAY").is_ok() {
                            extern "C" {
                                fn gdk_wayland_window_set_application_id(
                                    window: *mut std::ffi::c_void,
                                    application_id: *const std::ffi::c_char,
                                );
                            }

                            // Fenster ist beim Tauri-Setup bereits realized → direkt anwenden.
                            if let Some(gdk_win) = gtk_window.window() {
                                use glib::translate::ToGlibPtr;
                                let app_id =
                                    std::ffi::CString::new("simple-notes-desktop").unwrap();
                                let raw: *mut gtk::gdk::ffi::GdkWindow = gdk_win.to_glib_none().0;
                                unsafe {
                                    gdk_wayland_window_set_application_id(
                                        raw as *mut std::ffi::c_void,
                                        app_id.as_ptr(),
                                    );
                                }
                            } else {
                                // Fallback: noch nicht realized – beim Realize-Signal anwenden.
                                gtk_window.connect_realize(|w| {
                                    use glib::translate::ToGlibPtr;
                                    if let Some(gdk_win) = w.window() {
                                        let app_id =
                                            std::ffi::CString::new("simple-notes-desktop").unwrap();
                                        let raw: *mut gtk::gdk::ffi::GdkWindow =
                                            gdk_win.to_glib_none().0;
                                        unsafe {
                                            gdk_wayland_window_set_application_id(
                                                raw as *mut std::ffi::c_void,
                                                app_id.as_ptr(),
                                            );
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let tray_settings = app.state::<TraySettings>();
                let minimize = *lock_recover(&tray_settings.0);

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
            pin_notes,
            color_notes,
            get_platform,
            check_for_updates,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
