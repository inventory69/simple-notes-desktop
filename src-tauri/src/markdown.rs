use crate::models::{Note, NoteType};
use chrono::DateTime;

/// Konvertiert Unix-Millisekunden zu ISO8601 UTC String
///
/// Format: "2026-02-04T10:25:29Z" (ohne Millisekunden!)
pub fn timestamp_to_iso(ts: i64) -> String {
    DateTime::from_timestamp_millis(ts)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
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
        md.push_str(&format!("\nsort: {}", sort_option));
    }

    // Cross-App v2.5.0-Felder: gleiche Reihenfolge/Format wie die Android-App
    // (Note.toMarkdown): imported, labels, color, pinned. Nur schreiben wenn gesetzt.
    if let Some(imported) = note.imported_at {
        md.push_str(&format!("\nimported: {}", imported));
    }
    if let Some(ref labels) = note.labels {
        if !labels.is_empty() {
            let quoted = labels
                .iter()
                .map(|l| format!("\"{}\"", l))
                .collect::<Vec<_>>()
                .join(", ");
            md.push_str(&format!("\nlabels: [{}]", quoted));
        }
    }
    if let Some(ref color) = note.color {
        md.push_str(&format!("\ncolor: \"{}\"", color));
    }
    if let Some(is_pinned) = note.is_pinned {
        md.push_str(&format!("\npinned: {}", is_pinned));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NoteType;

    #[test]
    fn test_timestamp_to_iso_basic() {
        let ts = 1770202329000i64; // 2026-02-04 10:25:29 UTC
        let iso = timestamp_to_iso(ts);
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
    fn test_generate_markdown_includes_v250_fields() {
        // Cross-App-Parität: die Felder müssen im Frontmatter im selben Format wie
        // bei der Android-App (Note.toMarkdown) erscheinen.
        let mut note = Note::new("My Note".to_string(), "tauri-abc".to_string());
        note.color = Some("#FF5733".to_string());
        note.labels = Some(vec!["work".to_string(), "urgent".to_string()]);
        note.imported_at = Some(1699999999999);
        note.is_pinned = Some(true);

        let md = generate_markdown(&note);
        assert!(
            md.contains("\nimported: 1699999999999"),
            "missing imported line: {md}"
        );
        assert!(
            md.contains("\nlabels: [\"work\", \"urgent\"]"),
            "missing labels line: {md}"
        );
        assert!(
            md.contains("\ncolor: \"#FF5733\""),
            "missing color line: {md}"
        );
        assert!(md.contains("\npinned: true"), "missing pinned line: {md}");
    }

    #[test]
    fn test_generate_markdown_sort_option_uppercase() {
        // Android schreibt sort: UNCHECKED_FIRST (Rohwert), Desktop muss gleich sein
        let mut note = Note::new("Check".to_string(), "tauri-abc".to_string());
        note.note_type = NoteType::Checklist;
        note.checklist_sort_option = Some("UNCHECKED_FIRST".to_string());

        let md = generate_markdown(&note);
        assert!(
            md.contains("\nsort: UNCHECKED_FIRST"),
            "sort option should be uppercase (Android parity): {md}"
        );
        assert!(
            !md.contains("sort: unchecked_first"),
            "sort option must not be lowercased: {md}"
        );
    }

    #[test]
    fn test_generate_markdown_omits_unset_v250_fields() {
        let note = Note::new("Plain".to_string(), "tauri-abc".to_string());
        let md = generate_markdown(&note);
        assert!(!md.contains("color:"));
        assert!(!md.contains("labels:"));
        assert!(!md.contains("imported:"));
        assert!(!md.contains("pinned:"));
    }
}
