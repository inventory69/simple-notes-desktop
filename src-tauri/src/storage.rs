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
    pub theme: String,        // "light", "dark", "system"
    pub autosave: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            autosave: true,
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
}
