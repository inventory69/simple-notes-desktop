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
pub struct Settings {
    pub theme: String,           // "light", "dark", "system"
    pub autosave: bool,
    pub minimize_to_tray: bool,  // Minimize to system tray instead of closing
    pub autostart: bool,         // Start with system boot
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            autosave: true,
            minimize_to_tray: false,
            autostart: false,
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
    }

    #[test]
    fn test_settings_serialization_full() {
        let settings = Settings {
            theme: "dark".to_string(),
            autosave: false,
            minimize_to_tray: true,
            autostart: true,
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.theme, "dark");
        assert!(!parsed.autosave);
        assert!(parsed.minimize_to_tray);
        assert!(parsed.autostart);
    }

    #[test]
    fn test_settings_deserialization_missing_new_fields() {
        // Simulates loading old settings that don't have the new fields
        let old_json = r#"{"theme":"light","autosave":true}"#;
        let result: std::result::Result<Settings, _> = serde_json::from_str(old_json);
        // This should fail because the fields aren't optional with defaults in serde
        // but we handle defaults in the Tauri command layer (get_settings)
        assert!(result.is_err());
    }

    #[test]
    fn test_settings_json_roundtrip_all_combinations() {
        let combos = vec![
            (true, true),
            (true, false),
            (false, true),
            (false, false),
        ];

        for (tray, autostart) in combos {
            let settings = Settings {
                theme: "system".to_string(),
                autosave: true,
                minimize_to_tray: tray,
                autostart,
            };

            let json = serde_json::to_string(&settings).unwrap();
            let parsed: Settings = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed.minimize_to_tray, tray, "tray mismatch for ({}, {})", tray, autostart);
            assert_eq!(parsed.autostart, autostart, "autostart mismatch for ({}, {})", tray, autostart);
        }
    }

    #[test]
    fn test_settings_json_field_names() {
        let settings = Settings {
            theme: "dark".to_string(),
            autosave: false,
            minimize_to_tray: true,
            autostart: true,
        };

        let json = serde_json::to_string(&settings).unwrap();
        // Verify the JSON field names match what the frontend expects
        assert!(json.contains("\"minimize_to_tray\":true"));
        assert!(json.contains("\"autostart\":true"));
        assert!(json.contains("\"theme\":\"dark\""));
        assert!(json.contains("\"autosave\":false"));
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
        };

        let cloned = settings.clone();
        assert_eq!(cloned.theme, settings.theme);
        assert_eq!(cloned.minimize_to_tray, settings.minimize_to_tray);
        assert_eq!(cloned.autostart, settings.autostart);
    }

    #[test]
    fn test_settings_debug_format() {
        let settings = Settings::default();
        let debug = format!("{:?}", settings);
        assert!(debug.contains("minimize_to_tray"));
        assert!(debug.contains("autostart"));
    }
}
