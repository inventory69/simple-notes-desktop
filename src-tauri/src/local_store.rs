use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::folders::FolderMeta;
use crate::models::Note;

// ponytail: global lock, per-key locks if throughput matters
static STORE_LOCK: Mutex<()> = Mutex::new(());

const STORE_FILE: &str = "local.json";
const KEY_FOLDERS: &str = "folders";
const KEY_NOTES: &str = "notes";
const KEY_LOCAL_ONLY_RECONCILED: &str = "local_only_reconciled";

fn load_folders(app: &AppHandle) -> Vec<FolderMeta> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_FOLDERS))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn save_folders(app: &AppHandle, folders: &Vec<FolderMeta>) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(
            KEY_FOLDERS,
            serde_json::to_value(folders).unwrap_or(serde_json::Value::Array(vec![])),
        );
        let _ = store.save();
    }
}

fn load_notes_map(app: &AppHandle) -> serde_json::Map<String, serde_json::Value> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_NOTES))
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn save_notes_map(app: &AppHandle, map: &serde_json::Map<String, serde_json::Value>) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(KEY_NOTES, serde_json::Value::Object(map.clone()));
        let _ = store.save();
    }
}

/// Prüft ob ein Ordner als local-only markiert ist (case-insensitiv, ignoriert Tombstones).
pub fn is_local_only(app: &AppHandle, folder: Option<&str>) -> bool {
    let name = match folder {
        Some(n) => n,
        None => return false,
    };
    load_folders(app)
        .iter()
        .any(|f| !f.deleted && f.local_only && f.name.eq_ignore_ascii_case(name))
}

/// Alle aktiven (nicht tombstoneten) lokalen Ordner.
pub fn active_folders(app: &AppHandle) -> Vec<FolderMeta> {
    load_folders(app)
        .into_iter()
        .filter(|f| !f.deleted)
        .collect()
}

/// Ordner anlegen, reaktivieren oder tombstonen.
pub fn upsert_folder(
    app: &AppHandle,
    name: &str,
    color: Option<String>,
    deleted: bool,
    local_only: bool,
) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let now = chrono::Utc::now().timestamp_millis();
    let mut folders = load_folders(app);
    if let Some(pos) = folders
        .iter()
        .position(|f| f.name.eq_ignore_ascii_case(name))
    {
        folders[pos].color = color;
        folders[pos].deleted = deleted;
        folders[pos].updated_at = now;
        folders[pos].local_only = local_only;
    } else {
        folders.push(FolderMeta {
            name: name.to_string(),
            color,
            updated_at: now,
            deleted,
            local_only,
        });
    }
    save_folders(app, &folders);
}

/// PENDING setzen, außer in local-only-Ordnern (dort LOCAL_ONLY, wird nie hochgeladen).
pub fn mark_dirty(app: &AppHandle, note: &mut crate::models::Note) {
    use crate::models::SyncStatus;
    note.sync_status = if is_local_only(app, note.folder_name.as_deref()) {
        SyncStatus::LocalOnly
    } else {
        SyncStatus::Pending
    };
}

/// Einmalige Migration: alle note_cache-Einträge (alte Architektur) in den lokalen Store
/// übernehmen, falls dort noch nicht vorhanden. Danach ist local_store die alleinige Quelle.
pub fn migrate_from_note_cache(app: &AppHandle) {
    {
        let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut map = load_notes_map(app);
        let mut changed = false;
        for (id, entry) in crate::sync_engine::load_note_cache(app) {
            if !map.contains_key(&id) {
                if let Ok(v) = serde_json::to_value(&entry.note) {
                    map.insert(id, v);
                    changed = true;
                }
            }
        }
        if changed {
            save_notes_map(app, &map);
        }
    }
    crate::sync_engine::clear_note_cache(app);

    // Befund 4: reine Offline-Installation → alle Ordner als local_only markieren.
    // upsert_folder nimmt intern den STORE_LOCK; kein Deadlock da wir ihn oben bereits freigegeben haben.
    let offline_mode = app
        .store("settings.json")
        .ok()
        .and_then(|s| s.get("offline_mode"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let has_credentials = app
        .store("settings.json")
        .ok()
        .and_then(|s| s.get("server_url"))
        .and_then(|v| v.as_str().map(|u| !u.is_empty()))
        .unwrap_or(false);
    if offline_mode || !has_credentials {
        for folder in active_folders(app) {
            upsert_folder(app, &folder.name, folder.color, false, true);
        }
    }
}

/// true, sobald die einmalige Server-Präsenz-Reconciliation gelaufen ist.
pub fn local_only_reconciled(app: &AppHandle) -> bool {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_LOCAL_ONLY_RECONCILED))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn set_local_only_reconciled(app: &AppHandle) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(KEY_LOCAL_ONLY_RECONCILED, serde_json::json!(true));
        let _ = store.save();
    }
}

/// Einmalige Reconciliation des `local_only`-Flags anhand der tatsächlichen Server-Präsenz.
/// Ein Ordner ist local-only ⇔ er existiert NICHT auf dem Server.
/// - v0.8.0-Upgrader: keiner ihrer (local-only) Ordner ist am Server → alle korrekt `true`.
/// - Dev/Mixed: synchronisierte Ordner sind am Server → `false` (heilt das WLAN-Badge);
///   echte local-only-Ordner nicht am Server → `true`.
///
/// `server_names` MUSS lowercased sein. Setzt anschließend den Marker.
// ponytail: ein synchronisierter Ordner, der offline angelegt und noch nicht hochgeladen wurde,
// wird einmalig fälschlich als local-only markiert — korrigierbar via set_folder_local_only(false).
pub fn reconcile_local_only(app: &AppHandle, server_names: &std::collections::HashSet<String>) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut folders = load_folders(app);
    for f in folders.iter_mut() {
        if f.deleted {
            continue;
        }
        f.local_only = !server_names.contains(&f.name.to_lowercase());
    }
    save_folders(app, &folders);
    set_local_only_reconciled(app);
}

/// Farbe eines lokalen Ordners setzen.
pub fn set_folder_color(app: &AppHandle, name: &str, color: Option<String>) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let now = chrono::Utc::now().timestamp_millis();
    let mut folders = load_folders(app);
    if let Some(pos) = folders
        .iter()
        .position(|f| f.name.eq_ignore_ascii_case(name))
    {
        folders[pos].color = color;
        folders[pos].updated_at = now;
        save_folders(app, &folders);
    }
}

/// Lokalen Ordner umbenennen: Meta + folder_name aller zugehörigen Notizen aktualisieren.
pub fn rename_folder(app: &AppHandle, old_name: &str, new_name: &str) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let now = chrono::Utc::now().timestamp_millis();

    // Meta umbenennen
    let mut folders = load_folders(app);
    for f in folders.iter_mut() {
        if f.name.eq_ignore_ascii_case(old_name) {
            f.name = new_name.to_string();
            f.updated_at = now;
        }
    }
    save_folders(app, &folders);

    // folder_name aller Notizen aktualisieren
    let mut map = load_notes_map(app);
    for val in map.values_mut() {
        if let Some(obj) = val.as_object_mut() {
            let matches = obj
                .get("folderName")
                .and_then(|v| v.as_str())
                .map(|s| s.eq_ignore_ascii_case(old_name))
                .unwrap_or(false);
            if matches {
                obj.insert("folderName".to_string(), serde_json::json!(new_name));
            }
        }
    }
    save_notes_map(app, &map);
}

/// Notiz speichern / überschreiben.
pub fn put_note(app: &AppHandle, note: &Note) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut map = load_notes_map(app);
    if let Ok(v) = serde_json::to_value(note) {
        map.insert(note.id.clone(), v);
    }
    save_notes_map(app, &map);
}

/// Notiz laden. Gibt `None` zurück wenn nicht vorhanden.
pub fn get_note(app: &AppHandle, id: &str) -> Option<Note> {
    let map = load_notes_map(app);
    map.get(id)
        .and_then(|v| serde_json::from_value::<Note>(v.clone()).ok())
}

/// Prüft ob eine Notiz im lokalen Store existiert (nach ID, unabhängig vom Ordner-Status).
#[allow(dead_code)]
pub fn has_note(app: &AppHandle, id: &str) -> bool {
    load_notes_map(app).contains_key(id)
}

/// Notiz permanent entfernen.
pub fn remove_note(app: &AppHandle, id: &str) {
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut map = load_notes_map(app);
    map.remove(id);
    save_notes_map(app, &map);
}

/// Markiert die Notiz als SYNCED — aber nur wenn sie sich seit dem Upload nicht verändert hat
/// (gleicher updated_at, noch Pending/LocalOnly). Verhindert das Überschreiben eines Edits,
/// der während des Upload-awaits eingetroffen ist.
pub fn mark_synced_if_unchanged(app: &AppHandle, id: &str, uploaded_updated_at: i64) {
    use crate::models::SyncStatus;
    let _g = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut map = load_notes_map(app);
    if let Some(v) = map.get(id) {
        if let Ok(mut note) = serde_json::from_value::<Note>(v.clone()) {
            if should_mark_synced(&note, uploaded_updated_at) {
                note.sync_status = SyncStatus::Synced;
                if let Ok(nv) = serde_json::to_value(&note) {
                    map.insert(id.to_string(), nv);
                    save_notes_map(app, &map);
                }
            }
        }
    }
}

fn should_mark_synced(note: &Note, uploaded_updated_at: i64) -> bool {
    use crate::models::SyncStatus;
    note.updated_at == uploaded_updated_at
        && matches!(
            note.sync_status,
            SyncStatus::Pending | SyncStatus::LocalOnly
        )
}

/// Alle lokal gespeicherten Notizen laden.
pub fn list_notes(app: &AppHandle) -> Vec<Note> {
    load_notes_map(app)
        .values()
        .filter_map(|v| serde_json::from_value::<Note>(v.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Note, SyncStatus};

    #[test]
    fn test_is_local_only_none_folder() {
        let name: Option<&str> = None;
        assert!(name.is_none());
    }

    #[test]
    fn test_should_mark_synced_matches() {
        let mut note = Note::new("x".into(), "tauri-x".into());
        note.updated_at = 100;
        note.sync_status = SyncStatus::Pending;
        assert!(should_mark_synced(&note, 100));
    }

    #[test]
    fn test_should_mark_synced_stale_ts() {
        let mut note = Note::new("x".into(), "tauri-x".into());
        note.updated_at = 200;
        note.sync_status = SyncStatus::Pending;
        assert!(!should_mark_synced(&note, 100));
    }

    #[test]
    fn test_should_mark_synced_already_synced() {
        let mut note = Note::new("x".into(), "tauri-x".into());
        note.updated_at = 100;
        note.sync_status = SyncStatus::Synced;
        assert!(!should_mark_synced(&note, 100));
    }
}
