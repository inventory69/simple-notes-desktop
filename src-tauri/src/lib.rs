mod error;
mod folders;
mod local_store;
mod markdown;
mod models;
mod scheduler;
mod storage;
mod sync_engine;
mod sync_queue;
mod webdav;

use error::{AppError, Result};
use folders::{validate_folder_name, Folder};
use models::{Note, NoteMetadata, SyncStatus};
use std::sync::{Arc, Mutex};
use storage::{Credentials, Settings};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;
use webdav::WebDavClient;

pub(crate) const TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Global WebDAV Client State
pub(crate) struct WebDavState(Mutex<Option<WebDavClient>>);

/// Global Device ID
pub(crate) struct DeviceIdState(pub(crate) Mutex<Option<String>>);

/// Verhindert parallele Sync-Läufe (try_lock → Ok bei freiem Slot, Err wenn belegt).
pub(crate) struct SyncLockState(tokio::sync::Mutex<()>);

/// Recovers from a poisoned mutex by extracting the inner value.
pub(crate) fn lock_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Generiert oder lädt Device ID
pub(crate) fn get_or_create_device_id(
    app: &AppHandle,
    state: &State<DeviceIdState>,
) -> Result<String> {
    let mut device_id_lock = lock_recover(&state.0);

    if let Some(id) = &*device_id_lock {
        return Ok(id.clone());
    }

    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    if let Some(stored_id) = store.get("device_id") {
        if let Some(id_str) = stored_id.as_str() {
            *device_id_lock = Some(id_str.to_string());
            return Ok(id_str.to_string());
        }
    }

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
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<bool> {
    let folder = sync_folder.unwrap_or_else(|| "notes".to_string());
    let client = WebDavClient::new(&url, &username, &password, &folder)?;
    let success = client.test_connection().await?;

    if success {
        let _ = get_or_create_device_id(&app, &device_id_state)?;
        let mut client_lock = lock_recover(&state.0);
        *client_lock = Some(client);
        drop(client_lock);
        // Sofort synchronisieren (nicht erst nach Debounce), damit die Liste gleich aktuell ist.
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            scheduler::run_once(&app2).await;
        });
    } else {
        let _ = get_or_create_device_id(&app, &device_id_state);
    }

    Ok(success)
}

/// Abgelaufene getrashte Notiz lokal entfernen — und, falls sie eine Server-Kopie hatte,
/// eine echte Löschung (Server-Datei + Ledger-Tombstone) einreihen. Sonst würde die noch
/// am Server liegende Kopie beim nächsten Sync wieder heruntergeladen (Ping-Pong) und nie
/// vom Server entfernt. LOCAL_ONLY/DELETED_ON_SERVER-Notizen haben keine Server-Kopie.
fn purge_expired_trashed(app: &AppHandle, note: &Note) {
    if matches!(
        note.sync_status,
        SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
    ) {
        sync_queue::enqueue_deletions(app, &[(note.id.clone(), note.folder_name.clone())]);
        scheduler::trigger_sync(app);
    }
    local_store::remove_note(app, &note.id);
}

#[tauri::command]
async fn list_notes(app: AppHandle) -> Result<Vec<NoteMetadata>> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut notes = Vec::new();
    for mut note in local_store::list_notes(&app) {
        if let Some(trashed_at) = note.trashed_at {
            // Abgelaufene getrashte Notizen aufräumen (DeletedOnServer bleiben sichtbar im Trash)
            if note.sync_status != SyncStatus::DeletedOnServer
                && now - trashed_at > TRASH_RETENTION_MS
            {
                purge_expired_trashed(&app, &note);
            }
            continue; // getrashte Notizen erscheinen nicht in der Hauptliste
        }
        note.fix_note_type();
        notes.push(NoteMetadata::from(&note));
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
async fn get_note(id: String, app: AppHandle) -> Result<Note> {
    let mut note =
        local_store::get_note(&app, &id).ok_or_else(|| AppError::NoteNotFound(id.clone()))?;
    note.fix_note_type();
    Ok(note)
}

#[tauri::command]
async fn save_note(mut note: Note, app: AppHandle) -> Result<Note> {
    note.updated_at = chrono::Utc::now().timestamp_millis();
    local_store::mark_dirty(&app, &mut note);
    local_store::put_note(&app, &note);
    scheduler::trigger_sync(&app);
    Ok(note)
}

#[tauri::command]
async fn disconnect(state: State<'_, WebDavState>) -> Result<()> {
    let mut lock = lock_recover(&state.0);
    *lock = None;
    Ok(())
}

#[tauri::command]
async fn is_connected(state: State<'_, WebDavState>) -> Result<bool> {
    Ok(lock_recover(&state.0).is_some())
}

/// Reiner Verbindungstest — KEINE Seiteneffekte.
#[tauri::command]
async fn test_connection(
    url: String,
    username: String,
    password: String,
    sync_folder: Option<String>,
) -> Result<bool> {
    let folder = sync_folder.unwrap_or_else(|| "notes".to_string());
    let client = WebDavClient::new(&url, &username, &password, &folder)?;
    client.test_connection().await
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
async fn delete_note(id: String, app: AppHandle) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(mut note) = local_store::get_note(&app, &id) {
        note.trashed_at = Some(now);
        note.updated_at = now;
        local_store::mark_dirty(&app, &mut note);
        local_store::put_note(&app, &note);
        scheduler::trigger_sync(&app);
    }
    Ok(())
}

#[tauri::command]
async fn trash_note(id: String, app: AppHandle) -> Result<()> {
    delete_note(id, app).await
}

#[tauri::command]
async fn restore_note(id: String, app: AppHandle) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(mut note) = local_store::get_note(&app, &id) {
        // Ordner zwischenzeitlich gelöscht? → in den Root wiederherstellen statt in einen toten Ordner.
        if let Some(fname) = note.folder_name.clone() {
            let exists = local_store::active_folders(&app)
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&fname));
            if !exists {
                note.folder_name = None;
            }
        }
        note.trashed_at = None;
        note.updated_at = now;
        local_store::mark_dirty(&app, &mut note);
        local_store::put_note(&app, &note);
        scheduler::trigger_sync(&app);
    }
    Ok(())
}

#[tauri::command]
async fn delete_note_permanent(id: String, app: AppHandle) -> Result<()> {
    let note = local_store::get_note(&app, &id);
    local_store::remove_note(&app, &id);
    // Nur eine echte Server-Löschung einreihen, wenn die Notiz je am Server war —
    // sonst landet ein Geister-Tombstone im geteilten Ledger (das auch Android liest).
    if let Some(n) = note {
        if matches!(
            n.sync_status,
            SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
        ) {
            sync_queue::enqueue_deletions(&app, &[(id, n.folder_name)]);
        }
    }
    scheduler::trigger_sync(&app);
    Ok(())
}

#[tauri::command]
async fn list_trash(app: AppHandle) -> Result<Vec<NoteMetadata>> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut trashed: Vec<NoteMetadata> = Vec::new();
    for note in local_store::list_notes(&app) {
        if let Some(trashed_at) = note.trashed_at {
            if now - trashed_at > TRASH_RETENTION_MS {
                purge_expired_trashed(&app, &note);
            } else {
                trashed.push(NoteMetadata::from(&note));
            }
        }
    }
    trashed.sort_by_key(|n| std::cmp::Reverse(n.trashed_at));
    Ok(trashed)
}

#[tauri::command]
async fn empty_trash(app: AppHandle) -> Result<()> {
    let trashed: Vec<Note> = local_store::list_notes(&app)
        .into_iter()
        .filter(|n| n.trashed_at.is_some())
        .collect();
    // Nur Notizen mit Server-Kopie ins Lösch-Ledger einreihen (s. delete_note_permanent).
    let server_deletions: Vec<(String, Option<String>)> = trashed
        .iter()
        .filter(|n| {
            matches!(
                n.sync_status,
                SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
            )
        })
        .map(|n| (n.id.clone(), n.folder_name.clone()))
        .collect();
    for n in &trashed {
        local_store::remove_note(&app, &n.id);
    }
    sync_queue::enqueue_deletions(&app, &server_deletions);
    scheduler::trigger_sync(&app);
    Ok(())
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

    let mut map = serde_json::Map::new();
    for key in [
        "theme",
        "autosave",
        "minimize_to_tray",
        "autostart",
        "sync_folder",
        "update_notifications",
        "default_open_mode",
        "font_size",
        "offline_mode",
    ] {
        if let Some(val) = store.get(key) {
            map.insert(key.to_string(), val.clone());
        }
    }

    Ok(serde_json::from_value(serde_json::Value::Object(map)).unwrap_or_default())
}

#[tauri::command]
async fn save_settings(settings: Settings, app: AppHandle) -> Result<()> {
    let store = app
        .store("settings.json")
        .map_err(|e| AppError::StorageError(e.to_string()))?;

    let value = serde_json::to_value(&settings).map_err(|e| AppError::ParseError(e.to_string()))?;
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            store.set(k, v);
        }
    }

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

#[cfg(target_os = "windows")]
fn humanize_updater_error(e: impl ToString) -> AppError {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("error sending request")
        || lower.contains("dns error")
        || lower.contains("connection refused")
        || lower.contains("timed out")
        || lower.contains("failed to connect")
    {
        AppError::StorageError(
            "Network error while contacting the update server. Please check your connection and try again.".to_string(),
        )
    } else if lower.contains("different key") || lower.contains("signature") {
        AppError::StorageError(
            "Update signature mismatch — please reinstall manually from GitHub.".to_string(),
        )
    } else {
        AppError::StorageError(msg)
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<Option<String>> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    let update = updater.check().await.map_err(humanize_updater_error)?;
    Ok(update.map(|u| u.version))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn check_for_updates(_app: AppHandle) -> Result<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<()> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    if let Some(update) = updater.check().await.map_err(humanize_updater_error)? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(humanize_updater_error)?;
        app.exit(0);
        Ok(())
    } else {
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

#[tauri::command]
async fn update_tray_setting(enabled: bool, state: State<'_, TraySettings>) -> Result<()> {
    let mut lock = lock_recover(&state.0);
    *lock = enabled;
    Ok(())
}

#[tauri::command]
async fn color_notes(ids: Vec<String>, color: Option<String>, app: AppHandle) -> Result<()> {
    for id in &ids {
        if let Some(mut note) = local_store::get_note(&app, id) {
            note.color = color.clone();
            note.updated_at = chrono::Utc::now().timestamp_millis();
            local_store::mark_dirty(&app, &mut note);
            local_store::put_note(&app, &note);
        }
    }
    scheduler::trigger_sync(&app);
    Ok(())
}

#[tauri::command]
async fn pin_notes(ids: Vec<String>, pinned: bool, app: AppHandle) -> Result<()> {
    for id in &ids {
        if let Some(mut note) = local_store::get_note(&app, id) {
            note.is_pinned = if pinned { Some(true) } else { None };
            note.updated_at = chrono::Utc::now().timestamp_millis();
            local_store::mark_dirty(&app, &mut note);
            local_store::put_note(&app, &note);
        }
    }
    scheduler::trigger_sync(&app);
    Ok(())
}

// ── Ordner-Commands ──────────────────────────────────────────────────────────

#[tauri::command]
async fn list_folders(app: AppHandle) -> Result<Vec<Folder>> {
    let folders = local_store::active_folders(&app)
        .into_iter()
        .map(|meta| Folder {
            name: meta.name,
            color: meta.color,
            local_only: meta.local_only,
        })
        .collect();
    Ok(folders)
}

#[tauri::command]
async fn create_folder(
    name: String,
    color: Option<String>,
    local_only: bool,
    app: AppHandle,
) -> Result<Vec<Folder>> {
    if !validate_folder_name(&name) {
        return Err(AppError::WebDav(format!("Invalid folder name: {}", name)));
    }
    local_store::upsert_folder(&app, &name, color, false, local_only);
    if !local_only {
        scheduler::trigger_sync(&app);
    }
    list_folders(app).await
}

#[tauri::command]
async fn rename_folder(old_name: String, new_name: String, app: AppHandle) -> Result<Vec<Folder>> {
    if !validate_folder_name(&new_name) {
        return Err(AppError::WebDav(format!(
            "Invalid folder name: {}",
            new_name
        )));
    }
    let now = chrono::Utc::now().timestamp_millis();
    // Notizen mit Server-Kopie sammeln — für Move-Cleanup der alten Server-Pfade
    let to_move: Vec<(String, Option<String>)> = local_store::list_notes(&app)
        .into_iter()
        .filter(|n| {
            n.folder_name
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(&old_name))
                .unwrap_or(false)
                && matches!(
                    n.sync_status,
                    SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
                )
        })
        .map(|n| (n.id, Some(old_name.clone())))
        .collect();

    // Ordner-Meta + folder_name aller Notizen umbenennen
    local_store::rename_folder(&app, &old_name, &new_name);

    // Umbenannte Notizen als PENDING markieren (müssen zum neuen Pfad hochgeladen werden)
    for note in local_store::list_notes(&app) {
        if note
            .folder_name
            .as_deref()
            .map(|f| f.eq_ignore_ascii_case(&new_name))
            .unwrap_or(false)
        {
            let mut n = note;
            n.updated_at = now;
            local_store::mark_dirty(&app, &mut n);
            local_store::put_note(&app, &n);
        }
    }

    if !to_move.is_empty() {
        sync_queue::enqueue_move_deletions(&app, &to_move);
    }
    scheduler::trigger_sync(&app);
    list_folders(app).await
}

#[tauri::command]
async fn delete_folder(name: String, keep_notes: bool, app: AppHandle) -> Result<Vec<Folder>> {
    let now = chrono::Utc::now().timestamp_millis();
    let is_local = local_store::is_local_only(&app, Some(&name));
    let notes: Vec<_> = local_store::list_notes(&app)
        .into_iter()
        .filter(|n| {
            n.folder_name
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(&name))
                .unwrap_or(false)
        })
        .collect();

    if keep_notes {
        for note in &notes {
            if note.trashed_at.is_some() {
                continue;
            }
            // Alte Server-Datei aufräumen wenn Notiz synchronisiert war
            if !is_local
                && matches!(
                    note.sync_status,
                    SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
                )
            {
                sync_queue::enqueue_move_deletions(&app, &[(note.id.clone(), Some(name.clone()))]);
            }
            let mut n = note.clone();
            n.folder_name = None;
            n.updated_at = now;
            local_store::mark_dirty(&app, &mut n);
            local_store::put_note(&app, &n);
        }
    } else {
        for note in &notes {
            if note.trashed_at.is_some() {
                continue;
            }
            let mut n = note.clone();
            n.trashed_at = Some(now);
            n.updated_at = now;
            local_store::mark_dirty(&app, &mut n);
            local_store::put_note(&app, &n);
        }
    }

    local_store::upsert_folder(&app, &name, None, true, is_local);
    if !is_local {
        sync_queue::enqueue_folder_tombstone(&app, &name);
    }
    scheduler::trigger_sync(&app);
    list_folders(app).await
}

#[tauri::command]
async fn set_folder_color(
    name: String,
    color: Option<String>,
    app: AppHandle,
) -> Result<Vec<Folder>> {
    local_store::set_folder_color(&app, &name, color);
    let is_local = local_store::is_local_only(&app, Some(&name));
    if !is_local {
        scheduler::trigger_sync(&app);
    }
    list_folders(app).await
}

#[tauri::command]
async fn move_notes(ids: Vec<String>, target_folder: Option<String>, app: AppHandle) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut move_deletions: Vec<(String, Option<String>)> = Vec::new();
    for id in &ids {
        if let Some(mut note) = local_store::get_note(&app, id) {
            // Alte Server-Datei aufräumen wenn Notiz eine Server-Kopie hatte und Ordner wechselt
            // (Synced | Pending | Conflict — konsistent mit rename_folder/delete_folder).
            if matches!(
                note.sync_status,
                SyncStatus::Synced | SyncStatus::Pending | SyncStatus::Conflict
            ) && note.folder_name.as_deref().map(str::to_lowercase)
                != target_folder.as_deref().map(str::to_lowercase)
            {
                move_deletions.push((note.id.clone(), note.folder_name.clone()));
            }
            note.folder_name = target_folder.clone();
            note.updated_at = now;
            local_store::mark_dirty(&app, &mut note);
            local_store::put_note(&app, &note);
        }
    }
    if !move_deletions.is_empty() {
        sync_queue::enqueue_move_deletions(&app, &move_deletions);
    }
    scheduler::trigger_sync(&app);
    Ok(())
}

#[tauri::command]
async fn set_folder_local_only(
    name: String,
    local_only: bool,
    remove_from_server: bool,
    app: AppHandle,
) -> Result<Vec<Folder>> {
    if local_only {
        // Ordner als local-only markieren — Sync-Engine überspringt ihn künftig
        let color = local_store::active_folders(&app)
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(&name))
            .and_then(|f| f.color);
        local_store::upsert_folder(&app, &name, color, false, true);

        if remove_from_server {
            // Server-Kopien löschen + Ordner-Tombstone
            let ids: Vec<(String, Option<String>)> = local_store::list_notes(&app)
                .into_iter()
                .filter(|n| {
                    n.folder_name
                        .as_deref()
                        .map(|f| f.eq_ignore_ascii_case(&name))
                        .unwrap_or(false)
                        && matches!(n.sync_status, SyncStatus::Synced | SyncStatus::Pending)
                })
                .map(|n| (n.id, Some(name.clone())))
                .collect();
            sync_queue::enqueue_deletions(&app, &ids);
            sync_queue::enqueue_folder_tombstone(&app, &name);
            scheduler::trigger_sync(&app);
        }
    } else {
        // Ordner wieder in den Sync aufnehmen
        sync_queue::cancel_folder_deletions(&app, &name);
        let now = chrono::Utc::now().timestamp_millis();
        for note in local_store::list_notes(&app) {
            if note
                .folder_name
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(&name))
                .unwrap_or(false)
                && note.trashed_at.is_none()
            {
                let mut n = note;
                n.updated_at = now;
                n.sync_status = SyncStatus::Pending;
                local_store::put_note(&app, &n);
            }
        }
        let color = local_store::active_folders(&app)
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(&name))
            .and_then(|f| f.color);
        local_store::upsert_folder(&app, &name, color, false, false);
        scheduler::trigger_sync(&app);
    }
    list_folders(app).await
}

#[tauri::command]
async fn sync(
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
    sync_lock: State<'_, SyncLockState>,
) -> Result<()> {
    let _guard = match sync_lock.0.try_lock() {
        Ok(g) => g,
        Err(_) => return Ok(()),
    };
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = match client {
        Some(c) => c,
        None => return Ok(()),
    };
    let device_id = get_or_create_device_id(&app, &device_id_state)?;
    sync_engine::run_sync(&client, &app, &device_id, TRASH_RETENTION_MS).await;
    let _ = app.emit("notes-synced", ());
    Ok(())
}

#[tauri::command]
async fn resolve_conflict(
    id: String,
    resolution: String,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    match resolution.as_str() {
        "keep_mine" => {
            if let Some(mut note) = local_store::get_note(&app, &id) {
                note.sync_status = SyncStatus::Pending;
                note.updated_at = now;
                local_store::put_note(&app, &note);
                scheduler::trigger_sync(&app);
            }
        }
        "use_server" => {
            let client = {
                let lock = lock_recover(&state.0);
                lock.clone()
            };
            let client = client.ok_or(AppError::NotConnected)?;
            let folder = local_store::get_note(&app, &id).and_then(|n| n.folder_name);
            let mut note = client.get_note(&id, folder.as_deref()).await?;
            note.sync_status = SyncStatus::Synced;
            local_store::put_note(&app, &note);
        }
        other => {
            return Err(AppError::WebDav(format!(
                "Ungültige Konflikt-Auflösung: {}",
                other
            )));
        }
    }
    Ok(())
}

/// State to track minimize-to-tray setting at runtime
struct TraySettings(Mutex<bool>);

/// Zeigt das (per `visible: false` initial versteckte) Hauptfenster. Vom Frontend aufgerufen,
/// sobald die Splash-Markup im DOM ist — verhindert einen weißen Frame vor dem ersten Paint.
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }
}

fn restore_window(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;

        if let Ok(gtk_window) = window.gtk_window() {
            gtk_window.present();
        }

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
    // ponytail: Arc<Notify> für den Scheduler — einmal erstellt, dann geteilt zw. manage() und spawn()
    let notify = Arc::new(tokio::sync::Notify::new());
    let notify_for_manage = notify.clone();

    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
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
            #[cfg(debug_assertions)]
            eprintln!(
                "[SingleInstance] Second instance detected with args: {:?}",
                args
            );
            if let Some(window) = app.get_webview_window("main") {
                restore_window(&window);
            }
        }));

    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .manage(WebDavState(Mutex::new(None)))
        .manage(DeviceIdState(Mutex::new(None)))
        .manage(TraySettings(Mutex::new(false)))
        .manage(SyncLockState(tokio::sync::Mutex::new(())))
        .manage(scheduler::SyncTrigger(notify_for_manage))
        .setup(move |app| {
            // Einmalige Migration: note_cache → local_store
            local_store::migrate_from_note_cache(app.handle());

            // Einmalige Migration: Upgrade von einer Version ohne Offline-Modus.
            // Fehlt der offline_mode-Key, aber Credentials sind vorhanden, war der Nutzer
            // vorher online → online lassen (sonst erscheint die Notizliste nach dem Update leer).
            if let Ok(store) = app.store("settings.json") {
                let has_offline_key = store.get("offline_mode").is_some();
                let has_credentials = store.get("server_url").is_some();
                if !has_offline_key && has_credentials {
                    store.set("offline_mode", serde_json::json!(false));
                    let _ = store.save();
                }
            }

            // Hintergrund-Sync starten
            scheduler::spawn(app.handle().clone(), notify.clone());

            // Minimize-to-tray und Autostart aus gespeicherten Settings laden
            if let Ok(store) = app.store("settings.json") {
                let minimize = store
                    .get("minimize_to_tray")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let state = app.state::<TraySettings>();
                *lock_recover(&state.0) = minimize;

                let autostart = store
                    .get("autostart")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if autostart {
                    let _ = app.autolaunch().enable();
                }
            }

            // Tray-Menü aufbauen
            let show_item = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let icon_bytes = include_bytes!("../icons/32x32.png");
            let icon = tauri::image::Image::from_bytes(icon_bytes)
                .expect("failed to load tray icon")
                .to_owned();

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

            let window_icon_bytes = include_bytes!("../icons/128x128.png");
            let window_icon = tauri::image::Image::from_bytes(window_icon_bytes)
                .expect("failed to load window icon")
                .to_owned();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(window_icon);

                #[cfg(target_os = "linux")]
                {
                    use gtk::prelude::{GtkWindowExt, WidgetExt};
                    if let Ok(gtk_window) = window.gtk_window() {
                        gtk_window.set_titlebar(Option::<&gtk::Widget>::None);

                        if std::env::var("WAYLAND_DISPLAY").is_ok() {
                            extern "C" {
                                fn gdk_wayland_window_set_application_id(
                                    window: *mut std::ffi::c_void,
                                    application_id: *const std::ffi::c_char,
                                );
                            }

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

                // ponytail: Sicherheitsnetz, falls das Frontend show_main_window nie aufruft
                // (JS-Fehler o.ä.) — sonst bliebe das Fenster (visible: false) für immer versteckt.
                let window_for_timeout = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if let Ok(false) = window_for_timeout.is_visible() {
                        let _ = window_for_timeout.show();
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let tray_settings = app.state::<TraySettings>();
                let minimize = *lock_recover(&tray_settings.0);

                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            test_connection,
            is_connected,
            list_notes,
            get_note,
            save_note,
            create_note,
            delete_note,
            trash_note,
            restore_note,
            delete_note_permanent,
            list_trash,
            empty_trash,
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
            list_folders,
            create_folder,
            rename_folder,
            delete_folder,
            set_folder_color,
            set_folder_local_only,
            move_notes,
            sync,
            resolve_conflict,
            show_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
