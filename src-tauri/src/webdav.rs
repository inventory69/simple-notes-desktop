use crate::error::{AppError, Result};
use crate::folders::{parse_folders_json, sanitize_folder_name, FolderMeta};
use crate::markdown;
use crate::models::{DeletionLedger, DeletionRecord, Note};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use reqwest::{Client, Method, StatusCode};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

/// UUID.json Pattern – compiled once at program start
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\.json",
    )
    .expect("UUID pattern is valid")
});

// Die DAV-Muster nehmen jedes Namespace-Präfix oder keins, wie Androids `PropfindParser.kt`:
// `d:`/`D:` (Nextcloud, Koofr), `ns0:` (WsgiDAV), `a:` (IIS), `<href>` im Default-Namespace.

/// Regex zum Extrahieren von WebDAV `<d:href>`
static HREF_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(?:[a-z][\w.-]*:)?href>([^<]+)</(?:[a-z][\w.-]*:)?href>")
        .expect("HREF pattern is valid")
});

/// Ein einzelner `<d:response>`-Block einer PROPFIND-Antwort. Attribute am Element sind erlaubt:
/// Apache (mod_dav) schreibt `<D:response xmlns:lp1="DAV:" …>`.
static RESPONSE_BLOCK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:[a-z][\w.-]*:)?response(?:\s[^>]*)?>(.*?)</(?:[a-z][\w.-]*:)?response>")
        .expect("response block pattern is valid")
});

/// Ordner-Marker innerhalb eines Response-Blocks, präfixunabhängig wie Android
/// (`PropfindParser.kt`): `<D:collection/>` im `resourcetype` oder `httpd/unix-directory`.
static COLLECTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(?:[a-z][\w.-]*:)?collection[\s/>]|httpd/unix-directory")
        .expect("collection pattern is valid")
});

/// Regex zum Extrahieren von `<d:getlastmodified>` innerhalb eines Response-Blocks (RFC1123).
static LAST_MODIFIED_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)<(?:[a-z][\w.-]*:)?getlastmodified>([^<]+)</(?:[a-z][\w.-]*:)?getlastmodified>",
    )
    .expect("getlastmodified pattern is valid")
});

/// Regex zum Extrahieren von `<d:getetag>` innerhalb eines Response-Blocks. Live-Properties
/// haben bei Apache ein eigenes Präfix (`<lp1:getetag>`).
static ETAG_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:[a-z][\w.-]*:)?getetag>([^<]+)</(?:[a-z][\w.-]*:)?getetag>")
        .expect("getetag pattern is valid")
});

/// HTTP-Codes, mit denen ein Server signalisiert, dass er `If-Match` nicht auswerten kann.
const PRECONDITION_UNSUPPORTED_CODES: [u16; 2] = [400, 501];

/// Kennung in `{sync_folder}-e2ee/e2ee.json`, **mit** Anführungszeichen: sonst sperrte eine
/// Soft-404-Seite, die den Pfad `/simple-notes-e2ee/` zurückgibt. Vertrag (beide Clients,
/// Testvektoren): `project-docs/simple-notes-sync/e2ee/slice-1.md`.
const E2EE_MARKER: &str = "\"simple-notes-e2ee\"";
const E2EE_MAX_BODY_BYTES: usize = 64 * 1024;

/// Ergebnis der Marker-Prüfung (Android-Parität `E2eeGate.Probe`).
#[derive(Debug, PartialEq, Eq)]
pub enum E2eeProbe {
    Active,
    Inactive,
    Error,
}

/// Vertragstabelle. `body_prefix` sind höchstens die ersten 64 KiB, siehe [`push_capped`].
pub fn classify_e2ee_probe(status: u16, body_prefix: Option<&str>) -> E2eeProbe {
    match status {
        200..=299 if body_prefix.is_some_and(|b| b.contains(E2EE_MARKER)) => E2eeProbe::Active,
        200..=299 => E2eeProbe::Inactive,
        // Vorübergehend (Auth, Timeout, Rate-Limit): als INAKTIV gewertet schriebe genau dieser
        // Lauf in einen toten Ordner.
        401 | 407 | 408 | 425 | 429 => E2eeProbe::Error,
        400..=499 => E2eeProbe::Inactive,
        // 5xx und alles, was nach dem Redirect-Folgen noch 3xx ist
        _ => E2eeProbe::Error,
    }
}

/// Hängt `chunk` an `buf` an, aber nie über 64 KiB hinaus. `true` = voll, Stream schließen.
fn push_capped(buf: &mut Vec<u8>, chunk: &[u8]) -> bool {
    let room = E2EE_MAX_BODY_BYTES - buf.len();
    buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
    buf.len() >= E2EE_MAX_BODY_BYTES
}

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
    /// Merkt sich, dass dieser Server `If-Match` nicht auswerten kann (400/501). Geteilt über
    /// alle Clones des Clients, deshalb `Arc` — der Client wird pro Sync-Lauf geklont.
    preconditions_unsupported: Arc<AtomicBool>,
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
            preconditions_unsupported: Arc::new(AtomicBool::new(false)),
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

    /// URL der E2EE-Markierungsdatei: `{base}/{sync_folder}-e2ee/e2ee.json`. Ein leerer
    /// `-e2ee/`-Ordner sperrt nicht, nur diese Datei.
    pub fn e2ee_marker_url(&self) -> String {
        format!("{}/{}-e2ee/e2ee.json", self.base_url, self.sync_folder)
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
                // Verschlüsselter Ordner: nichts anlegen, aber Erfolg melden. Scheiterte
                // `connect`, käme der Client nie in den State und die Sperre höbe sich nie
                // selbst auf. Ein Prüffehler bricht ohne MKCOL ab.
                if !self.e2ee_active().await? {
                    self.ensure_directories(write_markdown).await?;
                }
                Ok(true)
            }
            status => Err(AppError::WebDav(format!(
                "Connection test failed: {}",
                status
            ))),
        }
    }

    /// Ein GET auf die E2EE-Markierungsdatei, ohne Cache. `Ok(true)` = ein anderes Gerät hat den
    /// Ordner verschlüsselt, der Server darf nicht mehr angefasst werden. Prüffehler (401, 429,
    /// 5xx, Timeout, …) kommen als `Err`: der Sync endet dann ohne Schreibzugriff, Direktpfade
    /// lehnen ab (fail-closed).
    pub async fn e2ee_active(&self) -> Result<bool> {
        let mut response = self
            .client
            .get(self.e2ee_marker_url())
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::NetworkError(e.to_string()))?;
        let status = response.status();
        let body = if status.is_success() {
            // Nicht `.bytes()`: das liest unbegrenzt.
            let mut buf = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| AppError::NetworkError(e.to_string()))?
            {
                if push_capped(&mut buf, &chunk) {
                    break;
                }
            }
            Some(String::from_utf8_lossy(&buf).into_owned())
        } else {
            None
        };
        match classify_e2ee_probe(status.as_u16(), body.as_deref()) {
            E2eeProbe::Active => Ok(true),
            E2eeProbe::Inactive => Ok(false),
            E2eeProbe::Error
                if status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::PROXY_AUTHENTICATION_REQUIRED =>
            {
                Err(AppError::InvalidCredentials)
            }
            E2eeProbe::Error => Err(AppError::WebDav(format!("E2EE check failed: {}", status))),
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

    /// Server-ETags aller `{uuid}.json` in **einem** Ordner (`None` = Root).
    /// Für den Pre-Upload-Snapshot: ein PROPFIND je Ordner, in den etwas hochgeladen wird.
    pub async fn list_note_etags(&self, folder: Option<&str>) -> Result<HashMap<String, String>> {
        let url = match folder {
            Some(f) => self.folder_json_dir_url(f),
            None => format!("{}/{}/", self.base_url, self.sync_folder),
        };
        Ok(parse_note_etags(&self.propfind_text(&url, "1").await?))
    }

    /// Extrahiert direkte Unterordner-Namen aus einer PROPFIND-Antwort auf das Root-Verzeichnis.
    /// Je Response-Block zählt der erste `href`, Ordner erkennt `is_collection` (Android-Regel).
    fn extract_subdirs_from_propfind(&self, text: &str) -> Vec<String> {
        let mut subdirs: Vec<String> = Vec::new();

        let collection_hrefs = RESPONSE_BLOCK_PATTERN.captures_iter(text).filter_map(|b| {
            let block = b.get(1)?.as_str();
            let href = HREF_PATTERN.captures(block)?.get(1)?.as_str().trim();
            is_collection(block, href).then_some(href)
        });

        for href in collection_hrefs {
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
        self.get_note_with_etag(id, folder).await.map(|(n, _)| n)
    }

    /// Wie `get_note`, gibt zusätzlich den ETag der Antwort zurück — also die Fassung, auf der
    /// die lokale Kopie danach aufsetzt. Genau dieser Wert gehört als ETag-Basis in den Store;
    /// ein aus dem Listing nachgereichter wäre schon wieder eine andere Fassung.
    pub async fn get_note_with_etag(
        &self,
        id: &str,
        folder: Option<&str>,
    ) -> Result<(Note, Option<String>)> {
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
                let etag = response
                    .headers()
                    .get("etag")
                    .or_else(|| response.headers().get("oc-etag"))
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let mut note: Note = response
                    .json()
                    .await
                    .map_err(|e| AppError::ParseError(e.to_string()))?;

                // Fix noteType basierend auf checklistItems (für alte Notizen ohne noteType-Feld)
                note.fix_note_type();

                // Pfad ist maßgebend — überschreibt was im JSON-Body steht
                note.folder_name = folder.map(str::to_owned);

                Ok((note, etag))
            }
            StatusCode::NOT_FOUND => Err(AppError::NoteNotFound(id.to_string())),
            status => Err(AppError::WebDav(format!("GET failed: {}", status))),
        }
    }

    /// Speichert eine Notiz (JSON immer, Markdown nur wenn `write_markdown`), ordner-bewusst.
    ///
    /// Löscht die alte `.md`-Datei wenn der Titel geändert wurde (nur wenn der Spiegel aktiv ist).
    ///
    /// `if_match` = der gecachte Server-ETag, auf dem diese Änderung aufsetzt. Gibt der Server
    /// eine fremde Fassung zurück, kommt `AppError::PreconditionFailed`. Rückgabe ist der ETag
    /// der PUT-Antwort (`None`, wenn der Server keinen schickt).
    pub async fn save_note(
        &self,
        note: &Note,
        write_markdown: bool,
        if_match: Option<&str>,
    ) -> Result<Option<String>> {
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

        let etag = self.save_json(note, if_match).await?;
        if write_markdown {
            self.save_markdown(note).await?;
        }
        Ok(etag)
    }

    async fn save_json(&self, note: &Note, if_match: Option<&str>) -> Result<Option<String>> {
        let url = self.note_json_url(note.folder_name.as_deref(), &note.id);

        #[cfg(debug_assertions)]
        eprintln!("[WebDAV] PUT JSON: {}", url);

        let json_content =
            serde_json::to_string_pretty(note).map_err(|e| AppError::ParseError(e.to_string()))?;

        // `If-Match` ist nur die zweite Reihe — der empfohlene Server (hacdias/webdav) wertet
        // Write-Preconditions gar nicht aus und antwortet mit 201. Der eigentliche Schutz ist
        // der ETag-Vergleich vor dem Upload (sync_engine::is_stale_against_server).
        let mut precondition = if_match
            .map(str::trim)
            .filter(|e| !e.is_empty() && !self.preconditions_unsupported.load(Ordering::Relaxed))
            .map(to_if_match_value);

        loop {
            let mut req = self
                .client
                .put(&url)
                .header("Authorization", &self.auth_header)
                .header("Content-Type", "application/json");
            if let Some(p) = &precondition {
                req = req.header("If-Match", p.as_str());
            }

            let response = req
                .body(json_content.clone())
                .send()
                .await
                .map_err(|e| AppError::NetworkError(e.to_string()))?;

            let status = response.status();
            if status.is_success() {
                // Nextcloud schickt bei 201 keinen `ETag`, wohl aber `OC-ETag` mit demselben
                // Wert, den ein späteres PROPFIND als `getetag` liefert.
                return Ok(response
                    .headers()
                    .get("etag")
                    .or_else(|| response.headers().get("oc-etag"))
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string));
            }
            if status == StatusCode::PRECONDITION_FAILED {
                return Err(AppError::PreconditionFailed);
            }
            if precondition.is_some() && PRECONDITION_UNSUPPORTED_CODES.contains(&status.as_u16()) {
                // Der Server kann die Precondition nicht auswerten. Einmal ohne wiederholen und
                // für diese Verbindung merken — ein dauerhaft blockierter Upload wäre schlimmer
                // als der fehlende Konfliktschutz.
                eprintln!(
                    "[WebDAV] Server lehnt If-Match ab ({}) — Wiederholung ohne Precondition",
                    status
                );
                self.preconditions_unsupported
                    .store(true, Ordering::Relaxed);
                precondition = None;
                continue;
            }

            let error_body = response.text().await.unwrap_or_default();
            return Err(AppError::WebDav(format!(
                "PUT JSON failed: {} - {}",
                status, error_body
            )));
        }
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

        // Am neuen Pfad speichern (JSON + MD). Ohne Precondition: das Ziel ist ein neuer Pfad,
        // die ETag-Basis der Notiz gehört zum alten.
        self.save_json(&note, None).await?;
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

/// Formt einen gespeicherten ETag zu einem gültigen `If-Match`-Wert.
/// Gespeichert wird der rohe Server-Wert — mit Quotes, ohne, oder als schwacher Tag (`W/"abc"`).
/// `If-Match` verlangt einen starken, gequoteten Entity-Tag.
fn to_if_match_value(etag: &str) -> String {
    format!("\"{}\"", normalize_etag(etag))
}

/// Ist dieser Response-Block ein Verzeichnis? `/` am `href`-Ende **oder** Ordner-Marker im Block
/// (Android-Parität; Koofr schickt Ordner ohne `/` am Ende).
fn is_collection(block: &str, href: &str) -> bool {
    href.ends_with('/') || COLLECTION_PATTERN.is_match(block)
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
        if is_collection(block, href) {
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
            preconditions_unsupported: Arc::new(AtomicBool::new(false)),
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
    fn test_to_if_match_value_always_strong_and_quoted() {
        assert_eq!(to_if_match_value("W/\"abc\""), "\"abc\"");
        assert_eq!(to_if_match_value("abc"), "\"abc\"");
        assert_eq!(to_if_match_value(" \"abc\" "), "\"abc\"");
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

    // Echte Koofr-Antworten (Issue #10, 04.10.2026): Ordner-hrefs ohne `/` am Ende, als
    // Ordner erkennbar nur an `<D:collection/>`. Leere Live-Properties stehen in einem
    // zweiten `404`-propstat.
    const KOOFR_PROPFIND_ROOT: &str = r#"<?xml version="1.0" encoding="UTF-8"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/Koofr/notes</D:href><D:propstat><D:prop><D:displayname>notes</D:displayname><D:resourcetype><D:collection xmlns:D="DAV:"/></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:54:17 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:getcontenttype></D:getcontenttype><D:getetag></D:getetag></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response><D:response><D:href>/dav/Koofr/notes/folders.json</D:href><D:propstat><D:prop><D:displayname>folders.json</D:displayname><D:getcontenttype>application/json</D:getcontenttype><D:resourcetype></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:54:21 GMT</D:getlastmodified><D:getetag>"18db5e92cbb866803c"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response><D:response><D:href>/dav/Koofr/notes/ideas</D:href><D:propstat><D:prop><D:displayname>ideas</D:displayname><D:resourcetype><D:collection xmlns:D="DAV:"/></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:54:18 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:getcontenttype></D:getcontenttype><D:getetag></D:getetag></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response></D:multistatus>"#;

    const KOOFR_PROPFIND_IDEAS: &str = r#"<?xml version="1.0" encoding="UTF-8"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/Koofr/notes/ideas</D:href><D:propstat><D:prop><D:displayname>ideas</D:displayname><D:resourcetype><D:collection xmlns:D="DAV:"/></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:54:18 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:getcontenttype></D:getcontenttype><D:getetag></D:getetag></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response><D:response><D:href>/dav/Koofr/notes/ideas/f58d47ad-5b35-456c-841b-eed400b74b6b.json</D:href><D:propstat><D:prop><D:displayname>f58d47ad-5b35-456c-841b-eed400b74b6b.json</D:displayname><D:getcontenttype>application/json</D:getcontenttype><D:resourcetype></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:54:21 GMT</D:getlastmodified><D:getetag>"18db5e92edb1e900ef"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;

    #[test]
    fn test_extract_subdirs_koofr() {
        let c = make_client();
        assert_eq!(
            c.extract_subdirs_from_propfind(KOOFR_PROPFIND_ROOT),
            vec!["ideas".to_string()]
        );
    }

    #[test]
    fn test_parse_note_etags_koofr() {
        let map = parse_note_etags(KOOFR_PROPFIND_IDEAS);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("f58d47ad-5b35-456c-841b-eed400b74b6b")
                .map(|s| s.as_str()),
            Some("\"18db5e92edb1e900ef\"")
        );
    }

    #[test]
    fn test_extract_subdirs_collection_without_slash() {
        let c = make_client();
        let body = r#"<d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/notes</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/notes/Work</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/notes/Old</d:href><d:propstat><d:prop><d:getcontenttype>httpd/unix-directory</d:getcontenttype></d:prop></d:propstat></d:response>
</d:multistatus>"#;
        assert_eq!(
            c.extract_subdirs_from_propfind(body),
            vec!["Work".to_string(), "Old".to_string()]
        );
    }

    #[test]
    fn test_extract_subdirs_files_never_folders() {
        let c = make_client();
        let body = r#"<d:multistatus xmlns:d="DAV:">
<d:response><d:href>/dav/notes/folders.json</d:href><d:propstat><d:prop><d:resourcetype/><d:getcontenttype>application/json</d:getcontenttype></d:prop></d:propstat></d:response>
<d:response><d:href>/dav/notes/11111111-1111-1111-1111-111111111111.json</d:href><d:propstat><d:prop><d:resourcetype></d:resourcetype></d:prop></d:propstat></d:response>
</d:multistatus>"#;
        assert!(c.extract_subdirs_from_propfind(body).is_empty());
    }

    #[test]
    fn test_parse_server_assets_koofr_skips_directory() {
        // Echte Koofr-Antwort auf `notes-assets/` mit einer Datei.
        let body = r#"<?xml version="1.0" encoding="UTF-8"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/dav/Koofr/notes-assets</D:href><D:propstat><D:prop><D:displayname>notes-assets</D:displayname><D:resourcetype><D:collection xmlns:D="DAV:"/></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 15:56:48 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat><D:propstat><D:prop><D:getcontenttype></D:getcontenttype><D:getetag></D:getetag></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat></D:response><D:response><D:href>/dav/Koofr/notes-assets/abc123.webp</D:href><D:propstat><D:prop><D:displayname>abc123.webp</D:displayname><D:getcontenttype>image/webp</D:getcontenttype><D:resourcetype></D:resourcetype><D:getlastmodified>Sun, 04 Oct 2026 16:31:09 GMT</D:getlastmodified><D:getetag>"18db6094f2de86004"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
        let assets = parse_server_assets(body);
        assert_eq!(
            assets.len(),
            1,
            "directory entry must be skipped: {assets:?}"
        );
        assert_eq!(assets[0].0, "abc123.webp");
    }

    // Echte Antworten fremder Server (04.10.2026). Apache 2.4.69 (mod_dav) setzt Namespaces an
    // `<D:response xmlns:lp1=…>` und gibt Live-Properties als `lp1:` aus, WsgiDAV 4.3.5 nimmt
    // `ns0:` als Präfix und liefert ETags ohne Quotes.
    const APACHE_PROPFIND_ROOT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes/</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>httpd/unix-directory</D:getcontenttype>
<lp1:resourcetype><D:collection/></lp1:resourcetype>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:37:00 GMT</lp1:getlastmodified>
<lp1:getetag>"50-65d065fe81930"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes/folders.json</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>application/json</D:getcontenttype>
<lp1:resourcetype/>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:37:00 GMT</lp1:getlastmodified>
<lp1:getetag>"55-65d065fe81930"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes/ideas/</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>httpd/unix-directory</D:getcontenttype>
<lp1:resourcetype><D:collection/></lp1:resourcetype>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</lp1:getlastmodified>
<lp1:getetag>"3c-65d065cac94d4"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
</D:multistatus>"#;

    const APACHE_PROPFIND_IDEAS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes/ideas/</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>httpd/unix-directory</D:getcontenttype>
<lp1:resourcetype><D:collection/></lp1:resourcetype>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:34:36 GMT</lp1:getlastmodified>
<lp1:getetag>W/"3c-65d0657523f23"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes/ideas/aaaaaaaa-1111-2222-3333-444444444444.json</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>application/json</D:getcontenttype>
<lp1:resourcetype/>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:34:36 GMT</lp1:getlastmodified>
<lp1:getetag>W/"ea-65d0657523f23"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
</D:multistatus>"#;

    const APACHE_PROPFIND_ASSETS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes-assets/</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>httpd/unix-directory</D:getcontenttype>
<lp1:resourcetype><D:collection/></lp1:resourcetype>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</lp1:getlastmodified>
<lp1:getetag>"3c-65d065cacdf0c"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/" xmlns:g0="DAV:">
<D:href>/dav/notes-assets/pic1.png</D:href>
<D:propstat>
<D:prop>
<D:getcontenttype>image/png</D:getcontenttype>
<lp1:resourcetype/>
<lp1:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</lp1:getlastmodified>
<lp1:getetag>"7b-65d065cac3714"</lp1:getetag>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
<D:propstat>
<D:prop>
<g0:displayname/>
</D:prop>
<D:status>HTTP/1.1 404 Not Found</D:status>
</D:propstat>
</D:response>
</D:multistatus>"#;

    const WSGIDAV_PROPFIND_ROOT: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<ns0:multistatus xmlns:ns0="DAV:"><ns0:response><ns0:href>/notes/</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>notes</ns0:displayname><ns0:resourcetype><ns0:collection /></ns0:resourcetype><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat><ns0:propstat><ns0:prop><ns0:getcontenttype /><ns0:getetag /></ns0:prop><ns0:status>HTTP/1.1 404 Not Found</ns0:status></ns0:propstat></ns0:response><ns0:response><ns0:href>/notes/folders.json</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>folders.json</ns0:displayname><ns0:getcontenttype>application/json</ns0:getcontenttype><ns0:resourcetype /><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified><ns0:getetag>1095-1791131766-60</ns0:getetag></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat></ns0:response><ns0:response><ns0:href>/notes/ideas/</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>ideas</ns0:displayname><ns0:resourcetype><ns0:collection /></ns0:resourcetype><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat><ns0:propstat><ns0:prop><ns0:getcontenttype /><ns0:getetag /></ns0:prop><ns0:status>HTTP/1.1 404 Not Found</ns0:status></ns0:propstat></ns0:response></ns0:multistatus>"#;

    const WSGIDAV_PROPFIND_IDEAS: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<ns0:multistatus xmlns:ns0="DAV:"><ns0:response><ns0:href>/notes/ideas/</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>ideas</ns0:displayname><ns0:resourcetype><ns0:collection /></ns0:resourcetype><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat><ns0:propstat><ns0:prop><ns0:getcontenttype /><ns0:getetag /></ns0:prop><ns0:status>HTTP/1.1 404 Not Found</ns0:status></ns0:propstat></ns0:response><ns0:response><ns0:href>/notes/ideas/bbbbbbbb-1111-2222-3333-444444444444.json</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>bbbbbbbb-1111-2222-3333-444444444444.json</ns0:displayname><ns0:getcontenttype>application/json</ns0:getcontenttype><ns0:resourcetype /><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified><ns0:getetag>1096-1791131766-235</ns0:getetag></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat></ns0:response></ns0:multistatus>"#;

    const WSGIDAV_PROPFIND_ASSETS: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<ns0:multistatus xmlns:ns0="DAV:"><ns0:response><ns0:href>/notes-assets/</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>notes-assets</ns0:displayname><ns0:resourcetype><ns0:collection /></ns0:resourcetype><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat><ns0:propstat><ns0:prop><ns0:getcontenttype /><ns0:getetag /></ns0:prop><ns0:status>HTTP/1.1 404 Not Found</ns0:status></ns0:propstat></ns0:response><ns0:response><ns0:href>/notes-assets/pic1.png</ns0:href><ns0:propstat><ns0:prop><ns0:displayname>pic1.png</ns0:displayname><ns0:getcontenttype>image/png</ns0:getcontenttype><ns0:resourcetype /><ns0:getlastmodified>Sun, 04 Oct 2026 16:36:06 GMT</ns0:getlastmodified><ns0:getetag>1098-1791131766-123</ns0:getetag></ns0:prop><ns0:status>HTTP/1.1 200 OK</ns0:status></ns0:propstat></ns0:response></ns0:multistatus>"#;

    #[test]
    fn test_extract_subdirs_apache_and_wsgidav() {
        let c = make_client();
        assert_eq!(
            c.extract_subdirs_from_propfind(APACHE_PROPFIND_ROOT),
            vec!["ideas".to_string()]
        );
        assert_eq!(
            c.extract_subdirs_from_propfind(WSGIDAV_PROPFIND_ROOT),
            vec!["ideas".to_string()]
        );
    }

    #[test]
    fn test_parse_note_etags_apache_and_wsgidav() {
        let apache = parse_note_etags(APACHE_PROPFIND_IDEAS);
        assert_eq!(
            apache
                .get("aaaaaaaa-1111-2222-3333-444444444444")
                .map(|s| s.as_str()),
            Some("W/\"ea-65d0657523f23\"")
        );
        let wsgidav = parse_note_etags(WSGIDAV_PROPFIND_IDEAS);
        assert_eq!(
            wsgidav
                .get("bbbbbbbb-1111-2222-3333-444444444444")
                .map(|s| s.as_str()),
            Some("1096-1791131766-235")
        );
    }

    #[test]
    fn test_parse_server_assets_apache_and_wsgidav() {
        for body in [APACHE_PROPFIND_ASSETS, WSGIDAV_PROPFIND_ASSETS] {
            let assets = parse_server_assets(body);
            assert_eq!(assets.len(), 1, "{assets:?}");
            assert_eq!(assets[0].0, "pic1.png");
            assert!(assets[0].1.is_some(), "mtime must parse: {assets:?}");
        }
    }

    // Android parst auch den Default-Namespace ohne Präfix (`PropfindParserTest`).
    #[test]
    fn test_propfind_default_namespace() {
        let c = make_client();
        let body = r#"<multistatus xmlns="DAV:">
<response><href>/dav/notes/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>
<response><href>/dav/notes/Work/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>
<response><href>/dav/notes/11111111-2222-3333-4444-555555555555.json</href><propstat><prop><resourcetype/><getetag>"e1"</getetag></prop></propstat></response>
</multistatus>"#;
        assert_eq!(
            c.extract_subdirs_from_propfind(body),
            vec!["Work".to_string()]
        );
        assert_eq!(
            parse_note_etags(body)
                .get("11111111-2222-3333-4444-555555555555")
                .map(|s| s.as_str()),
            Some("\"e1\"")
        );
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

    // ── E2EE-Gate (Slice 1): Testvektoren aus project-docs/.../e2ee/slice-1.md ────

    /// Body wie aus dem Stream: in 4-KiB-Stücken über `push_capped`, dann verlustbehaftet dekodiert.
    fn capped_body(body: &str) -> String {
        let mut buf = Vec::new();
        for chunk in body.as_bytes().chunks(4096) {
            if push_capped(&mut buf, chunk) {
                break;
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[test]
    fn test_classify_e2ee_probe_contract_vectors() {
        use E2eeProbe::*;
        let k = E2EE_MARKER;
        let at_limit = format!("{}{}", " ".repeat(65_517), k);
        let over_limit = format!("{}{}", " ".repeat(65_518), k);
        let bodies: [(u16, &str, E2eeProbe); 9] = [
            (200, r#"{"format":"simple-notes-e2ee","version":1}"#, Active), // 1
            (200, r#"{"format":"simple-notes-e2ee","#, Active),             // 2
            (207, r#"{"format":"simple-notes-e2ee"}"#, Active),             // 3
            (200, &at_limit, Active),                                       // 4
            (200, &over_limit, Inactive),                                   // 5
            (200, "", Inactive),                                            // 6
            (200, "<html><body>Login</body></html>", Inactive),             // 7
            (
                200,
                "<html>Not found: /simple-notes-e2ee/e2ee.json</html>",
                Inactive,
            ), // 8
            (200, r#"{"format":"simple-notes"}"#, Inactive),                // 9
        ];
        for (i, (status, body, expected)) in bodies.into_iter().enumerate() {
            let got = classify_e2ee_probe(status, Some(&capped_body(body)));
            assert_eq!(got, expected, "vector {}", i + 1);
        }
        // 10–14: Body egal, auch mit Kennung
        let by_status: [(&[u16], E2eeProbe); 5] = [
            (&[404, 410], Inactive),
            (&[400, 403, 405, 409, 418], Inactive),
            (&[401, 407, 408, 425, 429], Error),
            (&[500, 502, 503], Error),
            (&[301, 302, 307], Error),
        ];
        for (statuses, expected) in by_status {
            for &status in statuses {
                assert_eq!(classify_e2ee_probe(status, None), expected, "{}", status);
                assert_eq!(classify_e2ee_probe(status, Some(k)), expected, "{}", status);
            }
        }
    }

    #[test]
    fn test_e2ee_marker_url_uses_sanitized_folder() {
        assert_eq!(
            make_client().e2ee_marker_url(),
            "http://server/notes-e2ee/e2ee.json"
        );
        let c = WebDavClient::new("http://server/dav/", "u", "p", "my notes!").unwrap();
        assert_eq!(
            c.e2ee_marker_url(),
            "http://server/dav/mynotes-e2ee/e2ee.json"
        );
    }

    // ── HTTP-Schicht gegen einen Mock-Server ────────────────────────────────────

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const MARKER_PATH: &str = "/notes-e2ee/e2ee.json";

    async fn mock_marker(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(MARKER_PATH))
            .respond_with(response)
            .expect(1)
            .mount(server)
            .await;
    }

    async fn probe(response: ResponseTemplate) -> Result<bool> {
        let server = MockServer::start().await;
        mock_marker(&server, response).await;
        let client = WebDavClient::new(&server.uri(), "u", "p", "notes").unwrap();
        client.e2ee_active().await
        // `expect(1)` wird beim Drop des Servers geprüft: genau ein Request, kein Retry.
    }

    #[tokio::test]
    async fn test_e2ee_active_inactive_responses() {
        assert!(!probe(ResponseTemplate::new(404)).await.unwrap());
        assert!(!probe(ResponseTemplate::new(403)).await.unwrap());
        assert!(
            !probe(ResponseTemplate::new(200).set_body_string("<html>Login</html>"))
                .await
                .unwrap()
        );
        let over_limit = format!("{}{}", " ".repeat(65_518), E2EE_MARKER);
        assert!(
            !probe(ResponseTemplate::new(200).set_body_string(over_limit))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_e2ee_active_marker_present() {
        let body = r#"{"format":"simple-notes-e2ee","version":1}"#;
        assert!(probe(ResponseTemplate::new(200).set_body_string(body))
            .await
            .unwrap());
        let at_limit = format!("{}{}", " ".repeat(65_517), E2EE_MARKER);
        assert!(probe(ResponseTemplate::new(200).set_body_string(at_limit))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_e2ee_active_errors() {
        assert!(matches!(
            probe(ResponseTemplate::new(401)).await,
            Err(AppError::InvalidCredentials)
        ));
        assert!(matches!(
            probe(ResponseTemplate::new(429)).await,
            Err(AppError::WebDav(_))
        ));
        assert!(matches!(
            probe(ResponseTemplate::new(500)).await,
            Err(AppError::WebDav(_))
        ));
        // 15: keine Antwort (Port 1 nimmt nichts an)
        let unreachable = WebDavClient::new("http://127.0.0.1:1", "u", "p", "notes").unwrap();
        assert!(matches!(
            unreachable.e2ee_active().await,
            Err(AppError::NetworkError(_))
        ));
    }

    async fn test_connection_missing_folder(marker: ResponseTemplate, mkcols: u64) {
        let server = MockServer::start().await;
        Mock::given(method("PROPFIND"))
            .and(path("/notes/"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        mock_marker(&server, marker).await;
        Mock::given(method("MKCOL"))
            .respond_with(ResponseTemplate::new(201))
            .expect(mkcols)
            .mount(&server)
            .await;
        let client = WebDavClient::new(&server.uri(), "u", "p", "notes").unwrap();
        assert!(client.test_connection(false).await.unwrap());
    }

    #[tokio::test]
    async fn test_connection_encrypted_folder_creates_nothing() {
        let body = r#"{"format":"simple-notes-e2ee","version":1}"#;
        test_connection_missing_folder(ResponseTemplate::new(200).set_body_string(body), 0).await;
    }

    #[tokio::test]
    async fn test_connection_missing_folder_still_created_without_marker() {
        // {sf}/ und {sf}-assets/ (Markdown-Spiegel aus)
        test_connection_missing_folder(ResponseTemplate::new(404), 2).await;
    }
}
