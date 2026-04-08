use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::output::OutputFormat;
use cool_api::generated::models::DiscussionTopic;

#[derive(Subcommand)]
pub enum AnnouncementCommand {
    /// List announcements for a course
    List(AnnouncementListArgs),
    /// Show announcement details
    Show(AnnouncementShowArgs),
}

#[derive(Parser)]
pub struct AnnouncementListArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct AnnouncementShowArgs {
    /// Announcement (discussion topic) ID
    pub id: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

pub async fn run(cmd: AnnouncementCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        AnnouncementCommand::List(args) => list(&client, &args, fmt).await,
        AnnouncementCommand::Show(args) => show(&client, &args, fmt).await,
    }
}

async fn list(
    client: &cool_api::CoolClient,
    args: &AnnouncementListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;

    // Manual pagination: ListAnnouncementsParams has context_codes: Vec<String>
    // which serde_urlencoded cannot serialize. Use tuple query params instead.
    let context_code = format!("course_{}", course_id);
    let query = [("context_codes[]", context_code.as_str()), ("per_page", "50")];

    let mut announcements: Vec<DiscussionTopic> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/announcements");
        let page: cool_api::client::PaginatedResponse<DiscussionTopic> =
            client.get_paginated(path, Some(&query)).await?;
        announcements.extend(page.items);
        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&announcements)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["ID", "Title", "Posted At"]);

            for a in &announcements {
                table.add_row(vec![
                    a.id.map(|id| id.to_string()).unwrap_or_default(),
                    a.title.clone().unwrap_or_default(),
                    a.posted_at
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "-".to_string()),
                ]);
            }

            println!("{table}");
        }
    }

    Ok(())
}

async fn show(
    client: &cool_api::CoolClient,
    args: &AnnouncementShowArgs,
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
