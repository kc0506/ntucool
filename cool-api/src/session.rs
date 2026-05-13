use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
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
        crate::paths::session_path()
    }

    /// Remove session.json. Idempotent (NotFound is success). Used by `cool logout`.
    pub fn delete_default() -> Result<(), Error> {
        match fs::remove_file(Self::default_path()) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::SessionSave(e.to_string())),
        }
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

        // 0600 on Unix; Windows relies on the user-profile ACL of the
        // parent dir, which is already user-private under $APPDATA.
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(path, perms).map_err(|e| Error::SessionSave(e.to_string()))?;
        }

        Ok(())
    }
}
