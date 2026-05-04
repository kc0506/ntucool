use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum DiscussionCommand {
    /// List discussion topics for a course
    List(DiscussionListArgs),
    /// Show discussion topic details
    Show(DiscussionShowArgs),
}

#[derive(Parser)]
pub struct DiscussionListArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct DiscussionShowArgs {
    /// Discussion topic ID
    pub id: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

pub async fn run(cmd: DiscussionCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        DiscussionCommand::List(args) => list(&client, &args, fmt).await,
        DiscussionCommand::Show(args) => show(&client, &args, fmt).await,
    }
}

async fn list(
    client: &cool_api::CoolClient,
    args: &DiscussionListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();
    let topics = cool_tools::discussions::list(client, &cid).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&topics)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["ID", "Title", "Posted At", "Replies"]);

            for t in &topics {
                table.add_row(vec![
                    t.id.map(|id| id.to_string()).unwrap_or_default(),
                    t.title.clone().unwrap_or_default(),
                    t.posted_at
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    t.discussion_subentry_count
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "0".to_string()),
                ]);
            }

            println!("{table}");
        }
    }

    Ok(())
}

async fn show(
    client: &cool_api::CoolClient,
    args: &DiscussionShowArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();
    let topic = cool_tools::discussions::show(client, &cid, &args.id).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&topic)?);
        }
        OutputFormat::Table => {
            println!(
                "ID:        {}",
                topic.id.map(|id| id.to_string()).unwrap_or_default()
            );
            println!(
                "Title:     {}",
                topic.title.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "Author:    {}",
                topic.user_name.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "Posted At: {}",
                topic
                    .posted_at
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            println!(
                "Replies:   {}",
                topic
                    .discussion_subentry_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "0".to_string())
            );
            if let Some(ref msg) = topic.message {
                let text = cool_tools::text::html_to_text(msg);
                if !text.trim().is_empty() {
                    println!("\n{}", text.trim());
                }
            }
        }
    }

    Ok(())
}
