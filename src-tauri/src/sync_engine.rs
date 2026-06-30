use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::folders::FolderMeta;
use crate::local_store;
use crate::models::{Note, SyncStatus};
use crate::sync_queue;
use crate::webdav::WebDavClient;

const SYNC_STORE: &str = "sync_state.json";
const KEY_NOTE_CACHE: &str = "note_cache";
const KEY_LAST_SYNC: &str = "last_sync_at";

/// Ein Eintrag im lokalen Notiz-Cache (für Migration aus alter Architektur).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteCacheEntry {
    pub note: Note,
    pub last_synced_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

/// Ergebnis eines Sync-Laufs (für Logging / späteres Frontend-Feedback).
#[derive(Debug, Default)]
pub struct SyncSummary {
    pub notes_downloaded: usize,
    pub notes_uploaded: usize,
    pub conflicts_detected: usize,
    pub notes_deleted_on_server: usize,
}

// ── Cache-Zugriff (für Migration) ───────────────────────────────────────────

pub fn load_note_cache(app: &AppHandle) -> HashMap<String, NoteCacheEntry> {
    app.store(SYNC_STORE)
        .ok()
        .and_then(|s| s.get(KEY_NOTE_CACHE))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn save_note_cache(app: &AppHandle, cache: &HashMap<String, NoteCacheEntry>) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.set(
            KEY_NOTE_CACHE,
            serde_json::to_value(cache).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

/// note_cache-Key löschen — wird nach der einmaligen Migration aufgerufen.
pub fn clear_note_cache(app: &AppHandle) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.delete(KEY_NOTE_CACHE);
        let _ = store.save();
    }
}

fn save_last_sync_at(app: &AppHandle, ts: i64) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.set(KEY_LAST_SYNC, serde_json::json!(ts));
        let _ = store.save();
    }
}

// ── Sync-Logik ───────────────────────────────────────────────────────────────

/// Alle Server-Notizen abrufen (PROPFIND + GET je UUID).
async fn fetch_server_notes(client: &WebDavClient) -> crate::error::Result<Vec<Note>> {
    let note_locations = client.list_notes_with_folders().await?;
    let mut notes = Vec::new();
    for (id, folder) in note_locations {
        match client.get_note(&id, folder.as_deref()).await {
            Ok(note) => notes.push(note),
            Err(e) => eprintln!("[sync] get_note {} fehlgeschlagen: {}", id, e),
        }
    }
    Ok(notes)
}

/// Menge aller Ordnernamen, die auf dem Server existieren (lowercased).
/// Quelle: Ordner aus Notiz-Pfaden ∪ folders.json ∪ physische Verzeichnisse.
async fn collect_server_folder_names(
    client: &WebDavClient,
) -> crate::error::Result<HashSet<String>> {
    let mut names = HashSet::new();
    for (_id, folder) in client.list_notes_with_folders().await? {
        if let Some(f) = folder {
            names.insert(f.to_lowercase());
        }
    }
    for m in client.read_folders_meta().await {
        if !m.deleted {
            names.insert(m.name.to_lowercase());
        }
    }
    for d in client.discover_folders().await {
        names.insert(d.to_lowercase());
    }
    Ok(names)
}

/// Ordner-Sync: lokale (nicht local-only) Ordner mit Server-folders.json LWW-mergen
/// und fehlende Server-Verzeichnisse anlegen.
async fn sync_folders(client: &WebDavClient, app: &AppHandle) {
    let server_meta = client.read_folders_meta().await;

    // Lokale nicht-local-only Ordner für den Merge aufbereiten
    let local_meta: Vec<FolderMeta> = local_store::active_folders(app)
        .into_iter()
        .filter(|f| !f.local_only)
        .map(|f| FolderMeta {
            name: f.name,
            color: f.color,
            updated_at: f.updated_at,
            deleted: false,
            local_only: false,
        })
        .collect();

    // LWW-Merge
    let merged = crate::folders::merge_by_name(local_meta, server_meta);

    // Neue Server-Ordner in local_store aufnehmen (ohne local_only-Flag)
    for m in &merged {
        if !m.deleted && !local_store::is_local_only(app, Some(&m.name)) {
            let already = local_store::active_folders(app)
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&m.name));
            if !already {
                local_store::upsert_folder(app, &m.name, m.color.clone(), false, false);
            }
        }
    }

    // Server-Verzeichnisse für aktive lokale Nicht-local-only-Ordner anlegen
    for f in local_store::active_folders(app) {
        if !f.local_only {
            client.ensure_folder_dirs(&f.name).await;
        }
    }

    // folders.json auf dem Server mit gemergten Daten aktualisieren.
    // write_folders_meta_merged liest den Server unter „Lock" frisch neu — wir mergen unsere
    // Änderungen dort hinein (LWW), statt sie mit einem veralteten Stand zu überschreiben.
    // Sonst gehen Ordner-Änderungen verloren, die ein anderes Gerät zwischen unserem ersten
    // Read (oben) und diesem Write geschrieben hat.
    let to_write: Vec<FolderMeta> = merged.into_iter().filter(|m| !m.local_only).collect();
    if !to_write.is_empty() {
        let _ = client
            .write_folders_meta_merged(move |existing| {
                crate::folders::merge_by_name(to_write, existing)
            })
            .await;
    }
}

/// Server-Sync: local_store ↔ Server reconcilen.
///
/// Port von Android's `WebDavSyncService.syncNotes()`.
/// Sicherheitswächter verhindern Massen-Löschungen durch leere PROPFIND-Antworten.
pub async fn run_sync(
    client: &WebDavClient,
    app: &AppHandle,
    device_id: &str,
    retention_ms: i64,
) -> SyncSummary {
    let mut summary = SyncSummary::default();
    let now = chrono::Utc::now().timestamp_millis();

    // 1. Offline-Queue abarbeiten (ausstehende Löschungen + Move-Cleanups + Ordner-Tombstones)
    sync_queue::drain_sync_queue(client, app, device_id, retention_ms).await;

    // 1.5 Einmalige local_only-Reconciliation (nur bei erreichbarem Server)
    if !local_store::local_only_reconciled(app) {
        if let Ok(server_names) = collect_server_folder_names(client).await {
            local_store::reconcile_local_only(app, &server_names);
        }
        // Err → Server nicht erreichbar → Marker NICHT setzen, nächster Lauf versucht es erneut
    }

    // 2. Ordner-Sync
    sync_folders(client, app).await;

    // 3. Server-Notizen abrufen
    let server_notes = match fetch_server_notes(client).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[sync] fetch fehlgeschlagen: {}", e);
            return summary;
        }
    };
    let server_ids: HashSet<String> = server_notes.iter().map(|n| n.id.clone()).collect();

    // Local-only-Ordner einmal vorberechnen
    let local_only_set: HashSet<String> = local_store::active_folders(app)
        .into_iter()
        .filter(|f| f.local_only)
        .map(|f| f.name.to_lowercase())
        .collect();

    let ledger = client.read_deletions().await;
    let deletion_map: HashMap<String, i64> = ledger
        .deleted_notes
        .iter()
        .map(|r| (r.id.clone(), r.deleted_at))
        .collect();

    // Sicherheitswächter 1: leerer Server-Scan bei gefülltem Store → keine Löscherkennung
    let local_synced: Vec<Note> = local_store::list_notes(app)
        .into_iter()
        .filter(|n| {
            n.sync_status == SyncStatus::Synced
                && n.trashed_at.is_none()
                && !n
                    .folder_name
                    .as_deref()
                    .map(|f| local_only_set.contains(&f.to_lowercase()))
                    .unwrap_or(false)
        })
        .collect();
    let abort_deletion = server_notes.is_empty() && !local_synced.is_empty();
    if abort_deletion {
        eprintln!(
            "[sync] Sicherheitswächter: Server lieferte 0 Notizen, {} lokale SYNCED — Löscherkennung übersprungen",
            local_synced.len()
        );
    }

    // 4. Download / LWW-Merge → in local_store schreiben
    for sn in &server_notes {
        if sn
            .folder_name
            .as_deref()
            .map(|f| local_only_set.contains(&f.to_lowercase()))
            .unwrap_or(false)
        {
            continue;
        }
        match local_store::get_note(app, &sn.id) {
            None => {
                let mut n = sn.clone();
                n.sync_status = SyncStatus::Synced;
                local_store::put_note(app, &n);
                summary.notes_downloaded += 1;
            }
            Some(local) => {
                if sn.updated_at > local.updated_at {
                    if matches!(
                        local.sync_status,
                        SyncStatus::Pending | SyncStatus::Conflict
                    ) {
                        // Beide Seiten editiert → Konflikt
                        let mut c = local.clone();
                        c.sync_status = SyncStatus::Conflict;
                        local_store::put_note(app, &c);
                        summary.conflicts_detected += 1;
                        eprintln!("[sync] Konflikt erkannt für {}", sn.id);
                    } else {
                        // Server neuer → überschreiben
                        let mut n = sn.clone();
                        n.sync_status = SyncStatus::Synced;
                        local_store::put_note(app, &n);
                        summary.notes_downloaded += 1;
                    }
                }
                // sonst: lokal neuer/gleich → wird ggf. in Upload-Phase behandelt
            }
        }
    }

    // 5. Löscherkennung: SYNCED-Notizen, die nicht (mehr) am Server sind
    if !abort_deletion {
        let missing: Vec<&Note> = local_synced
            .iter()
            .filter(|n| !server_ids.contains(&n.id))
            .collect();

        let too_many = !local_synced.is_empty()
            && missing.len() >= 10
            && missing.len() * 10 >= local_synced.len() * 8;
        if too_many {
            eprintln!(
                "[sync] Sicherheitswächter: {}/{} SYNCED fehlen — Löscherkennung abgebrochen",
                missing.len(),
                local_synced.len()
            );
        } else {
            for n in missing {
                let intentional = n.trashed_at.is_some()
                    || deletion_map
                        .get(&n.id)
                        .map(|&d| d >= n.updated_at)
                        .unwrap_or(false);
                if intentional {
                    local_store::remove_note(app, &n.id);
                } else {
                    let mut z = n.clone();
                    z.sync_status = SyncStatus::DeletedOnServer;
                    z.trashed_at = Some(now);
                    local_store::put_note(app, &z);
                    eprintln!(
                        "[sync] {} auf Server verschwunden → DELETED_ON_SERVER",
                        n.id
                    );
                }
                summary.notes_deleted_on_server += 1;
            }
        }
    }

    // 6. Upload: PENDING (nicht local-only-Ordner) → Server, dann SYNCED
    let mut uploaded_ids: Vec<String> = Vec::new();
    for n in local_store::list_notes(app) {
        let skip = n
            .folder_name
            .as_deref()
            .map(|f| local_only_set.contains(&f.to_lowercase()))
            .unwrap_or(false);
        if skip || !matches!(n.sync_status, SyncStatus::Pending | SyncStatus::LocalOnly) {
            continue;
        }
        match client.save_note(&n).await {
            Ok(()) => {
                local_store::mark_synced_if_unchanged(app, &n.id, n.updated_at);
                uploaded_ids.push(n.id.clone());
                summary.notes_uploaded += 1;
            }
            Err(e) => eprintln!("[sync] upload {} fehlgeschlagen: {}", n.id, e),
        }
    }
    // Frisch (wieder-)hochgeladene Notizen aus dem Server-Lösch-Ledger streichen,
    // damit ein alter Tombstone sie nicht beim nächsten Sync wieder „löscht".
    if !uploaded_ids.is_empty() {
        client.remove_deletions(&uploaded_ids).await;
    }

    save_last_sync_at(app, now);
    eprintln!(
        "[sync] Abgeschlossen: {} heruntergeladen, {} hochgeladen, {} Konflikte, {} auf Server gelöscht",
        summary.notes_downloaded,
        summary.notes_uploaded,
        summary.conflicts_detected,
        summary.notes_deleted_on_server
    );
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_cache_entry_serde() {
        let now = 1700000000000i64;
        let note = Note::new("Test".to_string(), "tauri-abc".to_string());
        let entry = NoteCacheEntry {
            note: note.clone(),
            last_synced_at: now,
            etag: Some("etag-123".to_string()),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let restored: NoteCacheEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.note.id, note.id);
        assert_eq!(restored.last_synced_at, now);
        assert_eq!(restored.etag.as_deref(), Some("etag-123"));
    }

    #[test]
    fn test_note_cache_entry_no_etag() {
        let entry = NoteCacheEntry {
            note: Note::new("X".to_string(), "tauri-x".to_string()),
            last_synced_at: 0,
            etag: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(
            !json.contains("\"etag\""),
            "etag-Feld darf bei None nicht serialisiert werden"
        );
    }
}
