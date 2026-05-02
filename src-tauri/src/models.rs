use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// Konflikt-Auswahl-Optionen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictResolution {
    /// Lokale Version behalten
    KeepLocal,
    /// Server-Version behalten
    KeepRemote,
    /// Beide Versionen behalten (als separate Notizen)
    KeepBoth,
}

/// Konflikt-Informationen für die Frontend-Anzeige
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    /// Notiz-ID
    pub note_id: String,
    /// Lokale Version der Notiz
    pub local_note: Note,
    /// Server-Version der Notiz
    pub remote_note: Note,
    /// Letzter bekannter gemeinsamer Zustand (Hash)
    pub base_hash: Option<String>,
    /// Lokaler Änderungszeitpunkt
    pub local_modified_at: i64,
    /// Remote-Änderungszeitpunkt
    pub remote_modified_at: i64,
    /// Beschreibung der Unterschiede (für Anzeige)
    pub diff_summary: String,
}

impl ConflictInfo {
    /// Erstellt eine Zusammenfassung der Unterschiede zwischen lokaler und Remote-Version
    pub fn compute_diff_summary(local: &Note, remote: &Note) -> String {
        let mut diffs = Vec::new();

        if local.title != remote.title {
            diffs.push("Titel geändert".to_string());
        }

        if local.content != remote.content {
            diffs.push("Inhalt geändert".to_string());
        }

        if local.note_type != remote.note_type {
            diffs.push("Notiztyp geändert".to_string());
        }

        match (&local.checklist_items, &remote.checklist_items) {
            (Some(local_items), Some(remote_items)) => {
                if local_items.len() != remote_items.len() {
                    diffs.push(format!(
                        "Checklisten-Einträge: {} vs {}",
                        local_items.len(),
                        remote_items.len()
                    ));
                } else {
                    let mut has_changes = false;
                    for (l, r) in local_items.iter().zip(remote_items.iter()) {
                        if l.text != r.text || l.is_checked != r.is_checked || l.order != r.order {
                            has_changes = true;
                            break;
                        }
                    }
                    if has_changes {
                        diffs.push("Checklisten-Einträge geändert".to_string());
                    }
                }
            }
            (Some(_), None) => diffs.push("Checkliste entfernt".to_string()),
            (None, Some(_)) => diffs.push("Checkliste hinzugefügt".to_string()),
        }

        if diffs.is_empty() {
            "Keine erkennbaren Unterschiede".to_string()
        } else {
            diffs.join(", ")
        }
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl ChecklistItem {
    /// Erstellt ein neues Checklist-Item
    #[allow(dead_code)]
    pub fn new(text: String, order: i32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            text,
            is_checked: false,
            order,
        }
    }
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

    /// Letzter bekannter Server-Inhalts-Hash (für Konflikt-Erkennung)
    /// Wird nach jedem erfolgreichen Sync aktualisiert
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_hash: Option<String>,

    /// Letzter Sync-Zeitpunkt (Unix ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
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
            last_synced_hash: None,
            last_synced_at: None,
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

    /// Aktualisiert den Timestamp auf jetzt
    #[allow(dead_code)]
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Berechnet einen Hash über den gesamten Inhalt der Notiz (für Konflikt-Erkennung)
    /// Der Hash basiert auf: Titel, Inhalt, Checklist-Items, Checklist-Sort-Option
    pub fn compute_content_hash(&self) -> String {
        let mut hasher = Sha256::new();

        hasher.update(self.title.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.content.as_bytes());
        hasher.update(b"\0");

        if let Some(items) = &self.checklist_items {
            for item in items {
                hasher.update(item.id.as_bytes());
                hasher.update(b"\0");
                hasher.update(item.text.as_bytes());
                hasher.update(b"\0");
                hasher.update(if item.is_checked { b"1" } else { b"0" });
                hasher.update(b"\0");
                hasher.update(item.order.to_le_bytes());
                hasher.update(b"\0");
            }
        }

        if let Some(sort_option) = &self.checklist_sort_option {
            hasher.update(sort_option.as_bytes());
        }

        let result = hasher.finalize();
        format!("{:x}", result)
    }

    /// Prüft, ob der aktuelle Inhalt mit dem letzten synchronisierten Hash übereinstimmt
    pub fn content_matches_last_sync(&self) -> bool {
        self.last_synced_hash
            .as_ref()
            .map(|last_hash| last_hash == &self.compute_content_hash())
            .unwrap_or(true)
    }

    /// Aktualisiert die Sync-Metadaten nach einem erfolgreichen Sync
    pub fn update_sync_metadata(&mut self) {
        self.last_synced_hash = Some(self.compute_content_hash());
        self.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
        self.sync_status = SyncStatus::Synced;
    }

    /// Markiert die Notiz als im Konflikt stehend
    pub fn mark_as_conflict(&mut self) {
        self.sync_status = SyncStatus::Conflict;
    }

    /// Generiert Fallback-Content aus Checklist-Items
    #[allow(dead_code)]
    pub fn generate_checklist_fallback(&self) -> String {
        match &self.checklist_items {
            Some(items) => {
                let mut sorted = items.clone();
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
            None => String::new(),
        }
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
    #[serde(default)]
    pub sync_status: SyncStatus,
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
            sync_status: note.sync_status,
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
            },
            ChecklistItem {
                id: "2".to_string(),
                text: "Item B".to_string(),
                is_checked: true,
                order: 1,
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
}
