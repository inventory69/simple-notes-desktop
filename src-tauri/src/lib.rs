mod error;
mod folders;
mod local_store;
mod markdown;
mod models;
mod storage;
mod sync_engine;
mod sync_queue;
mod webdav;

use error::{AppError, Result};
use folders::{validate_folder_name, Folder, FolderMeta};
use models::{Note, NoteMetadata, SyncStatus};
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

const TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Global WebDAV Client State
struct WebDavState(Mutex<Option<WebDavClient>>);

/// Global Device ID
struct DeviceIdState(Mutex<Option<String>>);

/// Verhindert parallele Sync-Läufe (try_lock → Ok bei freiem Slot, Err wenn belegt).
/// tokio::sync::Mutex wird verwendet, damit MutexGuard Send ist (Tauri-Command-Anforderung).
struct SyncLockState(tokio::sync::Mutex<()>);

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

// ============ PRIVATE HELPERS ============

async fn fetch_all_notes(client: &WebDavClient) -> Result<Vec<models::Note>> {
    let note_locations = client.list_notes_with_folders().await?;
    let mut notes = Vec::new();
    for (id, folder) in note_locations {
        match client.get_note(&id, folder.as_deref()).await {
            Ok(note) => notes.push(note),
            Err(e) => eprintln!("[fetch_all_notes] failed to load {}: {}", id, e),
        }
    }
    Ok(notes)
}

/// Löscht eine Notiz permanent vom Server und schreibt einen Eintrag ins Lösch-Ledger.
/// Der Delete muss gelingen; das Ledger-Write ist best-effort (Fehler werden geloggt).
async fn purge_note(client: &WebDavClient, note: &Note, device_id: &str, now: i64) -> Result<()> {
    client.delete_note(note).await?;
    client
        .append_deletion(&note.id, device_id, now, TRASH_RETENTION_MS)
        .await;
    Ok(())
}

async fn purge_expired_trash(client: &WebDavClient, device_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    let notes = match fetch_all_notes(client).await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[purge_expired_trash] fetch failed: {}", e);
            return;
        }
    };
    for note in notes {
        if let Some(trashed_at) = note.trashed_at {
            if now - trashed_at > TRASH_RETENTION_MS {
                if let Err(e) = purge_note(client, &note, device_id, now).await {
                    eprintln!("[purge_expired_trash] purge {} failed: {}", note.id, e);
                }
            }
        }
    }
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
        // Device-ID vorinitialisieren (vermeidet Race beim ersten Lösch-Ledger-Write)
        let device_id = get_or_create_device_id(&app, &device_id_state)?;

        // Phase 2: Offline-Queue beim Verbinden abarbeiten
        sync_queue::drain_sync_queue(&client, &app, &device_id, TRASH_RETENTION_MS).await;

        let mut client_lock = lock_recover(&state.0);
        *client_lock = Some(client);
    } else {
        // Device-ID auch ohne erfolgreichen Connect initialisieren
        let _ = get_or_create_device_id(&app, &device_id_state);
    }

    Ok(success)
}

#[tauri::command]
async fn list_notes(
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<Vec<NoteMetadata>> {
    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let mut notes: Vec<NoteMetadata> = Vec::new();

    // Server-Notizen nur wenn verbunden — lokale Ordner funktionieren auch offline.
    if let Some(client) = client {
        let device_id = get_or_create_device_id(&app, &device_id_state)?;

        // Best-effort auto-purge: remove notes older than retention before listing
        purge_expired_trash(&client, &device_id).await;

        let all_notes = fetch_all_notes(&client).await?;

        // Zombie-Schutz: Notizen, die auf einem anderen Gerät permanent gelöscht wurden,
        // sollen nicht als "aktiv" aufgelistet werden. Das Lösch-Ledger wird einmal gelesen;
        // für jeden Treffer (updatedAt ≤ deletedAt) wird die verwaiste Datei bereinigt.
        let ledger = client.read_deletions().await;
        let deletion_map: std::collections::HashMap<&str, i64> = ledger
            .deleted_notes
            .iter()
            .map(|r| (r.id.as_str(), r.deleted_at))
            .collect();

        for note in &all_notes {
            if note.trashed_at.is_some() {
                continue;
            }
            // Phase 3: Server-Notizen aus lokal-exklusiven Ordnern überspringen.
            // "Keep on server"-Ausschluss lässt Server-Kopie bestehen; Notizen werden
            // aus dem lokalen Store bedient, um Duplikate zu vermeiden.
            if local_store::is_local_only(&app, note.folder_name.as_deref()) {
                continue;
            }
            if let Some(&deleted_at) = deletion_map.get(note.id.as_str()) {
                if note.updated_at <= deleted_at {
                    // Verwaiste Datei — vom Server entfernen
                    if let Err(e) = client.delete_note(note).await {
                        eprintln!("[list_notes] zombie cleanup {} failed: {}", note.id, e);
                    }
                    continue;
                }
            }
            notes.push(NoteMetadata::from(note));
        }
    }

    // Phase 6: Konflikt-/DeletedOnServer-Status aus Sync-Cache in Metadaten einbetten.
    // Der Cache wird vom `sync`-Command befüllt; ohne Cache-Eintrag bleibt der Status SYNCED.
    let note_cache = sync_engine::load_note_cache(&app);
    for meta in notes.iter_mut() {
        if let Some(entry) = note_cache.get(&meta.id) {
            if matches!(
                entry.note.sync_status,
                SyncStatus::Conflict | SyncStatus::DeletedOnServer
            ) {
                meta.sync_status = entry.note.sync_status;
            }
        }
    }

    // Lokale aktive Notizen anhängen (auch offline). Aktive Ordner einmal vorberechnen
    // (sonst O(n²): is_local_only deserialisiert die Ordnerliste je Notiz neu).
    let now = chrono::Utc::now().timestamp_millis();
    let active: std::collections::HashSet<String> = local_store::active_folders(&app)
        .into_iter()
        .map(|f| f.name.to_lowercase())
        .collect();
    for mut note in local_store::list_notes(&app) {
        if let Some(trashed_at) = note.trashed_at {
            // Abgelaufene lokale Papierkorb-Notizen aufräumen (Server-Parität).
            if now - trashed_at > TRASH_RETENTION_MS {
                local_store::remove_note(&app, &note.id);
            }
            continue;
        }
        let in_active = note
            .folder_name
            .as_deref()
            .map(|f| active.contains(&f.to_lowercase()))
            .unwrap_or(false);
        if !in_active {
            continue;
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
async fn get_note(
    id: String,
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<Note> {
    // Lokale Notiz? (nach ID, unabhängig vom aktuellen Ordner-Status)
    if let Some(mut note) = local_store::get_note(&app, &id) {
        note.fix_note_type();
        return Ok(note);
    }

    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    client.get_note(&id, folder_name.as_deref()).await
}

#[tauri::command]
async fn save_note(mut note: Note, app: AppHandle, state: State<'_, WebDavState>) -> Result<Note> {
    // Update timestamp to NOW before saving
    note.updated_at = chrono::Utc::now().timestamp_millis();

    // Lokaler Ordner (neue Notiz) ODER bereits lokal gespeicherte Notiz → lokaler Store.
    // Die ID-Prüfung hält eine Notiz lokal, auch wenn sich der Ordner-Status zwischenzeitlich
    // ändert (verhindert ein versehentliches Schreiben zum Server).
    if local_store::is_local_only(&app, note.folder_name.as_deref())
        || local_store::has_note(&app, &note.id)
    {
        local_store::put_note(&app, &note);
        return Ok(note);
    }

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
async fn delete_note(
    id: String,
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    // Soft-delete: in den Papierkorb verschieben statt permanent löschen (Android-Parität)
    let now = chrono::Utc::now().timestamp_millis();

    // Lokale Notiz (nach ID) → lokal in den Papierkorb (auch offline)
    if let Some(mut note) = local_store::get_note(&app, &id) {
        note.trashed_at = Some(now);
        note.updated_at = now;
        local_store::put_note(&app, &note);
        return Ok(());
    }

    let client = {
        let client_lock = lock_recover(&state.0);
        client_lock.clone()
    };

    let client = client.ok_or(AppError::NotConnected)?;
    let mut note = client.get_note(&id, folder_name.as_deref()).await?;
    note.trashed_at = Some(now);
    note.updated_at = now;
    client.save_note(&note).await
}

#[tauri::command]
async fn trash_note(
    id: String,
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();

    // Lokale Notiz (nach ID) → lokal in den Papierkorb (auch offline)
    if let Some(mut note) = local_store::get_note(&app, &id) {
        note.trashed_at = Some(now);
        note.updated_at = now;
        local_store::put_note(&app, &note);
        return Ok(());
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;
    let mut note = client.get_note(&id, folder_name.as_deref()).await?;
    note.trashed_at = Some(now);
    note.updated_at = now;
    client.save_note(&note).await
}

#[tauri::command]
async fn restore_note(
    id: String,
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();

    // Lokale Notiz (nach ID) wiederherstellen
    if let Some(mut note) = local_store::get_note(&app, &id) {
        note.trashed_at = None;
        note.updated_at = now;
        // Ordner nicht mehr local-only? → zum Server migrieren (nur wenn verbunden).
        let folder_local = local_store::is_local_only(&app, note.folder_name.as_deref());
        if !folder_local {
            let client = {
                let lock = lock_recover(&state.0);
                lock.clone()
            };
            if let Some(client) = client {
                // Erst NACH erfolgreichem Server-Upload lokal entfernen — sonst Datenverlust,
                // wenn save_note fehlschlägt.
                // folder_name bleibt erhalten, damit die Notiz im richtigen Server-Ordner landet.
                client.save_note(&note).await?;
                local_store::remove_note(&app, &id);
                return Ok(());
            }
            // Offline: Notiz lokal behalten (bleibt wiederherstellbar, sobald wieder verbunden).
        }
        local_store::put_note(&app, &note);
        return Ok(());
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;
    let mut note = client.get_note(&id, folder_name.as_deref()).await?;
    note.trashed_at = None;
    note.updated_at = now;

    // Fallback: Falls der Original-Ordner nicht mehr existiert, Notiz nach Root verschieben
    if let Some(ref fname) = note.folder_name.clone() {
        let existing_folders = client.discover_folders().await;
        let folder_exists = existing_folders
            .iter()
            .any(|f| f.eq_ignore_ascii_case(fname));
        if !folder_exists {
            note.folder_name = None;
        }
    }

    client.save_note(&note).await
}

#[tauri::command]
async fn delete_note_permanent(
    id: String,
    folder_name: Option<String>,
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<()> {
    // Lokale Notiz (nach ID) endgültig entfernen (auch offline)
    if local_store::has_note(&app, &id) {
        local_store::remove_note(&app, &id);
        return Ok(());
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;
    let device_id = get_or_create_device_id(&app, &device_id_state)?;
    let note = client.get_note(&id, folder_name.as_deref()).await?;
    let now = chrono::Utc::now().timestamp_millis();
    purge_note(&client, &note, &device_id, now).await
}

#[tauri::command]
async fn list_trash(
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<Vec<NoteMetadata>> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    let now = chrono::Utc::now().timestamp_millis();
    let mut trashed: Vec<NoteMetadata> = Vec::new();

    // Server-Papierkorb nur wenn verbunden — lokaler Papierkorb funktioniert auch offline.
    if let Some(client) = client {
        let device_id = get_or_create_device_id(&app, &device_id_state)?;
        let all_notes = fetch_all_notes(&client).await?;

        // Abgelaufene Tombstones zuerst entfernen und ins Lösch-Ledger schreiben
        for note in &all_notes {
            if let Some(trashed_at) = note.trashed_at {
                if now - trashed_at > TRASH_RETENTION_MS {
                    if let Err(e) = purge_note(&client, note, &device_id, now).await {
                        eprintln!("[list_trash] purge {} failed: {}", note.id, e);
                    }
                }
            }
        }

        trashed = all_notes
            .iter()
            .filter(|n| {
                n.trashed_at
                    .map(|t| now - t <= TRASH_RETENTION_MS)
                    .unwrap_or(false)
            })
            .map(NoteMetadata::from)
            .collect();
    }

    // Lokale getrashte Notizen einbeziehen (auch offline)
    for note in local_store::list_notes(&app) {
        if let Some(trashed_at) = note.trashed_at {
            if now - trashed_at > TRASH_RETENTION_MS {
                local_store::remove_note(&app, &note.id);
            } else {
                trashed.push(NoteMetadata::from(&note));
            }
        }
    }

    trashed.sort_by_key(|n| std::cmp::Reverse(n.trashed_at));
    Ok(trashed)
}

#[tauri::command]
async fn empty_trash(
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let now = chrono::Utc::now().timestamp_millis();

    // Server-Papierkorb nur wenn verbunden — lokaler Papierkorb wird auch offline geleert.
    if let Some(client) = client {
        let device_id = get_or_create_device_id(&app, &device_id_state)?;
        let all_notes = fetch_all_notes(&client).await?;
        for note in all_notes {
            if note.trashed_at.is_some() {
                if let Err(e) = purge_note(&client, &note, &device_id, now).await {
                    eprintln!("[empty_trash] purge {} failed: {}", note.id, e);
                }
            }
        }
    }

    // Lokale getrashte Notizen löschen (auch offline)
    for note in local_store::list_notes(&app) {
        if note.trashed_at.is_some() {
            local_store::remove_note(&app, &note.id);
        }
    }

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
        "update_notifications",
        "default_open_mode",
        "font_size",
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

/// Übersetzt rohe Updater-Fehlermeldungen in nutzbare Hinweise.
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

/// Prüft ob ein In-App-Update verfügbar ist.
/// Gibt die neue Versionsnummer zurück oder None wenn aktuell.
/// Auf Linux ist diese Funktion deaktiviert — Updates laufen über den Paketmanager.
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
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::StorageError(e.to_string()))?;
    if let Some(update) = updater.check().await.map_err(humanize_updater_error)? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(humanize_updater_error)?;
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
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    // Lokaler Ordner: nur den lokalen Store mutieren, nie zum Server
    if local_store::is_local_only(&app, folder_name.as_deref()) {
        for id in &ids {
            if let Some(mut note) = local_store::get_note(&app, id) {
                note.color = color.clone();
                note.updated_at = chrono::Utc::now().timestamp_millis();
                local_store::put_note(&app, &note);
            }
        }
        return Ok(());
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;

    for id in ids {
        match client.get_note(&id, folder_name.as_deref()).await {
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
async fn pin_notes(
    ids: Vec<String>,
    pinned: bool,
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    // Lokaler Ordner: nur den lokalen Store mutieren, nie zum Server
    if local_store::is_local_only(&app, folder_name.as_deref()) {
        for id in &ids {
            if let Some(mut note) = local_store::get_note(&app, id) {
                // None statt Some(false) beim Lösen — Android-kompatibel
                note.is_pinned = if pinned { Some(true) } else { None };
                note.updated_at = chrono::Utc::now().timestamp_millis();
                local_store::put_note(&app, &note);
            }
        }
        return Ok(());
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    let client = client.ok_or(AppError::NotConnected)?;

    for id in ids {
        match client.get_note(&id, folder_name.as_deref()).await {
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

// ── Ordner-Commands ──────────────────────────────────────────────────────────

/// Hilfsfunktion: Aktive (nicht tombstoned) Ordner als Folder-Liste sortiert nach Name.
/// Vereinigt `folders.json`-Einträge mit auf dem Server entdeckten Ordnern.
async fn build_folder_list(client: &WebDavClient) -> Vec<Folder> {
    let meta = client.read_folders_meta().await;
    let discovered = client.discover_folders().await;

    let mut names: Vec<(String, Option<String>)> = Vec::new();

    // Zuerst Meta-Einträge (nicht gelöscht)
    for m in &meta {
        if !m.deleted {
            names.push((m.name.clone(), m.color.clone()));
        }
    }

    // Dann auf dem Server entdeckte Ordner hinzufügen, die noch nicht in Meta sind
    for d in &discovered {
        let already = meta.iter().any(|m| m.name.eq_ignore_ascii_case(d));
        if !already {
            let name_exists = names.iter().any(|(n, _)| n.eq_ignore_ascii_case(d));
            if !name_exists {
                names.push((d.clone(), None));
            }
        }
    }

    names.sort_by_key(|(a, _)| a.to_lowercase());
    names
        .into_iter()
        .map(|(name, color)| Folder {
            name,
            color,
            local_only: false,
        })
        .collect()
}

/// Server-Ordner (sofern verbunden) + lokale Ordner zusammenführen und alphabetisch sortieren.
///
/// Phase 3: Server-Ordner, die lokal als local-only markiert sind, erhalten `local_only = true`
/// ("Keep on server"-Ausschluss — Server-Eintrag bleibt, Desktop behandelt den Ordner lokal).
async fn build_full_folder_list(client: Option<&WebDavClient>, app: &AppHandle) -> Vec<Folder> {
    let local_active: Vec<_> = local_store::active_folders(app);
    let local_names: std::collections::HashSet<String> =
        local_active.iter().map(|f| f.name.to_lowercase()).collect();

    let mut folders = match client {
        Some(c) => build_folder_list(c).await,
        None => Vec::new(),
    };

    // Server-Ordner, die lokal als local-only markiert sind, entsprechend kennzeichnen
    for f in folders.iter_mut() {
        if local_names.contains(&f.name.to_lowercase()) {
            f.local_only = true;
        }
    }

    // Rein lokale Ordner ergänzen, die nicht im Server-Listing auftauchen
    for meta in local_active {
        let already = folders
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(&meta.name));
        if !already {
            folders.push(Folder {
                name: meta.name,
                color: meta.color,
                local_only: true,
            });
        }
    }
    folders.sort_by_key(|f| f.name.to_lowercase());
    folders
}

/// Alle Ordner auflisten (Server + lokal)
#[tauri::command]
async fn list_folders(app: AppHandle, state: State<'_, WebDavState>) -> Result<Vec<Folder>> {
    // Auch offline: lokale Ordner werden ohne Server-Verbindung geliefert.
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };
    Ok(build_full_folder_list(client.as_ref(), &app).await)
}

/// Neuen Ordner erstellen (oder reaktivieren wenn tombstoned)
#[tauri::command]
async fn create_folder(
    name: String,
    color: Option<String>,
    local_only: bool,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<Vec<Folder>> {
    if !validate_folder_name(&name) {
        return Err(AppError::WebDav(format!("Invalid folder name: {}", name)));
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    if local_only {
        // Kollision mit gleichnamigem Server-Ordner verhindern: ein local-only-Ordner würde
        // sonst dessen Notizen verdecken (is_local_only → true → Routing in den leeren Store).
        if let Some(ref client) = client {
            let server = build_folder_list(client).await;
            if server.iter().any(|f| f.name.eq_ignore_ascii_case(&name)) {
                return Err(AppError::WebDav(format!(
                    "A synced folder named '{}' already exists",
                    name
                )));
            }
        }
        local_store::upsert_folder(&app, &name, color, false);
        return Ok(build_full_folder_list(client.as_ref(), &app).await);
    }

    let client = client.ok_or(AppError::NotConnected)?;

    let now = chrono::Utc::now().timestamp_millis();
    let new_name = name.clone();
    let new_color = color.clone();

    client
        .write_folders_meta_merged(move |mut existing| {
            if let Some(pos) = existing
                .iter()
                .position(|m| m.name.eq_ignore_ascii_case(&new_name))
            {
                // Reaktivieren
                existing[pos].deleted = false;
                existing[pos].color = new_color.clone();
                existing[pos].updated_at = now;
            } else {
                existing.push(FolderMeta {
                    name: new_name.clone(),
                    color: new_color.clone(),
                    updated_at: now,
                    deleted: false,
                });
            }
            existing
        })
        .await?;

    // Verzeichnisse anlegen
    client.ensure_folder_dirs(&name).await;

    Ok(build_full_folder_list(Some(&client), &app).await)
}

/// Ordner umbenennen: Notizen verschieben + Meta aktualisieren
#[tauri::command]
async fn rename_folder(
    old_name: String,
    new_name: String,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<Vec<Folder>> {
    if !validate_folder_name(&new_name) {
        return Err(AppError::WebDav(format!(
            "Invalid folder name: {}",
            new_name
        )));
    }

    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    // Lokaler Ordner: nur Meta + Notizen im lokalen Store umbenennen (auch offline)
    if local_store::is_local_only(&app, Some(&old_name)) {
        // Kollision mit gleichnamigem Server-Ordner verhindern (würde dessen Notizen verdecken).
        if let Some(ref client) = client {
            let server = build_folder_list(client).await;
            if server
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&new_name))
            {
                return Err(AppError::WebDav(format!(
                    "A synced folder named '{}' already exists",
                    new_name
                )));
            }
        }
        local_store::rename_folder(&app, &old_name, &new_name);
        return Ok(build_full_folder_list(client.as_ref(), &app).await);
    }

    let client = client.ok_or(AppError::NotConnected)?;

    // Alle Notizen im alten Ordner verschieben
    let note_locations = client.list_notes_with_folders().await?;
    for (id, folder) in note_locations {
        if folder
            .as_deref()
            .map(|f| f.eq_ignore_ascii_case(&old_name))
            .unwrap_or(false)
        {
            if let Err(e) = client
                .move_note_file(&id, Some(&old_name), Some(&new_name))
                .await
            {
                eprintln!("[rename_folder] move {} failed: {}", id, e);
            }
        }
    }

    // Meta: alten Ordner tombstonen + neuen hinzufügen/reaktivieren (Farbe übernehmen)
    let now = chrono::Utc::now().timestamp_millis();
    let old_name_c = old_name.clone();
    let new_name_c = new_name.clone();

    client
        .write_folders_meta_merged(move |mut existing| {
            let old_color = existing
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(&old_name_c))
                .and_then(|m| m.color.clone());

            // Alten Eintrag tombstonen
            for m in existing.iter_mut() {
                if m.name.eq_ignore_ascii_case(&old_name_c) {
                    m.deleted = true;
                    m.updated_at = now;
                }
            }

            // Neuen Eintrag hinzufügen oder reaktivieren
            if let Some(pos) = existing
                .iter()
                .position(|m| m.name.eq_ignore_ascii_case(&new_name_c))
            {
                existing[pos].deleted = false;
                existing[pos].color = old_color;
                existing[pos].updated_at = now;
            } else {
                existing.push(FolderMeta {
                    name: new_name_c.clone(),
                    color: old_color,
                    updated_at: now,
                    deleted: false,
                });
            }
            existing
        })
        .await?;

    // Neues Verzeichnis sicherstellen, altes entfernen
    client.ensure_folder_dirs(&new_name).await;
    client.delete_folder_dirs(&old_name).await;

    Ok(build_full_folder_list(Some(&client), &app).await)
}

/// Ordner löschen: Notizen behalten (→ Root verschieben) oder mitlöschen
#[tauri::command]
async fn delete_folder(
    name: String,
    keep_notes: bool,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<Vec<Folder>> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    // Lokaler Ordner
    if local_store::is_local_only(&app, Some(&name)) {
        let mut migrate_ok = true;
        if keep_notes {
            // Notizen zum Server migrieren — erfordert eine Verbindung.
            let client = client.as_ref().ok_or(AppError::NotConnected)?;
            for note in local_store::list_notes(&app) {
                if note
                    .folder_name
                    .as_deref()
                    .map(|f| f.eq_ignore_ascii_case(&name))
                    .unwrap_or(false)
                {
                    let mut n = note;
                    n.folder_name = None;
                    n.updated_at = chrono::Utc::now().timestamp_millis();
                    if let Err(e) = client.save_note(&n).await {
                        eprintln!("[delete_folder] migrate local note {} failed: {}", n.id, e);
                        migrate_ok = false;
                    } else {
                        local_store::remove_note(&app, &n.id);
                    }
                }
            }
        } else {
            // Lokale Notizen des Ordners löschen (auch offline)
            for note in local_store::list_notes(&app) {
                if note
                    .folder_name
                    .as_deref()
                    .map(|f| f.eq_ignore_ascii_case(&name))
                    .unwrap_or(false)
                {
                    local_store::remove_note(&app, &note.id);
                }
            }
        }
        // Ordner nur tombstonen, wenn jede Notiz migriert/gelöscht wurde — sonst bliebe eine
        // hängengebliebene Notiz unsichtbar (weder lokal aktiv noch auf dem Server).
        if !migrate_ok {
            return Err(AppError::WebDav(
                "Some notes could not be migrated; folder kept local".to_string(),
            ));
        }
        local_store::upsert_folder(&app, &name, None, true);
        return Ok(build_full_folder_list(client.as_ref(), &app).await);
    }

    let client = client.ok_or(AppError::NotConnected)?;

    // Notizen im Ordner behandeln
    let now = chrono::Utc::now().timestamp_millis();
    let note_locations = client.list_notes_with_folders().await?;
    for (id, folder) in &note_locations {
        if folder
            .as_deref()
            .map(|f| f.eq_ignore_ascii_case(&name))
            .unwrap_or(false)
        {
            if keep_notes {
                if let Err(e) = client.move_note_file(id, Some(&name), None).await {
                    eprintln!("[delete_folder] move {} to root failed: {}", id, e);
                }
            } else {
                match client.get_note(id, Some(&name)).await {
                    Ok(mut note) => {
                        note.folder_name = None;
                        note.trashed_at = Some(now);
                        note.updated_at = now;
                        if let Err(e) = client.save_note(&note).await {
                            eprintln!("[delete_folder] trash {} failed: {}", id, e);
                        } else {
                            let _ = client.delete_note_by_id_folder(id, Some(&name)).await;
                        }
                    }
                    Err(e) => eprintln!("[delete_folder] get {} failed: {}", id, e),
                }
            }
        }
    }

    // Ordner tombstonen
    let name_c = name.clone();
    client
        .write_folders_meta_merged(move |mut existing| {
            if let Some(pos) = existing
                .iter()
                .position(|m| m.name.eq_ignore_ascii_case(&name_c))
            {
                existing[pos].deleted = true;
                existing[pos].updated_at = now;
            } else {
                existing.push(FolderMeta {
                    name: name_c.clone(),
                    color: None,
                    updated_at: now,
                    deleted: true,
                });
            }
            existing
        })
        .await?;

    client.delete_folder_dirs(&name).await;

    Ok(build_full_folder_list(Some(&client), &app).await)
}

/// Ordner-Farbe setzen oder entfernen
#[tauri::command]
async fn set_folder_color(
    name: String,
    color: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<Vec<Folder>> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    // Lokaler Ordner: nur lokale Meta aktualisieren (auch offline)
    if local_store::is_local_only(&app, Some(&name)) {
        local_store::set_folder_color(&app, &name, color);
        return Ok(build_full_folder_list(client.as_ref(), &app).await);
    }

    let client = client.ok_or(AppError::NotConnected)?;

    let now = chrono::Utc::now().timestamp_millis();
    let name_c = name.clone();
    let color_c = color.clone();

    client
        .write_folders_meta_merged(move |mut existing| {
            if let Some(pos) = existing
                .iter()
                .position(|m| m.name.eq_ignore_ascii_case(&name_c))
            {
                existing[pos].color = color_c.clone();
                existing[pos].updated_at = now;
            } else {
                existing.push(FolderMeta {
                    name: name_c.clone(),
                    color: color_c.clone(),
                    updated_at: now,
                    deleted: false,
                });
            }
            existing
        })
        .await?;

    Ok(build_full_folder_list(Some(&client), &app).await)
}

/// Notizen in einen anderen Ordner verschieben
#[tauri::command]
async fn move_notes(
    ids: Vec<String>,
    source_folder: Option<String>,
    target_folder: Option<String>,
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    let src_local = local_store::is_local_only(&app, source_folder.as_deref());
    let dst_local = local_store::is_local_only(&app, target_folder.as_deref());

    // Verbindung nur erforderlich, wenn eine Server-Seite beteiligt ist
    // (rein lokale Verschiebungen funktionieren offline).
    if (!src_local || !dst_local) && client.is_none() {
        return Err(AppError::NotConnected);
    }

    // Ziel-Verzeichnis anlegen (nur Server-Ziel)
    if !dst_local {
        if let (Some(client), Some(f)) = (client.as_ref(), target_folder.as_ref()) {
            client.ensure_folder_dirs(f).await;
        }
    }

    // Phase 1: Server→Lokal-Verschiebungen schreiben ans Lösch-Ledger (verhindert Android-Trash).
    let mut server_to_local_deleted: Vec<String> = Vec::new();

    for id in &ids {
        if src_local && dst_local {
            // Lokal → Lokal: nur folder_name aktualisieren (kein Server nötig)
            if let Some(mut note) = local_store::get_note(&app, id) {
                note.folder_name = target_folder.clone();
                note.updated_at = chrono::Utc::now().timestamp_millis();
                local_store::put_note(&app, &note);
            }
        } else if let Some(client) = client.as_ref() {
            // Ab hier ist garantiert verbunden (siehe Guard oben).
            if src_local {
                // Lokal → Server: hochladen + aus lokalem Store entfernen
                if let Some(mut note) = local_store::get_note(&app, id) {
                    note.folder_name = target_folder.clone();
                    note.updated_at = chrono::Utc::now().timestamp_millis();
                    if let Err(e) = client.save_note(&note).await {
                        eprintln!("[move_notes] upload local note {} failed: {}", id, e);
                    } else {
                        local_store::remove_note(&app, id);
                    }
                }
            } else if dst_local {
                // Server → Lokal: runterladen, in lokalem Store speichern, vom Server löschen
                match client.get_note(id, source_folder.as_deref()).await {
                    Ok(mut note) => {
                        note.folder_name = target_folder.clone();
                        note.updated_at = chrono::Utc::now().timestamp_millis();
                        local_store::put_note(&app, &note);
                        if let Err(e) = client.delete_note(&note).await {
                            eprintln!("[move_notes] delete server note {} failed: {}", id, e);
                        } else {
                            server_to_local_deleted.push(id.clone());
                        }
                    }
                    Err(e) => eprintln!("[move_notes] get server note {} failed: {}", id, e),
                }
            } else {
                // Server → Server
                if let Err(e) = client
                    .move_note_file(id, source_folder.as_deref(), target_folder.as_deref())
                    .await
                {
                    eprintln!("[move_notes] move {} failed: {}", id, e);
                }
            }
        }
    }

    // Phase 1: Batch-Ledger-Write für Server→Lokal-Verschiebungen
    if !server_to_local_deleted.is_empty() {
        if let Some(client) = client.as_ref() {
            let device_id = get_or_create_device_id(&app, &device_id_state)?;
            let now = chrono::Utc::now().timestamp_millis();
            client
                .append_deletions(
                    &server_to_local_deleted,
                    &device_id,
                    now,
                    TRASH_RETENTION_MS,
                )
                .await;
        }
    }

    Ok(())
}

/// Ordner zwischen Server-Sync und Local-Only umschalten.
///
/// `local_only = true`:  Server-Notizen in lokalen Store übertragen + Server-Ordner behandeln.
/// `local_only = false`: Lokale Notizen zum Server hochladen + lokalen Ordner entfernen.
///
/// `remove_from_server` (Phase 3, nur bei `local_only = true`):
///   - `true`:  Notizen vom Server löschen + Ordner tombstonen (bisheriges Verhalten).
///   - `false`: Notizen auf Server belassen + Ordner NICHT tombstonen ("Keep on server").
#[tauri::command]
async fn set_folder_local_only(
    name: String,
    local_only: bool,
    remove_from_server: bool,
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
) -> Result<Vec<Folder>> {
    let client = {
        let lock = lock_recover(&state.0);
        lock.clone()
    };

    if local_only {
        // ── Online-Pfad: Notizen vom Server holen ────────────────────────────
        if let Some(ref client) = client {
            let note_locations = client.list_notes_with_folders().await?;
            let mut deleted_ids: Vec<String> = Vec::new();

            for (id, folder) in &note_locations {
                if folder
                    .as_deref()
                    .map(|f| f.eq_ignore_ascii_case(&name))
                    .unwrap_or(false)
                {
                    match client.get_note(id, Some(&name)).await {
                        Ok(note) => {
                            local_store::put_note(&app, &note);
                            if remove_from_server {
                                // Phase 1: Löschen + Ledger-Eintrag sammeln
                                if let Err(e) = client.delete_note(&note).await {
                                    eprintln!(
                                        "[set_folder_local_only] delete {} failed: {}",
                                        id, e
                                    );
                                } else {
                                    deleted_ids.push(id.clone());
                                }
                            }
                        }
                        Err(e) => eprintln!("[set_folder_local_only] get {} failed: {}", id, e),
                    }
                }
            }

            // Phase 1: Alle erfolgreichen Löschungen in einem Batch ins Ledger schreiben
            if !deleted_ids.is_empty() {
                let device_id = get_or_create_device_id(&app, &device_id_state)?;
                let now = chrono::Utc::now().timestamp_millis();
                client
                    .append_deletions(&deleted_ids, &device_id, now, TRASH_RETENTION_MS)
                    .await;
            }

            // Server-Ordner tombstonen und Verzeichnisse löschen (nur bei remove_from_server = true)
            if remove_from_server {
                let now = chrono::Utc::now().timestamp_millis();
                let name_c = name.clone();
                client
                    .write_folders_meta_merged(move |mut existing| {
                        if let Some(pos) = existing
                            .iter()
                            .position(|m| m.name.eq_ignore_ascii_case(&name_c))
                        {
                            existing[pos].deleted = true;
                            existing[pos].updated_at = now;
                        } else {
                            existing.push(FolderMeta {
                                name: name_c.clone(),
                                color: None,
                                updated_at: now,
                                deleted: true,
                            });
                        }
                        existing
                    })
                    .await?;
                client.delete_folder_dirs(&name).await;
            }
        } else {
            // ── Offline-Pfad (Phase 2) ────────────────────────────────────────
            // Notizen die bereits im lokalen Store für diesen Ordner liegen einreihen
            // (für server-synchronisierte Ordner typischerweise leer; Phase 4 erweitert dies).
            if remove_from_server {
                let local_ids: Vec<(String, Option<String>)> = local_store::list_notes(&app)
                    .into_iter()
                    .filter(|n| {
                        n.folder_name
                            .as_deref()
                            .map(|f| f.eq_ignore_ascii_case(&name))
                            .unwrap_or(false)
                    })
                    .map(|n| (n.id, n.folder_name))
                    .collect();

                sync_queue::enqueue_deletions(&app, &local_ids);
                sync_queue::enqueue_folder_tombstone(&app, &name);
            }
            // Offline ohne remove_from_server → nur lokal markieren, nichts queuen
        }

        // Ordner-Farbe aus Server-Meta übernehmen (wenn verbunden)
        let color = if let Some(ref client) = client {
            let meta = client.read_folders_meta().await;
            meta.iter()
                .find(|m| m.name.eq_ignore_ascii_case(&name))
                .and_then(|m| m.color.clone())
        } else {
            None
        };

        local_store::upsert_folder(&app, &name, color, false);
    } else {
        // ── Ordner wieder in den Sync aufnehmen ──────────────────────────────
        let client = client.ok_or(AppError::NotConnected)?;

        // Phase 2: Queue-Einträge für diesen Ordner löschen (verhindert Race mit Re-Upload)
        sync_queue::cancel_folder_deletions(&app, &name);

        // Timestamp vor dem Upload setzen — Notizen müssen einen neueren updated_at als
        // den deleted_at-Eintrag im Lösch-Ledger haben, sonst löscht der Zombie-Check
        // in list_notes() die gerade hochgeladenen Notizen sofort wieder.
        let now = chrono::Utc::now().timestamp_millis();

        // Lokale Notizen zum Server hochladen (ausgenommen Papierkorb-Notizen)
        let mut upload_ok = true;
        let mut uploaded_ids: Vec<String> = Vec::new();
        for note in local_store::list_notes(&app) {
            if note
                .folder_name
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(&name))
                .unwrap_or(false)
            {
                // Bereits gelöschte Notizen nicht hochladen — sie bleiben im lokalen Papierkorb.
                if note.trashed_at.is_some() {
                    continue;
                }
                let mut note = note;
                note.updated_at = now;
                if let Err(e) = client.save_note(&note).await {
                    eprintln!("[set_folder_local_only] upload {} failed: {}", note.id, e);
                    upload_ok = false;
                } else {
                    uploaded_ids.push(note.id.clone());
                    local_store::remove_note(&app, &note.id);
                }
            }
        }

        if !upload_ok {
            return Err(AppError::WebDav(
                "Some notes could not be uploaded; folder kept local".to_string(),
            ));
        }

        // Stale Tombstones aus dem geteilten Lösch-Ledger entfernen: re-uploadete Notizen
        // dürfen nicht weiterhin als „gelöscht" im Ledger stehen (Zombie-Schutz in list_notes
        // und Android detectDeletions würden sie sonst fälschlicherweise entfernen).
        client.remove_deletions(&uploaded_ids).await;
        // Sync-Cache-Einträge bereinigen, damit veraltete Conflict/DeletedOnServer-Badges
        // nicht weiter in list_notes() eingeblendet werden.
        sync_engine::remove_cache_entries(&app, &uploaded_ids);

        // Lokalen Ordner tombstonen; Server-Ordner reaktivieren
        local_store::upsert_folder(&app, &name, None, true);
        let name_c = name.clone();
        client
            .write_folders_meta_merged(move |mut existing| {
                if let Some(pos) = existing
                    .iter()
                    .position(|m| m.name.eq_ignore_ascii_case(&name_c))
                {
                    existing[pos].deleted = false;
                    existing[pos].updated_at = now;
                } else {
                    existing.push(FolderMeta {
                        name: name_c.clone(),
                        color: None,
                        updated_at: now,
                        deleted: false,
                    });
                }
                existing
            })
            .await?;

        client.ensure_folder_dirs(&name).await;

        return Ok(build_full_folder_list(Some(&client), &app).await);
    }

    Ok(build_full_folder_list(client.as_ref(), &app).await)
}

/// Server-Sync ausführen: Notizen herunter-/hochladen, Konflikte und Löschungen erkennen.
/// Exklusiver Sync-Lock verhindert parallele Läufe.
#[tauri::command]
async fn sync(
    app: AppHandle,
    device_id_state: State<'_, DeviceIdState>,
    state: State<'_, WebDavState>,
    sync_lock: State<'_, SyncLockState>,
) -> Result<()> {
    // Bei bereits laufendem Sync sofort zurückkehren (kein Fehler, kein zweiter Lauf)
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
    Ok(())
}

/// Einen Sync-Konflikt auflösen: eigene Version behalten oder Server-Version übernehmen.
#[tauri::command]
async fn resolve_conflict(
    id: String,
    resolution: String, // "keep_mine" oder "use_server"
    folder_name: Option<String>,
    app: AppHandle,
    state: State<'_, WebDavState>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    let cache_entry = sync_engine::load_note_cache(&app).get(&id).cloned();

    match resolution.as_str() {
        "keep_mine" => {
            // Eigene Version behalten: als PENDING markieren → nächster Sync lädt hoch
            if let Some(mut entry) = cache_entry {
                entry.note.sync_status = models::SyncStatus::Pending;
                entry.note.updated_at = now;
                sync_engine::update_cache_entry(&app, entry);
            }
        }
        "use_server" => {
            // Server-Version übernehmen: aktuellen Stand laden und Cache überschreiben
            let client = {
                let lock = lock_recover(&state.0);
                lock.clone()
            };
            let client = client.ok_or(AppError::NotConnected)?;
            let mut note = client.get_note(&id, folder_name.as_deref()).await?;
            note.sync_status = models::SyncStatus::Synced;
            sync_engine::update_cache_entry(
                &app,
                sync_engine::NoteCacheEntry {
                    note,
                    last_synced_at: now,
                    etag: None,
                },
            );
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
        .manage(SyncLockState(tokio::sync::Mutex::new(())))
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
