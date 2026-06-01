use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Ordner-Metadaten für `folders.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMeta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub deleted: bool,
}

/// UI-facing Ordner-Typ (an das Frontend zurückgegeben)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Validiert einen Ordnernamen (Port von `FolderNameValidator.kt`)
pub fn validate_folder_name(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return false;
    }
    if trimmed == "." || trimmed == ".." {
        return false;
    }
    for c in trimmed.chars() {
        if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return false;
        }
        if (c as u32) < 0x20 {
            return false;
        }
    }
    true
}

/// Bereinigt einen rohen Verzeichnisnamen (z.B. aus WebDAV href).
/// Gibt `None` zurück wenn nichts Valides übrig bleibt.
pub fn sanitize_folder_name(raw: &str) -> Option<String> {
    let decoded = urlencoding::decode(raw)
        .unwrap_or_else(|_| raw.into())
        .into_owned();
    let trimmed = decoded.trim().trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let cleaned: String = trimmed
        .chars()
        .filter(|&c| {
            !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && (c as u32) >= 0x20
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Parst `folders.json` im neuen Format (Array von Objekten) **und** im
/// Legacy-Format (Array von Strings). Leer-/Whitespace-Namen werden gefiltert.
pub fn parse_folders_json(text: &str) -> Vec<FolderMeta> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for item in arr {
        if let Some(s) = item.as_str() {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                result.push(FolderMeta {
                    name: trimmed,
                    color: None,
                    updated_at: 0,
                    deleted: false,
                });
            }
        } else if item.is_object() {
            if let Ok(meta) = serde_json::from_value::<FolderMeta>(item.clone()) {
                let trimmed = meta.name.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(FolderMeta {
                        name: trimmed,
                        ..meta
                    });
                }
            }
        }
    }
    result
}

/// LWW-Merge zweier `FolderMeta`-Listen (case-insensitiver Schlüssel).
/// Höhere `updatedAt` gewinnt; bei Gleichstand gewinnt Tombstone über Lebender.
/// Insertion-Order der local-Liste bleibt erhalten (neue remote-Einträge hinten).
/// Wird im Server-Format-Test und für zukünftige Multi-Device-Sync-Fälle gebraucht.
#[allow(dead_code)]
pub fn merge_by_name(local: Vec<FolderMeta>, remote: Vec<FolderMeta>) -> Vec<FolderMeta> {
    let mut result: Vec<FolderMeta> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for meta in local.into_iter().chain(remote) {
        let key = meta.name.to_lowercase();
        if let Some(&pos) = index.get(&key) {
            let existing = &result[pos];
            let replace = meta.updated_at > existing.updated_at
                || (meta.updated_at == existing.updated_at && meta.deleted && !existing.deleted);
            if replace {
                result[pos] = meta;
            }
        } else {
            let pos = result.len();
            index.insert(key, pos);
            result.push(meta);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Validator ────────────────────────────────────────────────────────────────

    #[test]
    fn test_validate_valid_names() {
        assert!(validate_folder_name("Work"));
        assert!(validate_folder_name("My Project"));
        assert!(validate_folder_name("notes-2024"));
        assert!(validate_folder_name(&"a".repeat(64)));
    }

    #[test]
    fn test_validate_empty_or_whitespace() {
        assert!(!validate_folder_name(""));
        assert!(!validate_folder_name("   "));
    }

    #[test]
    fn test_validate_dot_names() {
        assert!(!validate_folder_name("."));
        assert!(!validate_folder_name(".."));
    }

    #[test]
    fn test_validate_too_long() {
        assert!(!validate_folder_name(&"a".repeat(65)));
    }

    #[test]
    fn test_validate_forbidden_chars() {
        for ch in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert!(
                !validate_folder_name(&format!("test{}name", ch)),
                "should reject '{}'",
                ch
            );
        }
    }

    #[test]
    fn test_validate_control_chars_in_middle() {
        assert!(!validate_folder_name("test\x00name"));
        assert!(!validate_folder_name("test\x1fname"));
        assert!(!validate_folder_name("test\nname"));
        assert!(!validate_folder_name("test\tname"));
    }

    // ── Sanitize ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_sanitize_normal() {
        assert_eq!(sanitize_folder_name("Work"), Some("Work".to_string()));
    }

    #[test]
    fn test_sanitize_url_encoded() {
        assert_eq!(
            sanitize_folder_name("My%20Notes"),
            Some("My Notes".to_string())
        );
    }

    #[test]
    fn test_sanitize_strips_leading_trailing_slashes() {
        assert_eq!(sanitize_folder_name("/Work/"), Some("Work".to_string()));
    }

    #[test]
    fn test_sanitize_strips_forbidden_chars() {
        assert_eq!(
            sanitize_folder_name("test:name"),
            Some("testname".to_string())
        );
        assert_eq!(
            sanitize_folder_name("test/name"),
            Some("testname".to_string())
        );
    }

    #[test]
    fn test_sanitize_empty_returns_none() {
        assert_eq!(sanitize_folder_name(""), None);
        assert_eq!(sanitize_folder_name("/"), None);
        assert_eq!(sanitize_folder_name(":*?"), None);
    }

    #[test]
    fn test_sanitize_truncates_at_64() {
        let result = sanitize_folder_name(&"a".repeat(80));
        assert_eq!(result.unwrap().len(), 64);
    }

    // ── Parse ────────────────────────────────────────────────────────────────────

    #[test]
    fn test_parse_new_format() {
        let json =
            r##"[{"name":"Work","color":"#FF8C42","updatedAt":1234567890,"deleted":false}]"##;
        let folders = parse_folders_json(json);
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "Work");
        assert_eq!(folders[0].color.as_deref(), Some("#FF8C42"));
        assert_eq!(folders[0].updated_at, 1234567890);
        assert!(!folders[0].deleted);
    }

    #[test]
    fn test_parse_legacy_string_format() {
        let json = r#"["Work","Home",""]"#;
        let folders = parse_folders_json(json);
        assert_eq!(folders.len(), 2, "empty string should be filtered");
        assert_eq!(folders[0].name, "Work");
        assert_eq!(folders[0].color, None);
        assert_eq!(folders[0].updated_at, 0);
        assert!(!folders[0].deleted);
        assert_eq!(folders[1].name, "Home");
    }

    #[test]
    fn test_parse_filters_whitespace_names() {
        let json = r#"[{"name":"Work"},{"name":"  "},{"name":"Home"}]"#;
        let folders = parse_folders_json(json);
        assert_eq!(folders.len(), 2);
    }

    #[test]
    fn test_parse_invalid_json_returns_empty() {
        assert!(parse_folders_json("not json").is_empty());
        assert!(parse_folders_json("{}").is_empty()); // object, not array
    }

    // ── Merge ────────────────────────────────────────────────────────────────────

    #[test]
    fn test_merge_remote_higher_ts_wins() {
        let local = vec![FolderMeta {
            name: "Work".into(),
            color: None,
            updated_at: 100,
            deleted: false,
        }];
        let remote = vec![FolderMeta {
            name: "Work".into(),
            color: Some("#FF0000".into()),
            updated_at: 200,
            deleted: false,
        }];
        let merged = merge_by_name(local, remote);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].color.as_deref(), Some("#FF0000"));
    }

    #[test]
    fn test_merge_local_higher_ts_wins() {
        let local = vec![FolderMeta {
            name: "Work".into(),
            color: Some("#00FF00".into()),
            updated_at: 300,
            deleted: false,
        }];
        let remote = vec![FolderMeta {
            name: "Work".into(),
            color: Some("#FF0000".into()),
            updated_at: 200,
            deleted: false,
        }];
        let merged = merge_by_name(local, remote);
        assert_eq!(merged[0].color.as_deref(), Some("#00FF00"));
    }

    #[test]
    fn test_merge_tombstone_wins_on_tie() {
        let local = vec![FolderMeta {
            name: "Work".into(),
            color: None,
            updated_at: 100,
            deleted: true,
        }];
        let remote = vec![FolderMeta {
            name: "Work".into(),
            color: None,
            updated_at: 100,
            deleted: false,
        }];
        let merged = merge_by_name(local, remote);
        assert!(merged[0].deleted, "tombstone should win on timestamp tie");
    }

    #[test]
    fn test_merge_case_insensitive_key() {
        let local = vec![FolderMeta {
            name: "work".into(),
            color: None,
            updated_at: 100,
            deleted: false,
        }];
        let remote = vec![FolderMeta {
            name: "Work".into(),
            color: Some("#FF0000".into()),
            updated_at: 200,
            deleted: false,
        }];
        let merged = merge_by_name(local, remote);
        assert_eq!(merged.len(), 1, "case-insensitive dedup");
    }

    #[test]
    fn test_merge_new_remote_folder_appended() {
        let local = vec![FolderMeta {
            name: "Work".into(),
            color: None,
            updated_at: 100,
            deleted: false,
        }];
        let remote = vec![
            FolderMeta {
                name: "Work".into(),
                color: None,
                updated_at: 100,
                deleted: false,
            },
            FolderMeta {
                name: "Home".into(),
                color: None,
                updated_at: 150,
                deleted: false,
            },
        ];
        let merged = merge_by_name(local, remote);
        assert_eq!(merged.len(), 2);
    }
}
