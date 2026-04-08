use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Session load error: {0}")]
    SessionLoad(String),

    #[error("Session save error: {0}")]
    SessionSave(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Session expired, please run `cool login`")]
    SessionExpired,

    #[error("No credentials found at {0}")]
    NoCredentials(String),

    #[error("Password command failed: {0}")]
    PasswordCmd(String),

    #[error("Upload error: {0}")]
    Upload(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("IO error: {0}")]
    Io(String),
}
