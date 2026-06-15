use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::local_store;
use crate::models::{Note, SyncStatus};
use crate::sync_queue;
use crate::webdav::WebDavClient;

const SYNC_STORE: &str = "sync_state.json";
const KEY_NOTE_CACHE: &str = "note_cache";
const KEY_LAST_SYNC: &str = "last_sync_at";

/// Ein Eintrag im lokalen Notiz-Cache (sync_state.json).
/// Enthält die vollständige Notiz und Sync-Metadaten.
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

// ── Cache-Zugriff ────────────────────────────────────────────────────────────

pub fn load_note_cache(app: &AppHandle) -> HashMap<String, NoteCacheEntry> {
    app.store(SYNC_STORE)
        .ok()
        .and_then(|s| s.get(KEY_NOTE_CACHE))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn save_note_cache(app: &AppHandle, cache: &HashMap<String, NoteCacheEntry>) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.set(
            KEY_NOTE_CACHE,
            serde_json::to_value(cache).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

/// Einzelnen Cache-Eintrag aktualisieren und sofort persistieren.
pub fn update_cache_entry(app: &AppHandle, entry: NoteCacheEntry) {
    let mut cache = load_note_cache(app);
    cache.insert(entry.note.id.clone(), entry);
    save_note_cache(app, &cache);
}

/// Mehrere Cache-Einträge per ID entfernen und sofort persistieren.
/// Wird beim Wieder-Einschluss eines local-only-Ordners aufgerufen, damit veraltete
/// Conflict/DeletedOnServer-Badges aus list_notes() verschwinden.
pub fn remove_cache_entries(app: &AppHandle, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let mut cache = load_note_cache(app);
    for id in ids {
        cache.remove(id);
    }
    save_note_cache(app, &cache);
}

fn save_last_sync_at(app: &AppHandle, ts: i64) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.set(KEY_LAST_SYNC, serde_json::json!(ts));
        let _ = store.save();
    }
}

// ── Sync-Logik ───────────────────────────────────────────────────────────────

/// Server-Sync: Notizen herunterladen, Konflikte erkennen, Löschungen erkennen,
/// ausstehende lokale Änderungen hochladen.
///
/// Port von Android's `WebDavSyncService.syncNotes()` (Phasen 5+6).
/// Sicherheitswächter verhindern Massen-Löschungen durch leere PROPFIND-Antworten.
pub async fn run_sync(
    client: &WebDavClient,
    app: &AppHandle,
    device_id: &str,
    retention_ms: i64,
) -> SyncSummary {
    let mut summary = SyncSummary::default();

    // 1. Offline-Queue abarbeiten (Phase 2)
    sync_queue::drain_sync_queue(client, app, device_id, retention_ms).await;

    // 2. Aktuellen lokalen Cache laden
    let mut cache = load_note_cache(app);

    // 3. Alle Server-Notizen abrufen
    let note_locations = match client.list_notes_with_folders().await {
        Ok(locs) => locs,
        Err(e) => {
            eprintln!("[sync] list_notes_with_folders fehlgeschlagen: {}", e);
            return summary;
        }
    };

    let mut server_notes: Vec<Note> = Vec::new();
    for (id, folder) in &note_locations {
        match client.get_note(id, folder.as_deref()).await {
            Ok(note) => server_notes.push(note),
            Err(e) => eprintln!("[sync] get_note {} fehlgeschlagen: {}", id, e),
        }
    }

    // Sicherheitswächter 1: Leerer Server-Scan bei gefülltem Cache → Löscherkennung überspringen
    // (schützt vor Massen-Trash durch leere PROPFIND-Antwort bei Server-Problem)
    let abort_deletion = server_notes.is_empty() && !cache.is_empty();
    if abort_deletion {
        eprintln!(
            "[sync] Sicherheitswächter: Server lieferte 0 Notizen, Cache hat {} Einträge \
             — Löscherkennung übersprungen",
            cache.len()
        );
    }

    let server_ids: HashSet<String> = server_notes.iter().map(|n| n.id.clone()).collect();
    let now = chrono::Utc::now().timestamp_millis();

    // Lokal-exklusive Ordner einmal vorberechnen (verhindert O(n) Store-Reads in den Loops).
    let local_only_folders: HashSet<String> = local_store::active_folders(app)
        .into_iter()
        .map(|f| f.name.to_lowercase())
        .collect();

    // 4. Lösch-Ledger lesen (für Unterscheidung absichtlich vs. unerwartet gelöscht)
    let ledger = client.read_deletions().await;
    let deletion_map: HashMap<String, i64> = ledger
        .deleted_notes
        .iter()
        .map(|r| (r.id.clone(), r.deleted_at))
        .collect();

    // 5. Download-Phase: Server-Notizen mit Cache vergleichen (LWW-Merge)
    for server_note in &server_notes {
        // Server-Notizen aus lokal-exklusiven Ordnern überspringen (Phase 3)
        if server_note
            .folder_name
            .as_deref()
            .map(|f| local_only_folders.contains(&f.to_lowercase()))
            .unwrap_or(false)
        {
            continue;
        }

        if let Some(entry) = cache.get_mut(&server_note.id) {
            if server_note.updated_at > entry.note.updated_at {
                if entry.note.sync_status == SyncStatus::Pending {
                    // Lokale ausstehende Änderung + Server neuer → Konflikt (Phase 6)
                    entry.note.sync_status = SyncStatus::Conflict;
                    summary.conflicts_detected += 1;
                    eprintln!("[sync] Konflikt erkannt für Notiz {}", server_note.id);
                } else {
                    // Server-Version ist neuer → lokalen Cache überschreiben
                    entry.note = server_note.clone();
                    entry.note.sync_status = SyncStatus::Synced;
                    entry.last_synced_at = now;
                    summary.notes_downloaded += 1;
                }
            } else {
                // Lokale Version gleich alt oder neuer → Sync-Zeitstempel auffrischen
                entry.last_synced_at = now;
            }
        } else {
            // Neue Notiz vom Server → in Cache aufnehmen
            let mut n = server_note.clone();
            n.sync_status = SyncStatus::Synced;
            cache.insert(
                n.id.clone(),
                NoteCacheEntry {
                    note: n,
                    last_synced_at: now,
                    etag: None,
                },
            );
            summary.notes_downloaded += 1;
        }
    }

    // 6. Server-Löscherkennung (Port von NoteDownloader.detectDeletions)
    if !abort_deletion {
        // Alle SYNCED-Cache-Einträge die NICHT in einem local-only Ordner sind
        let synced_ids: Vec<String> = cache
            .values()
            .filter(|e| {
                e.note.sync_status == SyncStatus::Synced
                    && !e
                        .note
                        .folder_name
                        .as_deref()
                        .map(|f| local_only_folders.contains(&f.to_lowercase()))
                        .unwrap_or(false)
            })
            .map(|e| e.note.id.clone())
            .collect();

        let missing: Vec<String> = synced_ids
            .iter()
            .filter(|id| !server_ids.contains(*id))
            .cloned()
            .collect();

        // Sicherheitswächter 2: Zu viele gleichzeitig verschwunden → Erkennung abbrechen
        // Schwellenwert: ≥10 fehlende UND ≥80% aller SYNCED-Notizen fehlen
        let too_many_missing = !synced_ids.is_empty()
            && missing.len() >= 10
            && missing.len() * 10 >= synced_ids.len() * 8;

        if too_many_missing {
            eprintln!(
                "[sync] Sicherheitswächter: {}/{} SYNCED-Notizen fehlen im Server-Scan \
                 — Löscherkennung abgebrochen (Schutz vor Massen-Trash)",
                missing.len(),
                synced_ids.len()
            );
        } else {
            for id in &missing {
                if let Some(entry) = cache.get_mut(id) {
                    let already_trashed = entry.note.trashed_at.is_some();
                    // Im Lösch-Ledger mit deleted_at ≥ note.updated_at → absichtlich gelöscht
                    let in_ledger = deletion_map
                        .get(id)
                        .map(|&deleted_at| deleted_at >= entry.note.updated_at)
                        .unwrap_or(false);

                    if already_trashed || in_ledger {
                        // Sauber gelöscht — aus Cache entfernen
                        cache.remove(id);
                        summary.notes_deleted_on_server += 1;
                    } else {
                        // Unerwartet verschwunden → als "auf Server gelöscht" markieren.
                        // In den lokalen Store legen, damit die Notiz im Papierkorb sichtbar ist
                        // und der Nutzer sie wiederherstellen oder endgültig löschen kann.
                        entry.note.sync_status = SyncStatus::DeletedOnServer;
                        entry.note.trashed_at = Some(now);
                        entry.note.updated_at = now;
                        local_store::put_note(app, &entry.note);
                        summary.notes_deleted_on_server += 1;
                        eprintln!(
                            "[sync] Notiz {} auf Server verschwunden → DELETED_ON_SERVER",
                            id
                        );
                    }
                }
            }
        }
    }

    // 7. PENDING-Notizen hochladen (lokale Änderungen aus Cache)
    // Hinweis: Derzeit schreibt save_note() direkt zum Server (SYNCED).
    // Dieser Pfad wird aktiv wenn die Architektur auf local-first umgestellt wird (Phase 4 voll).
    let pending_ids: Vec<String> = cache
        .values()
        .filter(|e| e.note.sync_status == SyncStatus::Pending)
        .map(|e| e.note.id.clone())
        .collect();

    for id in &pending_ids {
        if let Some(entry) = cache.get_mut(id) {
            entry.note.updated_at = now;
            match client.save_note(&entry.note).await {
                Ok(()) => {
                    entry.note.sync_status = SyncStatus::Synced;
                    entry.last_synced_at = now;
                    summary.notes_uploaded += 1;
                }
                Err(e) => eprintln!("[sync] Upload {} fehlgeschlagen: {}", id, e),
            }
        }
    }

    // 8. Cache und letzten Sync-Zeitstempel persistieren
    save_note_cache(app, &cache);
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
