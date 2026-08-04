use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::activity_log::{self, Op, Src};
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
    pub notes_healed: usize,
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

/// Anzahl zusätzlicher Versuche für Asset-Up-/Downloads (Android-Parität: `putWithRetry`/
/// `getWithRetry`). Sequenziell statt parallel — Desktop-Sync läuft selten, Bilder sind klein.
// ponytail: kein Semaphore/paralleler Up-/Download wie Android; Parallelisierung erst wenn
// messbar zu langsam.
const ASSET_RETRIES: u32 = 2;

async fn put_asset_with_retry(
    client: &WebDavClient,
    name: &str,
    bytes: &[u8],
    mime: &str,
) -> crate::error::Result<()> {
    let mut last_err = None;
    for _ in 0..=ASSET_RETRIES {
        match client.put_asset(name, bytes, mime).await {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("loop runs at least once"))
}

async fn get_asset_with_retry(client: &WebDavClient, name: &str) -> crate::error::Result<Vec<u8>> {
    let mut last_err = None;
    for _ in 0..=ASSET_RETRIES {
        match client.get_asset(name).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("loop runs at least once"))
}

fn save_last_sync_at(app: &AppHandle, ts: i64) {
    if let Ok(store) = app.store(SYNC_STORE) {
        store.set(KEY_LAST_SYNC, serde_json::json!(ts));
        let _ = store.save();
    }
}

// ── Sync-Logik ───────────────────────────────────────────────────────────────

/// Alle Server-Notizen abrufen (PROPFIND + GET je UUID).
///
/// Gibt neben den erfolgreich geladenen Notizen auch `server_ids` zurück — die Menge **aller**
/// IDs aus dem Listing, unabhängig davon, ob der einzelne GET geklappt hat. Löscherkennung MUSS
/// gegen `server_ids` prüfen, nicht gegen `notes`: ein einzelner GET-Timeout darf eine Notiz
/// nicht als serverseitig gelöscht erscheinen lassen (das war die wahrscheinlichste Ursache für
/// den Datenverlust in #128 — der Desktop GETtet bei jedem Sync jede Notiz einzeln).
/// `complete` ist `false`, wenn das Ordner-Listing lückenhaft war oder mindestens ein GET
/// fehlschlug — in dem Fall bricht `run_sync` die Löscherkennung für diesen Zyklus komplett ab.
async fn fetch_server_notes(
    client: &WebDavClient,
) -> crate::error::Result<(Vec<Note>, HashSet<String>, bool)> {
    let (note_locations, listing_complete) = client.list_notes_with_folders().await?;
    let server_ids: HashSet<String> = note_locations.iter().map(|(id, _)| id.clone()).collect();
    let mut complete = listing_complete;
    let mut notes = Vec::new();
    for (id, folder) in note_locations {
        match client.get_note(&id, folder.as_deref()).await {
            Ok(note) => notes.push(note),
            Err(e) => {
                eprintln!("[sync] get_note {} fehlgeschlagen: {}", id, e);
                complete = false;
            }
        }
    }
    Ok((notes, server_ids, complete))
}

/// Menge aller Ordnernamen, die auf dem Server existieren (lowercased).
/// Quelle: Ordner aus Notiz-Pfaden ∪ folders.json ∪ physische Verzeichnisse.
async fn collect_server_folder_names(
    client: &WebDavClient,
) -> crate::error::Result<HashSet<String>> {
    let mut names = HashSet::new();
    let (locations, _complete) = client.list_notes_with_folders().await?;
    for (_id, folder) in locations {
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
async fn sync_folders(client: &WebDavClient, app: &AppHandle, write_markdown: bool) {
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
            client.ensure_folder_dirs(&f.name, write_markdown).await;
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

/// Sicherheitswächter: Löscherkennung darf nur auf einem vollständigen Server-Listing laufen.
/// `listing_complete=false` (Ordner-PROPFIND oder GET fehlgeschlagen) muss dieselbe Abbruch-
/// Reaktion auslösen wie der alte "0 Notizen bei gefülltem Store"-Fall — sonst wird ein einzelner
/// Timeout wieder zur Fehl-Löschung (#128).
fn should_abort_deletion(
    listing_complete: bool,
    server_notes_empty: bool,
    local_synced_nonempty: bool,
) -> bool {
    !listing_complete || (server_notes_empty && local_synced_nonempty)
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
    // Einmal pro Lauf gelesen (nicht auf dem Client gecacht), damit ein Toggle in den
    // Einstellungen ohne Reconnect beim nächsten Sync greift.
    let write_markdown = crate::markdown_export_enabled(app);

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
    sync_folders(client, app, write_markdown).await;

    // 3. Server-Notizen abrufen
    let (server_notes, server_ids, listing_complete) = match fetch_server_notes(client).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[sync] fetch fehlgeschlagen: {}", e);
            activity_log::log(app, Op::SyncFail, Src::Local, None, None, None, None, Some(&e.to_string()));
            return summary;
        }
    };

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
    let abort_deletion = should_abort_deletion(
        listing_complete,
        server_notes.is_empty(),
        !local_synced.is_empty(),
    );
    if abort_deletion {
        if !listing_complete {
            eprintln!(
                "[sync] Sicherheitswächter: Server-Listing unvollständig (Ordner-PROPFIND oder GET fehlgeschlagen) — Löscherkennung übersprungen"
            );
            activity_log::log(
                app,
                Op::DeletionSkipped,
                Src::Local,
                None,
                None,
                None,
                Some("listing_incomplete"),
                None,
            );
        } else {
            eprintln!(
                "[sync] Sicherheitswächter: Server lieferte 0 Notizen, {} lokale SYNCED — Löscherkennung übersprungen",
                local_synced.len()
            );
        }
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
                activity_log::log(
                    app,
                    Op::Download,
                    Src::Remote,
                    Some(&sn.id),
                    Some(&sn.title),
                    sn.folder_name.as_deref(),
                    None,
                    None,
                );
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
                        activity_log::log(
                            app,
                            Op::Conflict,
                            Src::Remote,
                            Some(&sn.id),
                            Some(&sn.title),
                            sn.folder_name.as_deref(),
                            None,
                            None,
                        );
                    } else {
                        // Server neuer → überschreiben
                        let mut n = sn.clone();
                        n.sync_status = SyncStatus::Synced;
                        local_store::put_note(app, &n);
                        summary.notes_downloaded += 1;
                        activity_log::log(
                            app,
                            Op::Download,
                            Src::Remote,
                            Some(&sn.id),
                            Some(&sn.title),
                            sn.folder_name.as_deref(),
                            None,
                            None,
                        );
                    }
                } else if local.sync_status == SyncStatus::DeletedOnServer {
                    // Self-Heal: die Notiz ist (noch/wieder) auf dem Server vorhanden, also war
                    // die frühere Löscherkennung ein Fehlalarm (unvollständiges Listing/GET).
                    // Getrashte Notizen haben nie DELETED_ON_SERVER, das setzt ausschließlich
                    // Abschnitt 5 unten — ein echter Papierkorb-Eintrag kann so nicht zurückgeholt
                    // werden.
                    let mut healed = local.clone();
                    healed.sync_status = SyncStatus::Synced;
                    healed.folder_name = sn.folder_name.clone();
                    healed.trashed_at = None;
                    local_store::put_note(app, &healed);
                    summary.notes_healed += 1;
                    activity_log::log(
                        app,
                        Op::Restore,
                        Src::Remote,
                        Some(&sn.id),
                        Some(&sn.title),
                        sn.folder_name.as_deref(),
                        Some("self_heal_present_on_server"),
                        None,
                    );
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
                    activity_log::log(
                        app,
                        Op::Trash,
                        Src::Remote,
                        Some(&n.id),
                        Some(&n.title),
                        n.folder_name.as_deref(),
                        Some("server_deletion_detected"),
                        None,
                    );
                }
                summary.notes_deleted_on_server += 1;
            }
        }
    }

    // 5.5 Asset-Upload (assets-first, Android-Parität E1): referenzierte, lokal vorhandene,
    // serverseitig fehlende Assets hochladen — bevor Notizen mit neuen `.assets/`-Links
    // gesynct werden, sonst zeigt ein zweites Gerät kurzzeitig ein kaputtes Bild.
    let _ = client.ensure_directories(write_markdown).await; // legt auch das Asset-Verzeichnis an
    let server_assets = client.list_server_assets().await.unwrap_or_default();
    let server_asset_names: HashSet<String> =
        server_assets.iter().map(|(n, _)| n.clone()).collect();
    // Referenzmenge über ALLE Notizen (inkl. Trash/Archiv) — eine getrashte Notiz behält ihr Bild.
    let all_notes_for_assets = local_store::list_notes(app);
    let referenced = crate::assets::extract_all_referenced(&all_notes_for_assets);
    let local_asset_names: HashSet<String> = crate::assets::list_local(app)
        .unwrap_or_default()
        .into_iter()
        .map(|(n, _)| n)
        .collect();

    for name in referenced
        .iter()
        .filter(|n| local_asset_names.contains(*n) && !server_asset_names.contains(*n))
    {
        let Ok(path) = crate::assets::asset_path(app, name) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let ext = name.rsplit('.').next().unwrap_or("");
        let mime = crate::assets::mime_for_ext(ext);
        if let Err(e) = put_asset_with_retry(client, name, &bytes, mime).await {
            eprintln!("[assets] Upload {} fehlgeschlagen: {}", name, e);
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
        match client.save_note(&n, write_markdown).await {
            Ok(()) => {
                local_store::mark_synced_if_unchanged(app, &n.id, n.updated_at);
                uploaded_ids.push(n.id.clone());
                summary.notes_uploaded += 1;
                // Trash-Zustand wird schon lokal geloggt (Op::Trash bei trash_note) — hier nur
                // echte Content-Uploads, sonst doppelt geloggt (Android-Parität NoteUploader).
                if n.trashed_at.is_none() {
                    activity_log::log(
                        app,
                        Op::Upload,
                        Src::Local,
                        Some(&n.id),
                        Some(&n.title),
                        n.folder_name.as_deref(),
                        None,
                        None,
                    );
                }
            }
            Err(e) => eprintln!("[sync] upload {} fehlgeschlagen: {}", n.id, e),
        }
    }
    // Frisch (wieder-)hochgeladene Notizen aus dem Server-Lösch-Ledger streichen,
    // damit ein alter Tombstone sie nicht beim nächsten Sync wieder „löscht".
    if !uploaded_ids.is_empty() {
        client.remove_deletions(&uploaded_ids).await;
    }

    // 7. Asset-Download + GC: fehlende referenzierte Assets nachladen (Referenzmenge aus dem
    // frisch gemergten Korpus), dann unreferenzierte Assets lokal + (guarded) remote aufräumen.
    let final_notes = local_store::list_notes(app);
    let referenced_final = crate::assets::extract_all_referenced(&final_notes);
    let local_names_after: HashSet<String> = crate::assets::list_local(app)
        .unwrap_or_default()
        .into_iter()
        .map(|(n, _)| n)
        .collect();

    let mut asset_download_had_errors = false;
    for name in referenced_final
        .iter()
        .filter(|n| !local_names_after.contains(*n) && server_asset_names.contains(*n))
    {
        match get_asset_with_retry(client, name).await {
            Ok(bytes) => {
                if let Err(e) = crate::assets::save_asset(app, name, &bytes) {
                    eprintln!("[assets] Speichern von {} fehlgeschlagen: {}", name, e);
                    asset_download_had_errors = true;
                }
            }
            Err(e) => {
                eprintln!("[assets] Download {} fehlgeschlagen: {}", name, e);
                asset_download_had_errors = true;
            }
        }
    }

    // Guard (Android-Parität allowRemoteSweep): kein Remote-Sweep bei leerem Notizbestand oder
    // wenn die Download-Phase Fehler hatte — sonst löscht ein kaputter Zyklus fremde Assets.
    let allow_remote_sweep = !final_notes.is_empty() && !asset_download_had_errors;
    let local_mtimes_final = crate::assets::list_local(app).unwrap_or_default();
    let (local_to_delete, remote_to_delete) = crate::assets::compute_gc_targets(
        &referenced_final,
        &local_mtimes_final,
        &server_assets,
        now,
        allow_remote_sweep,
        crate::assets::GRACE_MS,
    );
    for name in &local_to_delete {
        crate::assets::delete_local(app, name);
    }
    for name in &remote_to_delete {
        if let Err(e) = client.delete_asset(name).await {
            eprintln!("[assets] Remote-Löschung {} fehlgeschlagen: {}", name, e);
        }
    }

    save_last_sync_at(app, now);
    eprintln!(
        "[sync] Abgeschlossen: {} heruntergeladen, {} hochgeladen, {} Konflikte, {} auf Server gelöscht, {} wiederhergestellt",
        summary.notes_downloaded,
        summary.notes_uploaded,
        summary.conflicts_detected,
        summary.notes_deleted_on_server,
        summary.notes_healed
    );
    // Nur protokollieren, wenn der Sync tatsächlich etwas bewegt hat (Android-Parität):
    // ein Leerlauf-Sync soll das Protokoll nicht mit "Sync abgeschlossen" zutapezieren.
    if summary.notes_downloaded + summary.notes_uploaded > 0 {
        activity_log::log(
            app,
            Op::SyncOk,
            Src::Local,
            None,
            None,
            None,
            Some(&format!(
                "downloaded={} uploaded={} conflicts={} deleted={} healed={}",
                summary.notes_downloaded,
                summary.notes_uploaded,
                summary.conflicts_detected,
                summary.notes_deleted_on_server,
                summary.notes_healed
            )),
            None,
        );
    }
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

    // ── #128 Regression: unvollständiges Listing darf nie zur Löscherkennung führen ──────────

    #[test]
    fn test_abort_deletion_on_incomplete_listing_even_with_notes() {
        // Ordner-PROPFIND oder ein einzelner GET ist fehlgeschlagen, aber der Rest der Notizen
        // kam durch — genau der Fall, der #128 auslöste. Muss trotzdem abbrechen.
        assert!(should_abort_deletion(false, false, true));
        assert!(should_abort_deletion(false, false, false));
    }

    #[test]
    fn test_abort_deletion_on_empty_server_with_local_notes() {
        // Alter Wächter: 0 Notizen bei gefülltem lokalem Store bleibt abgedeckt.
        assert!(should_abort_deletion(true, true, true));
    }

    #[test]
    fn test_no_abort_on_complete_listing() {
        assert!(!should_abort_deletion(true, false, true));
        assert!(!should_abort_deletion(true, true, false));
    }
}
