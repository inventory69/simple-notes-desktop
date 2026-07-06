use chrono::DateTime;
use uuid::Uuid;

/// Escaped ein Text-Feld nach RFC5545 (iCalendar TEXT-Werte).
fn escape_ics_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}

/// Konvertiert Unix-Millisekunden zu iCalendar UTC-Zeitformat: "20260204T102529Z"
fn timestamp_to_ics(ts_ms: i64) -> String {
    DateTime::from_timestamp_millis(ts_ms)
        .map(|dt| dt.format("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_else(|| "19700101T000000Z".to_string())
}

/// Generiert ein minimales VEVENT-.ics: Start = jetzt, Ende = jetzt + 1h.
/// Spiegelt exakt das Verhalten der Android-App (ComposeNoteEditorActivity.handleCalendarExport):
/// kein Reminder, kein All-Day-Flag, TRANSP:OPAQUE für "busy" (Android: AVAILABILITY_BUSY).
pub fn generate_ics(title: &str, description: &str) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let end = now + 60 * 60 * 1000;

    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//SimpleNotes//Desktop//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:{uid}@simplenotes\r\n\
         DTSTAMP:{stamp}\r\n\
         DTSTART:{start}\r\n\
         DTEND:{end}\r\n\
         SUMMARY:{summary}\r\n\
         DESCRIPTION:{description}\r\n\
         TRANSP:OPAQUE\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR\r\n",
        uid = Uuid::new_v4(),
        stamp = timestamp_to_ics(now),
        start = timestamp_to_ics(now),
        end = timestamp_to_ics(end),
        summary = escape_ics_text(title),
        description = escape_ics_text(description),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_ics_text_special_chars() {
        assert_eq!(escape_ics_text("a,b;c\\d\ne"), "a\\,b\\;c\\\\d\\ne");
    }

    #[test]
    fn test_escape_ics_text_plain() {
        assert_eq!(escape_ics_text("Buy milk"), "Buy milk");
    }

    #[test]
    fn test_timestamp_to_ics_format() {
        let ics = timestamp_to_ics(1770202329000i64); // 2026-02-04 10:52:09 UTC
        assert_eq!(ics, "20260204T105209Z");
    }

    #[test]
    fn test_generate_ics_contains_required_fields() {
        let ics = generate_ics("Buy milk", "Line1\nLine2");
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert!(ics.contains("BEGIN:VEVENT\r\n"));
        assert!(ics.contains("END:VEVENT\r\n"));
        assert!(ics.contains("SUMMARY:Buy milk\r\n"));
        assert!(ics.contains("DESCRIPTION:Line1\\nLine2\r\n"));
        assert!(ics.contains("DTSTART:"));
        assert!(ics.contains("DTEND:"));
        assert!(ics.contains("UID:"));
        assert!(ics.contains("TRANSP:OPAQUE\r\n"));
    }

    #[test]
    fn test_generate_ics_end_is_one_hour_after_start() {
        let ics = generate_ics("T", "D");
        let start = ics
            .lines()
            .find(|l| l.starts_with("DTSTART:"))
            .unwrap()
            .trim_end_matches('\r')
            .trim_start_matches("DTSTART:");
        let end = ics
            .lines()
            .find(|l| l.starts_with("DTEND:"))
            .unwrap()
            .trim_end_matches('\r')
            .trim_start_matches("DTEND:");
        let start_dt = chrono::NaiveDateTime::parse_from_str(start, "%Y%m%dT%H%M%SZ").unwrap();
        let end_dt = chrono::NaiveDateTime::parse_from_str(end, "%Y%m%dT%H%M%SZ").unwrap();
        assert_eq!((end_dt - start_dt).num_seconds(), 3600);
    }
}
