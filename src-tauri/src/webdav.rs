use crate::error::{AppError, Result};
use crate::markdown;
use crate::models::Note;
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use reqwest::{Client, Method, StatusCode};
use std::sync::LazyLock;

/// UUID.json Pattern – compiled once at program start
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\.json",
    )
    .expect("UUID pattern is valid")
});

#[derive(Clone)]
/// WebDAV Client für Server-Kommunikation
pub struct WebDavClient {
    client: Client,
    base_url: String,
    auth_header: String,
}

impl WebDavClient {
    /// Erstellt einen neuen WebDAV Client
    pub fn new(url: &str, username: &str, password: &str) -> Result<Self> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        let auth = format!("{}:{}", username, password);
        let auth_header = format!("Basic {}", STANDARD.encode(auth));
        let base_url = url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            auth_header,
        })
    }

    /// Testet die Verbindung zum Server
    pub async fn test_connection(&self) -> Result<bool> {
        let url = format!("{}/notes/", self.base_url);

        let response = self
            .client
            .request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Authorization", &self.auth_header)
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match response.status() {
            StatusCode::OK | StatusCode::MULTI_STATUS => Ok(true),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AppError::InvalidCredentials),
            StatusCode::NOT_FOUND => {
                self.ensure_directories().await?;
                Ok(true)
            }
            status => Err(AppError::WebDav(format!(
                "Connection test failed: {}",
                status
            ))),
        }
    }

    /// Stellt sicher, dass /notes/ und /notes-md/ existieren
    pub async fn ensure_directories(&self) -> Result<()> {
        let notes_url = format!("{}/notes/", self.base_url);
        let _ = self
            .client
            .request(Method::from_bytes(b"MKCOL").unwrap(), &notes_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        let notes_md_url = format!("{}/notes-md/", self.base_url);
        let _ = self
            .client
            .request(Method::from_bytes(b"MKCOL").unwrap(), &notes_md_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        Ok(())
    }

    /// Listet alle JSON-Dateien in /notes/
    pub async fn list_json_files(&self) -> Result<Vec<String>> {
        let url = format!("{}/notes/", self.base_url);

        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname/>
    <d:getcontenttype/>
  </d:prop>
</d:propfind>"#;

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PROPFIND: {}", url);

        let response = self
            .client
            .request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Authorization", &self.auth_header)
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        if !response.status().is_success() && response.status() != StatusCode::MULTI_STATUS {
            return Err(AppError::WebDav(format!(
                "PROPFIND failed: {}",
                response.status()
            )));
        }

        let text = response
            .text()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PROPFIND response length: {} bytes", text.len());
        #[cfg(debug_assertions)]
        eprintln!(
            "[WebDAV] Response preview: {}",
            &text[..text.len().min(500)]
        );

        // Parse alle .json Dateien aus der WebDAV Response
        let mut ids: Vec<String> = Vec::new();

        // Decode URL-encoded content first
        let decoded_text = urlencoding::decode(&text).unwrap_or_else(|_| text.clone().into());

        // Suche nach UUID.json Pattern im gesamten Text
        for cap in UUID_PATTERN.captures_iter(&decoded_text) {
            if let Some(uuid_match) = cap.get(1) {
                let id = uuid_match.as_str().to_lowercase();
                if !ids.contains(&id) {
                    #[cfg(debug_assertions)]
                    eprintln!("[WebDAV] Found note ID: {}", id);
                    ids.push(id);
                }
            }
        }

        // Auch in URL-encoded Variante suchen
        for cap in UUID_PATTERN.captures_iter(&text) {
            if let Some(uuid_match) = cap.get(1) {
                let id = uuid_match.as_str().to_lowercase();
                if !ids.contains(&id) {
                    #[cfg(debug_assertions)]
                    eprintln!("[WebDAV] Found note ID (encoded): {}", id);
                    ids.push(id);
                }
            }
        }

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] Total found: {} note IDs", ids.len());

        Ok(ids)
    }

    /// Lädt eine einzelne Notiz
    pub async fn get_note(&self, id: &str) -> Result<Note> {
        let url = format!("{}/notes/{}.json", self.base_url, id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match response.status() {
            StatusCode::OK => {
                let mut note: Note = response
                    .json()
                    .await
                    .map_err(|e| AppError::ParseError(e.to_string()))?;

                // Fix noteType basierend auf checklistItems (für alte Notizen ohne noteType-Feld)
                note.fix_note_type();

                Ok(note)
            }
            StatusCode::NOT_FOUND => Err(AppError::NoteNotFound(id.to_string())),
            status => Err(AppError::WebDav(format!("GET failed: {}", status))),
        }
    }

    /// Speichert eine Notiz (Dual-Write: JSON + Markdown)
    pub async fn save_note(&self, note: &Note) -> Result<()> {
        self.save_json(note).await?;
        self.save_markdown(note).await?;
        Ok(())
    }

    async fn save_json(&self, note: &Note) -> Result<()> {
        let url = format!("{}/notes/{}.json", self.base_url, note.id);

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PUT JSON: {}", url);

        let json_content =
            serde_json::to_string_pretty(note).map_err(|e| AppError::ParseError(e.to_string()))?;

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] JSON content length: {} bytes", json_content.len());

        let response = self
            .client
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .body(json_content)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        let status = response.status();
        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PUT JSON response status: {}", status);

        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            #[cfg(debug_assertions)]
            eprintln!("[WebDAV] PUT JSON error body: {}", error_body);
            return Err(AppError::WebDav(format!(
                "PUT JSON failed: {} - {}",
                status, error_body
            )));
        }

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PUT JSON successful");
        Ok(())
    }

    async fn save_markdown(&self, note: &Note) -> Result<()> {
        let markdown_content = markdown::generate_markdown(note);
        let safe_title = sanitize_filename(&note.title);
        let url = format!("{}/notes-md/{}.md", self.base_url, safe_title);

        let response = self
            .client
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "text/markdown; charset=utf-8")
            .body(markdown_content)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            eprintln!(
                "Warning: PUT Markdown failed: {} for note {}",
                response.status(),
                note.id
            );
        }

        Ok(())
    }

    /// Löscht eine Notiz (beide Dateien)
    pub async fn delete_note(&self, note: &Note) -> Result<()> {
        let json_url = format!("{}/notes/{}.json", self.base_url, note.id);
        let _ = self
            .client
            .delete(&json_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        let safe_title = sanitize_filename(&note.title);
        let md_url = format!("{}/notes-md/{}.md", self.base_url, safe_title);
        let _ = self
            .client
            .delete(&md_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        Ok(())
    }
}

fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Normal Title"), "Normal Title");
        assert_eq!(sanitize_filename("With/Slash"), "With_Slash");
        assert_eq!(sanitize_filename("Test:Colon"), "Test_Colon");
        assert_eq!(sanitize_filename("Multi<>Special"), "Multi__Special");
        assert_eq!(sanitize_filename("  Trimmed  "), "Trimmed");
    }
}
