use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use cool_api::config::WriteLevel;
use cool_tools::assignments::{SubmitContent, SubmitOptions};
use cool_tools::types::RiskSeverity;

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
    /// Local file(s) to submit as an online_upload. Repeatable. Omit when using --text.
    pub files: Vec<String>,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
    /// Assignment ID
    #[arg(short, long)]
    pub assignment: String,
    /// Submit a text body (online_text_entry) instead of files.
    #[arg(long, conflicts_with = "files")]
    pub text: Option<String>,
    /// Optional comment delivered to the grader.
    #[arg(long)]
    pub comment: Option<String>,
    /// Acknowledge soft risks (past due / re-submission) and skip the
    /// interactive confirmation. Hard risks still abort.
    #[arg(long)]
    pub i_understand: bool,
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
    let assignment_id: i64 = args.assignment.parse().map_err(|_| {
        anyhow::anyhow!("assignment must be a numeric ID, got `{}`", args.assignment)
    })?;

    // Resolve content from the flags. --text and positional files are
    // mutually exclusive (clap enforces it too); one must be present.
    let content = match (&args.text, args.files.is_empty()) {
        (Some(text), true) => SubmitContent::Text(text.clone()),
        (Some(_), false) => anyhow::bail!("Pass either file path(s) or --text, not both."),
        (None, false) => {
            SubmitContent::Files(args.files.iter().map(PathBuf::from).collect())
        }
        (None, true) => anyhow::bail!("Nothing to submit — give file path(s) or --text."),
    };

    // Resolve the write policy up front so `none` fails fast — before a
    // preflight round-trip and a confirmation prompt.
    let level = cool_api::config::write_level();
    if level == WriteLevel::None {
        anyhow::bail!(
            "Writes are disabled (write_level = none). Enable with env \
             NTUCOOL_WRITE_LEVEL=guarded, or a .ntucool.json containing \
             {{\"write_level\": \"guarded\"}}. See docs/TOOLS.md."
        );
    }

    // Preflight: print exactly what would be submitted, plus every risk.
    // `unguarded` skips the gate — the preflight is shown as information only.
    let pf = cool_tools::assignments::preflight(client, course_id, assignment_id, &content).await?;
    eprintln!(
        "About to submit to: {} (course {}, assignment {})",
        pf.assignment_name, pf.course_id, pf.assignment_id
    );
    eprintln!("Submission type:    {}", pf.submission_type);
    eprintln!("Write level:        {}", level.as_str());
    match &content {
        SubmitContent::Files(fs) => {
            eprintln!("Files ({}):", fs.len());
            for f in fs {
                eprintln!("  - {}", f.display());
            }
        }
        SubmitContent::Text(t) => {
            eprintln!("Text body:          {} char(s)", t.chars().count());
        }
    }

    let hard: Vec<_> = pf
        .risks
        .iter()
        .filter(|r| r.severity == RiskSeverity::Hard)
        .collect();
    let soft: Vec<_> = pf
        .risks
        .iter()
        .filter(|r| r.severity == RiskSeverity::Soft)
        .collect();

    if !hard.is_empty() {
        eprintln!("\n{} issue(s) Canvas would reject:", hard.len());
        for r in &hard {
            eprintln!("  ✗ [{}] {}", r.code, r.message);
        }
        // `unguarded` sends anyway and lets Canvas be the judge.
        if level != WriteLevel::Unguarded {
            anyhow::bail!("submission refused ({} blocking issue(s))", hard.len());
        }
    }

    if !soft.is_empty() {
        eprintln!("\nRisks:");
        for r in &soft {
            eprintln!("  ! [{}] {}", r.code, r.message);
        }
        // `safe` blocks every risky submission outright.
        if level == WriteLevel::Safe {
            anyhow::bail!(
                "submission refused — write_level is `safe`; raise it to `guarded` \
                 (and pass --i-understand) to submit despite the risk(s) above"
            );
        }
    }

    // Confirm. `unguarded` already opted out of guards; otherwise --i-understand
    // skips the prompt. On a non-TTY the prompt errors → treated as "no" → abort.
    if level != WriteLevel::Unguarded && !args.i_understand {
        let prompt = if soft.is_empty() {
            "Submit now? (this is irreversible)".to_string()
        } else {
            format!("Submit anyway, accepting the {} risk(s) above?", soft.len())
        };
        let ok = dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()
            .unwrap_or(false);
        if !ok {
            eprintln!("Aborted — nothing submitted.");
            return Ok(());
        }
    }

    // CLI has already gated above, so soft risks are acknowledged here.
    let opts = SubmitOptions {
        comment: args.comment.clone(),
        i_understand: true,
    };
    let receipt =
        cool_tools::assignments::submit(client, course_id, assignment_id, &content, &opts).await?;

    eprintln!("\n✓ Submitted.");
    eprintln!(
        "  workflow_state: {}",
        receipt.workflow_state.as_deref().unwrap_or("?")
    );
    if let Some(at) = &receipt.submitted_at {
        eprintln!("  submitted_at:   {at}");
    }
    if let Some(n) = receipt.attempt {
        eprintln!("  attempt:        {n}");
    }
    if receipt.late == Some(true) {
        eprintln!("  late:           yes");
    }
    if let Some(u) = &receipt.preview_url {
        eprintln!("  preview:        {u}");
    }
    Ok(())
}
