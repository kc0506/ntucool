use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use futures::StreamExt;

use crate::output::OutputFormat;
use cool_api::generated::endpoints;
use cool_api::generated::models::DiscussionTopic;
use cool_api::generated::params::ListDiscussionTopicsCoursesParams;

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

    let params = ListDiscussionTopicsCoursesParams {
        include: None,
        order_by: None,
        scope: None,
        only_announcements: None,
        filter_by: None,
        search_term: None,
        exclude_context_module_locked_topics: None,
    };

    let mut topics: Vec<DiscussionTopic> = Vec::new();
    let mut stream =
        std::pin::pin!(endpoints::list_discussion_topics_courses(client, &cid, &params));
    while let Some(item) = stream.next().await {
        topics.push(item?);
    }

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

    let topic: DiscussionTopic = client
        .get(
            &format!(
                "/api/v1/courses/{}/discussion_topics/{}",
                course_id, args.id
            ),
            None::<&()>,
        )
        .await?;

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
                let text = super::assignment::html_to_text(msg);
                if !text.trim().is_empty() {
                    println!("\n{}", text.trim());
                }
            }
        }
    }

    Ok(())
}
