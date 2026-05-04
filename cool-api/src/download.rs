use std::path::Path;

use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::client::CoolClient;
use crate::error::Error;
use crate::generated::models::File;

/// Download a file from Canvas to a local path using streaming.
pub async fn download_file(
    client: &CoolClient,
    file: &File,
    dest: &str,
) -> Result<u64, Error> {
    download_file_with_progress(client, file, dest, |_| {}).await
}

/// Download a file from Canvas, calling `on_chunk` with the byte count of
/// each streamed chunk as it is written to disk. Useful for driving a
/// progress bar without coupling cool-api to any UI library.
pub async fn download_file_with_progress(
    client: &CoolClient,
    file: &File,
    dest: &str,
    mut on_chunk: impl FnMut(u64),
) -> Result<u64, Error> {
    let url = file
        .url
        .as_ref()
        .ok_or_else(|| Error::Download("File has no download URL".into()))?;

    let session = client.session().await;
    let cookie_header: String = session
        .cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");

    let http = reqwest::Client::builder()
        .user_agent(crate::auth::USER_AGENT)
        .build()
        .map_err(|e| Error::Download(format!("Build HTTP client failed: {e}")))?;
    let resp = http
        .get(url)
        .header("Cookie", &cookie_header)
        .send()
        .await
        .map_err(|e| Error::Download(format!("Download failed: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Download(format!("Download HTTP error: {e}")))?;

    let dest_path = Path::new(dest);
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Io(format!("Create dir failed: {e}")))?;
        }
    }

    let mut file_out = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| Error::Io(format!("Create file failed: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Download(format!("Stream read error: {e}")))?;
        file_out
            .write_all(&chunk)
            .await
            .map_err(|e| Error::Io(format!("Write file failed: {e}")))?;
        let n = chunk.len() as u64;
        written += n;
        on_chunk(n);
    }

    file_out
        .flush()
        .await
        .map_err(|e| Error::Io(format!("Flush file failed: {e}")))?;

    Ok(written)
}
