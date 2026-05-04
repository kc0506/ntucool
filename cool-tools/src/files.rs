//! Files & folders — listing, navigation, search, download, upload.

use std::path::Path;

use anyhow::Result;
use futures::StreamExt;

use cool_api::generated::endpoints;
pub use cool_api::generated::models::{File, Folder};
use cool_api::generated::params::{
    GetFileCoursesParams, ListFilesCoursesParams, ListFilesFoldersParams,
};
use cool_api::CoolClient;

/// Combined contents of a folder: subfolders + files.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FolderListing {
    pub folders: Vec<Folder>,
    pub files: Vec<File>,
}

/// Get the root folder for a course.
pub async fn root_folder(client: &CoolClient, course_id: &str) -> Result<Folder> {
    let folder: Folder = client
        .get(
            &format!("/api/v1/courses/{}/folders/root", course_id),
            None::<&()>,
        )
        .await?;
    Ok(folder)
}

/// Walk a `/`-separated folder path from the course root.
pub async fn navigate_to_folder(
    client: &CoolClient,
    course_id: &str,
    path: &str,
) -> Result<Folder> {
    let root = root_folder(client, course_id).await?;
    if path.is_empty() || path == "/" {
        return Ok(root);
    }

    let mut current = root;
    for part in path.trim_matches('/').split('/') {
        let folder_id = current
            .id
            .ok_or_else(|| anyhow::anyhow!("Folder has no ID"))?;
        let fid = folder_id.to_string();

        let mut subfolders: Vec<Folder> = Vec::new();
        let mut stream = std::pin::pin!(endpoints::list_folders(client, &fid));
        while let Some(item) = stream.next().await {
            subfolders.push(item?);
        }

        current = subfolders
            .into_iter()
            .find(|f| f.name.as_deref() == Some(part))
            .ok_or_else(|| anyhow::anyhow!("Folder not found: {part}"))?;
    }

    Ok(current)
}

/// List a single folder's contents (subfolders + files).
pub async fn list_folder(client: &CoolClient, folder_id: &str) -> Result<FolderListing> {
    let mut folders: Vec<Folder> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_folders(client, folder_id));
    while let Some(item) = stream.next().await {
        folders.push(item?);
    }

    let params = ListFilesFoldersParams {
        content_types: None,
        exclude_content_types: None,
        search_term: None,
        include: None,
        only: None,
        sort: None,
        order: None,
    };
    let mut files: Vec<File> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_files_folders(
        client, folder_id, &params
    ));
    while let Some(item) = stream.next().await {
        files.push(item?);
    }

    Ok(FolderListing { folders, files })
}

/// List the contents of a folder within a course by `/`-path.
/// `path = None` (or empty / "/") returns the root folder's contents.
pub async fn list_in_course(
    client: &CoolClient,
    course_id: &str,
    path: Option<&str>,
) -> Result<FolderListing> {
    let folder = match path {
        Some(p) if !p.is_empty() && p != "/" => navigate_to_folder(client, course_id, p).await?,
        _ => root_folder(client, course_id).await?,
    };
    let folder_id = folder
        .id
        .ok_or_else(|| anyhow::anyhow!("Folder has no ID"))?
        .to_string();
    list_folder(client, &folder_id).await
}

/// Server-side filename search across a course (`search_term`).
///
/// Canvas requires `search_term` to be at least 3 bytes; shorter queries
/// return HTTP 400. We surface a clear error rather than the raw HTTP one.
pub async fn search(client: &CoolClient, course_id: &str, query: &str) -> Result<Vec<File>> {
    if query.as_bytes().len() < 3 {
        anyhow::bail!("search query too short (Canvas requires at least 3 bytes)");
    }
    let params = ListFilesCoursesParams {
        content_types: None,
        exclude_content_types: None,
        search_term: Some(query.to_string()),
        include: None,
        only: None,
        sort: None,
        order: None,
    };
    let mut files: Vec<File> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_files_courses(
        client, course_id, &params
    ));
    while let Some(item) = stream.next().await {
        files.push(item?);
    }
    Ok(files)
}

/// Get a single file's metadata by ID, scoped to a course.
pub async fn get_metadata(
    client: &CoolClient,
    course_id: &str,
    file_id: &str,
) -> Result<File> {
    let params = GetFileCoursesParams::default();
    let file = endpoints::get_file_courses(client, course_id, file_id, &params).await?;
    Ok(file)
}

/// Download a file by `File` object (must have `url` populated by Canvas).
/// Returns bytes written.
pub async fn download(client: &CoolClient, file: &File, dest: &Path) -> Result<u64> {
    let dest_str = dest.to_string_lossy().to_string();
    let bytes = cool_api::download::download_file(client, file, &dest_str).await?;
    Ok(bytes)
}

/// Convenience: fetch metadata then download. `dest = None` uses
/// `display_name` in the current directory.
pub async fn download_by_id(
    client: &CoolClient,
    course_id: &str,
    file_id: &str,
    dest: Option<&Path>,
) -> Result<(File, u64)> {
    let file = get_metadata(client, course_id, file_id).await?;
    let dest_buf = match dest {
        Some(p) => p.to_path_buf(),
        None => std::path::PathBuf::from(
            file.display_name
                .clone()
                .unwrap_or_else(|| "download".to_string()),
        ),
    };
    let bytes = download(client, &file, &dest_buf).await?;
    Ok((file, bytes))
}

/// Resolve `target` (numeric ID or display_name/filename) to a `File` within
/// a course. For non-numeric inputs uses server-side `search_term`.
pub async fn resolve_in_course(
    client: &CoolClient,
    course_id: &str,
    target: &str,
) -> Result<File> {
    if target.parse::<i64>().is_ok() {
        return get_metadata(client, course_id, target).await;
    }

    let params = ListFilesCoursesParams {
        content_types: None,
        exclude_content_types: None,
        search_term: Some(target.to_string()),
        include: None,
        only: None,
        sort: None,
        order: None,
    };
    let mut files: Vec<File> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_files_courses(
        client, course_id, &params
    ));
    while let Some(item) = stream.next().await {
        files.push(item?);
    }
    files
        .into_iter()
        .find(|f| {
            f.display_name.as_deref() == Some(target) || f.filename.as_deref() == Some(target)
        })
        .ok_or_else(|| anyhow::anyhow!("File not found: {target}"))
}

/// Two-step upload: notify Canvas, then PUT the bytes.
pub async fn upload_to_course(
    client: &CoolClient,
    course_id: &str,
    local_path: &Path,
    dest_folder: Option<&str>,
) -> Result<File> {
    if !local_path.exists() {
        anyhow::bail!("File not found: {}", local_path.display());
    }

    let file_name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let file_size = std::fs::metadata(local_path)?.len();

    let mut step1_body = serde_json::json!({
        "name": file_name,
        "size": file_size,
    });
    if let Some(folder) = dest_folder {
        step1_body["parent_folder_path"] = serde_json::Value::String(folder.to_string());
    }

    let upload_token: cool_api::upload::UploadToken = client
        .post(&format!("/api/v1/courses/{}/files", course_id), &step1_body)
        .await?;

    let file_obj = cool_api::upload::execute_upload(client, &upload_token, local_path).await?;
    Ok(file_obj)
}
