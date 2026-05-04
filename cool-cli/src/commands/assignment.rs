use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::output::OutputFormat;

#[derive(Parser)]
pub struct AssignmentArgs {
    #[command(subcommand)]
    pub command: Option<AssignmentCommand>,
}

#[derive(Subcommand)]
pub enum AssignmentCommand {
    /// List assignments for a course
    List(AssignmentListArgs),
    /// Show assignment details
    Info(AssignmentInfoArgs),
    /// Submit a file for an assignment
    Submit(AssignmentSubmitArgs),
}

#[derive(Parser)]
pub struct AssignmentListArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Filter by Canvas bucket: upcoming|past|overdue|undated|ungraded|unsubmitted|future
    #[arg(long)]
    pub bucket: Option<String>,
}

#[derive(Parser)]
pub struct AssignmentInfoArgs {
    /// Assignment ID
    pub id: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct AssignmentSubmitArgs {
    /// Local file to submit
    pub file: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Assignment ID
    #[arg(short, long)]
    pub assignment: String,
}

pub async fn run(args: AssignmentArgs, opts: &super::GlobalOpts) -> Result<()> {
    let Some(cmd) = args.command else {
        let client = super::get_client()?;
        return crate::tui::browser::run_browser(&client).await;
    };

    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        AssignmentCommand::List(args) => list(&client, &args, fmt).await,
        AssignmentCommand::Info(args) => info(&client, &args, fmt).await,
        AssignmentCommand::Submit(args) => submit(&client, &args).await,
    }
}

async fn list(
    client: &cool_api::CoolClient,
    args: &AssignmentListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let filter = cool_tools::assignments::ListFilter {
        bucket: args.bucket.clone(),
        ..Default::default()
    };
    let assignments = cool_tools::assignments::list(client, &cid, filter).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&assignments)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["ID", "Name", "Due", "Points"]);

            for a in &assignments {
                table.add_row(vec![
                    a.id.map(|id| id.to_string()).unwrap_or_default(),
                    a.name.clone().unwrap_or_default(),
                    a.due_at
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    a.points_possible
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                ]);
            }

            println!("{table}");
        }
    }

    Ok(())
}

async fn info(
    client: &cool_api::CoolClient,
    args: &AssignmentInfoArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    let assignment = cool_tools::assignments::show(client, &cid, &args.id).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&assignment)?);
        }
        OutputFormat::Table => {
            println!(
                "ID:          {}",
                assignment.id.map(|id| id.to_string()).unwrap_or_default()
            );
            println!(
                "Name:        {}",
                assignment.name.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "Due:         {}",
                assignment
                    .due_at
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            println!(
                "Points:      {}",
                assignment
                    .points_possible
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            if let Some(ref desc) = assignment.description {
                let text = cool_tools::text::html_to_text(desc);
                if !text.trim().is_empty() {
                    println!("Description:\n{}", text.trim());
                }
            }
        }
    }

    Ok(())
}

async fn submit(client: &cool_api::CoolClient, args: &AssignmentSubmitArgs) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    cool_tools::assignments::submit_file(client, &cid, &args.assignment, Path::new(&args.file))
        .await?;

    eprintln!("Submitted {} for assignment {}.", args.file, args.assignment);
    Ok(())
}
