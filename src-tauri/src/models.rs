use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sync-Status einer Notiz
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncStatus {
    /// Erfolgreich synchronisiert
    #[default]
    Synced,
    /// Lokale Änderungen warten auf Sync
    Pending,
    /// Noch nie synchronisiert
    LocalOnly,
    /// Konflikt erkannt
    Conflict,
}

/// Typ der Notiz
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoteType {
    /// Normale Text-Notiz
    #[default]
    Text,
    /// Checkliste
    Checklist,
}

/// Ein Item in einer Checkliste
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    /// UUID v4 für das Item
    pub id: String,
    /// Text des Items
    pub text: String,
    /// Ob das Item abgehakt ist
    pub is_checked: bool,
    /// Sortierreihenfolge (0-basiert)
    pub order: i32,
    /// Android v1.9.0 (F04): Ursprüngliche Sort-Position für MANUAL-Modus.
    /// Wird gemeinsam mit `order` bei jedem strukturellen Eingriff (Add, Delete,
    /// Drag-Drop) zementiert — identisches Verhalten zur Android-App.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_order: Option<i32>,
    /// Cross-App-Kompatibilität: alle Item-Felder, die die Desktop-App (noch)
    /// nicht modelliert — Android schreibt z.B. `createdAt`, `indentationLevel`,
    /// und künftige Felder kommen dazu. Werden 1:1 erhalten,
    /// damit ein Speichern auf dem Desktop sie nicht aus dem Server-JSON entfernt.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}


/// Eine Notiz
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// UUID v4, auch Dateiname
    pub id: String,

    /// Titel der Notiz
    pub title: String,

    /// Inhalt (Text oder Checklist-Fallback)
    pub content: String,

    /// Erstellungszeitpunkt (Unix ms)
    pub created_at: i64,

    /// Letzte Änderung (Unix ms)
    /// KRITISCH für Sync!
    pub updated_at: i64,

    /// Geräte-ID
    pub device_id: String,

    /// Sync-Status
    #[serde(default)]
    pub sync_status: SyncStatus,

    /// Typ der Notiz
    #[serde(default)]
    pub note_type: NoteType,

    /// Checklist-Items (nur für CHECKLIST)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklist_items: Option<Vec<ChecklistItem>>,

    /// Checklist Sort-Option (nur für CHECKLIST)
    /// Werte: "MANUAL", "ALPHABETICAL_ASC", "ALPHABETICAL_DESC", "UNCHECKED_FIRST", "CHECKED_FIRST"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklist_sort_option: Option<String>,

    /// Android v2.5.0: Hintergrundfarbe der Notiz, Hex `#RRGGBB`. None = Standardfarbe.
    /// Desktop erhält den Wert bereits, rendert ihn aber noch NICHT (geplant, siehe Plan v0.5.0+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Android v2.5.0: Labels/Tags der Notiz (serverseitig zusätzlich in notes_labels.json aggregiert).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,

    /// Android v2.5.0: Google-Keep-Import-Marker (Epoch ms). None = nicht aus Keep importiert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<i64>,

    /// Android v2.5.0: Pin-Status. None = nicht angepinnt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_pinned: Option<bool>,

    /// Cross-App-Kompatibilität: Auffangbecken für alle weiteren Notiz-Felder, die
    /// die Desktop-App nicht modelliert (inkl. künftiger Android-Schema-Erweiterungen).
    /// Werden beim Round-Trip 1:1 erhalten — der Kern des Datenverlust-Fix.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Note {
    /// Erstellt eine neue leere Notiz
    pub fn new(title: String, device_id: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            content: String::new(),
            created_at: now,
            updated_at: now,
            device_id,
            sync_status: SyncStatus::Synced,
            note_type: NoteType::Text,
            checklist_items: None,
            checklist_sort_option: None,
            color: None,
            labels: None,
            imported_at: None,
            is_pinned: None,
            extra: serde_json::Map::new(),
        }
    }

    /// Erstellt eine neue Checklisten-Notiz
    pub fn new_checklist(title: String, device_id: String) -> Self {
        let mut note = Self::new(title, device_id);
        note.note_type = NoteType::Checklist;
        note.checklist_items = Some(Vec::new());
        note.checklist_sort_option = Some("UNCHECKED_FIRST".to_string());
        note
    }

    /// Korrigiert den noteType basierend auf vorhandenen checklistItems
    /// Wird nach dem Deserialisieren aufgerufen um alte Notizen ohne noteType-Feld zu fixen
    pub fn fix_note_type(&mut self) {
        // Wenn checklistItems vorhanden sind, muss es eine CHECKLIST sein
        if let Some(items) = &self.checklist_items {
            if !items.is_empty() {
                self.note_type = NoteType::Checklist;
                return;
            }
        }
        // Wenn noteType CHECKLIST ist, aber keine Items vorhanden → initialisiere leere Liste
        if self.note_type == NoteType::Checklist && self.checklist_items.is_none() {
            self.checklist_items = Some(Vec::new());
        }
    }
}

/// Metadaten für die Notizen-Liste (mit Preview)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMetadata {
    pub id: String,
    pub title: String,
    pub content: String,
    pub updated_at: i64,
    pub note_type: NoteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklist_items: Option<Vec<ChecklistItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checklist_sort_option: Option<String>,
}

impl From<&Note> for NoteMetadata {
    fn from(note: &Note) -> Self {
        Self {
            id: note.id.clone(),
            title: note.title.clone(),
            content: note.content.clone(),
            updated_at: note.updated_at,
            note_type: note.note_type,
            checklist_items: note.checklist_items.clone(),
            checklist_sort_option: note.checklist_sort_option.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_creation() {
        let note = Note::new("Test Title".to_string(), "tauri-abc123".to_string());

        assert!(!note.id.is_empty());
        assert_eq!(note.title, "Test Title");
        assert_eq!(note.device_id, "tauri-abc123");
        assert_eq!(note.note_type, NoteType::Text);
        assert!(note.created_at > 0);
        assert_eq!(note.created_at, note.updated_at);
    }

    #[test]
    fn test_checklist_note_creation() {
        let note = Note::new_checklist("Shopping".to_string(), "tauri-abc123".to_string());

        assert_eq!(note.note_type, NoteType::Checklist);
        assert!(note.checklist_items.is_some());
        assert!(note.checklist_items.unwrap().is_empty());
    }

    #[test]
    fn test_checklist_item_creation() {
        let item = ChecklistItem::new("Buy milk".to_string(), 0);

        assert!(!item.id.is_empty());
        assert_eq!(item.text, "Buy milk");
        assert!(!item.is_checked);
        assert_eq!(item.order, 0);
    }

    #[test]
    fn test_note_touch_updates_timestamp() {
        let mut note = Note::new("Test".to_string(), "tauri-abc".to_string());
        let original = note.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        note.touch();

        assert!(note.updated_at > original);
    }

    #[test]
    fn test_generate_checklist_fallback() {
        let mut note = Note::new_checklist("List".to_string(), "tauri-abc".to_string());
        note.checklist_items = Some(vec![
            ChecklistItem {
                id: "1".to_string(),
                text: "Item A".to_string(),
                is_checked: false,
                order: 0,
                ..Default::default()
            },
            ChecklistItem {
                id: "2".to_string(),
                text: "Item B".to_string(),
                is_checked: true,
                order: 1,
                ..Default::default()
            },
        ]);

        let fallback = note.generate_checklist_fallback();

        assert!(fallback.contains("[ ] Item A"));
        assert!(fallback.contains("[x] Item B"));
    }

    #[test]
    fn test_note_metadata_from_note() {
        let note = Note::new("My Note".to_string(), "tauri-abc".to_string());
        let meta: NoteMetadata = (&note).into();

        assert_eq!(meta.id, note.id);
        assert_eq!(meta.title, note.title);
        assert_eq!(meta.updated_at, note.updated_at);
        assert_eq!(meta.note_type, note.note_type);
    }

    // ── Cross-App-Kompatibilität: Felder-Erhaltung beim JSON-Round-Trip ──────────
    // Regression-Schutz für den Datenverlust-Bug: Speichern auf dem Desktop darf
    // KEINE Felder entfernen, die die Android-App schreibt (color, labels, …) und
    // auch keine künftigen, der Desktop-App unbekannten Felder.

    #[test]
    fn test_note_preserves_v250_and_unknown_fields() {
        // Notiz wie von der Android-App (v2.5.0) geschrieben + ein hypothetisches Zukunftsfeld.
        let json = r##"{
            "id": "abc",
            "title": "T",
            "content": "C",
            "createdAt": 1700000000000,
            "updatedAt": 1700000000001,
            "deviceId": "android-xyz",
            "syncStatus": "SYNCED",
            "noteType": "TEXT",
            "color": "#FF5733",
            "labels": ["work", "urgent"],
            "importedAt": 1699999999999,
            "isPinned": true,
            "futureField": {"nested": 42}
        }"##;

        let note: Note = serde_json::from_str(json).unwrap();
        assert_eq!(note.color.as_deref(), Some("#FF5733"));
        assert_eq!(
            note.labels.as_deref(),
            Some(&["work".to_string(), "urgent".to_string()][..])
        );
        assert_eq!(note.imported_at, Some(1699999999999));
        assert_eq!(note.is_pinned, Some(true));
        assert!(
            note.extra.contains_key("futureField"),
            "unknown field must land in extra"
        );

        // Re-Serialisierung darf nichts davon verlieren (der eigentliche Bug).
        let out = serde_json::to_value(&note).unwrap();
        assert_eq!(out["color"], "#FF5733");
        assert_eq!(out["labels"][0], "work");
        assert_eq!(out["labels"][1], "urgent");
        assert_eq!(out["importedAt"], 1699999999999i64);
        assert_eq!(out["isPinned"], true);
        assert_eq!(out["futureField"]["nested"], 42);
    }

    #[test]
    fn test_note_without_extra_fields_stays_clean() {
        // Eine frische Notiz darf weder null-Felder noch einen "extra"-Key serialisieren.
        let note = Note::new("T".to_string(), "tauri-x".to_string());
        let out = serde_json::to_value(&note).unwrap();
        let obj = out.as_object().unwrap();
        assert!(!obj.contains_key("color"));
        assert!(!obj.contains_key("labels"));
        assert!(!obj.contains_key("importedAt"));
        assert!(!obj.contains_key("isPinned"));
        // flatten darf NIE einen wörtlichen "extra"-Key erzeugen.
        assert!(!obj.contains_key("extra"));
    }

    #[test]
    fn test_checklist_item_preserves_android_fields() {
        // Android-ChecklistItem-Felder: originalOrder (v1.9.0 — jetzt benanntes Feld),
        // createdAt (v1.11.0), indentationLevel (v2.5.0 — weiterhin in extra).
        let json = r#"{
            "id": "i1",
            "text": "milk",
            "isChecked": false,
            "order": 0,
            "originalOrder": 3,
            "createdAt": 1700000000000,
            "indentationLevel": 2
        }"#;

        let item: ChecklistItem = serde_json::from_str(json).unwrap();

        // originalOrder landet jetzt im benannten Feld, nicht mehr in extra.
        assert_eq!(item.original_order, Some(3));
        assert!(
            !item.extra.contains_key("originalOrder"),
            "originalOrder darf nicht doppelt in extra stehen"
        );

        // Übrige Android-Felder bleiben in extra.
        assert_eq!(
            item.extra.get("createdAt").and_then(|v| v.as_i64()),
            Some(1700000000000)
        );
        assert_eq!(
            item.extra.get("indentationLevel").and_then(|v| v.as_i64()),
            Some(2)
        );

        // Re-Serialisierung: originalOrder erscheint genau einmal im Output.
        let out = serde_json::to_value(&item).unwrap();
        assert_eq!(out["originalOrder"], 3);
        assert_eq!(out["createdAt"], 1700000000000i64);
        assert_eq!(out["indentationLevel"], 2);

        // Kein wörtlicher "extra"-Key im Output.
        assert!(!out.as_object().unwrap().contains_key("extra"));
    }

    #[test]
    fn test_checklist_item_original_order_absent_when_none() {
        // Neu erstelltes Item (kein originalOrder) → Feld fehlt im JSON-Output.
        let item = ChecklistItem::new("task".to_string(), 2);
        assert_eq!(item.original_order, None);
        let out = serde_json::to_value(&item).unwrap();
        assert!(!out.as_object().unwrap().contains_key("originalOrder"));
    }
}
