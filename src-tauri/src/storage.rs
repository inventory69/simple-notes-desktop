use serde::{Deserialize, Serialize};

/// WebDAV Credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// App Settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String, // "light", "dark", "system"
    pub autosave: bool,
    pub minimize_to_tray: bool, // Minimize to system tray instead of closing
    pub autostart: bool,        // Start with system boot
    pub sync_folder: String,    // WebDAV sync folder name (default: "notes")
    pub update_notifications: bool, // Show popup when an update is available (Windows only)
    pub default_open_mode: String, // "edit" | "preview" — open text notes in edit vs preview
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            autosave: true,
            minimize_to_tray: false,
            autostart: false,
            sync_folder: "notes".to_string(),
            update_notifications: true,
            default_open_mode: "edit".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.theme, "system");
        assert!(settings.autosave);
        assert!(!settings.minimize_to_tray);
        assert!(!settings.autostart);
        assert_eq!(settings.sync_folder, "notes");
        assert!(settings.update_notifications);
    }

    #[test]
    fn test_settings_serialization_full() {
        let settings = Settings {
            theme: "dark".to_string(),
            autosave: false,
            minimize_to_tray: true,
            autostart: true,
            sync_folder: "my-notes".to_string(),
            update_notifications: false,
            default_open_mode: "edit".to_string(),
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.theme, "dark");
        assert!(!parsed.autosave);
        assert!(parsed.minimize_to_tray);
        assert!(parsed.autostart);
        assert_eq!(parsed.sync_folder, "my-notes");
        assert!(!parsed.update_notifications);
    }

    #[test]
    fn test_settings_deserialization_missing_new_fields() {
        // Old settings.json without newer fields must deserialize cleanly via #[serde(default)]
        let old_json = r#"{"theme":"light","autosave":true}"#;
        let result: Settings = serde_json::from_str(old_json)
            .expect("missing fields must be filled from Settings::default()");
        assert_eq!(result.theme, "light");
        assert!(result.autosave);
        assert!(!result.minimize_to_tray);
        assert!(!result.autostart);
        assert_eq!(result.sync_folder, "notes");
        assert!(result.update_notifications); // default true
    }

    #[test]
    fn test_settings_json_roundtrip_all_combinations() {
        let combos = vec![(true, true), (true, false), (false, true), (false, false)];

        for (tray, autostart) in combos {
            let settings = Settings {
                theme: "system".to_string(),
                autosave: true,
                minimize_to_tray: tray,
                autostart,
                sync_folder: "notes".to_string(),
                update_notifications: true,
                default_open_mode: "edit".to_string(),
            };

            let json = serde_json::to_string(&settings).unwrap();
            let parsed: Settings = serde_json::from_str(&json).unwrap();

            assert_eq!(
                parsed.minimize_to_tray, tray,
                "tray mismatch for ({}, {})",
                tray, autostart
            );
            assert_eq!(
                parsed.autostart, autostart,
                "autostart mismatch for ({}, {})",
                tray, autostart
            );
        }
    }

    #[test]
    fn test_settings_json_field_names() {
        let settings = Settings {
            theme: "dark".to_string(),
            autosave: false,
            minimize_to_tray: true,
            autostart: true,
            sync_folder: "notes".to_string(),
            update_notifications: false,
            default_open_mode: "edit".to_string(),
        };

        let json = serde_json::to_string(&settings).unwrap();
        // Verify the JSON field names match what the frontend expects
        assert!(json.contains("\"minimize_to_tray\":true"));
        assert!(json.contains("\"autostart\":true"));
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"autosave\":false"));
        assert!(json.contains("\"sync_folder\":\"notes\""));
        assert!(json.contains("\"update_notifications\":false"));
    }

    #[test]
    fn test_credentials_serialization() {
        let creds = Credentials {
            url: "http://localhost:8080".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
        };

        let json = serde_json::to_string(&creds).unwrap();
        let parsed: Credentials = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.url, creds.url);
        assert_eq!(parsed.username, creds.username);
        assert_eq!(parsed.password, creds.password);
    }

    #[test]
    fn test_settings_clone() {
        let settings = Settings {
            theme: "dark".to_string(),
            autosave: true,
            minimize_to_tray: true,
            autostart: false,
            sync_folder: "custom".to_string(),
            update_notifications: false,
            default_open_mode: "edit".to_string(),
        };

        let cloned = settings.clone();
        assert_eq!(cloned.theme, settings.theme);
        assert_eq!(cloned.minimize_to_tray, settings.minimize_to_tray);
        assert_eq!(cloned.autostart, settings.autostart);
        assert_eq!(cloned.sync_folder, settings.sync_folder);
        assert_eq!(cloned.update_notifications, settings.update_notifications);
    }

    #[test]
    fn test_get_settings_keys_match_settings_struct() {
        // get_settings() in lib.rs reads a hardcoded list of store keys and must
        // cover every field in Settings. This test serializes Settings::default()
        // and asserts that the hardcoded list and the struct fields are identical.
        // If you add a field to Settings, CI will catch the mismatch here before
        // it becomes a silent "new setting is never persisted" bug.
        let actual_keys: std::collections::HashSet<String> =
            serde_json::to_value(Settings::default())
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();

        let hardcoded_keys: std::collections::HashSet<String> = [
            "theme",
            "autosave",
            "minimize_to_tray",
            "autostart",
            "sync_folder",
            "update_notifications",
            "default_open_mode",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(
            actual_keys,
            hardcoded_keys,
            "get_settings() key list in lib.rs is out of sync with the Settings struct fields.\n\
             In actual but not hardcoded: {:?}\n\
             In hardcoded but not actual: {:?}",
            actual_keys.difference(&hardcoded_keys).collect::<Vec<_>>(),
            hardcoded_keys.difference(&actual_keys).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn test_settings_debug_format() {
        let settings = Settings::default();
        let debug = format!("{:?}", settings);
        assert!(debug.contains("minimize_to_tray"));
        assert!(debug.contains("autostart"));
        assert!(debug.contains("sync_folder"));
    }
}
