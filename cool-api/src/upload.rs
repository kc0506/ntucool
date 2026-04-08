use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::client::CoolClient;
use crate::error::Error;
use crate::generated::models::File;

/// Response from Canvas Step 1 (file upload notification).
#[derive(Debug, Deserialize)]
pub struct UploadToken {
    pub upload_url: String,
    pub upload_params: HashMap<String, String>,
    pub file_param: Option<String>,
}

/// Execute Steps 2-3 of the Canvas file upload protocol.
///
/// Step 2: multipart POST to `upload_url` with `upload_params` + the file.
/// Step 3: parse the response (handling `while(1);` prefix) and return a `File`.
pub async fn execute_upload(
    _client: &CoolClient,
    token: &UploadToken,
    local_path: &Path,
) -> Result<File, Error> {
    let file_param = token
        .file_param
        .as_deref()
        .unwrap_or("file");

    let file_name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let file_bytes = tokio::fs::read(local_path)
        .await
        .map_err(|e| Error::Io(format!("Failed to read file: {e}")))?;

    let mut form = reqwest::multipart::Form::new();

    // Add upload_params first (order matters for S3-style uploads)
    for (key, value) in &token.upload_params {
        form = form.text(key.clone(), value.clone());
    }

    // Add the file
    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("application/octet-stream")
        .map_err(|e| Error::Upload(format!("MIME error: {e}")))?;
    form = form.part(file_param.to_string(), part);

    let http = reqwest::Client::new();
    let resp = http
        .post(&token.upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| Error::Upload(format!("Upload POST failed: {e}")))?;

    let body = resp
        .text()
        .await
        .map_err(|e| Error::Upload(format!("Upload response read failed: {e}")))?;

    // Handle `while(1);` prefix that Canvas sometimes adds
    let json_str = body
        .strip_prefix("while(1);")
        .unwrap_or(&body);

    let file: File =
        serde_json::from_str(json_str).map_err(|e| Error::Upload(format!("Upload parse failed: {e}")))?;

    Ok(file)
}
