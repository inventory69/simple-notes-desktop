use crate::error::{AppError, Result};
use crate::models::Note;
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;
use tauri::{AppHandle, Manager};

/// Referenz-Pattern für Bild-Anhänge im Notiz-Content: `![alt](.assets/<name>)`.
/// Identisch zu Android `AssetReferences.kt` / `MarkdownEngine.IMAGE_REGEX`.
static EXTRACT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[[^\]]*]\(\.assets/([A-Za-z0-9][A-Za-z0-9._-]*)\)")
        .expect("asset extract regex is valid")
});

/// Grace-Period für die GC: unreferenzierte Assets erst nach 72h löschen (deckt
/// den Zeitraum ab, in dem ein zweites Gerät die Notiz noch nicht heruntergeladen hat).
pub const GRACE_MS: i64 = 72 * 60 * 60 * 1000;

/// Extrahiert alle `.assets/<name>`-Referenzen aus einem einzelnen Content-String.
pub fn extract_asset_names(content: &str) -> HashSet<String> {
    EXTRACT_REGEX
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

/// Extrahiert alle Referenzen über den gesamten Notiz-Bestand (inkl. Trash/Archiv —
/// eine getrashte Notiz referenziert ihr Bild weiterhin, GC darf es nicht löschen).
pub fn extract_all_referenced(notes: &[Note]) -> HashSet<String> {
    notes
        .iter()
        .flat_map(|n| extract_asset_names(&n.content))
        .collect()
}

/// Lehnt Namen ab, die als Pfad-Traversal missbraucht werden könnten. Die Extract-Regex lässt
/// `/` und `..` ohnehin nicht durch — das hier ist eine defensive Zusatzsperre vor jedem Join.
fn guard_name(name: &str) -> Result<()> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::Image(format!("invalid asset name: {}", name)));
    }
    Ok(())
}

/// Lokales Asset-Verzeichnis: `app_data_dir()/assets/` (persistent, nicht cache_dir —
/// Assets müssen bis zum erfolgreichen Upload überleben).
pub fn assets_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(e.to_string()))?
        .join("assets");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(dir)
}

pub fn asset_path(app: &AppHandle, name: &str) -> Result<PathBuf> {
    guard_name(name)?;
    Ok(assets_dir(app)?.join(name))
}

pub fn is_cached(app: &AppHandle, name: &str) -> bool {
    asset_path(app, name).map(|p| p.exists()).unwrap_or(false)
}

/// Speichert ein Asset atomar (Temp-Datei + rename), wie Androids AssetStore.
pub fn save_asset(app: &AppHandle, name: &str, bytes: &[u8]) -> Result<()> {
    let path = asset_path(app, name)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| AppError::Io(e.to_string()))?;
    std::fs::rename(&tmp, &path).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

/// Alle lokal zwischengespeicherten Assets mit Änderungszeit (Unix ms).
pub fn list_local(app: &AppHandle) -> Result<Vec<(String, i64)>> {
    let dir = assets_dir(app)?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| AppError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::Io(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".tmp") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let mtime_ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        out.push((name, mtime_ms));
    }
    Ok(out)
}

pub fn delete_local(app: &AppHandle, name: &str) {
    if let Ok(path) = asset_path(app, name) {
        let _ = std::fs::remove_file(path);
    }
}

/// MIME-Typ für die Content-Type-Header beim Rendern/Upload. Unbekannte Endungen fallen auf
/// `application/octet-stream` zurück statt einen Fehler zu werfen (best-effort).
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

/// `data:<mime>;base64,<...>` für ein Asset — Transportformat für Clipboard/Share.
pub fn data_url(name: &str, bytes: &[u8]) -> String {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    format!(
        "data:{};base64,{}",
        mime_for_ext(ext),
        STANDARD.encode(bytes)
    )
}

/// Ermittelt, welche Assets lokal und remote per GC gelöscht werden dürfen.
/// Port von Android `AssetGc.kt`: unreferenziert UND älter als die Grace-Period.
/// Remote-Sweep nur, wenn `allow_remote_sweep` (Guard gegen kaputte Zyklen) und die
/// Server-mtime bekannt ist.
pub fn compute_gc_targets(
    referenced: &HashSet<String>,
    local_mtimes: &[(String, i64)],
    server_mtimes: &[(String, Option<i64>)],
    now: i64,
    allow_remote_sweep: bool,
    grace_ms: i64,
) -> (Vec<String>, Vec<String>) {
    let local_to_delete: Vec<String> = local_mtimes
        .iter()
        .filter(|(name, mtime)| !referenced.contains(name) && now - mtime > grace_ms)
        .map(|(name, _)| name.clone())
        .collect();

    let remote_to_delete: Vec<String> = if allow_remote_sweep {
        server_mtimes
            .iter()
            .filter_map(|(name, mtime)| mtime.map(|m| (name, m)))
            .filter(|(name, mtime)| !referenced.contains(*name) && now - mtime > grace_ms)
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        Vec::new()
    };

    (local_to_delete, remote_to_delete)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_asset_names_single() {
        let content = "Look at this ![photo](.assets/abc1234567890def.webp) here";
        let names = extract_asset_names(content);
        assert_eq!(names.len(), 1);
        assert!(names.contains("abc1234567890def.webp"));
    }

    #[test]
    fn test_extract_asset_names_multiple_dedup() {
        let content = "![a](.assets/x.webp) text ![b](.assets/y.jpg) ![c](.assets/x.webp)";
        let names = extract_asset_names(content);
        assert_eq!(names.len(), 2);
        assert!(names.contains("x.webp"));
        assert!(names.contains("y.jpg"));
    }

    #[test]
    fn test_extract_asset_names_ignores_non_asset_links() {
        let content = "![alt](https://example.com/x.png) and [link](.assets/foo.png)";
        let names = extract_asset_names(content);
        assert!(names.is_empty());
    }

    #[test]
    fn test_extract_asset_names_no_match_on_empty() {
        assert!(extract_asset_names("plain text, no images").is_empty());
    }

    #[test]
    fn test_extract_all_referenced_across_notes() {
        let mut n1 = Note::new("A".to_string(), "tauri-x".to_string());
        n1.content = "![a](.assets/one.webp)".to_string();
        let mut n2 = Note::new("B".to_string(), "tauri-x".to_string());
        n2.content = "![b](.assets/two.webp)".to_string();
        let notes = vec![n1, n2];
        let refs = extract_all_referenced(&notes);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_guard_name_rejects_traversal() {
        assert!(guard_name("../evil.png").is_err());
        assert!(guard_name("a/b.png").is_err());
        assert!(guard_name("a\\b.png").is_err());
        assert!(guard_name("normal.png").is_ok());
    }

    #[test]
    fn test_mime_for_ext() {
        assert_eq!(mime_for_ext("webp"), "image/webp");
        assert_eq!(mime_for_ext("JPG"), "image/jpeg");
        assert_eq!(mime_for_ext("png"), "image/png");
        assert_eq!(mime_for_ext("gif"), "image/gif");
        assert_eq!(mime_for_ext("bin"), "application/octet-stream");
    }

    #[test]
    fn test_data_url() {
        assert_eq!(data_url("a.webp", b"xy"), "data:image/webp;base64,eHk=");
        assert_eq!(data_url("a.png", b"xy"), "data:image/png;base64,eHk=");
        assert_eq!(
            data_url("a.bin", b"xy"),
            "data:application/octet-stream;base64,eHk="
        );
    }

    // ── compute_gc_targets ───────────────────────────────────────────────────────

    #[test]
    fn test_gc_keeps_referenced_assets() {
        let referenced: HashSet<String> = ["kept.webp".to_string()].into_iter().collect();
        let local = vec![("kept.webp".to_string(), 0i64)];
        let (local_del, _) =
            compute_gc_targets(&referenced, &local, &[], 1_000_000, true, GRACE_MS);
        assert!(local_del.is_empty(), "referenced asset must survive GC");
    }

    #[test]
    fn test_gc_deletes_unreferenced_past_grace() {
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let local = vec![("stale.webp".to_string(), now - GRACE_MS - 1)];
        let (local_del, _) = compute_gc_targets(&referenced, &local, &[], now, true, GRACE_MS);
        assert_eq!(local_del, vec!["stale.webp".to_string()]);
    }

    #[test]
    fn test_gc_keeps_unreferenced_within_grace() {
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let local = vec![("fresh.webp".to_string(), now - GRACE_MS + 1)];
        let (local_del, _) = compute_gc_targets(&referenced, &local, &[], now, true, GRACE_MS);
        assert!(
            local_del.is_empty(),
            "asset within grace period must survive"
        );
    }

    #[test]
    fn test_gc_boundary_exact_grace_survives() {
        // now - mtime == grace_ms is NOT > grace_ms → must survive (strict inequality)
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let local = vec![("boundary.webp".to_string(), now - GRACE_MS)];
        let (local_del, _) = compute_gc_targets(&referenced, &local, &[], now, true, GRACE_MS);
        assert!(
            local_del.is_empty(),
            "exact grace boundary must not be deleted yet"
        );
    }

    #[test]
    fn test_gc_remote_sweep_guard_false_skips_remote() {
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let server = vec![("orphan.webp".to_string(), Some(now - GRACE_MS - 1))];
        let (_, remote_del) = compute_gc_targets(&referenced, &[], &server, now, false, GRACE_MS);
        assert!(
            remote_del.is_empty(),
            "guard=false must skip remote sweep entirely"
        );
    }

    #[test]
    fn test_gc_remote_sweep_guard_true_deletes_orphan() {
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let server = vec![("orphan.webp".to_string(), Some(now - GRACE_MS - 1))];
        let (_, remote_del) = compute_gc_targets(&referenced, &[], &server, now, true, GRACE_MS);
        assert_eq!(remote_del, vec!["orphan.webp".to_string()]);
    }

    #[test]
    fn test_gc_remote_unknown_mtime_never_deleted() {
        let referenced: HashSet<String> = HashSet::new();
        let now = 1_000_000_000i64;
        let server = vec![("unknown-mtime.webp".to_string(), None)];
        let (_, remote_del) = compute_gc_targets(&referenced, &[], &server, now, true, GRACE_MS);
        assert!(
            remote_del.is_empty(),
            "asset with unknown mtime must never be swept"
        );
    }

    #[test]
    fn test_gc_trash_referenced_asset_survives() {
        // A note in the trash still counts as "referenced" — extract_all_referenced doesn't
        // filter by trashedAt, so the caller passes it in like any active note's asset.
        let referenced: HashSet<String> = ["trashed-note-image.webp".to_string()]
            .into_iter()
            .collect();
        let now = 1_000_000_000i64;
        let local = vec![("trashed-note-image.webp".to_string(), now - GRACE_MS - 1)];
        let server = vec![(
            "trashed-note-image.webp".to_string(),
            Some(now - GRACE_MS - 1),
        )];
        let (local_del, remote_del) =
            compute_gc_targets(&referenced, &local, &server, now, true, GRACE_MS);
        assert!(local_del.is_empty());
        assert!(remote_del.is_empty());
    }
}
