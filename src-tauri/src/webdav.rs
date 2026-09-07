use crate::error::{AppError, Result};
use crate::folders::{parse_folders_json, sanitize_folder_name, FolderMeta};
use crate::markdown;
use crate::models::{DeletionLedger, DeletionRecord, Note};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use reqwest::{Client, Method, StatusCode};
use std::collections::HashMap;
use std::sync::LazyLock;

/// UUID.json Pattern – compiled once at program start
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\.json",
    )
    .expect("UUID pattern is valid")
});

/// Regex zum Extrahieren von WebDAV `<d:href>` (Namespace-Prefix-Varianten: d/D)
static HREF_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<[Dd]:[Hh][Rr][Ee][Ff]>([^<]+)</[Dd]:[Hh][Rr][Ee][Ff]>")
        .expect("HREF pattern is valid")
});

/// Ein einzelner `<d:response>`-Block einer PROPFIND-Antwort (für die Href+mtime-Paarung
/// beim Asset-Listing — das reine HREF_PATTERN liefert Namen ohne mtime).
static RESPONSE_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[Dd]:response>(.*?)</[Dd]:response>")
        .expect("response block pattern is valid")
});

/// Regex zum Extrahieren von `<d:getlastmodified>` innerhalb eines Response-Blocks (RFC1123).
static LAST_MODIFIED_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[Dd]:getlastmodified>([^<]+)</[Dd]:getlastmodified>")
        .expect("getlastmodified pattern is valid")
});

/// Regex zum Extrahieren von `<d:getetag>` innerhalb eines Response-Blocks.
/// Toleranter im Namespace-Prefix als die anderen Muster: `getetag` ist eine Live-Property,
/// Apache/mod_dav gibt sie als `<lp1:getetag>` aus, Nextcloud als `<d:getetag>`.
static ETAG_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<[A-Za-z0-9]*:?getetag>([^<]+)</[A-Za-z0-9]*:?getetag>")
        .expect("getetag pattern is valid")
});

/// PROPFIND and MKCOL are not in reqwest's built-in Method constants — define them once here
/// rather than calling from_bytes().unwrap() at every call site.
static PROPFIND: LazyLock<Method> =
    LazyLock::new(|| Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid HTTP method"));
static MKCOL: LazyLock<Method> =
    LazyLock::new(|| Method::from_bytes(b"MKCOL").expect("MKCOL is a valid HTTP method"));

#[derive(Clone)]
/// WebDAV Client für Server-Kommunikation
pub struct WebDavClient {
    client: Client,
    base_url: String,
    auth_header: String,
    /// Sync folder name (default: "notes"). JSON stored in `/{sync_folder}/`, Markdown in `/{sync_folder}-md/`.
    sync_folder: String,
}

impl WebDavClient {
    /// Erstellt einen neuen WebDAV Client
    pub fn new(url: &str, username: &str, password: &str, sync_folder: &str) -> Result<Self> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            // Ohne User-Agent blockieren viele WAFs (z.B. Cloudflare-Regel
            // `http.user_agent eq ""`) den Request am Edge mit 403.
            .user_agent(concat!("SimpleNotesDesktop/", env!("CARGO_PKG_VERSION")))
            // connect_timeout: schnelles Fehlschlagen wenn der Server nicht erreichbar ist
            // (sonst hängt "Test connection" bis zum 30s-Request-Timeout).
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        let auth = format!("{}:{}", username, password);
        let auth_header = format!("Basic {}", STANDARD.encode(auth));
        let base_url = url.trim_end_matches('/').to_string();

        // Sanitize sync folder: only allow ASCII alphanumeric, underscore, dash (Android parity).
        // Must use is_ascii_alphanumeric() — is_alphanumeric() accepts Unicode letters which
        // would produce a different path than the JS frontend's /[^a-zA-Z0-9_-]/g regex.
        let sanitized_folder = sync_folder
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();
        let sync_folder = if sanitized_folder.is_empty() {
            "notes".to_string()
        } else {
            sanitized_folder.chars().take(50).collect()
        };

        Ok(Self {
            client,
            base_url,
            auth_header,
            sync_folder,
        })
    }

    // ── URL-Builder ─────────────────────────────────────────────────────────────

    /// JSON-URL einer Notiz: `{base}/{sync_folder}/{enc(folder)/}{id}.json`
    fn note_json_url(&self, folder: Option<&str>, id: &str) -> String {
        match folder {
            Some(f) => format!(
                "{}/{}/{}/{}.json",
                self.base_url,
                self.sync_folder,
                urlencoding::encode(f),
                id
            ),
            None => format!("{}/{}/{}.json", self.base_url, self.sync_folder, id),
        }
    }

    /// Markdown-URL einer Notiz: `{base}/{sync_folder}-md/{enc(folder)/}{title}.md`
    fn note_md_url(&self, folder: Option<&str>, safe_title: &str) -> String {
        match folder {
            Some(f) => format!(
                "{}/{}-md/{}/{}.md",
                self.base_url,
                self.sync_folder,
                urlencoding::encode(f),
                safe_title
            ),
            None => format!(
                "{}/{}-md/{}.md",
                self.base_url, self.sync_folder, safe_title
            ),
        }
    }

    /// URL zum JSON-Unterverzeichnis eines Ordners: `{base}/{sync_folder}/{enc(folder)}/`
    fn folder_json_dir_url(&self, folder: &str) -> String {
        format!(
            "{}/{}/{}/",
            self.base_url,
            self.sync_folder,
            urlencoding::encode(folder)
        )
    }

    /// URL zum Markdown-Unterverzeichnis eines Ordners: `{base}/{sync_folder}-md/{enc(folder)}/`
    fn folder_md_dir_url(&self, folder: &str) -> String {
        format!(
            "{}/{}-md/{}/",
            self.base_url,
            self.sync_folder,
            urlencoding::encode(folder)
        )
    }

    /// URL zur zentralen `folders.json`: `{base}/{sync_folder}/folders.json`
    fn folders_file_url(&self) -> String {
        format!("{}/{}/folders.json", self.base_url, self.sync_folder)
    }

    /// URL zum gemeinsamen Lösch-Ledger: `{base}/{sync_folder}/deletions.json`
    fn deletions_file_url(&self) -> String {
        format!("{}/{}/deletions.json", self.base_url, self.sync_folder)
    }

    /// URL zum Bild-Anhang-Verzeichnis: `{base}/{sync_folder}-assets/` — bewusst NICHT
    /// unter `{sync_folder}/`, da `extract_subdirs_from_propfind` jedes Unterverzeichnis
    /// des Notiz-Baums als Notiz-Ordner interpretiert (Alt-Client-Kompatibilität).
    fn assets_dir_url(&self) -> String {
        format!("{}/{}-assets/", self.base_url, self.sync_folder)
    }

    /// URL eines einzelnen Assets: `{base}/{sync_folder}-assets/{enc(name)}`
    fn asset_url(&self, name: &str) -> String {
        format!(
            "{}/{}-assets/{}",
            self.base_url,
            self.sync_folder,
            urlencoding::encode(name)
        )
    }

    // ── MKCOL-Helfer ────────────────────────────────────────────────────────────

    /// Erstellt das JSON-Unterverzeichnis und, falls `write_markdown`, das MD-Unterverzeichnis
    /// eines Ordners. Fehler (z.B. 405 Method Not Allowed wenn das Verzeichnis bereits existiert)
    /// werden ignoriert.
    pub async fn ensure_folder_dirs(&self, folder: &str, write_markdown: bool) {
        let _ = self
            .client
            .request(MKCOL.clone(), self.folder_json_dir_url(folder))
            .header("Authorization", &self.auth_header)
            .send()
            .await;
        if write_markdown {
            let _ = self
                .client
                .request(MKCOL.clone(), self.folder_md_dir_url(folder))
                .header("Authorization", &self.auth_header)
                .send()
                .await;
        }
    }

    /// Löscht das JSON-Unterverzeichnis und das MD-Unterverzeichnis eines Ordners.
    /// Fehler werden ignoriert (404 = bereits gelöscht, 409 = nicht leer, etc.).
    pub async fn delete_folder_dirs(&self, folder: &str) {
        let _ = self
            .client
            .delete(self.folder_json_dir_url(folder))
            .header("Authorization", &self.auth_header)
            .send()
            .await;
        let _ = self
            .client
            .delete(self.folder_md_dir_url(folder))
            .header("Authorization", &self.auth_header)
            .send()
            .await;
    }

    // ── Verbindungstest & Verzeichnisse ─────────────────────────────────────────

    /// Testet die Verbindung zum Server. `write_markdown` steuert, ob beim Anlegen fehlender
    /// Verzeichnisse (404-Fall) auch `{sync_folder}-md/` erstellt wird.
    pub async fn test_connection(&self, write_markdown: bool) -> Result<bool> {
        let url = format!("{}/{}/", self.base_url, self.sync_folder);

        let response = self
            .client
            .request(PROPFIND.clone(), &url)
            .header("Authorization", &self.auth_header)
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match response.status() {
            StatusCode::OK | StatusCode::MULTI_STATUS => Ok(true),
            StatusCode::UNAUTHORIZED => Err(AppError::InvalidCredentials),
            // 403 kommt meist nicht vom WebDAV-Server selbst, sondern von einem Proxy/WAF
            // davor — nicht als "Invalid credentials" ausgeben.
            StatusCode::FORBIDDEN => Err(AppError::WebDav(
                "403 Forbidden — server or a proxy/firewall in front of it rejected the request"
                    .to_string(),
            )),
            StatusCode::NOT_FOUND => {
                self.ensure_directories(write_markdown).await?;
                Ok(true)
            }
            status => Err(AppError::WebDav(format!(
                "Connection test failed: {}",
                status
            ))),
        }
    }

    /// Stellt sicher, dass `/{sync_folder}/` existiert; `/{sync_folder}-md/` nur wenn
    /// `write_markdown` (der Markdown-Spiegel ist ein opt-in Export, default aus).
    pub async fn ensure_directories(&self, write_markdown: bool) -> Result<()> {
        let notes_url = format!("{}/{}/", self.base_url, self.sync_folder);
        let _ = self
            .client
            .request(MKCOL.clone(), &notes_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        if write_markdown {
            let notes_md_url = format!("{}/{}-md/", self.base_url, self.sync_folder);
            let _ = self
                .client
                .request(MKCOL.clone(), &notes_md_url)
                .header("Authorization", &self.auth_header)
                .send()
                .await;
        }

        // Asset-Verzeichnis ist kein Opt-in (anders als der Markdown-Spiegel) — Bilder
        // sollen ohne zusätzliches Setting funktionieren.
        let _ = self
            .client
            .request(MKCOL.clone(), self.assets_dir_url())
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        Ok(())
    }

    /// Prüft, ob der Markdown-Spiegelordner `{sync_folder}-md/` bereits auf dem Server existiert.
    /// `200`/`207` → true, `404` → false. Nur informativ (für die Verbindungstest-Meldung).
    pub async fn md_mirror_exists(&self) -> Result<bool> {
        let url = format!("{}/{}-md/", self.base_url, self.sync_folder);
        let response = self
            .client
            .request(PROPFIND.clone(), &url)
            .header("Authorization", &self.auth_header)
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;
        match response.status() {
            StatusCode::OK | StatusCode::MULTI_STATUS => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(AppError::InvalidCredentials),
            status => Err(AppError::WebDav(format!(
                "Markdown mirror check failed: {}",
                status
            ))),
        }
    }

    // ── Notiz-Listing ────────────────────────────────────────────────────────────

    /// Listet alle Notizen mit ihrer Ordner-Zuordnung.
    /// Gibt `(id, folder_name)`, die Server-ETags je Notiz-ID sowie ein `complete`-Flag zurück —
    /// `false`, wenn mindestens ein Unterordner-PROPFIND fehlschlug (das Listing also lückenhaft
    /// ist). Aufrufer dürfen in dem Fall keine Löscherkennung auf Basis der zurückgegebenen IDs
    /// durchführen.
    ///
    /// Die ETags werden aus denselben Antworten geerntet (kein zusätzlicher Request) und
    /// getrennt von der ID-Liste geführt: ein Server ohne `getetag` liefert weiter ein
    /// vollständiges Listing, nur eben ohne Konfliktschutz.
    #[allow(clippy::type_complexity)]
    pub async fn list_notes_with_folders(
        &self,
    ) -> Result<(Vec<(String, Option<String>)>, HashMap<String, String>, bool)> {
        let root_url = format!("{}/{}/", self.base_url, self.sync_folder);
        let text = self.propfind_text(&root_url, "1").await?;

        let mut result: Vec<(String, Option<String>)> = Vec::new();
        let mut etags = parse_note_etags(&text);

        // Schritt 1: Root-Notizen aus dem Depth-1 PROPFIND (nur direkte Kinder)
        let decoded_text = urlencoding::decode(&text).unwrap_or_else(|_| text.clone().into());
        for cap in UUID_PATTERN.captures_iter(&decoded_text) {
            let id = cap[1].to_lowercase();
            if !result.iter().any(|(i, _)| i == &id) {
                result.push((id, None));
            }
        }
        for cap in UUID_PATTERN.captures_iter(&text) {
            let id = cap[1].to_lowercase();
            if !result.iter().any(|(i, _)| i == &id) {
                result.push((id, None));
            }
        }

        // Schritt 2: Unterordner aus href-Werten extrahieren
        let subdirs = self.extract_subdirs_from_propfind(&text);

        let mut complete = true;
        for folder_name in subdirs {
            let subdir_url = self.folder_json_dir_url(&folder_name);
            match self.propfind_text(&subdir_url, "1").await {
                Ok(sub_text) => {
                    etags.extend(parse_note_etags(&sub_text));
                    let sub_decoded =
                        urlencoding::decode(&sub_text).unwrap_or_else(|_| sub_text.clone().into());
                    for cap in UUID_PATTERN.captures_iter(&sub_decoded) {
                        let id = cap[1].to_lowercase();
                        if !result.iter().any(|(i, _)| i == &id) {
                            result.push((id, Some(folder_name.clone())));
                        }
                    }
                    for cap in UUID_PATTERN.captures_iter(&sub_text) {
                        let id = cap[1].to_lowercase();
                        if !result.iter().any(|(i, _)| i == &id) {
                            result.push((id, Some(folder_name.clone())));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[WebDAV] PROPFIND subdir {} failed: {}", folder_name, e);
                    complete = false;
                }
            }
        }

        Ok((result, etags, complete))
    }

    /// Extrahiert direkte Unterordner-Namen aus einer PROPFIND-Antwort auf das Root-Verzeichnis.
    fn extract_subdirs_from_propfind(&self, text: &str) -> Vec<String> {
        let mut subdirs: Vec<String> = Vec::new();

        for cap in HREF_PATTERN.captures_iter(text) {
            let href = cap[1].trim();
            if !href.ends_with('/') {
                continue;
            }

            // URL-decode und letztes Pfadsegment ermitteln
            let decoded = urlencoding::decode(href.trim_end_matches('/'))
                .unwrap_or_else(|_| href.trim_end_matches('/').into())
                .into_owned();

            let last_seg = match decoded.rsplit('/').next() {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };

            // Root-Ordner selbst und sync_folder überspringen
            if last_seg == self.sync_folder {
                continue;
            }

            // Validieren und bereinigen
            if let Some(folder_name) = sanitize_folder_name(last_seg) {
                if !subdirs.contains(&folder_name) {
                    subdirs.push(folder_name);
                }
            }
        }

        subdirs
    }

    /// Gibt alle Ordner-Namen zurück, die als physische Verzeichnisse auf dem Server existieren.
    #[allow(dead_code)]
    pub async fn discover_folders(&self) -> Vec<String> {
        let root_url = format!("{}/{}/", self.base_url, self.sync_folder);
        match self.propfind_text(&root_url, "1").await {
            Ok(text) => self.extract_subdirs_from_propfind(&text),
            Err(_) => Vec::new(),
        }
    }

    // ── Einzel-Notiz ────────────────────────────────────────────────────────────

    /// Lädt eine einzelne Notiz aus dem angegebenen Ordner.
    /// `folder` = None → Root-Ebene; der path ist maßgebend für `note.folder_name`.
    pub async fn get_note(&self, id: &str, folder: Option<&str>) -> Result<Note> {
        let url = self.note_json_url(folder, id);

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

                // Pfad ist maßgebend — überschreibt was im JSON-Body steht
                note.folder_name = folder.map(str::to_owned);

                Ok(note)
            }
            StatusCode::NOT_FOUND => Err(AppError::NoteNotFound(id.to_string())),
            status => Err(AppError::WebDav(format!("GET failed: {}", status))),
        }
    }

    /// Speichert eine Notiz (JSON immer, Markdown nur wenn `write_markdown`), ordner-bewusst.
    ///
    /// Löscht die alte `.md`-Datei wenn der Titel geändert wurde (nur wenn der Spiegel aktiv ist).
    pub async fn save_note(&self, note: &Note, write_markdown: bool) -> Result<()> {
        let folder = note.folder_name.as_deref();

        // MKCOL Unterverzeichnisse, falls Notiz in einem Ordner liegt
        if let Some(f) = folder {
            self.ensure_folder_dirs(f, write_markdown).await;
        }

        if write_markdown {
            // Titel-Diff: alte .md entfernen wenn der Titel sich geändert hat.
            if let Ok(existing) = self.get_note(&note.id, folder).await {
                if existing.title != note.title {
                    let old_safe = sanitize_filename(&existing.title, &note.id);
                    let old_md_url = self.note_md_url(folder, &old_safe);
                    let _ = self
                        .client
                        .delete(&old_md_url)
                        .header("Authorization", &self.auth_header)
                        .send()
                        .await;
                }
            }
        }

        self.save_json(note).await?;
        if write_markdown {
            self.save_markdown(note).await?;
        }
        Ok(())
    }

    async fn save_json(&self, note: &Note) -> Result<()> {
        let url = self.note_json_url(note.folder_name.as_deref(), &note.id);

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PUT JSON: {}", url);

        let json_content =
            serde_json::to_string_pretty(note).map_err(|e| AppError::ParseError(e.to_string()))?;

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
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(AppError::WebDav(format!(
                "PUT JSON failed: {} - {}",
                status, error_body
            )));
        }

        Ok(())
    }

    async fn save_markdown(&self, note: &Note) -> Result<()> {
        // Getrashte Notizen haben keinen Markdown-Export (Android-Parität): .md löschen statt PUT.
        if note.trashed_at.is_some() {
            let safe_title = sanitize_filename(&note.title, &note.id);
            let md_url = self.note_md_url(note.folder_name.as_deref(), &safe_title);
            let _ = self
                .client
                .delete(&md_url)
                .header("Authorization", &self.auth_header)
                .send()
                .await;
            return Ok(());
        }

        // Bild-Referenzen sind eine App-interne Konvention (`.assets/<name>`, von jeder App
        // selbst aufgelöst) — für externe Markdown-Viewer auf einen echten relativen Pfad
        // zum Geschwister-Ordner `{sync_folder}-assets/` umschreiben. Tiefe hängt davon ab,
        // ob die Notiz in einem Unterordner liegt (ein Verzeichnis-Level mehr).
        let asset_prefix = if note.folder_name.is_some() {
            "../../"
        } else {
            "../"
        };
        let markdown_content = markdown::generate_markdown(note).replace(
            "](.assets/",
            &format!("]({}{}-assets/", asset_prefix, self.sync_folder),
        );
        let safe_title = sanitize_filename(&note.title, &note.id);
        let url = self.note_md_url(note.folder_name.as_deref(), &safe_title);

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
            return Err(AppError::WebDav(format!(
                "PUT Markdown failed: {} for note {}",
                response.status(),
                note.id
            )));
        }

        Ok(())
    }

    /// Löscht eine Notiz (JSON + Markdown) aus dem in `note.folder_name` angegebenen Ordner.
    #[allow(dead_code)]
    pub async fn delete_note(&self, note: &Note) -> Result<()> {
        let folder = note.folder_name.as_deref();

        let json_url = self.note_json_url(folder, &note.id);
        let _ = self
            .client
            .delete(&json_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        let safe_title = sanitize_filename(&note.title, &note.id);
        let md_url = self.note_md_url(folder, &safe_title);
        let _ = self
            .client
            .delete(&md_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        Ok(())
    }

    /// Verschiebt eine Notiz von `from_folder` nach `to_folder` (Copy-then-Delete).
    /// Toleriert 404 beim Löschen des alten Pfades.
    #[allow(dead_code)]
    pub async fn move_note_file(
        &self,
        id: &str,
        from_folder: Option<&str>,
        to_folder: Option<&str>,
    ) -> Result<()> {
        // Notiz am alten Pfad laden
        let mut note = self.get_note(id, from_folder).await?;

        // Ordner aktualisieren
        note.folder_name = to_folder.map(str::to_owned);
        // Timestamp aktualisieren, damit Android die neuere Server-Version zieht
        // und nicht seine lokale Kopie (gleicher Timestamp) erneut hochlädt.
        note.updated_at = chrono::Utc::now().timestamp_millis();

        // Ziel-Verzeichnisse anlegen (falls Ordner)
        if let Some(f) = to_folder {
            self.ensure_folder_dirs(f, true).await;
        }

        // Am neuen Pfad speichern (JSON + MD)
        self.save_json(&note).await?;
        self.save_markdown(&note).await?;

        // Alten JSON-Pfad löschen (Fehler ignorieren)
        let old_json = self.note_json_url(from_folder, id);
        let _ = self
            .client
            .delete(&old_json)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        // Alten MD-Pfad löschen (Fehler ignorieren)
        let safe_title = sanitize_filename(&note.title, id);
        let old_md = self.note_md_url(from_folder, &safe_title);
        let _ = self
            .client
            .delete(&old_md)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        Ok(())
    }

    // ── Ordner-Metadaten ────────────────────────────────────────────────────────

    /// Lädt `folders.json` vom Server (404 → leere Liste).
    pub async fn read_folders_meta(&self) -> Vec<FolderMeta> {
        let url = self.folders_file_url();
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        let Ok(resp) = resp else {
            return Vec::new();
        };

        if resp.status() == StatusCode::NOT_FOUND {
            return Vec::new();
        }

        let Ok(text) = resp.text().await else {
            return Vec::new();
        };

        parse_folders_json(&text)
    }

    /// Read-Modify-Write für `folders.json`:
    /// GET remote → `mutation` anwenden → PUT zurück.
    /// `mutation` erhält die aktuelle Liste und gibt die veränderte zurück.
    pub async fn write_folders_meta_merged(
        &self,
        mutation: impl FnOnce(Vec<FolderMeta>) -> Vec<FolderMeta>,
    ) -> Result<Vec<FolderMeta>> {
        let remote = self.read_folders_meta().await;
        let updated = mutation(remote);
        let json = serde_json::to_string_pretty(&updated)
            .map_err(|e| AppError::ParseError(e.to_string()))?;

        let url = self.folders_file_url();
        let resp = self
            .client
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .body(json)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AppError::WebDav(format!(
                "PUT folders.json failed: {}",
                resp.status()
            )));
        }

        Ok(updated)
    }

    // ── Lösch-Ledger ────────────────────────────────────────────────────────────

    /// Lädt `deletions.json` vom Server (404 oder Parse-Fehler → leeres Ledger).
    pub async fn read_deletions(&self) -> DeletionLedger {
        let url = self.deletions_file_url();
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await;

        let Ok(resp) = resp else {
            return DeletionLedger::default();
        };

        if resp.status() == StatusCode::NOT_FOUND {
            return DeletionLedger::default();
        }

        let Ok(text) = resp.text().await else {
            return DeletionLedger::default();
        };

        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Read-Modify-Write für `deletions.json`: GET → `mutation` → PUT.
    async fn write_deletions_merged(
        &self,
        mutation: impl FnOnce(DeletionLedger) -> DeletionLedger,
    ) -> Result<()> {
        let remote = self.read_deletions().await;
        let updated = mutation(remote);
        let json = serde_json::to_string_pretty(&updated)
            .map_err(|e| AppError::ParseError(e.to_string()))?;

        let url = self.deletions_file_url();
        let resp = self
            .client
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .body(json)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AppError::WebDav(format!(
                "PUT deletions.json failed: {}",
                resp.status()
            )));
        }

        Ok(())
    }

    /// Fügt einen Lösch-Eintrag ins gemeinsame Ledger ein (read-modify-write).
    /// Best-effort: Fehler werden geloggt, aber nicht propagiert.
    #[allow(dead_code)]
    pub async fn append_deletion(&self, id: &str, device_id: &str, now: i64, retention_ms: i64) {
        let id = id.to_string();
        let device_id = device_id.to_string();
        let result = self
            .write_deletions_merged(move |ledger| {
                merge_deletion(ledger, &id, &device_id, now, retention_ms)
            })
            .await;
        if let Err(e) = result {
            eprintln!("[append_deletion] ledger write failed: {}", e);
        }
    }

    /// Hängt mehrere IDs in einem einzigen read-modify-write ans Lösch-Ledger.
    /// Effizienter als n einzelne `append_deletion`-Aufrufe (ein GET+PUT statt n).
    /// Best-effort: Fehler werden geloggt, nicht propagiert.
    pub async fn append_deletions(
        &self,
        ids: &[String],
        device_id: &str,
        now: i64,
        retention_ms: i64,
    ) {
        if ids.is_empty() {
            return;
        }
        let ids = ids.to_vec();
        let device_id = device_id.to_string();
        let result = self
            .write_deletions_merged(move |mut ledger| {
                for id in &ids {
                    ledger = merge_deletion(ledger, id, &device_id, now, retention_ms);
                }
                ledger
            })
            .await;
        if let Err(e) = result {
            eprintln!("[append_deletions] ledger write failed: {}", e);
        }
    }

    /// Entfernt Lösch-Einträge per ID aus dem geteilten Ledger (read-modify-write).
    /// Wird beim Wieder-Einschluss eines local-only-Ordners aufgerufen, damit re-uploadete
    /// Notizen nicht als gelöscht markiert bleiben. Best-effort: Fehler werden geloggt.
    #[allow(dead_code)]
    pub async fn remove_deletions(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let set: std::collections::HashSet<String> = ids.iter().cloned().collect();
        let result = self
            .write_deletions_merged(move |mut ledger| {
                ledger.deleted_notes.retain(|r| !set.contains(&r.id));
                ledger
            })
            .await;
        if let Err(e) = result {
            eprintln!("[remove_deletions] ledger write failed: {}", e);
        }
    }

    /// Löscht eine Notiz-JSON per ID und Ordner-Pfad ohne vollständiges Note-Objekt.
    /// 404 gilt als Erfolg — Datei ist bereits nicht mehr vorhanden.
    /// Wird von der Sync-Queue beim Drain verwendet.
    pub async fn delete_note_by_id_folder(&self, id: &str, folder: Option<&str>) -> Result<()> {
        let json_url = self.note_json_url(folder, id);
        let resp = self
            .client
            .delete(&json_url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match resp.status() {
            s if s.is_success() || s == StatusCode::NOT_FOUND => Ok(()),
            s => Err(AppError::WebDav(format!(
                "DELETE {} fehlgeschlagen: {}",
                id, s
            ))),
        }
    }

    // ── Bild-Anhänge ────────────────────────────────────────────────────────────

    /// Lädt ein Asset vom Server (`{sync_folder}-assets/{name}`).
    pub async fn get_asset(&self, name: &str) -> Result<Vec<u8>> {
        let url = self.asset_url(name);
        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match response.status() {
            StatusCode::OK => response
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| AppError::NetworkError(e.to_string())),
            StatusCode::NOT_FOUND => Err(AppError::WebDav(format!("Asset not found: {}", name))),
            status => Err(AppError::WebDav(format!(
                "GET asset failed: {} for {}",
                status, name
            ))),
        }
    }

    /// Lädt ein Asset hoch. Assets sind immutable (content-addressed) — ein PUT auf einen
    /// bereits vorhandenen Namen überschreibt lediglich mit identischen Bytes.
    pub async fn put_asset(&self, name: &str, bytes: &[u8], mime: &str) -> Result<()> {
        let url = self.asset_url(name);
        let response = self
            .client
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", mime)
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::WebDav(format!(
                "PUT asset failed: {} for {}",
                response.status(),
                name
            )));
        }
        Ok(())
    }

    /// Löscht ein Asset. 404 gilt als Erfolg (bereits nicht mehr vorhanden).
    pub async fn delete_asset(&self, name: &str) -> Result<()> {
        let url = self.asset_url(name);
        let response = self
            .client
            .delete(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        match response.status() {
            s if s.is_success() || s == StatusCode::NOT_FOUND => Ok(()),
            s => Err(AppError::WebDav(format!(
                "DELETE asset failed: {} for {}",
                s, name
            ))),
        }
    }

    /// Listet alle Assets auf dem Server mit ihrer Änderungszeit (Unix ms, `None` falls
    /// `getlastmodified` fehlt oder nicht parsbar ist — solche Assets sweept die GC nie).
    /// Best-effort: Server-/Verzeichnis-Fehler liefern eine leere Liste statt eines Err
    /// (Asset-Sync ist ein optionaler Zusatzschritt, kein sync-kritischer Pfad).
    pub async fn list_server_assets(&self) -> Result<Vec<(String, Option<i64>)>> {
        let url = self.assets_dir_url();
        let text = match self.propfind_text(&url, "1").await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[assets] PROPFIND {} fehlgeschlagen: {}", url, e);
                return Ok(Vec::new());
            }
        };

        Ok(parse_server_assets(&text))
    }

    // ── Interner PROPFIND-Helfer ─────────────────────────────────────────────────

    async fn propfind_text(&self, url: &str, depth: &str) -> Result<String> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname/>
    <d:getcontenttype/>
    <d:resourcetype/>
    <d:getlastmodified/>
    <d:getetag/>
  </d:prop>
</d:propfind>"#;

        let response = self
            .client
            .request(PROPFIND.clone(), url)
            .header("Authorization", &self.auth_header)
            .header("Depth", depth)
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

        response
            .text()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))
    }
}

/// Fügt einen Lösch-Eintrag in ein Ledger ein, dedupliziert nach id (neuestes
/// `deleted_at` gewinnt) und bereinigt Einträge älter als `retention_ms`.
fn merge_deletion(
    mut ledger: DeletionLedger,
    id: &str,
    device_id: &str,
    now: i64,
    retention_ms: i64,
) -> DeletionLedger {
    ledger.version = 1;
    if let Some(pos) = ledger.deleted_notes.iter().position(|r| r.id == id) {
        if ledger.deleted_notes[pos].deleted_at >= now {
            // Vorhandener Eintrag ist neuer oder gleich alt → nur bereinigen
            ledger
                .deleted_notes
                .retain(|r| now - r.deleted_at <= retention_ms);
            return ledger;
        }
        ledger.deleted_notes.remove(pos);
    }
    ledger.deleted_notes.push(DeletionRecord {
        id: id.to_string(),
        deleted_at: now,
        device_id: device_id.to_string(),
    });
    ledger
        .deleted_notes
        .retain(|r| now - r.deleted_at <= retention_ms);
    ledger
}

/// Vergleicht zwei ETags formattolerant: `W/"abc"`, `"abc"` und `abc` gelten als gleich.
///
/// Derselbe Server liefert denselben Wert im PUT-`ETag`-Header anders als im PROPFIND-`getetag`.
/// Ohne die Normalisierung gäbe das Phantom-Konflikte. `None` matcht nie — „kein ETag" ist keine
/// Aussage über Gleichheit. Android-Parität: `etagsMatch`.
pub fn etags_match(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => normalize_etag(a) == normalize_etag(b),
        _ => false,
    }
}

fn normalize_etag(etag: &str) -> &str {
    etag.trim()
        .trim_start_matches("W/")
        .trim_start_matches("w/")
        .trim_matches('"')
}

/// Sammelt `{uuid}.json` → ETag aus den Response-Blöcken einer PROPFIND-Antwort.
/// Reine Funktion (wie `parse_server_assets`), damit sie ohne Server testbar ist.
fn parse_note_etags(text: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for block_cap in RESPONSE_BLOCK_PATTERN.captures_iter(text) {
        let block = &block_cap[1];
        let Some(href_cap) = HREF_PATTERN.captures(block) else {
            continue;
        };
        let href = href_cap[1].trim();
        let decoded = urlencoding::decode(href)
            .unwrap_or_else(|_| href.into())
            .into_owned();
        let Some(id) = UUID_PATTERN.captures(&decoded).map(|c| c[1].to_lowercase()) else {
            continue;
        };
        let Some(etag) = ETAG_PATTERN
            .captures(block)
            .map(|c| c[1].trim().to_string())
            .filter(|e| !e.is_empty())
        else {
            continue;
        };
        result.insert(id, etag);
    }
    result
}

/// Parst die Response-Blöcke einer PROPFIND-Antwort auf `{sync_folder}-assets/` in
/// (Dateiname, mtime-in-ms) Paare. Reine Funktion, unabhängig von `WebDavClient` testbar.
fn parse_server_assets(text: &str) -> Vec<(String, Option<i64>)> {
    let mut result = Vec::new();
    for block_cap in RESPONSE_BLOCK_PATTERN.captures_iter(text) {
        let block = &block_cap[1];
        let Some(href_cap) = HREF_PATTERN.captures(block) else {
            continue;
        };
        let href = href_cap[1].trim();
        if href.ends_with('/') {
            continue; // das Verzeichnis selbst
        }
        let decoded = urlencoding::decode(href)
            .unwrap_or_else(|_| href.into())
            .into_owned();
        let Some(name) = decoded.rsplit('/').next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let mtime = LAST_MODIFIED_PATTERN
            .captures(block)
            .and_then(|c| chrono::DateTime::parse_from_rfc2822(c[1].trim()).ok())
            .map(|dt| dt.timestamp_millis());
        result.push((name.to_string(), mtime));
    }
    result
}

fn sanitize_filename(title: &str, id: &str) -> String {
    let sanitized: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        format!("untitled-{}", &id[..8.min(id.len())])
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run the sync-folder sanitization logic extracted from WebDavClient::new
    fn sanitize_sync_folder(input: &str) -> String {
        let sanitized: String = input
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if sanitized.is_empty() {
            "notes".to_string()
        } else {
            sanitized.chars().take(50).collect()
        }
    }

    #[test]
    fn test_sanitize_sync_folder() {
        // Normal cases
        assert_eq!(sanitize_sync_folder("notes"), "notes");
        assert_eq!(sanitize_sync_folder("my-notes"), "my-notes");
        assert_eq!(sanitize_sync_folder("my_notes"), "my_notes");
        assert_eq!(sanitize_sync_folder("Notes123"), "Notes123");
        // Empty → default
        assert_eq!(sanitize_sync_folder(""), "notes");
        assert_eq!(sanitize_sync_folder("!!!"), "notes");
        // Non-ASCII must be stripped (not passed through as Unicode alphanumeric)
        assert_eq!(sanitize_sync_folder("café"), "caf");
        assert_eq!(sanitize_sync_folder("Nötig"), "Ntig");
        assert_eq!(sanitize_sync_folder("📝notes"), "notes");
        // Spaces and slashes stripped
        assert_eq!(sanitize_sync_folder("my notes"), "mynotes");
        assert_eq!(sanitize_sync_folder("my/notes"), "mynotes");
        // Max 50 chars enforced
        let long = "a".repeat(60);
        assert_eq!(sanitize_sync_folder(&long).len(), 50);
    }

    #[test]
    fn test_sanitize_filename() {
        let id = "abcdef12-0000-0000-0000-000000000000";
        assert_eq!(sanitize_filename("Normal Title", id), "Normal Title");
        assert_eq!(sanitize_filename("With/Slash", id), "With_Slash");
        assert_eq!(sanitize_filename("Test:Colon", id), "Test_Colon");
        assert_eq!(sanitize_filename("Multi<>Special", id), "Multi__Special");
        assert_eq!(sanitize_filename("  Trimmed  ", id), "Trimmed");
        assert_eq!(sanitize_filename("", id), "untitled-abcdef12");
        assert_eq!(sanitize_filename("   ", id), "untitled-abcdef12");
    }

    // ── Folder URL-Builder Tests ─────────────────────────────────────────────────
    // Prüft dass die URL-Builder das korrekte %20-Encoding für Leerzeichen liefern
    // (Android verwendet URLEncoder…replace("+","%20")).

    fn make_client() -> WebDavClient {
        WebDavClient {
            client: Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap(),
            base_url: "http://server".to_string(),
            auth_header: "Basic dGVzdA==".to_string(),
            sync_folder: "notes".to_string(),
        }
    }

    #[test]
    fn test_note_json_url_root() {
        let c = make_client();
        let url = c.note_json_url(None, "abc-123");
        assert_eq!(url, "http://server/notes/abc-123.json");
    }

    #[test]
    fn test_note_json_url_folder() {
        let c = make_client();
        let url = c.note_json_url(Some("Work"), "abc-123");
        assert_eq!(url, "http://server/notes/Work/abc-123.json");
    }

    #[test]
    fn test_note_json_url_folder_with_space() {
        let c = make_client();
        let url = c.note_json_url(Some("My Folder"), "abc-123");
        // Space must be encoded as %20 (not +)
        assert!(url.contains("%20"), "space must be %20-encoded: {}", url);
        assert_eq!(url, "http://server/notes/My%20Folder/abc-123.json");
    }

    #[test]
    fn test_note_md_url_root() {
        let c = make_client();
        let url = c.note_md_url(None, "My Note");
        assert_eq!(url, "http://server/notes-md/My Note.md");
    }

    #[test]
    fn test_note_md_url_folder() {
        let c = make_client();
        let url = c.note_md_url(Some("Work"), "My Note");
        assert_eq!(url, "http://server/notes-md/Work/My Note.md");
    }

    #[test]
    fn test_folder_json_dir_url() {
        let c = make_client();
        assert_eq!(c.folder_json_dir_url("Work"), "http://server/notes/Work/");
        // Space → %20
        assert_eq!(
            c.folder_json_dir_url("My Folder"),
            "http://server/notes/My%20Folder/"
        );
    }

    #[test]
    fn test_folder_md_dir_url() {
        let c = make_client();
        assert_eq!(c.folder_md_dir_url("Work"), "http://server/notes-md/Work/");
    }

    #[test]
    fn test_folders_file_url() {
        let c = make_client();
        assert_eq!(c.folders_file_url(), "http://server/notes/folders.json");
    }

    #[test]
    fn test_assets_dir_url() {
        let c = make_client();
        assert_eq!(c.assets_dir_url(), "http://server/notes-assets/");
    }

    #[test]
    fn test_asset_url_encodes_name() {
        let c = make_client();
        assert_eq!(
            c.asset_url("abc1234567890def.webp"),
            "http://server/notes-assets/abc1234567890def.webp"
        );
    }

    // ── parse_server_assets ─────────────────────────────────────────────────────

    // ── ETag-Helfer ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_etags_match_ignores_weak_prefix_and_quotes() {
        assert!(etags_match(Some("W/\"abc123\""), Some("\"abc123\"")));
        assert!(etags_match(Some("\"abc123\""), Some("abc123")));
        assert!(etags_match(Some(" \"abc\" "), Some("abc")));
        assert!(etags_match(Some("w/\"abc\""), Some("W/abc")));
    }

    #[test]
    fn test_etags_match_different_values() {
        assert!(!etags_match(Some("\"abc\""), Some("\"def\"")));
    }

    #[test]
    fn test_etags_match_none_never_matches() {
        // „Kein ETag" ist keine Aussage über Gleichheit — auch nicht None == None.
        assert!(!etags_match(None, Some("\"abc\"")));
        assert!(!etags_match(Some("\"abc\""), None));
        assert!(!etags_match(None, None));
    }

    #[test]
    fn test_parse_note_etags_maps_uuid_to_etag() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/notes/</d:href>
    <d:propstat><d:prop><d:getetag>"dir"</d:getetag></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/notes/AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE.json</d:href>
    <d:propstat><d:prop><d:getetag>W/"abc123"</d:getetag></d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/notes/11111111-2222-3333-4444-555555555555.json</d:href>
    <d:propstat><d:prop><d:getlastmodified>Mon, 01 Jan 2024 00:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let map = parse_note_etags(xml);
        // Verzeichnis-Block enthält keine UUID → kein Eintrag; ID wird lowercased.
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
                .map(|s| s.as_str()),
            Some("W/\"abc123\"")
        );
    }

    #[test]
    fn test_parse_note_etags_url_encoded_and_apache_namespace() {
        let xml = r#"<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/notes/Mein%20Ordner/11111111-2222-3333-4444-555555555555.json</D:href>
    <D:propstat><D:prop><lp1:getetag>"deadbeef"</lp1:getetag></D:prop></D:propstat>
  </D:response>
</D:multistatus>"#;
        let map = parse_note_etags(xml);
        assert_eq!(
            map.get("11111111-2222-3333-4444-555555555555")
                .map(|s| s.as_str()),
            Some("\"deadbeef\"")
        );
    }

    #[test]
    fn test_parse_note_etags_empty_body() {
        assert!(parse_note_etags("").is_empty());
    }

    #[test]
    fn test_parse_server_assets_extracts_name_and_mtime() {
        let body = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/notes-assets/</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/notes-assets/abc123.webp</d:href>
    <d:propstat><d:prop>
      <d:getlastmodified>Wed, 04 Feb 2026 10:25:29 GMT</d:getlastmodified>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let assets = parse_server_assets(body);
        assert_eq!(assets.len(), 1, "directory entry itself must be skipped");
        assert_eq!(assets[0].0, "abc123.webp");
        assert!(assets[0].1.is_some());
    }

    #[test]
    fn test_parse_server_assets_missing_mtime_is_none() {
        let body = r#"<d:response><d:href>/dav/notes-assets/no-mtime.png</d:href></d:response>"#;
        let assets = parse_server_assets(body);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].1, None);
    }

    #[test]
    fn test_parse_server_assets_empty_body() {
        assert!(parse_server_assets("").is_empty());
    }

    #[test]
    fn test_extract_subdirs_skips_root_and_uuid_files() {
        let c = make_client();
        // Simulate a PROPFIND response with hrefs
        let propfind_body = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/notes/</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/notes/11111111-1111-1111-1111-111111111111.json</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/notes/folders.json</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/notes/Work/</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/notes/My%20Notes/</d:href>
  </d:response>
</d:multistatus>"#;

        let subdirs = c.extract_subdirs_from_propfind(propfind_body);
        assert_eq!(
            subdirs.len(),
            2,
            "should find Work and My Notes: {:?}",
            subdirs
        );
        assert!(subdirs.contains(&"Work".to_string()));
        assert!(subdirs.contains(&"My Notes".to_string()));
    }

    #[test]
    fn test_extract_subdirs_deduplicates() {
        let c = make_client();
        let body = r#"
<d:response><d:href>/dav/notes/Work/</d:href></d:response>
<d:response><d:href>/dav/notes/Work/</d:href></d:response>
"#;
        let subdirs = c.extract_subdirs_from_propfind(body);
        assert_eq!(subdirs.len(), 1);
    }

    // ── merge_deletion Tests ─────────────────────────────────────────────────────

    fn make_ledger(records: &[(&str, i64)]) -> crate::models::DeletionLedger {
        crate::models::DeletionLedger {
            version: 1,
            deleted_notes: records
                .iter()
                .map(|(id, deleted_at)| crate::models::DeletionRecord {
                    id: id.to_string(),
                    deleted_at: *deleted_at,
                    device_id: "tauri-test".to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_merge_deletion_adds_new_entry() {
        let ledger = DeletionLedger::default();
        let result = merge_deletion(ledger, "id-a", "tauri-x", 1000, 30_000);
        assert_eq!(result.deleted_notes.len(), 1);
        assert_eq!(result.deleted_notes[0].id, "id-a");
        assert_eq!(result.deleted_notes[0].deleted_at, 1000);
        assert_eq!(result.version, 1);
    }

    #[test]
    fn test_merge_deletion_dedup_keeps_newest() {
        // Existing entry at t=500, new entry at t=1000 → replace with newer
        let ledger = make_ledger(&[("id-a", 500)]);
        let result = merge_deletion(ledger, "id-a", "tauri-x", 1000, 100_000);
        assert_eq!(result.deleted_notes.len(), 1);
        assert_eq!(result.deleted_notes[0].deleted_at, 1000);
    }

    #[test]
    fn test_merge_deletion_dedup_keeps_existing_if_newer() {
        // Existing entry at t=2000, new entry at t=1000 → keep existing (newer)
        let ledger = make_ledger(&[("id-a", 2000)]);
        let result = merge_deletion(ledger, "id-a", "tauri-x", 1000, 100_000);
        assert_eq!(result.deleted_notes.len(), 1);
        assert_eq!(result.deleted_notes[0].deleted_at, 2000);
    }

    #[test]
    fn test_merge_deletion_prunes_expired() {
        // now=100_000, retention=30_000 → entries with deleted_at < 70_000 must be dropped
        let ledger = make_ledger(&[("old", 60_000), ("recent", 80_000)]);
        let result = merge_deletion(ledger, "new", "tauri-x", 100_000, 30_000);
        let ids: Vec<&str> = result.deleted_notes.iter().map(|r| r.id.as_str()).collect();
        assert!(!ids.contains(&"old"), "expired entry must be pruned");
        assert!(ids.contains(&"recent"));
        assert!(ids.contains(&"new"));
    }

    #[test]
    fn test_append_deletions_batch_merges_all_ids() {
        // Simuliert den Kern von append_deletions: mehrere IDs in einem Schritt mergen
        let ledger = DeletionLedger::default();
        let ids = ["id-a", "id-b", "id-c"];
        let now = 5000i64;
        let retention = 100_000i64;

        let mut result = ledger;
        for id in &ids {
            result = merge_deletion(result, id, "tauri-x", now, retention);
        }

        assert_eq!(result.deleted_notes.len(), 3);
        assert!(result.deleted_notes.iter().any(|r| r.id == "id-a"));
        assert!(result.deleted_notes.iter().any(|r| r.id == "id-b"));
        assert!(result.deleted_notes.iter().any(|r| r.id == "id-c"));
        assert!(result.deleted_notes.iter().all(|r| r.deleted_at == now));
    }

    #[test]
    fn test_remove_deletions_removes_matching_ids() {
        let ledger = make_ledger(&[("id-a", 1000), ("id-b", 2000), ("id-c", 3000)]);
        let to_remove = vec!["id-a".to_string(), "id-c".to_string()];
        let set: std::collections::HashSet<String> = to_remove.into_iter().collect();
        let mut result = ledger;
        result.deleted_notes.retain(|r| !set.contains(&r.id));
        assert_eq!(result.deleted_notes.len(), 1);
        assert_eq!(result.deleted_notes[0].id, "id-b");
    }

    #[test]
    fn test_remove_deletions_noop_on_empty_list() {
        let ledger = make_ledger(&[("id-a", 1000)]);
        let set: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut result = ledger;
        result.deleted_notes.retain(|r| !set.contains(&r.id));
        assert_eq!(
            result.deleted_notes.len(),
            1,
            "empty removal set must leave ledger unchanged"
        );
    }

    #[test]
    fn test_remove_deletions_unknown_id_is_noop() {
        let ledger = make_ledger(&[("id-a", 1000), ("id-b", 2000)]);
        let set: std::collections::HashSet<String> = vec!["id-z".to_string()].into_iter().collect();
        let mut result = ledger;
        result.deleted_notes.retain(|r| !set.contains(&r.id));
        assert_eq!(
            result.deleted_notes.len(),
            2,
            "removing unknown id must not change ledger"
        );
    }

    #[test]
    fn test_append_deletions_deduplicates_existing() {
        // Vorhandene Einträge werden korrekt dedupliziert (neuestes deleted_at gewinnt)
        let ledger = make_ledger(&[("id-a", 1000), ("id-b", 2000)]);
        let ids = ["id-a", "id-c"]; // id-a aktualisieren, id-c neu hinzufügen
        let now = 3000i64;
        let retention = 100_000i64;

        let mut result = ledger;
        for id in &ids {
            result = merge_deletion(result, id, "tauri-x", now, retention);
        }

        // id-a: aktualisiert auf 3000 (neuer als 1000)
        // id-b: unverändert 2000
        // id-c: neu mit 3000
        assert_eq!(result.deleted_notes.len(), 3);
        let a = result
            .deleted_notes
            .iter()
            .find(|r| r.id == "id-a")
            .unwrap();
        assert_eq!(
            a.deleted_at, 3000,
            "id-a muss auf neueren Wert aktualisiert werden"
        );
        let b = result
            .deleted_notes
            .iter()
            .find(|r| r.id == "id-b")
            .unwrap();
        assert_eq!(b.deleted_at, 2000, "id-b muss unverändert bleiben");
        let c = result
            .deleted_notes
            .iter()
            .find(|r| r.id == "id-c")
            .unwrap();
        assert_eq!(c.deleted_at, 3000, "id-c muss neu hinzugefügt werden");
    }

    #[test]
    fn test_merge_deletion_prune_only_on_same_id_newer_existing() {
        // id-a already has a newer entry; only pruning should happen, no duplicate added
        let ledger = make_ledger(&[("id-a", 5000), ("old", 0)]);
        let result = merge_deletion(ledger, "id-a", "tauri-x", 1000, 100_000);
        // id-a kept with deleted_at=5000 (newer), "old" may be pruned if expired (0 < 100_000-100_000=0? no, 100_000-0=100_000 > 100_000 false, so "old" survives here)
        let a = result
            .deleted_notes
            .iter()
            .find(|r| r.id == "id-a")
            .unwrap();
        assert_eq!(a.deleted_at, 5000, "newer existing entry must be preserved");
        assert_eq!(
            result
                .deleted_notes
                .iter()
                .filter(|r| r.id == "id-a")
                .count(),
            1,
            "no duplicate"
        );
    }
}
