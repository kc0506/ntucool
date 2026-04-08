use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use futures::StreamExt;

use crate::output::OutputFormat;
use cool_api::generated::endpoints;
use cool_api::generated::models::{File as CoolFile, Folder};

#[derive(Subcommand)]
pub enum FileCommand {
    /// List files in a course folder
    Ls(FileLsArgs),
    /// Download a file
    Download(FileDownloadArgs),
    /// Upload a file
    Upload(FileUploadArgs),
}

#[derive(Parser)]
pub struct FileLsArgs {
    /// Path within the course (default: root)
    pub path: Option<String>,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct FileDownloadArgs {
    /// File ID or path
    pub target: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Output file path
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Parser)]
pub struct FileUploadArgs {
    /// Local file path
    pub path: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Remote destination folder path
    #[arg(long)]
    pub to: Option<String>,
}

pub async fn run(cmd: FileCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        FileCommand::Ls(args) => ls(&client, &args, fmt).await,
        FileCommand::Download(args) => download(&client, &args).await,
        FileCommand::Upload(args) => upload(&client, &args).await,
    }
}

async fn get_root_folder(client: &cool_api::CoolClient, course_id: &str) -> Result<Folder> {
    let folder: Folder = client
        .get(
            &format!("/api/v1/courses/{}/folders/root", course_id),
            None::<&()>,
        )
        .await?;
    Ok(folder)
}

async fn navigate_to_folder(
    client: &cool_api::CoolClient,
    course_id: &str,
    path: &str,
) -> Result<Folder> {
    let root = get_root_folder(client, course_id).await?;

    if path.is_empty() || path == "/" {
        return Ok(root);
    }

    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let mut current_folder = root;

    for part in parts {
        let folder_id = current_folder
            .id
            .ok_or_else(|| anyhow::anyhow!("Folder has no ID"))?;

        // List subfolders of current folder
        let mut subfolders: Vec<Folder> = Vec::new();
        let fid = folder_id.to_string();
        let mut stream = std::pin::pin!(endpoints::list_folders(client, &fid));
        while let Some(item) = stream.next().await {
            subfolders.push(item?);
        }

        let target = subfolders
            .into_iter()
            .find(|f| f.name.as_deref() == Some(part))
            .ok_or_else(|| anyhow::anyhow!("Folder not found: {part}"))?;

        current_folder = target;
    }

    Ok(current_folder)
}

async fn ls(
    client: &cool_api::CoolClient,
    args: &FileLsArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let folder = match &args.path {
        Some(p) => navigate_to_folder(client, &cid, p).await?,
        None => get_root_folder(client, &cid).await?,
    };

    let folder_id = folder
        .id
        .ok_or_else(|| anyhow::anyhow!("Folder has no ID"))?
        .to_string();

    // List subfolders
    let mut subfolders: Vec<Folder> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_folders(client, &folder_id));
    while let Some(item) = stream.next().await {
        subfolders.push(item?);
    }

    // List files
    let file_params = cool_api::generated::params::ListFilesFoldersParams {
        content_types: None,
        exclude_content_types: None,
        search_term: None,
        include: None,
        only: None,
        sort: None,
        order: None,
    };
    let mut files: Vec<CoolFile> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_files_folders(
        client, &folder_id, &file_params
    ));
    while let Some(item) = stream.next().await {
        files.push(item?);
    }

    match fmt {
        OutputFormat::Json => {
            let combined = serde_json::json!({
                "folders": subfolders,
                "files": files,
            });
            println!("{}", serde_json::to_string_pretty(&combined)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["Type", "ID", "Name", "Size"]);

            for f in &subfolders {
                table.add_row(vec![
                    "dir".to_string(),
                    f.id.map(|id| id.to_string()).unwrap_or_default(),
                    f.name.clone().unwrap_or_default(),
                    "-".to_string(),
                ]);
            }

            for f in &files {
                table.add_row(vec![
                    "file".to_string(),
                    f.id.map(|id| id.to_string()).unwrap_or_default(),
                    f.display_name.clone().unwrap_or_default(),
                    f.size
                        .map(format_size)
                        .unwrap_or_else(|| "-".to_string()),
                ]);
            }

            println!("{table}");
        }
    }

    Ok(())
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

async fn download(client: &cool_api::CoolClient, args: &FileDownloadArgs) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    // Try to parse as file ID first
    let file: CoolFile = if args.target.parse::<i64>().is_ok() {
        let params = cool_api::generated::params::GetFileCoursesParams {
            include: None,
            replacement_chain_context_type: None,
            replacement_chain_context_id: None,
        };
        endpoints::get_file_courses(client, &cid, &args.target, &params).await?
    } else {
        // Try path-based lookup: list files in course and find by name
        let file_params = cool_api::generated::params::ListFilesCoursesParams {
            content_types: None,
            exclude_content_types: None,
            search_term: Some(args.target.clone()),
            include: None,
            only: None,
            sort: None,
            order: None,
        };
        let mut files: Vec<CoolFile> = Vec::new();
        let mut stream = std::pin::pin!(endpoints::list_files_courses(
            client, &cid, &file_params
        ));
        while let Some(item) = stream.next().await {
            files.push(item?);
        }
        files
            .into_iter()
            .find(|f| {
                f.display_name.as_deref() == Some(&args.target)
                    || f.filename.as_deref() == Some(&args.target)
            })
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", args.target))?
    };

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| file.display_name.clone().unwrap_or_else(|| "download".to_string()));

    let bytes = cool_api::download::download_file(client, &file, &output_path).await?;
    eprintln!("Downloaded: {output_path} ({bytes} bytes)");

    Ok(())
}

async fn upload(client: &cool_api::CoolClient, args: &FileUploadArgs) -> Result<()> {
    let local_path = std::path::Path::new(&args.path);
    if !local_path.exists() {
        anyhow::bail!("File not found: {}", args.path);
    }

    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let file_name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let file_size = std::fs::metadata(local_path)?.len();

    // Determine parent folder
    let mut step1_body = serde_json::json!({
        "name": file_name,
        "size": file_size,
    });

    if let Some(ref remote_path) = args.to {
        step1_body["parent_folder_path"] = serde_json::Value::String(remote_path.clone());
    }

    // Step 1: Notify Canvas
    let upload_token: cool_api::upload::UploadToken = client
        .post(
            &format!("/api/v1/courses/{}/files", cid),
            &step1_body,
        )
        .await?;

    // Step 2-3: Upload
    let file_obj = cool_api::upload::execute_upload(client, &upload_token, local_path).await?;

    eprintln!(
        "Uploaded: {} (id: {})",
        file_obj.display_name.as_deref().unwrap_or(file_name),
        file_obj.id.map(|id| id.to_string()).unwrap_or_default()
    );

    Ok(())
}
