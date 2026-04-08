use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use futures::StreamExt;

use crate::output::OutputFormat;
use cool_api::generated::endpoints;
use cool_api::generated::models::Course;
use cool_api::generated::params::ListYourCoursesParams;

const COURSE_CACHE_TTL_SECS: i64 = 3600; // 1 hour

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
        fetch_courses_by_semester(client, term_filter).await?
    } else if args.all {
        fetch_all_courses(client, true).await?
    } else {
        fetch_courses_cached(client).await?
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

async fn course_info(client: &cool_api::CoolClient, id: &str, fmt: OutputFormat) -> Result<()> {
    let params = cool_api::generated::params::GetSingleCourseCoursesParams {
        include: None,
        teacher_limit: None,
    };
    let course = endpoints::get_single_course_courses(client, id, &params).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&course)?);
        }
        OutputFormat::Table => {
            println!("ID:          {}", course.id.map(|id| id.to_string()).unwrap_or_default());
            println!("Name:        {}", course.name.as_deref().unwrap_or("(unknown)"));
            println!("Course Code: {}", course.course_code.as_deref().unwrap_or("(unknown)"));
            println!(
                "State:       {}",
                course.workflow_state.as_deref().unwrap_or("(unknown)")
            );
        }
    }

    Ok(())
}

/// Resolve a course specifier (ID or name substring) to a course ID.
pub async fn resolve_course(client: &cool_api::CoolClient, spec: &str) -> Result<i64> {
    // If it's a number, use directly
    if let Ok(id) = spec.parse::<i64>() {
        return Ok(id);
    }

    // Otherwise, fuzzy match by name/course_code (with cache)
    let courses = fetch_courses_cached(client).await?;

    let spec_lower = spec.to_lowercase();
    let matches: Vec<&Course> = courses
        .iter()
        .filter(|c| {
            let name_match = c
                .name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&spec_lower))
                .unwrap_or(false);
            let code_match = c
                .course_code
                .as_ref()
                .map(|n| n.to_lowercase().contains(&spec_lower))
                .unwrap_or(false);
            name_match || code_match
        })
        .collect();

    match matches.len() {
        0 => anyhow::bail!("No course matching '{spec}'"),
        1 => matches[0]
            .id
            .ok_or_else(|| anyhow::anyhow!("Matched course has no ID")),
        _ => {
            eprintln!("Multiple courses match '{spec}':");
            for c in &matches {
                eprintln!(
                    "  {} - {}",
                    c.id.map(|id| id.to_string()).unwrap_or_default(),
                    c.name.as_deref().unwrap_or("(unknown)")
                );
            }
            anyhow::bail!("Ambiguous course name. Please specify the course ID.");
        }
    }
}

fn cache_path() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".cache")
        });
    cache_home.join("ntucool").join("courses.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CourseCache {
    fetched_at: chrono::DateTime<chrono::Utc>,
    courses: Vec<Course>,
}

/// Public wrapper for the browser to access cached courses.
pub async fn fetch_courses_cached_pub(client: &cool_api::CoolClient) -> Result<Vec<Course>> {
    fetch_courses_cached(client).await
}

/// Fetch courses with a file-based cache (TTL: 1 hour).
async fn fetch_courses_cached(client: &cool_api::CoolClient) -> Result<Vec<Course>> {
    let path = cache_path();

    // Try reading cache
    if let Ok(data) = tokio::fs::read_to_string(&path).await {
        if let Ok(cache) = serde_json::from_str::<CourseCache>(&data) {
            let age = chrono::Utc::now() - cache.fetched_at;
            if age.num_seconds() < COURSE_CACHE_TTL_SECS {
                return Ok(cache.courses);
            }
        }
    }

    // Cache miss or stale — fetch from API
    let courses = fetch_courses_from_api(client).await?;

    // Write cache (best-effort, don't fail on cache write errors)
    let cache = CourseCache {
        fetched_at: chrono::Utc::now(),
        courses: courses.clone(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&path, json).await;
    }

    Ok(courses)
}

async fn fetch_courses_from_api(client: &cool_api::CoolClient) -> Result<Vec<Course>> {
    let params = ListYourCoursesParams {
        enrollment_type: None,
        enrollment_role: None,
        enrollment_role_id: None,
        enrollment_state: Some("active".to_string()),
        exclude_blueprint_courses: None,
        include: None,
        state: None,
    };

    let mut courses: Vec<Course> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_your_courses(client, &params));
    while let Some(item) = stream.next().await {
        courses.push(item?);
    }
    Ok(courses)
}

/// Fetch all courses with `include=term`, filter by matching `term_filter` against
/// both `term.name` and `term.sis_term_id`.
///
/// Uses raw `serde_json::Value` for filtering because the generated `Term` struct
/// doesn't have `sis_term_id` (only `EnrollmentTerm` does), so that field is lost
/// during typed deserialization.
async fn fetch_courses_by_semester(
    client: &cool_api::CoolClient,
    term_filter: &str,
) -> Result<Vec<Course>> {
    let query = [("include[]", "term"), ("per_page", "50")];
    let filter_lower = term_filter.to_lowercase();

    let mut courses: Vec<Course> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/courses");
        let page: cool_api::client::PaginatedResponse<serde_json::Value> =
            client.get_paginated(path, Some(&query)).await?;

        for val in page.items {
            let matches = val.get("term").map_or(false, |term| {
                let name_match = term
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.to_lowercase().contains(&filter_lower))
                    .unwrap_or(false);
                let sis_match = term
                    .get("sis_term_id")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_lowercase().contains(&filter_lower))
                    .unwrap_or(false);
                name_match || sis_match
            });

            if matches {
                if let Ok(course) = serde_json::from_value::<Course>(val) {
                    courses.push(course);
                }
            }
        }

        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    if courses.is_empty() {
        eprintln!(
            "Warning: no courses matched semester '{}'. Term data may not be available.",
            term_filter
        );
    }

    Ok(courses)
}

/// Fetch all courses (no enrollment_state filter), optionally including term data.
///
/// Uses manual pagination with tuple query params instead of the generated
/// streaming endpoint, because `serde_urlencoded` (used by reqwest's `.query()`)
/// cannot serialize `Vec<String>` — passing `include: Some(vec!["term"])` via
/// `ListYourCoursesParams` causes "builder error: unsupported value".
async fn fetch_all_courses(
    client: &cool_api::CoolClient,
    include_term: bool,
) -> Result<Vec<Course>> {
    let mut query: Vec<(&str, &str)> = vec![("per_page", "50")];
    if include_term {
        query.push(("include[]", "term"));
    }

    let mut all_courses: Vec<Course> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url
            .as_deref()
            .unwrap_or("/api/v1/courses");

        let page: cool_api::client::PaginatedResponse<Course> =
            client.get_paginated(path, Some(&query)).await?;
        all_courses.extend(page.items);

        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    Ok(all_courses)
}
