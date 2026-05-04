//! Courses — list, show, and resolve names → IDs.
//!
//! `resolve` returns a `Vec<Match>` and lets callers decide how to handle 0 or
//! >1 matches; `resolve_one` is the strict "must be exactly one" wrapper.

use std::path::PathBuf;

use anyhow::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use cool_api::client::PaginatedResponse;
use cool_api::generated::endpoints;
pub use cool_api::generated::models::Course;
use cool_api::generated::params::{GetSingleCourseCoursesParams, ListYourCoursesParams};
use cool_api::CoolClient;

const COURSE_CACHE_TTL_SECS: i64 = 3600;

/// One result from `resolve`.
#[derive(Debug, Clone, Serialize)]
pub struct ResolveMatch {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
}

/// Active enrolled courses (cached, 1h TTL).
pub async fn list_active(client: &CoolClient) -> Result<Vec<Course>> {
    fetch_courses_cached(client).await
}

/// All courses (no enrollment_state filter), optionally with `term` data.
///
/// Manual pagination with tuple query params because `serde_urlencoded` (used
/// by reqwest's `.query()`) cannot serialize `Vec<String>` — passing
/// `include: Some(vec!["term"])` via `ListYourCoursesParams` causes
/// "builder error: unsupported value".
pub async fn list_all(client: &CoolClient, include_term: bool) -> Result<Vec<Course>> {
    let mut query: Vec<(&str, &str)> = vec![("per_page", "50")];
    if include_term {
        query.push(("include[]", "term"));
    }

    let mut all: Vec<Course> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/courses");
        let page: PaginatedResponse<Course> = client.get_paginated(path, Some(&query)).await?;
        all.extend(page.items);
        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    Ok(all)
}

/// Filter courses by semester term name or sis_term_id substring.
///
/// Uses raw `serde_json::Value` for filtering because the generated `Term`
/// struct lacks `sis_term_id` (only `EnrollmentTerm` has it), so a typed
/// deserialization would lose that field before we could match against it.
pub async fn list_by_semester(
    client: &CoolClient,
    term_filter: &str,
) -> Result<Vec<Course>> {
    let query = [("include[]", "term"), ("per_page", "50")];
    let filter_lower = term_filter.to_lowercase();

    let mut courses: Vec<Course> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/courses");
        let page: PaginatedResponse<serde_json::Value> =
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

    Ok(courses)
}

/// Single course details.
pub async fn show(client: &CoolClient, id: &str) -> Result<Course> {
    let params = GetSingleCourseCoursesParams {
        include: None,
        teacher_limit: None,
    };
    let course = endpoints::get_single_course_courses(client, id, &params).await?;
    Ok(course)
}

/// Resolve a course query (numeric ID or substring of name/course_code) into
/// zero or more matches. Numeric IDs return a single match without hitting the
/// API; otherwise we search the cached active-course list.
pub async fn resolve(client: &CoolClient, query: &str) -> Result<Vec<ResolveMatch>> {
    if let Ok(id) = query.parse::<i64>() {
        return Ok(vec![ResolveMatch {
            id,
            name: String::new(),
            course_code: None,
        }]);
    }

    let courses = fetch_courses_cached(client).await?;
    let q = query.to_lowercase();
    let matches = courses
        .into_iter()
        .filter_map(|c| {
            let id = c.id?;
            let name = c.name.unwrap_or_default();
            let code = c.course_code;
            let name_hit = name.to_lowercase().contains(&q);
            let code_hit = code.as_ref().map(|s| s.to_lowercase().contains(&q)).unwrap_or(false);
            if name_hit || code_hit {
                Some(ResolveMatch {
                    id,
                    name,
                    course_code: code,
                })
            } else {
                None
            }
        })
        .collect();
    Ok(matches)
}

/// Resolve to exactly one course ID. Errors on 0 or >1 matches.
pub async fn resolve_one(client: &CoolClient, query: &str) -> Result<i64> {
    let matches = resolve(client, query).await?;
    match matches.len() {
        0 => anyhow::bail!("No course matching '{query}'"),
        1 => Ok(matches[0].id),
        _ => {
            let listing = matches
                .iter()
                .map(|m| format!("  {} - {}", m.id, m.name))
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::bail!(
                "Ambiguous course query '{query}'. Candidates:\n{listing}\n\nSpecify the course ID."
            )
        }
    }
}

// ---------- internal cache ----------

fn cache_path() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".cache")
        });
    cache_home.join("ntucool").join("courses.json")
}

#[derive(Serialize, Deserialize)]
struct CourseCache {
    fetched_at: chrono::DateTime<chrono::Utc>,
    courses: Vec<Course>,
}

async fn fetch_courses_cached(client: &CoolClient) -> Result<Vec<Course>> {
    let path = cache_path();

    if let Ok(data) = tokio::fs::read_to_string(&path).await {
        if let Ok(cache) = serde_json::from_str::<CourseCache>(&data) {
            let age = chrono::Utc::now() - cache.fetched_at;
            if age.num_seconds() < COURSE_CACHE_TTL_SECS {
                return Ok(cache.courses);
            }
        }
    }

    let courses = fetch_active_from_api(client).await?;

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

async fn fetch_active_from_api(client: &CoolClient) -> Result<Vec<Course>> {
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
