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
use cool_api::generated::params::ListYourCoursesParams;
use cool_api::CoolClient;

use crate::types::{CourseDetail, CourseSummary, ResolveMatch as ContractResolveMatch, TeacherSummary};

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
///
/// Uses manual tuple-query because `GetSingleCourseCoursesParams.include:
/// Option<Vec<String>>` runs into the same `serde_urlencoded` limitation
/// that bites `list_all`.
pub async fn show(client: &CoolClient, id: &str) -> Result<Course> {
    let query: [(&str, &str); 3] = [
        ("include[]", "term"),
        ("include[]", "teachers"),
        ("include[]", "syllabus_body"),
    ];
    let course: Course = client
        .get(&format!("/api/v1/courses/{}", id), Some(&query))
        .await?;
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

// ────────────────────────────────────────────────────────────────────────────
// Contract-shape adapters (consumed by cool-mcp; CLI keeps using raw fns)
// ────────────────────────────────────────────────────────────────────────────

/// Map a raw Canvas Course to the contract `CourseSummary`. `term` is `None`
/// unless the source listing was fetched with `include[]=term`.
fn course_to_summary(c: &Course) -> Option<CourseSummary> {
    Some(CourseSummary {
        id: c.id?,
        name: c.name.clone().unwrap_or_default(),
        course_code: c.course_code.clone(),
        term: c.term.as_ref().and_then(|t| t.name.clone()),
    })
}

#[derive(Debug, Clone, Copy)]
pub enum ListFilter {
    Active,
    All,
}

/// Contract-shape course list. `filter` chooses active-only vs all enrolments;
/// `term` (case-insensitive substring of term name or sis_term_id) filters the
/// All listing. `term` is ignored when `filter == Active` because the cached
/// active-list is fetched without `include[]=term`.
pub async fn list_summaries(
    client: &CoolClient,
    filter: ListFilter,
    term: Option<&str>,
) -> Result<Vec<CourseSummary>> {
    let courses = match (filter, term) {
        (ListFilter::Active, _) => list_active(client).await?,
        (ListFilter::All, Some(t)) => list_by_semester(client, t).await?,
        (ListFilter::All, None) => list_all(client, true).await?,
    };
    Ok(courses.iter().filter_map(course_to_summary).collect())
}

/// Contract-shape resolver. Numeric IDs score 1.0; substring matches score
/// `match_len / longer(name, code)` clamped to 0.6..0.95.
pub async fn resolve_with_score(
    client: &CoolClient,
    query: &str,
) -> Result<Vec<ContractResolveMatch>> {
    if let Ok(id) = query.parse::<i64>() {
        return Ok(vec![ContractResolveMatch {
            id,
            name: String::new(),
            course_code: None,
            score: 1.0,
        }]);
    }

    let courses = fetch_courses_cached(client).await?;
    let q = query.to_lowercase();
    let q_len = q.chars().count() as f32;

    let mut matches: Vec<ContractResolveMatch> = courses
        .into_iter()
        .filter_map(|c| {
            let id = c.id?;
            let name = c.name.unwrap_or_default();
            let code = c.course_code;
            let name_l = name.to_lowercase();
            let code_l = code.as_deref().map(str::to_lowercase);
            let name_hit = name_l.contains(&q);
            let code_hit = code_l.as_deref().map(|c| c.contains(&q)).unwrap_or(false);
            if !(name_hit || code_hit) {
                return None;
            }
            let target_len = name_l
                .chars()
                .count()
                .max(code_l.as_ref().map(|c| c.chars().count()).unwrap_or(0))
                as f32;
            let raw = if target_len > 0.0 { q_len / target_len } else { 0.0 };
            let score = raw.clamp(0.6, 0.95);
            Some(ContractResolveMatch {
                id,
                name,
                course_code: code,
                score,
            })
        })
        .collect();
    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(matches)
}

/// Contract-shape course detail (syllabus + term + teachers).
///
/// Fetches with `include[]=term&include[]=teachers&include[]=syllabus_body`
/// via an untyped `serde_json::Value` so we can read the `teachers` array
/// (the generated `Course` struct lacks that field).
pub async fn get_detail(client: &CoolClient, id: i64) -> Result<CourseDetail> {
    let query: [(&str, &str); 3] = [
        ("include[]", "term"),
        ("include[]", "teachers"),
        ("include[]", "syllabus_body"),
    ];
    let raw: serde_json::Value = client
        .get(&format!("/api/v1/courses/{}", id), Some(&query))
        .await?;

    let teachers = raw
        .get("teachers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let id = t.get("id").and_then(|v| v.as_i64())?;
                    let name = t
                        .get("display_name")
                        .or_else(|| t.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Some(TeacherSummary { id, name })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(CourseDetail {
        id: raw.get("id").and_then(|v| v.as_i64()).unwrap_or(id),
        name: raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        course_code: raw
            .get("course_code")
            .and_then(|v| v.as_str())
            .map(String::from),
        term: raw
            .get("term")
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from),
        syllabus_html: raw
            .get("syllabus_body")
            .and_then(|v| v.as_str())
            .map(String::from),
        teachers,
        default_view: raw
            .get("default_view")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
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
