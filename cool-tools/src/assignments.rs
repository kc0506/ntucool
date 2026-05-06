//! Assignments — list, show, submit.

use std::path::Path;

use anyhow::Result;
use futures::StreamExt;

use cool_api::generated::endpoints;
pub use cool_api::generated::models::Assignment;
use cool_api::generated::params::{ListAssignmentsAssignmentsParams, SubmitAssignmentCoursesParams};
use cool_api::CoolClient;

use crate::attachments;
use crate::text;
use crate::types::{
    AssignmentDetail as ContractAssignmentDetail, AssignmentSummary, RubricCriterion as ContractRubricCriterion,
};

/// Optional filters for `list`. All `None` = unfiltered.
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub bucket: Option<String>, // upcoming | past | overdue | undated | ungraded | unsubmitted | future
    pub search_term: Option<String>,
    pub include: Option<Vec<String>>,
}

pub async fn list(
    client: &CoolClient,
    course_id: &str,
    filter: ListFilter,
) -> Result<Vec<Assignment>> {
    let params = ListAssignmentsAssignmentsParams {
        include: filter.include,
        search_term: filter.search_term,
        override_assignment_dates: None,
        needs_grading_count_by_section: None,
        bucket: filter.bucket,
        assignment_ids: None,
        order_by: None,
        post_to_sis: None,
        new_quizzes: None,
    };

    let mut out = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_assignments_assignments(
        client, course_id, &params
    ));
    while let Some(item) = stream.next().await {
        out.push(item?);
    }
    Ok(out)
}

pub async fn show(
    client: &CoolClient,
    course_id: &str,
    assignment_id: &str,
) -> Result<Assignment> {
    let assignment: Assignment = client
        .get(
            &format!("/api/v1/courses/{}/assignments/{}", course_id, assignment_id),
            None::<&()>,
        )
        .await?;
    Ok(assignment)
}

// ────────────────────────────────────────────────────────────────────────────
// Contract-shape adapters
// ────────────────────────────────────────────────────────────────────────────

fn assignment_to_summary(a: &Assignment, bucket: Option<&str>) -> Option<AssignmentSummary> {
    Some(AssignmentSummary {
        id: a.id?,
        course_id: a.course_id.unwrap_or(0),
        name: a.name.clone().unwrap_or_default(),
        due_at: a.due_at.map(|t| t.to_rfc3339()),
        points_possible: a.points_possible,
        bucket: bucket.map(str::to_string),
        html_url: a.html_url.clone(),
    })
}

pub async fn list_summaries(
    client: &CoolClient,
    course_id: i64,
    bucket: Option<&str>,
) -> Result<Vec<AssignmentSummary>> {
    let course_id_str = course_id.to_string();
    let raw = list(
        client,
        &course_id_str,
        ListFilter {
            bucket: bucket.map(str::to_string),
            search_term: None,
            include: None,
        },
    )
    .await?;
    Ok(raw
        .iter()
        .filter_map(|a| assignment_to_summary(a, bucket))
        .collect())
}

pub async fn get_detail(
    client: &CoolClient,
    course_id: i64,
    assignment_id: i64,
) -> Result<ContractAssignmentDetail> {
    let course_id_str = course_id.to_string();
    let assignment_id_str = assignment_id.to_string();
    let a = show(client, &course_id_str, &assignment_id_str).await?;

    let rubric = a
        .rubric
        .unwrap_or_default()
        .into_iter()
        .map(|c| ContractRubricCriterion {
            description: c.description.unwrap_or_default(),
            points: c.points.unwrap_or(0.0),
            long_description: c.long_description,
        })
        .collect();

    let attachments = a
        .description
        .as_deref()
        .map(attachments::extract_attachments)
        .unwrap_or_default();

    Ok(ContractAssignmentDetail {
        id: a.id.unwrap_or(assignment_id),
        course_id: a.course_id.unwrap_or(course_id),
        name: a.name.unwrap_or_default(),
        description_text: a.description.as_deref().map(text::html_to_text),
        due_at: a.due_at.map(|t| t.to_rfc3339()),
        points_possible: a.points_possible,
        submission_types: a.submission_types.unwrap_or_default(),
        html_url: a.html_url,
        rubric,
        attachments,
    })
}

/// Submit a single file as an `online_upload` submission.
///
/// Multi-step Canvas flow:
///   1. POST /courses/.../assignments/<id>/submissions/self/files → upload token
///   2. PUT bytes to S3-style upload URL
///   3. POST /courses/.../assignments/<id>/submissions with the file_id
pub async fn submit_file(
    client: &CoolClient,
    course_id: &str,
    assignment_id: &str,
    local_path: &Path,
) -> Result<()> {
    if !local_path.exists() {
        anyhow::bail!("File not found: {}", local_path.display());
    }

    let file_name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let file_size = std::fs::metadata(local_path)?.len();

    let step1_body = serde_json::json!({ "name": file_name, "size": file_size });

    let upload_token: cool_api::upload::UploadToken = client
        .post(
            &format!(
                "/api/v1/courses/{}/assignments/{}/submissions/self/files",
                course_id, assignment_id
            ),
            &step1_body,
        )
        .await?;

    let file_obj = cool_api::upload::execute_upload(client, &upload_token, local_path).await?;
    let file_id = file_obj
        .id
        .ok_or_else(|| anyhow::anyhow!("Upload succeeded but no file ID returned"))?;

    let submit_params = SubmitAssignmentCoursesParams {
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

    endpoints::submit_assignment_courses(client, course_id, assignment_id, &submit_params).await?;
    Ok(())
}
