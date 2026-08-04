//! Menschenlesbares Aktivitätsprotokoll — Issue #128 Teil 3. Format-Spec:
//! `project-docs/simple-notes-sync/activity-log-format.md`. Android-Äquivalent: `utils/ActivityLog.kt`.
//!
//! Kern-Funktionen nehmen ein Verzeichnis (`&Path`) statt eines `AppHandle` entgegen — testbar
//! ohne Tauri-Mock (dieses Repo hat dafür keine Infrastruktur, s. `local_store.rs`/`webdav.rs`
//! Tests). Die dünnen `AppHandle`-Wrapper am Ende sind wie der Rest der store-gebundenen App-Logik
//! ungetestet (Konvention dieses Repos).

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const FILE_NAME: &str = "activity.jsonl";
pub const FILE_NAME_BAK: &str = "activity.jsonl.1";

/// Seitengröße für den Settings-UI-Screen — kein Infinite-Scroll wie Android (Format-Spec
/// Desktop-Parität: einfache Anzeige im bestehenden Einstellungs-UI reicht).
pub const UI_PAGE_SIZE: usize = 200;

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024; // lokales Safety-Net, Rotation per rename statt Rewrite
const TAIL_CHUNK_BYTES: u64 = 64 * 1024;
const TAIL_CHUNK_GROWTH: u64 = 4;
const TITLE_MAX_LEN: usize = 200;
const ERR_MAX_LEN: usize = 200;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Op {
    Create,
    Edit,
    Trash,
    Restore,
    Purge,
    Upload,
    Download,
    Conflict,
    FolderDelete,
    SyncOk,
    SyncFail,
    DeletionSkipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Src {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub v: u32,
    pub ts: i64,
    pub op: Op,
    pub src: Src,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub dev: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Menschenlesbarer Gerätename — Desktop-Äquivalent zu Androids `Build.MANUFACTURER MODEL`.
// ponytail: kein `hostname`-Crate für einen Nice-to-have; Plattform+DE reicht, um die Frage aus
// #128 ("welcher Client war es?") zu beantworten. Echten Hostnamen ergänzen, falls gewünscht.
pub fn device_display_name() -> String {
    match crate::desktop_environment_impl() {
        Some(de) => format!("Desktop ({}/{})", crate::platform_impl(), de),
        None => format!("Desktop ({})", crate::platform_impl()),
    }
}

pub fn parse_line(line: &str) -> Option<Entry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

/// Nur noch Test-Helfer (Gegenstück zu [`parse_line`]) — geschrieben wird zeilenweise per
/// [`append_entry`], und ohne Server-Sync gibt es keinen Pfad, der eine ganze Liste serialisiert.
#[cfg(test)]
pub fn serialize(entries: &[Entry]) -> String {
    entries
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .map(|s| s + "\n")
        .collect()
}

// ── Kern (verzeichnisbasiert, testbar) ──────────────────────────────────────────

/// Schreibt einen Eintrag ans Ende von `<dir>/activity.jsonl` und rotiert bei Bedarf.
/// Best-effort: Fehler werden geloggt, nie propagiert — ein Logging-Fehler darf den Aufrufer
/// (Trash/Sync/…) nie stoppen.
pub fn append_entry(dir: &Path, entry: &Entry) {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("[activity] Verzeichnis anlegen fehlgeschlagen: {}", e);
        return;
    }
    let line = match serde_json::to_string(entry) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[activity] Serialisierung fehlgeschlagen: {}", e);
            return;
        }
    };
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE_NAME))
        .and_then(|mut f| writeln!(f, "{}", line));
    if let Err(e) = result {
        eprintln!("[activity] Schreiben fehlgeschlagen: {}", e);
        return;
    }
    rotate_if_needed(dir);
}

fn rotate_if_needed(dir: &Path) {
    let path = dir.join(FILE_NAME);
    let Ok(meta) = std::fs::metadata(&path) else {
        return;
    };
    if meta.len() <= MAX_FILE_BYTES {
        return;
    }
    let backup = dir.join(FILE_NAME_BAK);
    let _ = std::fs::remove_file(&backup);
    if let Err(e) = std::fs::rename(&path, &backup) {
        eprintln!("[activity] Rotation fehlgeschlagen: {}", e);
    }
}

/// Liest den kompletten lokalen Bestand (Backup + aktuelle Datei, älteste zuerst). Kaputte
/// Zeilen werden übersprungen, nie die ganze Datei verworfen.
///
/// Nur noch Test-Helfer: der Upload-Pfad liest seit der Umstellung auf ein festes Zeilenfenster
/// per [`read_tail_from_dir`], damit nie die ganze Datei geparst wird.
#[cfg(test)]
pub fn read_all_from_dir(dir: &Path) -> Vec<Entry> {
    let mut lines = read_lines(&dir.join(FILE_NAME_BAK));
    lines.extend(read_lines(&dir.join(FILE_NAME)));
    lines.iter().filter_map(|l| parse_line(l)).collect()
}

#[cfg(test)]
fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Liest die letzten `max_lines` vollständigen Zeilen (neueste zuletzt), ohne die Datei komplett
/// zu laden — wächst in 64-KB-Schritten bis genug Zeilen gefunden sind oder der Dateianfang
/// erreicht ist. Fällt bei Bedarf auf das Rotations-Backup zurück.
pub fn read_tail_from_dir(dir: &Path, max_lines: usize) -> Vec<Entry> {
    let from_main: Vec<Entry> = tail_lines(&dir.join(FILE_NAME), max_lines)
        .iter()
        .filter_map(|l| parse_line(l))
        .collect();
    if from_main.len() >= max_lines {
        return from_main;
    }
    let remaining = max_lines - from_main.len();
    let mut from_backup: Vec<Entry> = tail_lines(&dir.join(FILE_NAME_BAK), remaining)
        .iter()
        .filter_map(|l| parse_line(l))
        .collect();
    from_backup.extend(from_main);
    from_backup
}

fn tail_lines(path: &Path, min_lines: usize) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    if len == 0 {
        return Vec::new();
    }
    let mut chunk = TAIL_CHUNK_BYTES;
    loop {
        let start = len.saturating_sub(chunk);
        if file.seek(SeekFrom::Start(start)).is_err() {
            return Vec::new();
        }
        let mut buf = vec![0u8; (len - start) as usize];
        if file.read_exact(&mut buf).is_err() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&buf);
        let mut lines: Vec<String> = text
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect();
        // Chunk-Anfang kann eine abgeschnittene (oder bei Rotation während des Lesens kaputte)
        // Zeile sein — defensiv verwerfen, außer wir sind am Dateianfang.
        if start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        if lines.len() >= min_lines || start == 0 {
            let skip = lines.len().saturating_sub(min_lines);
            return lines.split_off(skip);
        }
        chunk = chunk.saturating_mul(TAIL_CHUNK_GROWTH);
    }
}

/// Löscht Log-Datei + Backup. Idempotent.
pub fn clear_dir(dir: &Path) {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = std::fs::remove_file(dir.join(FILE_NAME));
    let _ = std::fs::remove_file(dir.join(FILE_NAME_BAK));
}

// ── AppHandle-Wrapper ────────────────────────────────────────────────────────

fn app_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

/// Schreibt einen Eintrag mit aktuellem Timestamp + Gerätename; kappt Titel/Fehlertext.
/// Reihenfolge der optionalen Parameter analog Kotlin `ActivityLog.log(op, src, id, title,
/// folder, why, err)`.
#[allow(clippy::too_many_arguments)]
pub fn log(
    app: &AppHandle,
    op: Op,
    src: Src,
    id: Option<&str>,
    title: Option<&str>,
    folder: Option<&str>,
    why: Option<&str>,
    err: Option<&str>,
) {
    let Some(dir) = app_dir(app) else { return };
    let entry = Entry {
        v: 1,
        ts: chrono::Utc::now().timestamp_millis(),
        op,
        src,
        id: id.map(str::to_string),
        title: title.map(|t| truncate(t, TITLE_MAX_LEN)),
        folder: folder.map(str::to_string),
        dev: device_display_name(),
        why: why.map(str::to_string),
        err: err.map(|e| truncate(e, ERR_MAX_LEN)),
    };
    append_entry(&dir, &entry);
}

pub fn read_tail(app: &AppHandle, max_lines: usize) -> Vec<Entry> {
    app_dir(app)
        .map(|d| read_tail_from_dir(&d, max_lines))
        .unwrap_or_default()
}

pub fn clear_local(app: &AppHandle) {
    if let Some(dir) = app_dir(app) {
        clear_dir(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "activity-log-test-{}-{}-{}",
            name,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(ts: i64) -> Entry {
        Entry {
            v: 1,
            ts,
            op: Op::SyncOk,
            src: Src::Local,
            id: None,
            title: None,
            folder: None,
            dev: "Test Device".to_string(),
            why: None,
            err: None,
        }
    }

    #[test]
    fn test_append_and_read_round_trip() {
        let dir = temp_dir("round-trip");
        let entry = Entry {
            v: 1,
            ts: 1000,
            op: Op::Trash,
            src: Src::Local,
            id: Some("n1".to_string()),
            title: Some("Einkaufsliste".to_string()),
            folder: Some("Haushalt".to_string()),
            dev: "Desktop (linux)".to_string(),
            why: None,
            err: None,
        };
        append_entry(&dir, &entry);
        let entries = read_all_from_dir(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].op, Op::Trash);
        assert_eq!(entries[0].src, Src::Local);
        assert_eq!(entries[0].id.as_deref(), Some("n1"));
        assert_eq!(entries[0].folder.as_deref(), Some("Haushalt"));
    }

    #[test]
    fn test_folder_omitted_round_trips_as_none() {
        let dir = temp_dir("root-note");
        append_entry(&dir, &sample(1));
        let entries = read_all_from_dir(&dir);
        assert_eq!(entries[0].folder, None);
    }

    #[test]
    fn test_op_serializes_to_expected_json_names() {
        let e = Entry {
            folder: None,
            ..sample(1)
        };
        let mut e2 = e.clone();
        e2.op = Op::FolderDelete;
        let mut e3 = e.clone();
        e3.op = Op::DeletionSkipped;
        assert!(serde_json::to_string(&e).unwrap().contains("\"SYNC_OK\""));
        assert!(serde_json::to_string(&e2)
            .unwrap()
            .contains("\"FOLDER_DELETE\""));
        assert!(serde_json::to_string(&e3)
            .unwrap()
            .contains("\"DELETION_SKIPPED\""));
    }

    #[test]
    fn test_corrupt_line_is_skipped_valid_lines_survive() {
        let dir = temp_dir("corrupt-line");
        let text = format!(
            "{}{{not valid json\n{}",
            serialize(&[sample(1)]),
            serialize(&[sample(2)])
        );
        std::fs::write(dir.join(FILE_NAME), text).unwrap();
        let entries = read_all_from_dir(&dir);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn test_blank_line_is_skipped() {
        let dir = temp_dir("blank-line");
        let text = format!("\n{}\n", serialize(&[sample(5)]));
        std::fs::write(dir.join(FILE_NAME), text).unwrap();
        assert_eq!(read_all_from_dir(&dir).len(), 1);
    }

    #[test]
    fn test_rotation_renames_main_to_backup_once_over_threshold() {
        let dir = temp_dir("rotation");
        let long_title = "x".repeat(500);
        for i in 0..20_000 {
            let entry = Entry {
                title: Some(truncate(&long_title, TITLE_MAX_LEN)),
                id: Some(format!("n{}", i)),
                ..sample(i as i64)
            };
            append_entry(&dir, &entry);
        }
        assert!(
            dir.join(FILE_NAME_BAK).exists(),
            "expected rotation backup to exist after exceeding size threshold"
        );
    }

    #[test]
    fn test_read_tail_returns_most_recent_entries_oldest_first() {
        let dir = temp_dir("tail");
        for i in 0..10 {
            let entry = Entry {
                id: Some(format!("n{}", i)),
                ..sample(i as i64)
            };
            append_entry(&dir, &entry);
        }
        let tail = read_tail_from_dir(&dir, 3);
        assert_eq!(tail.len(), 3);
        let ids: Vec<&str> = tail.iter().map(|e| e.id.as_deref().unwrap()).collect();
        assert_eq!(ids, vec!["n7", "n8", "n9"]);
    }

    #[test]
    fn test_clear_dir_removes_files() {
        let dir = temp_dir("clear");
        append_entry(&dir, &sample(1));
        clear_dir(&dir);
        assert!(read_all_from_dir(&dir).is_empty());
        assert!(!dir.join(FILE_NAME).exists());
    }

    #[test]
    fn test_parse_line_rejects_blank() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
    }

    #[test]
    fn test_parse_line_rejects_garbage() {
        assert!(parse_line("{not json").is_none());
    }
}
