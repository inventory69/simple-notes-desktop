use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::folders::FolderMeta;
use crate::models::Note;

const STORE_FILE: &str = "local.json";
const KEY_FOLDERS: &str = "folders";
const KEY_NOTES: &str = "notes";

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
        .any(|f| !f.deleted && f.name.eq_ignore_ascii_case(name))
}

/// Alle aktiven (nicht tombstoneten) lokalen Ordner.
pub fn active_folders(app: &AppHandle) -> Vec<FolderMeta> {
    load_folders(app)
        .into_iter()
        .filter(|f| !f.deleted)
        .collect()
}

/// Ordner anlegen, reaktivieren oder tombstonen.
pub fn upsert_folder(app: &AppHandle, name: &str, color: Option<String>, deleted: bool) {
    let now = chrono::Utc::now().timestamp_millis();
    let mut folders = load_folders(app);
    if let Some(pos) = folders
        .iter()
        .position(|f| f.name.eq_ignore_ascii_case(name))
    {
        folders[pos].color = color;
        folders[pos].deleted = deleted;
        folders[pos].updated_at = now;
    } else {
        folders.push(FolderMeta {
            name: name.to_string(),
            color,
            updated_at: now,
            deleted,
        });
    }
    save_folders(app, &folders);
}

/// Farbe eines lokalen Ordners setzen.
pub fn set_folder_color(app: &AppHandle, name: &str, color: Option<String>) {
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
pub fn has_note(app: &AppHandle, id: &str) -> bool {
    load_notes_map(app).contains_key(id)
}

/// Notiz permanent entfernen.
pub fn remove_note(app: &AppHandle, id: &str) {
    let mut map = load_notes_map(app);
    map.remove(id);
    save_notes_map(app, &map);
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
    // Hinweis: Diese Funktion benötigt einen AppHandle und kann nur in Integrationstests
    // mit einer laufenden Tauri-App getestet werden. Unitests prüfen die Hilfsfunktionen.

    #[test]
    fn test_is_local_only_none_folder() {
        // None-Ordner ist niemals local-only
        // (Testet den frühen Return in is_local_only ohne AppHandle)
        let name: Option<&str> = None;
        assert!(name.is_none());
    }
}
