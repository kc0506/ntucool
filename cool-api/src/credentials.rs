//! Saved login credentials for non-interactive re-authentication.
//!
//! Schema (`credentials.json`, mode 0600):
//! ```json
//! { "username": "...", "password_cmd": "pass show ntucool/password" }
//! ```
//!
//! `password` (plaintext) is supported but discouraged; `password_cmd` is the
//! canonical path. The shell command must print the password on stdout —
//! `Credentials::resolve_password` runs it through `sh -c`, trims, and
//! rejects obvious placeholder values (a recurring footgun left over from
//! when `cool login` wrote `echo 'TODO: ...'` as a default).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::paths;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Credentials {
    pub username: String,
    /// Plaintext password. Stored only if the user explicitly opts in.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub password: Option<String>,
    /// Shell command whose stdout becomes the password. Preferred path —
    /// integrates with `pass`, `op`, `secret-tool`, etc.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub password_cmd: Option<String>,
}

impl Credentials {
    pub fn default_path() -> PathBuf {
        paths::credentials_path()
    }

    pub fn load() -> Result<Self, Error> {
        Self::load_from(&Self::default_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, Error> {
        let data = fs::read_to_string(path)
            .map_err(|_| Error::NoCredentials(path.display().to_string()))?;
        let creds: Self =
            serde_json::from_str(&data).map_err(|e| Error::Auth(e.to_string()))?;
        Ok(creds)
    }

    /// Atomic write to the canonical path, mode 0600. Returns the path
    /// written for the caller to surface.
    pub fn save(&self) -> Result<PathBuf, Error> {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self).map_err(Error::Json)?;
        fs::write(&tmp, &data).map_err(|e| Error::Io(e.to_string()))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::Io(e.to_string()))?;
        fs::rename(&tmp, &path).map_err(|e| Error::Io(e.to_string()))?;
        Ok(path)
    }

    /// Remove credentials.json. Idempotent (NotFound is success).
    pub fn delete() -> Result<(), Error> {
        let path = Self::default_path();
        match fs::remove_file(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io(e.to_string())),
        }
    }

    /// Resolve the actual password — either the plaintext field, or run
    /// `password_cmd` and capture stdout.
    ///
    /// Rejects obvious placeholder values BEFORE invoking the shell, and
    /// re-checks the captured output for the same. This guards against the
    /// historical bug where `cool login` saved a literal
    /// `echo 'TODO: replace ...'` placeholder, which exited 0 and quietly
    /// passed the string `"TODO: ..."` through as the password.
    pub fn resolve_password(&self) -> Result<String, Error> {
        if let Some(ref pw) = self.password {
            if pw.is_empty() {
                return Err(Error::PasswordCmd("password field is empty".into()));
            }
            return Ok(pw.clone());
        }
        let cmd = self.password_cmd.as_deref().ok_or_else(|| {
            Error::Auth("credentials.json has neither password nor password_cmd".into())
        })?;

        if is_placeholder_cmd(cmd) {
            return Err(Error::PasswordCmd(format!(
                "password_cmd looks like a placeholder ({cmd:?}); run `cool login` to set a real command or `cool logout --purge` to clear it"
            )));
        }

        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| Error::PasswordCmd(e.to_string()))?;
        if !output.status.success() {
            return Err(Error::PasswordCmd(format!(
                "command `{}` exited with {} (stderr: {})",
                cmd,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let pw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if pw.is_empty() {
            return Err(Error::PasswordCmd(
                "password_cmd succeeded but produced no output".into(),
            ));
        }
        if looks_like_placeholder_value(&pw) {
            return Err(Error::PasswordCmd(format!(
                "password_cmd output {pw:?} looks like a placeholder; refusing to send to ADFS"
            )));
        }
        Ok(pw)
    }
}

fn is_placeholder_cmd(cmd: &str) -> bool {
    let upper = cmd.to_uppercase();
    upper.contains("TODO") || upper.contains("PLACEHOLDER") || upper.contains("REPLACE WITH")
}

fn looks_like_placeholder_value(pw: &str) -> bool {
    let upper = pw.to_uppercase();
    upper.starts_with("TODO") || upper.contains("PLACEHOLDER") || upper.contains("REPLACE WITH")
}
