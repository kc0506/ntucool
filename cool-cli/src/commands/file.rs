use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::output::OutputFormat;

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
    /// File ID or display_name/filename
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

async fn ls(
    client: &cool_api::CoolClient,
    args: &FileLsArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let listing = cool_tools::files::list_in_course(client, &cid, args.path.as_deref()).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&listing)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["Type", "ID", "Name", "Size"]);

            for f in &listing.folders {
                table.add_row(vec![
                    "dir".to_string(),
                    f.id.map(|id| id.to_string()).unwrap_or_default(),
                    f.name.clone().unwrap_or_default(),
                    "-".to_string(),
                ]);
            }
            for f in &listing.files {
                table.add_row(vec![
                    "file".to_string(),
                    f.id.map(|id| id.to_string()).unwrap_or_default(),
                    f.display_name.clone().unwrap_or_default(),
                    f.size.map(format_size).unwrap_or_else(|| "-".to_string()),
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

    let file = cool_tools::files::resolve_in_course(client, &cid, &args.target).await?;

    let output_path = args.output.clone().unwrap_or_else(|| {
        file.display_name
            .clone()
            .unwrap_or_else(|| "download".to_string())
    });

    let bytes = cool_tools::files::download(client, &file, Path::new(&output_path)).await?;
    eprintln!("Downloaded: {output_path} ({bytes} bytes)");
    Ok(())
}

async fn upload(client: &cool_api::CoolClient, args: &FileUploadArgs) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();
    let local_path = Path::new(&args.path);

    let file_obj =
        cool_tools::files::upload_to_course(client, &cid, local_path, args.to.as_deref()).await?;

    eprintln!(
        "Uploaded: {} (id: {})",
        file_obj
            .display_name
            .as_deref()
            .or_else(|| file_obj.filename.as_deref())
            .unwrap_or("?"),
        file_obj.id.map(|id| id.to_string()).unwrap_or_default()
    );

    Ok(())
}
