use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use cool_tools::modules::CanvasModule;

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum ModuleCommand {
    /// List modules for a course
    List(ModuleListArgs),
    /// Show module item details
    Show(ModuleShowArgs),
    /// Download all module files into a folder, preserving module grouping
    Download(ModuleDownloadArgs),
}

#[derive(Parser)]
pub struct ModuleListArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct ModuleShowArgs {
    /// Module ID
    pub id: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct ModuleDownloadArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Output directory (defaults to course id, e.g. ./<course_id>)
    #[arg(short, long)]
    pub output: Option<String>,
    /// Overwrite files that already exist locally
    #[arg(long)]
    pub overwrite: bool,
    /// Max concurrent downloads (kept low to avoid being rate-limited)
    #[arg(short = 'j', long, default_value_t = 4)]
    pub concurrency: usize,
}

pub async fn run(cmd: ModuleCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        ModuleCommand::List(args) => list(&client, &args, fmt).await,
        ModuleCommand::Show(args) => show(&client, &args, fmt).await,
        ModuleCommand::Download(args) => download(&client, &args).await,
    }
}

async fn list(
    client: &cool_api::CoolClient,
    args: &ModuleListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let modules = cool_tools::modules::list_with_items(client, course_id, &["items"]).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&modules)?);
        }
        OutputFormat::Table => {
            for m in &modules {
                let name = m.name.as_deref().unwrap_or("(unnamed module)");
                println!("Module: \"{}\"", name);

                match &m.items {
                    Some(items) if !items.is_empty() => {
                        for item in items {
                            let type_icon = match item.item_type.as_deref().unwrap_or("") {
                                "File" => "📁",
                                "Page" => "📄",
                                "Assignment" => "📝",
                                "Discussion" => "💬",
                                "Quiz" => "❓",
                                "ExternalUrl" | "ExternalTool" => "🔗",
                                "SubHeader" => "──",
                                _ => "  ",
                            };
                            let title = item.title.as_deref().unwrap_or("(untitled)");
                            let itype = item.item_type.as_deref().unwrap_or("Unknown");
                            println!("  {} {}: {}", type_icon, itype, title);
                        }
                    }
                    _ => {
                        println!("  (no items)");
                    }
                }
                println!();
            }
        }
    }

    Ok(())
}

async fn show(
    client: &cool_api::CoolClient,
    args: &ModuleShowArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let module = cool_tools::modules::show_with_items(client, course_id, &args.id).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&module)?);
        }
        OutputFormat::Table => {
            println!("Module: {}", module.name.as_deref().unwrap_or("(unnamed)"));
            println!(
                "ID:     {}",
                module.id.map(|id| id.to_string()).unwrap_or_default()
            );
            println!();

            match &module.items {
                Some(items) if !items.is_empty() => {
                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL_CONDENSED);
                    table.set_header(vec!["Type", "Title", "URL"]);

                    for item in items {
                        table.add_row(vec![
                            item.item_type.clone().unwrap_or_default(),
                            item.title.clone().unwrap_or_default(),
                            item.html_url.clone().unwrap_or_else(|| "-".to_string()),
                        ]);
                    }

                    println!("{table}");
                }
                _ => {
                    println!("(no items)");
                }
            }
        }
    }

    Ok(())
}

/// Sanitize a string so it can safely be used as a single path component.
/// Replaces path separators and other invalid characters with `_`, trims
/// leading/trailing whitespace and dots, and falls back to a placeholder
/// when the result would be empty.
fn sanitize_path_component(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        "_".to_string()
    } else {
        s
    }
}

/// One file-download unit. The `bar` is shared between all jobs of the same
/// module, so its `pos`/`len` count tracks files done within that folder and
/// its `message` shows the file currently being processed.
struct DownloadJob {
    item_title: String,
    content_id: i64,
    module_dir: PathBuf,
    bar: ProgressBar,
}

enum JobOutcome {
    Downloaded,
    Skipped,
    Failed,
}

async fn download(
    client: &cool_api::CoolClient,
    args: &ModuleDownloadArgs,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let output_root: PathBuf = args
        .output
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&cid));

    tokio::fs::create_dir_all(&output_root).await?;

    let concurrency = args.concurrency.max(1);

    let modules: Vec<CanvasModule> =
        cool_tools::modules::list_with_items(client, course_id, &["items"]).await?;
    if modules.is_empty() {
        eprintln!("No modules found for course {cid}.");
        return Ok(());
    }

    // One bar per module folder; collect the file jobs for each in module order.
    let multi = MultiProgress::new();
    let module_style = ProgressStyle::with_template(
        "{prefix:30} {bar:25.cyan/blue} {pos:>3}/{len:<3} {wide_msg}",
    )
    .unwrap()
    .progress_chars("=>-");

    // Per-module job lists, kept in module order. We interleave them
    // round-robin into the final job stream so `buffer_unordered` pulls one
    // file from each module first instead of draining one folder at a time —
    // otherwise all visible bars appear "stuck" until earlier modules finish.
    let mut per_module_jobs: Vec<Vec<DownloadJob>> = Vec::new();
    let mut module_bars: Vec<ProgressBar> = Vec::new();

    for (idx, module) in modules.iter().enumerate() {
        let module_name = module
            .name
            .clone()
            .unwrap_or_else(|| format!("Module {}", idx + 1));

        let items = match &module.items {
            Some(items) if !items.is_empty() => items,
            _ => continue,
        };

        // Only File items contribute to a bar; require content_id to be useful.
        let file_items: Vec<(i64, String)> = items
            .iter()
            .filter(|i| i.item_type.as_deref() == Some("File"))
            .filter_map(|i| {
                let title = i.title.clone().unwrap_or_else(|| "(untitled)".to_string());
                i.content_id.map(|cid| (cid, title))
            })
            .collect();

        if file_items.is_empty() {
            continue;
        }

        let module_dir = output_root.join(sanitize_path_component(&module_name));
        tokio::fs::create_dir_all(&module_dir).await?;

        let bar = multi.add(ProgressBar::new(file_items.len() as u64));
        bar.set_style(module_style.clone());
        bar.set_prefix(truncate_prefix(&module_name, 30));
        bar.enable_steady_tick(std::time::Duration::from_millis(120));
        module_bars.push(bar.clone());

        let module_jobs: Vec<DownloadJob> = file_items
            .into_iter()
            .map(|(content_id, item_title)| DownloadJob {
                item_title,
                content_id,
                module_dir: module_dir.clone(),
                bar: bar.clone(),
            })
            .collect();
        per_module_jobs.push(module_jobs);
    }

    let jobs = interleave_round_robin(per_module_jobs);

    if jobs.is_empty() {
        eprintln!("No file items to download.");
        return Ok(());
    }

    let overall_style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {bar:25.green/blue} {pos:>3}/{len:<3} files {wide_msg}",
    )
    .unwrap()
    .progress_chars("=>-");
    let overall = multi.add(ProgressBar::new(jobs.len() as u64));
    overall.set_style(overall_style);
    overall.set_message(format!("→ {}", output_root.display()));
    overall.enable_steady_tick(std::time::Duration::from_millis(120));

    let overwrite = args.overwrite;
    let cid_ref = &cid;
    let overall_ref = &overall;
    let mut stream = stream::iter(jobs.into_iter().map(|job| async move {
        run_job(client, cid_ref, job, overwrite, overall_ref).await
    }))
    .buffer_unordered(concurrency);

    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    while let Some(outcome) = stream.next().await {
        match outcome {
            JobOutcome::Downloaded => downloaded += 1,
            JobOutcome::Skipped => skipped += 1,
            JobOutcome::Failed => failed += 1,
        }
    }

    for bar in &module_bars {
        bar.finish();
    }
    overall.finish();

    eprintln!(
        "Done. {} downloaded, {} skipped, {} failed → {}",
        downloaded,
        skipped,
        failed,
        output_root.display()
    );

    Ok(())
}

/// Round-robin merge: take one item from each group in order, repeat until
/// every group is empty. Preserves the order *within* a group.
fn interleave_round_robin<T>(groups: Vec<Vec<T>>) -> Vec<T> {
    let total: usize = groups.iter().map(|g| g.len()).sum();
    let mut queues: Vec<VecDeque<T>> = groups.into_iter().map(VecDeque::from).collect();
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        for q in queues.iter_mut() {
            if let Some(item) = q.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

/// Truncate `s` to fit within `max` chars (counting code points, not bytes),
/// appending `…` when truncated. Padding to the prefix width is left to
/// indicatif's format string.
fn truncate_prefix(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let taken: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{taken}…")
    }
}

async fn run_job(
    client: &cool_api::CoolClient,
    course_id: &str,
    job: DownloadJob,
    overwrite: bool,
    overall: &ProgressBar,
) -> JobOutcome {
    job.bar.set_message(job.item_title.clone());

    let file = match cool_tools::files::get_metadata(
        client,
        course_id,
        &job.content_id.to_string(),
    )
    .await
    {
        Ok(f) => f,
        Err(_) => {
            job.bar.set_message(format!("✗ {}", job.item_title));
            job.bar.inc(1);
            overall.inc(1);
            return JobOutcome::Failed;
        }
    };

    let filename = file
        .display_name
        .clone()
        .or_else(|| file.filename.clone())
        .unwrap_or_else(|| job.item_title.clone());
    let dest = job.module_dir.join(sanitize_path_component(&filename));
    job.bar.set_message(filename.clone());

    if !overwrite && dest.exists() {
        job.bar.set_message(format!("· {filename}"));
        job.bar.inc(1);
        overall.inc(1);
        return JobOutcome::Skipped;
    }

    match cool_tools::files::download(client, &file, &dest).await {
        Ok(_) => {
            job.bar.inc(1);
            overall.inc(1);
            JobOutcome::Downloaded
        }
        Err(_) => {
            job.bar.set_message(format!("✗ {filename}"));
            job.bar.inc(1);
            overall.inc(1);
            JobOutcome::Failed
        }
    }
}
