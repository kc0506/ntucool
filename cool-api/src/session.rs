use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub base_url: String,
    pub cookies: HashMap<String, String>,
}

impl Session {
    pub fn new(base_url: String, cookies: HashMap<String, String>) -> Self {
        Self {
            version: 1,
            created_at: Utc::now(),
            base_url,
            cookies,
        }
    }

    /// Default session file path: $XDG_DATA_HOME/ntucool/session.json
    pub fn default_path() -> PathBuf {
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").expect("HOME not set");
                PathBuf::from(home).join(".local/share")
            });
        data_home.join("ntucool").join("session.json")
    }

    pub fn load(path: &PathBuf) -> Result<Self, Error> {
        let data = fs::read_to_string(path).map_err(|e| Error::SessionLoad(e.to_string()))?;
        let session: Session =
            serde_json::from_str(&data).map_err(|e| Error::SessionLoad(e.to_string()))?;
        Ok(session)
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::SessionSave(e.to_string()))?;
        }

        let data =
            serde_json::to_string_pretty(self).map_err(|e| Error::SessionSave(e.to_string()))?;
        fs::write(path, &data).map_err(|e| Error::SessionSave(e.to_string()))?;

        // Set file permissions to 0600
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| Error::SessionSave(e.to_string()))?;

        Ok(())
    }
}
