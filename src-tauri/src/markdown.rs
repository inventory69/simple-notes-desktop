use crate::error::{AppError, Result};
use crate::models::{ChecklistItem, Note, NoteType};
use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;
use uuid::Uuid;

/// Konvertiert Unix-Millisekunden zu ISO8601 UTC String
///
/// Format: "2026-02-04T10:25:29Z" (ohne Millisekunden!)
pub fn timestamp_to_iso(ts: i64) -> String {
    DateTime::from_timestamp_millis(ts)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Parst ISO8601 String zu Unix-Millisekunden
///
/// Unterstützt mehrere Formate:
/// - "2026-02-04T10:25:29Z"
/// - "2026-02-04T10:25:29+01:00"
/// - "2026-02-04T10:25:29.123Z"
/// - "2026-02-04 10:25:29"
#[allow(dead_code)]
pub fn iso_to_timestamp(s: &str) -> Result<i64> {
    let normalized = s.trim().replace(' ', "T");

    // Versuche mit Timezone-Aware Parsing
    let formats_with_tz = [
        "%Y-%m-%dT%H:%M:%S%:z",    // 2026-02-04T10:25:29+01:00
        "%Y-%m-%dT%H:%M:%S%z",     // 2026-02-04T10:25:29+0100
        "%Y-%m-%dT%H:%M:%S%.f%:z", // 2026-02-04T10:25:29.123+01:00
        "%Y-%m-%dT%H:%M:%S%.f%z",  // 2026-02-04T10:25:29.123+0100
    ];

    for fmt in formats_with_tz {
        if let Ok(dt) = DateTime::parse_from_str(&normalized, fmt) {
            return Ok(dt.timestamp_millis());
        }
    }

    // Versuche mit "Z" (UTC) - manuell ersetzen da "Z" kein valider Chrono Format-Specifier ist
    let utc_normalized = normalized.replace('Z', "+00:00");
    for fmt in formats_with_tz {
        if let Ok(dt) = DateTime::parse_from_str(&utc_normalized, fmt) {
            return Ok(dt.timestamp_millis());
        }
    }

    // Fallback: ohne Timezone als UTC interpretieren
    let formats_naive = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f"];

    for fmt in formats_naive {
        if let Ok(dt) = NaiveDateTime::parse_from_str(normalized.trim_end_matches('Z'), fmt) {
            return Ok(dt.and_utc().timestamp_millis());
        }
    }

    Err(AppError::InvalidTimestamp(s.to_string()))
}

/// Generiert Markdown mit YAML-Frontmatter aus einer Note
pub fn generate_markdown(note: &Note) -> String {
    let created_iso = timestamp_to_iso(note.created_at);
    let updated_iso = timestamp_to_iso(note.updated_at);
    let note_type = match note.note_type {
        NoteType::Text => "text",
        NoteType::Checklist => "checklist",
    };

    let mut md = format!(
        "---\n\
         id: {}\n\
         created: {}\n\
         updated: {}\n\
         device: {}\n\
         type: {}",
        note.id, created_iso, updated_iso, note.device_id, note_type
    );

    // F2: Sort-Option in Frontmatter schreiben
    if let Some(ref sort_option) = note.checklist_sort_option {
        md.push_str(&format!("\nsort: {}", sort_option.to_lowercase()));
    }

    md.push_str(&format!("\n---\n\n# {}\n\n", note.title));

    match note.note_type {
        NoteType::Text => {
            md.push_str(&note.content);
        }
        NoteType::Checklist => {
            if let Some(items) = &note.checklist_items {
                let mut sorted = items.clone();
                sorted.sort_by_key(|i| i.order);
                for item in sorted {
                    let checkbox = if item.is_checked { "[x]" } else { "[ ]" };
                    md.push_str(&format!("- {} {}\n", checkbox, item.text));
                }
            }
        }
    }

    md
}

/// Regex für Frontmatter (lazy static)
#[allow(dead_code)]
fn frontmatter_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^---\n([\s\S]*?)\n---\n([\s\S]*)$").unwrap())
}

/// Regex für Checklist-Items mit - Prefix
#[allow(dead_code)]
fn checklist_items_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^-\s*\[([ xX])\]\s*(.+)$").unwrap())
}

/// Regex für Checklist-Items ohne - Prefix (für Recovery)
#[allow(dead_code)]
fn checklist_recovery_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*\[([ xX])\]\s*(.+)$").unwrap())
}

/// Parst Markdown mit YAML-Frontmatter zu einer Note
///
/// # Arguments
/// * `md` - Markdown-String mit Frontmatter
/// * `server_mtime` - Optional: Server-Datei mtime für Timestamp-Priorität
#[allow(dead_code)]
pub fn parse_markdown(md: &str, server_mtime: Option<i64>) -> Result<Note> {
    let re = frontmatter_regex();

    let captures = re
        .captures(md)
        .ok_or_else(|| AppError::ParseError("No frontmatter found".to_string()))?;

    let yaml_block = captures.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    // YAML parsen (einfach per Zeilen-Split)
    let mut metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in yaml_block.lines() {
        if let Some((key, value)) = line.split_once(':') {
            metadata.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    // Titel aus erstem # Heading extrahieren
    let title = body
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line[2..].trim().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Content nach dem Titel extrahieren
    let content_start = body
        .find("\n# ")
        .map(|i| {
            body[i + 1..]
                .find('\n')
                .map(|j| i + 1 + j + 1)
                .unwrap_or(body.len())
        })
        .unwrap_or(0);

    let content_after_title = body[content_start..].trim();

    // Note-Typ bestimmen
    let note_type = match metadata.get("type").map(|s| s.as_str()) {
        Some("checklist") => NoteType::Checklist,
        _ => NoteType::Text,
    };

    // Timestamps parsen
    let created_at = metadata
        .get("created")
        .and_then(|s| iso_to_timestamp(s).ok())
        .unwrap_or_else(|| Utc::now().timestamp_millis());

    let yaml_updated_at = metadata
        .get("updated")
        .and_then(|s| iso_to_timestamp(s).ok())
        .unwrap_or(created_at);

    // Server mtime hat Priorität (v1.7.2 IMPL_014)
    let updated_at = match server_mtime {
        Some(mtime) if mtime > yaml_updated_at => mtime,
        _ => yaml_updated_at,
    };

    // Checklist-Items parsen
    let (content, checklist_items) = if note_type == NoteType::Checklist {
        let items = parse_checklist_items(content_after_title);
        let fallback = generate_checklist_fallback(&items);
        (fallback, Some(items))
    } else {
        (content_after_title.to_string(), None)
    };

    // F2: Parse checklist sort option from frontmatter
    let checklist_sort_option = metadata
        .get("sort")
        .map(|v| v.to_uppercase().replace('-', "_"));

    Ok(Note {
        id: metadata
            .get("id")
            .cloned()
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        title,
        content,
        created_at,
        updated_at,
        device_id: metadata
            .get("device")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        sync_status: crate::models::SyncStatus::Synced,
        note_type,
        checklist_items,
        checklist_sort_option,
    })
}

/// Parst Checklist-Items aus Markdown
#[allow(dead_code)]
fn parse_checklist_items(content: &str) -> Vec<ChecklistItem> {
    let re = checklist_items_regex();

    content
        .lines()
        .filter_map(|line| re.captures(line))
        .enumerate()
        .map(|(order, caps)| {
            let is_checked = caps
                .get(1)
                .map(|m| m.as_str().to_lowercase() == "x")
                .unwrap_or(false);
            let text = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            ChecklistItem {
                id: Uuid::new_v4().to_string(),
                text,
                is_checked,
                order: order as i32,
            }
        })
        .collect()
}

/// Generiert Fallback-Content für Checklisten (ohne - Prefix)
#[allow(dead_code)]
fn generate_checklist_fallback(items: &[ChecklistItem]) -> String {
    let mut sorted = items.to_vec();
    sorted.sort_by_key(|i| i.order);
    sorted
        .iter()
        .map(|item| {
            let check = if item.is_checked { "x" } else { " " };
            format!("[{}] {}", check, item.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Versucht Checklist-Items aus Content-Fallback zu recovern
#[allow(dead_code)]
pub fn recover_checklist_from_content(content: &str) -> Vec<ChecklistItem> {
    let re = checklist_recovery_regex();

    content
        .lines()
        .filter_map(|line| re.captures(line))
        .enumerate()
        .map(|(order, caps)| {
            let is_checked = caps
                .get(1)
                .map(|m| m.as_str().to_lowercase() == "x")
                .unwrap_or(false);
            let text = caps
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            ChecklistItem {
                id: Uuid::new_v4().to_string(),
                text,
                is_checked,
                order: order as i32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_to_iso_basic() {
        let ts = 1770202329000i64; // 2026-02-04 10:25:29 UTC
        let iso = timestamp_to_iso(ts);
        // Check format, not exact match (timezone can vary)
        assert!(iso.starts_with("2026-02-04T"));
        assert!(iso.ends_with("Z"));
        assert!(!iso.contains("."));
    }

    #[test]
    fn test_timestamp_to_iso_no_milliseconds() {
        let ts = 1770202329186i64;
        let iso = timestamp_to_iso(ts);
        assert!(!iso.contains("."));
        assert!(iso.ends_with("Z"));
    }

    #[test]
    fn test_iso_to_timestamp_basic() {
        let iso = "2026-02-04T10:25:29Z";
        let ts = iso_to_timestamp(iso).unwrap();
        // Konvertiere zurück und vergleiche
        let back = timestamp_to_iso(ts);
        assert_eq!(back, iso);
    }

    #[test]
    fn test_timestamp_roundtrip() {
        let original = 1770202329000i64;
        let iso = timestamp_to_iso(original);
        let parsed = iso_to_timestamp(&iso).unwrap();
        // Should roundtrip exactly
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_parse_checklist_items() {
        let content = "- [ ] Item 1\n- [x] Item 2\n- [X] Item 3";
        let items = parse_checklist_items(content);
        assert_eq!(items.len(), 3);
        assert!(!items[0].is_checked);
        assert!(items[1].is_checked);
        assert!(items[2].is_checked);
    }
}
