use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum CourseCommand {
    /// List enrolled courses
    List(CourseListArgs),
    /// Show course details
    Info {
        /// Course ID
        id: String,
    },
}

#[derive(Parser)]
pub struct CourseListArgs {
    /// Show courses from all semesters (not just active)
    #[arg(long, conflicts_with = "semester")]
    pub all: bool,

    /// Filter by semester term (e.g. "1131" for 113學年第1學期)
    #[arg(long, conflicts_with = "all")]
    pub semester: Option<String>,
}

pub async fn run(cmd: CourseCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        CourseCommand::List(args) => list_courses(&client, &args, fmt).await,
        CourseCommand::Info { id } => course_info(&client, &id, fmt).await,
    }
}

async fn list_courses(
    client: &cool_api::CoolClient,
    args: &CourseListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let show_term = args.all || args.semester.is_some();

    let courses = if let Some(ref term_filter) = args.semester {
        let cs = cool_tools::courses::list_by_semester(client, term_filter).await?;
        if cs.is_empty() {
            eprintln!(
                "Warning: no courses matched semester '{}'. Term data may not be available.",
                term_filter
            );
        }
        cs
    } else if args.all {
        cool_tools::courses::list_all(client, true).await?
    } else {
        cool_tools::courses::list_active(client).await?
    };

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&courses)?);
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);

            if show_term {
                table.set_header(vec!["ID", "Course Code", "Name", "Term"]);
            } else {
                table.set_header(vec!["ID", "Course Code", "Name"]);
            }

            for c in &courses {
                let mut row = vec![
                    c.id.map(|id| id.to_string()).unwrap_or_default(),
                    c.course_code.clone().unwrap_or_default(),
                    c.name.clone().unwrap_or_default(),
                ];
                if show_term {
                    row.push(
                        c.term
                            .as_ref()
                            .and_then(|t| t.name.clone())
                            .unwrap_or_else(|| "-".to_string()),
                    );
                }
                table.add_row(row);
            }

            println!("{table}");
        }
    }

    Ok(())
}

async fn course_info(
    client: &cool_api::CoolClient,
    id: &str,
    fmt: OutputFormat,
) -> Result<()> {
    let course = cool_tools::courses::show(client, id).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&course)?);
        }
        OutputFormat::Table => {
            println!(
                "ID:          {}",
                course.id.map(|id| id.to_string()).unwrap_or_default()
            );
            println!("Name:        {}", course.name.as_deref().unwrap_or("(unknown)"));
            println!(
                "Course Code: {}",
                course.course_code.as_deref().unwrap_or("(unknown)")
            );
            println!(
                "State:       {}",
                course.workflow_state.as_deref().unwrap_or("(unknown)")
            );
        }
    }

    Ok(())
}

/// Resolve a course specifier (ID or name substring) → course ID.
/// Thin wrapper around `cool_tools::courses::resolve_one` that the other CLI
/// commands call via `super::course::resolve_course`.
pub async fn resolve_course(client: &cool_api::CoolClient, spec: &str) -> Result<i64> {
    cool_tools::courses::resolve_one(client, spec).await
}
