use thiserror::Error;

/// Alle möglichen App-Fehler
#[derive(Debug, Error)]
pub enum AppError {
    /// WebDAV-Operation fehlgeschlagen
    #[error("WebDAV error: {0}")]
    WebDav(String),

    /// Keine Verbindung zum Server
    #[error("Not connected to server")]
    NotConnected,

    /// Notiz nicht gefunden
    #[error("Note not found: {0}")]
    NoteNotFound(String),

    /// Parsing-Fehler (JSON, YAML, etc.)
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Storage-Fehler (Credentials, Settings)
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Ungültige Credentials
    #[error("Invalid credentials")]
    InvalidCredentials,

    /// Netzwerk-Fehler
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Ungültiges Timestamp-Format
    #[allow(dead_code)]
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
}

/// Serialisierung für Tauri (Fehler als String)
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Result-Alias für einfacheres Error Handling
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::NotConnected;
        assert_eq!(err.to_string(), "Not connected to server");
    }

    #[test]
    fn test_error_webdav() {
        let err = AppError::WebDav("404 Not Found".to_string());
        assert!(err.to_string().contains("404"));
    }

    #[test]
    fn test_error_serialize() {
        let err = AppError::NoteNotFound("abc123".to_string());
        let json = serde_json::to_string(&err).unwrap();

        assert!(json.contains("Note not found"));
        assert!(json.contains("abc123"));
    }
}
