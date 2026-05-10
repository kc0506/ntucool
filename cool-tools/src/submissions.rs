//! `submissions_mine` — list the logged-in user's submissions.
//!
//! Canvas endpoint: `/api/v1/courses/:cid/students/submissions` with
//! `student_ids[]=self&include[]=assignment&per_page=100`. Codegen marks this
//! `Result<()>` because the OpenAPI spec lacks a response schema; we bypass
//! codegen and deserialize into our own `RawSubmission` shape, which has all
//! the fields the AI/CLI actually wants (score, grade, graded_at, late /
//! missing / excused, assignment.name + points_possible).
//!
//! When `course_id` is omitted we walk every active enrolment via
//! `courses::list_summaries(Active)` and fan out per course sequentially.
//! That's N+1 HTTP calls; acceptable for a typical student (≲ 20 active
//! courses) and fine for the bursty AI-assistant call pattern. If this
//! becomes a hot path we can switch to bounded-concurrency `buffer_unordered`.

use anyhow::Result;
use serde::Deserialize;

use cool_api::CoolClient;

use crate::courses;
use crate::types::SubmissionMine;

/// Workflow-state filter accepted by Canvas. `"submitted"` matches anything
/// turned in; `"graded"` matches anything with a teacher-set score.
#[derive(Debug, Clone, Copy)]
pub enum WorkflowFilter {
    Submitted,
    Unsubmitted,
    Graded,
    PendingReview,
}

impl WorkflowFilter {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "submitted" => Some(Self::Submitted),
            "unsubmitted" => Some(Self::Unsubmitted),
            "graded" => Some(Self::Graded),
            "pending_review" => Some(Self::PendingReview),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Unsubmitted => "unsubmitted",
            Self::Graded => "graded",
            Self::PendingReview => "pending_review",
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawSubmission {
    course_id: Option<i64>,
    assignment_id: Option<i64>,
    score: Option<f64>,
    grade: Option<String>,
    workflow_state: Option<String>,
    submitted_at: Option<String>,
    graded_at: Option<String>,
    late: Option<bool>,
    missing: Option<bool>,
    excused: Option<bool>,
    assignment: Option<RawAssignment>,
}

#[derive(Debug, Deserialize)]
struct RawAssignment {
    name: Option<String>,
    points_possible: Option<f64>,
}

/// List my submissions in a single course. Canvas returns one entry per
/// assignment, including unsubmitted (workflow_state="unsubmitted") so the
/// AI can see what's still missing.
pub async fn submissions_mine_in_course(
    client: &CoolClient,
    course_id: i64,
    workflow: Option<WorkflowFilter>,
) -> Result<Vec<SubmissionMine>> {
    let path = format!("/api/v1/courses/{course_id}/students/submissions");
    let mut query: Vec<(&str, String)> = vec![
        ("student_ids[]", "self".to_string()),
        ("include[]", "assignment".to_string()),
        ("per_page", "100".to_string()),
    ];
    if let Some(w) = workflow {
        query.push(("workflow_state", w.as_str().to_string()));
    }

    let mut out = Vec::new();
    let mut next: Option<String> = None;
    loop {
        let url_or_path = next.as_deref().unwrap_or(&path);
        let page = if next.is_some() {
            client
                .get_paginated::<RawSubmission, ()>(url_or_path, None)
                .await?
        } else {
            client
                .get_paginated::<RawSubmission, _>(url_or_path, Some(&query))
                .await?
        };
        for raw in page.items {
            out.push(into_submission_mine(course_id, raw));
        }
        match page.next_url {
            Some(u) => next = Some(u),
            None => break,
        }
    }
    Ok(out)
}

/// List my submissions across all active enrolments. Iterates `courses::list_summaries(Active)`
/// then calls `submissions_mine_in_course` per course sequentially. See module doc for
/// rationale on serial vs concurrent.
pub async fn submissions_mine_all(
    client: &CoolClient,
    workflow: Option<WorkflowFilter>,
) -> Result<Vec<SubmissionMine>> {
    let courses = courses::list_summaries(client, courses::ListFilter::Active, None).await?;
    let mut out = Vec::new();
    for c in courses {
        match submissions_mine_in_course(client, c.id, workflow).await {
            Ok(mut subs) => out.append(&mut subs),
            // Per-course failure (e.g., 401 on a stale enrolment) shouldn't
            // poison the whole listing — surface it via eprintln and continue.
            Err(e) => eprintln!("submissions_mine: course {} failed: {}", c.id, e),
        }
    }
    Ok(out)
}

fn into_submission_mine(fallback_course_id: i64, raw: RawSubmission) -> SubmissionMine {
    let (assignment_name, points_possible) = match raw.assignment {
        Some(a) => (a.name, a.points_possible),
        None => (None, None),
    };
    SubmissionMine {
        course_id: raw.course_id.unwrap_or(fallback_course_id),
        assignment_id: raw.assignment_id.unwrap_or(0),
        assignment_name,
        points_possible,
        score: raw.score,
        grade: raw.grade,
        workflow_state: raw.workflow_state,
        submitted_at: raw.submitted_at,
        graded_at: raw.graded_at,
        late: raw.late,
        missing: raw.missing,
        excused: raw.excused,
    }
}
