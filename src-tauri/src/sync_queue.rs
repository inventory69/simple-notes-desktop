use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::folders::FolderMeta;
use crate::webdav::WebDavClient;

const STORE_FILE: &str = "sync_state.json";
const KEY_DELETIONS: &str = "pending_deletions";
const KEY_TOMBSTONES: &str = "pending_folder_tombstones";

/// Eine in der Offline-Queue gespeicherte Löschoperation.
/// `folder` ist der Server-Ordner-Pfad zum Zeitpunkt der Einreihung —
/// wird beim Drain verwendet, ohne ihn neu aufzulösen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingDeletion {
    pub id: String,
    pub folder: Option<String>,
}

// ── Interne Lade-/Speicherfunktionen ────────────────────────────────────────

fn load_deletions(app: &AppHandle) -> Vec<PendingDeletion> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_DELETIONS))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn save_deletions(app: &AppHandle, items: &[PendingDeletion]) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(
            KEY_DELETIONS,
            serde_json::to_value(items).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

fn load_tombstones(app: &AppHandle) -> Vec<String> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_TOMBSTONES))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn save_tombstones(app: &AppHandle, names: &[String]) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(
            KEY_TOMBSTONES,
            serde_json::to_value(names).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

// ── Öffentliche API ──────────────────────────────────────────────────────────

/// Hängt Notiz-IDs (mit Ordner-Zuordnung) in die Offline-Lösch-Queue ein.
/// Dedupliziert nach ID — idempotent bei wiederholtem Aufruf.
pub fn enqueue_deletions(app: &AppHandle, items: &[(String, Option<String>)]) {
    if items.is_empty() {
        return;
    }
    let mut existing = load_deletions(app);
    for (id, folder) in items {
        if !existing.iter().any(|d| d.id == *id) {
            existing.push(PendingDeletion {
                id: id.clone(),
                folder: folder.clone(),
            });
        }
    }
    save_deletions(app, &existing);
}

/// Alle ausstehenden Löschoperationen.
pub fn all_deletions(app: &AppHandle) -> Vec<PendingDeletion> {
    load_deletions(app)
}

/// Entfernt erfolgreich ausgeführte Löschungen aus der Queue.
pub fn remove_deletions(app: &AppHandle, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let existing = load_deletions(app);
    let updated: Vec<_> = existing
        .into_iter()
        .filter(|d| !ids.contains(&d.id))
        .collect();
    save_deletions(app, &updated);
}

/// Hängt einen Ordner-Tombstone in die Offline-Queue ein.
/// Dedupliziert nach Name (case-insensitiv).
pub fn enqueue_folder_tombstone(app: &AppHandle, name: &str) {
    let mut names = load_tombstones(app);
    if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        names.push(name.to_string());
    }
    save_tombstones(app, &names);
}

/// Alle ausstehenden Ordner-Tombstones.
pub fn all_folder_tombstones(app: &AppHandle) -> Vec<String> {
    load_tombstones(app)
}

/// Entfernt einen Ordner-Tombstone aus der Queue.
pub fn remove_folder_tombstone(app: &AppHandle, name: &str) {
    let names = load_tombstones(app);
    let updated: Vec<_> = names
        .into_iter()
        .filter(|n| !n.eq_ignore_ascii_case(name))
        .collect();
    save_tombstones(app, &updated);
}

/// Entfernt alle Queue-Einträge die zu einem bestimmten Ordner gehören.
/// Wird aufgerufen wenn ein Ordner wieder in den Sync aufgenommen wird
/// (Phase 2 / includeFoldersInSync-Parität mit Android).
pub fn cancel_folder_deletions(app: &AppHandle, folder_name: &str) {
    let existing = load_deletions(app);
    let updated: Vec<_> = existing
        .into_iter()
        .filter(|d| {
            !d.folder
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(folder_name))
                .unwrap_or(false)
        })
        .collect();
    save_deletions(app, &updated);
    remove_folder_tombstone(app, folder_name);
}

// ── Drain ────────────────────────────────────────────────────────────────────

/// Verarbeitet die Offline-Queue: führt ausstehende Löschungen und
/// Ordner-Tombstones aus, schreibt erfolgreich gelöschte IDs ins Lösch-Ledger.
/// Port von Android's `processPendingServerDeletions`.
pub async fn drain_sync_queue(
    client: &WebDavClient,
    app: &AppHandle,
    device_id: &str,
    retention_ms: i64,
) {
    // Ausstehende Löschungen
    let deletions = all_deletions(app);
    if !deletions.is_empty() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut success_ids: Vec<String> = Vec::new();

        for d in &deletions {
            match client
                .delete_note_by_id_folder(&d.id, d.folder.as_deref())
                .await
            {
                Ok(()) => success_ids.push(d.id.clone()),
                Err(e) => eprintln!(
                    "[drain_sync_queue] delete {} fehlgeschlagen, bleibt in Queue: {}",
                    d.id, e
                ),
            }
        }

        if !success_ids.is_empty() {
            client
                .append_deletions(&success_ids, device_id, now, retention_ms)
                .await;
            remove_deletions(app, &success_ids);
        }
    }

    // Ausstehende Ordner-Tombstones
    for name in all_folder_tombstones(app) {
        let now = chrono::Utc::now().timestamp_millis();
        let name_c = name.clone();
        match client
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
            .await
        {
            Ok(_) => {
                remove_folder_tombstone(app, &name);
                client.delete_folder_dirs(&name).await;
            }
            Err(e) => eprintln!(
                "[drain_sync_queue] tombstone '{}' fehlgeschlagen: {}",
                name, e
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_deletion_serde() {
        let item = PendingDeletion {
            id: "abc-123".to_string(),
            folder: Some("Work".to_string()),
        };
        let json = serde_json::to_string(&item).unwrap();
        let restored: PendingDeletion = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "abc-123");
        assert_eq!(restored.folder.as_deref(), Some("Work"));
    }

    #[test]
    fn test_pending_deletion_root_folder() {
        let item = PendingDeletion {
            id: "def-456".to_string(),
            folder: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        let restored: PendingDeletion = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.folder, None);
    }
}
