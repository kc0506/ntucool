use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Empirical TTL for NTU COOL's session cookie. Canvas's `_normandy_session`
/// (and `canvas_session`) is set with no `Max-Age`, but in practice ADFS
/// SAML logins are valid for ~24 hours before re-auth is required. Past this
/// boundary, calls 401 with the standard ADFS-redirect HTML body.
pub const SESSION_HARD_TTL_HOURS: i64 = 24;

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

    /// Whole hours since this session was created. Useful for surfacing a
    /// "your session is X hours old" hint without doing the math at every
    /// call site.
    pub fn age_hours(&self) -> i64 {
        (Utc::now() - self.created_at).num_hours()
    }

    /// Heuristic: true if the session was created more than 24 hours ago.
    /// We don't actually know the cookie's real expiry — Canvas doesn't
    /// emit `Max-Age` — so this is best-effort. Past this point, callers
    /// should expect a re-login and surface that to the user *before*
    /// firing requests that 401 deep inside the API client.
    pub fn is_likely_expired(&self) -> bool {
        Utc::now() - self.created_at > Duration::hours(SESSION_HARD_TTL_HOURS)
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
