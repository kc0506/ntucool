use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use futures::StreamExt;

use crate::output::OutputFormat;
use cool_api::generated::endpoints;
use cool_api::generated::params::ListAssignmentsAssignmentsParams;

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

    let params = ListAssignmentsAssignmentsParams {
        include: None,
        search_term: None,
        override_assignment_dates: None,
        needs_grading_count_by_section: None,
        bucket: None,
        assignment_ids: None,
        order_by: None,
        post_to_sis: None,
        new_quizzes: None,
    };

    let mut assignments = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_assignments_assignments(
        client, &cid, &params
    ));
    while let Some(item) = stream.next().await {
        assignments.push(item?);
    }

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

    // Use get endpoint for single assignment
    let assignment: cool_api::generated::models::Assignment = client
        .get(
            &format!("/api/v1/courses/{}/assignments/{}", cid, args.id),
            None::<&()>,
        )
        .await?;

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
                let text = html_to_text(desc);
                if !text.trim().is_empty() {
                    println!("Description:\n{}", text.trim());
                }
            }
        }
    }

    Ok(())
}

/// Convert HTML to plain text using scraper for proper parsing,
/// with basic block-element handling for readability.
pub fn html_to_text(html: &str) -> String {
    let document = scraper::Html::parse_fragment(html);
    let mut result = String::new();
    extract_text(&document.tree.root(), &mut result);
    // Collapse runs of 3+ newlines into 2
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result
}

fn extract_text(node: &ego_tree::NodeRef<scraper::Node>, out: &mut String) {
    for child in node.children() {
        match child.value() {
            scraper::Node::Text(text) => out.push_str(text),
            scraper::Node::Element(el) => {
                let tag = el.name();
                let is_block = matches!(
                    tag,
                    "p" | "div" | "br" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                        | "li" | "tr" | "blockquote" | "pre"
                );
                if tag == "br" {
                    out.push('\n');
                }
                if is_block && tag != "br" && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                extract_text(&child, out);
                if is_block && tag != "br" {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
}

async fn submit(client: &cool_api::CoolClient, args: &AssignmentSubmitArgs) -> Result<()> {
    let local_path = std::path::Path::new(&args.file);
    if !local_path.exists() {
        anyhow::bail!("File not found: {}", args.file);
    }

    let course_id = super::course::resolve_course(client, &args.course).await?;
    let cid = course_id.to_string();

    // Step 1: Notify Canvas about the upload
    let file_name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let file_size = std::fs::metadata(local_path)?.len();

    let step1_body = serde_json::json!({
        "name": file_name,
        "size": file_size,
    });

    let upload_token: cool_api::upload::UploadToken = client
        .post(
            &format!(
                "/api/v1/courses/{}/assignments/{}/submissions/self/files",
                cid, args.assignment
            ),
            &step1_body,
        )
        .await?;

    // Step 2-3: Upload file and get File object
    let file_obj = cool_api::upload::execute_upload(client, &upload_token, local_path).await?;

    let file_id = file_obj
        .id
        .ok_or_else(|| anyhow::anyhow!("Upload succeeded but no file ID returned"))?;

    // Step 4: Submit the assignment
    let submit_params = cool_api::generated::params::SubmitAssignmentCoursesParams {
        comment_text_comment: None,
        submission_group_comment: None,
        submission_submission_type: Some("online_upload".to_string()),
        submission_body: None,
        submission_url: None,
        submission_file_ids: Some(vec![file_id]),
        submission_media_comment_id: None,
        submission_media_comment_type: None,
        submission_user_id: None,
        submission_annotatable_attachment_id: None,
        submission_submitted_at: None,
    };

    endpoints::submit_assignment_courses(client, &cid, &args.assignment, &submit_params).await?;

    eprintln!("Submitted {} for assignment {}.", args.file, args.assignment);

    Ok(())
}
