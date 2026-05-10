use anyhow::Result;
use clap::{Args, Subcommand};

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum SubmissionCommand {
    /// List my own submissions (across one or all active courses)
    Mine(SubmissionMineArgs),
}

#[derive(Args)]
pub struct SubmissionMineArgs {
    /// Optional course filter. Omit to fan out across every active enrolment.
    #[arg(long)]
    pub course: Option<i64>,
    /// workflow_state filter: "submitted" / "unsubmitted" / "graded" / "pending_review".
    #[arg(long)]
    pub status: Option<String>,
}

pub async fn run(cmd: SubmissionCommand, opts: &super::GlobalOpts) -> Result<()> {
    match cmd {
        SubmissionCommand::Mine(args) => mine(args, opts).await,
    }
}

async fn mine(args: SubmissionMineArgs, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let workflow = args
        .status
        .as_deref()
        .and_then(cool_tools::submissions::WorkflowFilter::parse);
    let subs = match args.course {
        Some(cid) => cool_tools::submissions::submissions_mine_in_course(&client, cid, workflow).await?,
        None => cool_tools::submissions::submissions_mine_all(&client, workflow).await?,
    };

    let fmt = OutputFormat::from_flag(opts.json);
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&subs)?),
        OutputFormat::Table => {
            if subs.is_empty() {
                println!("(no submissions matched)");
                return Ok(());
            }
            for s in &subs {
                let name = s.assignment_name.as_deref().unwrap_or("(unknown)");
                let score = match (s.score, s.points_possible) {
                    (Some(sc), Some(pp)) => format!("{}/{}", sc, pp),
                    (Some(sc), None) => format!("{}", sc),
                    (None, Some(pp)) => format!("—/{}", pp),
                    (None, None) => "—".to_string(),
                };
                let state = s.workflow_state.as_deref().unwrap_or("?");
                let flags = [
                    s.late.unwrap_or(false).then_some("late"),
                    s.missing.unwrap_or(false).then_some("missing"),
                    s.excused.unwrap_or(false).then_some("excused"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(",");
                let flag_suffix = if flags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", flags)
                };
                println!(
                    "[c{} a{}] {} — {} ({}){}",
                    s.course_id, s.assignment_id, name, score, state, flag_suffix
                );
            }
        }
    }
    Ok(())
}
