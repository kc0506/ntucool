//! Assignments — list, show, submit.

use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use futures::StreamExt;
use serde::Deserialize;

use cool_api::config::WriteLevel;
use cool_api::generated::endpoints;
pub use cool_api::generated::models::Assignment;
use cool_api::generated::params::ListAssignmentsAssignmentsParams;
use cool_api::CoolClient;

use crate::attachments;
use crate::text;
use crate::types::{
    AssignmentDetail as ContractAssignmentDetail, AssignmentSummary, RiskSeverity,
    RubricCriterion as ContractRubricCriterion, SubmissionReceipt, SubmitPreflight, SubmitRisk,
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
    with_html: bool,
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

    let raw_html = a.description.as_deref();
    let description_md = raw_html.map(text::html_to_md);
    let description_html = if with_html { raw_html.map(str::to_string) } else { None };
    let references = raw_html
        .map(attachments::extract_references)
        .unwrap_or_default();

    Ok(ContractAssignmentDetail {
        id: a.id.unwrap_or(assignment_id),
        course_id: a.course_id.unwrap_or(course_id),
        name: a.name.unwrap_or_default(),
        description_md,
        description_html,
        due_at: a.due_at.map(|t| t.to_rfc3339()),
        points_possible: a.points_possible,
        submission_types: a.submission_types.unwrap_or_default(),
        html_url: a.html_url,
        rubric,
        references,
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Submission (write path)
// ────────────────────────────────────────────────────────────────────────────
//
// Canvas's submit endpoint wants the submission fields nested — form params
// `submission[submission_type]=…` or JSON `{"submission": {…}}`. The codegen'd
// `SubmitAssignmentCoursesParams` serialises them as DOT-flattened keys
// (`"submission.submission_type"`) which Rails never reassembles, so codegen's
// `submit_assignment_courses` cannot work. We bypass codegen here and hand-build
// the nested JSON — the same workaround `submissions.rs` uses for its read path.

/// What the user wants to turn in. The variant fixes the Canvas
/// `submission_type`: `Files` → `online_upload`, `Text` → `online_text_entry`.
#[derive(Debug, Clone)]
pub enum SubmitContent {
    /// One or more local files, uploaded then attached as `online_upload`.
    Files(Vec<PathBuf>),
    /// An HTML / plain-text body submitted as `online_text_entry`.
    Text(String),
}

impl SubmitContent {
    /// The Canvas `submission_type` string this content maps to.
    pub fn submission_type(&self) -> &'static str {
        match self {
            Self::Files(_) => "online_upload",
            Self::Text(_) => "online_text_entry",
        }
    }
}

/// What we deserialize from Canvas's `POST .../submissions` response. The
/// codegen `Submission` model can't be used here: it types nested fields like
/// `submission_comments[].author` as `String` while Canvas returns objects, so
/// the full model fails to decode. Same bypass rationale as `submissions.rs`.
#[derive(Debug, Deserialize)]
struct RawSubmitResponse {
    workflow_state: Option<String>,
    submission_type: Option<String>,
    submitted_at: Option<String>,
    attempt: Option<i64>,
    late: Option<bool>,
    preview_url: Option<String>,
}

/// Knobs for `submit` beyond the content itself.
#[derive(Debug, Clone, Default)]
pub struct SubmitOptions {
    /// Optional comment delivered to the grader alongside the submission.
    pub comment: Option<String>,
    /// Acknowledge every `Soft` risk surfaced by preflight. `Hard` risks abort
    /// regardless. Without this, a soft risk makes `submit` refuse.
    pub i_understand: bool,
}

/// Fetch the assignment (with the caller's current submission included) and
/// assess every risk of submitting `content` to it — submitting nothing.
pub async fn preflight(
    client: &CoolClient,
    course_id: i64,
    assignment_id: i64,
    content: &SubmitContent,
) -> Result<SubmitPreflight> {
    let assignment = show_with_submission(client, course_id, assignment_id).await?;
    Ok(assess(course_id, assignment_id, content, &assignment))
}

/// Submit `content` to an assignment, gated by a preflight safety check.
///
/// Aborts on any `Hard` risk. Aborts on `Soft` risks too unless
/// `opts.i_understand` is set. On success returns a `SubmissionReceipt` built
/// from Canvas's response (workflow_state, attempt, late, …).
pub async fn submit(
    client: &CoolClient,
    course_id: i64,
    assignment_id: i64,
    content: &SubmitContent,
    opts: &SubmitOptions,
) -> Result<SubmissionReceipt> {
    // Cheap local validation before any network write.
    match content {
        SubmitContent::Files(paths) => {
            if paths.is_empty() {
                anyhow::bail!("No files given to submit.");
            }
            for p in paths {
                if !p.exists() {
                    anyhow::bail!("File not found: {}", p.display());
                }
            }
        }
        SubmitContent::Text(body) => {
            if body.trim().is_empty() {
                anyhow::bail!("Refusing to submit an empty text body.");
            }
        }
    }

    // Resolve the write policy (env NTUCOOL_WRITE_LEVEL > .ntucool.json > `none`).
    let level = cool_api::config::write_level();
    if level == WriteLevel::None {
        anyhow::bail!(
            "Writes are disabled (write_level = none). Enable submission by either:\n  \
             - set env NTUCOOL_WRITE_LEVEL=guarded   (one-off)\n  \
             - add {{\"write_level\": \"guarded\"}} to .ntucool.json in your project\n\
             Levels: safe (clean submissions only) | guarded (risky ones need \
             i_understand) | unguarded (no checks)."
        );
    }

    // `unguarded` skips preflight entirely — Canvas is the sole authority.
    // Every other level runs preflight and applies the gate.
    if level != WriteLevel::Unguarded {
        let pf = preflight(client, course_id, assignment_id, content).await?;
        enforce_gate(&pf.risks, level, opts.i_understand)?;
    }

    // Build the nested `submission` object Canvas expects (see module note).
    let mut submission = serde_json::Map::new();
    submission.insert(
        "submission_type".into(),
        serde_json::Value::String(content.submission_type().into()),
    );
    match content {
        SubmitContent::Files(paths) => {
            let file_ids = upload_files(client, course_id, assignment_id, paths).await?;
            submission.insert("file_ids".into(), serde_json::json!(file_ids));
        }
        SubmitContent::Text(body) => {
            submission.insert("body".into(), serde_json::Value::String(body.clone()));
        }
    }

    let mut payload = serde_json::Map::new();
    payload.insert("submission".into(), serde_json::Value::Object(submission));
    if let Some(comment) = opts.comment.as_deref().filter(|c| !c.trim().is_empty()) {
        payload.insert("comment".into(), serde_json::json!({ "text_comment": comment }));
    }

    let path = format!("/api/v1/courses/{course_id}/assignments/{assignment_id}/submissions");
    let created: RawSubmitResponse = client
        .post(&path, &serde_json::Value::Object(payload))
        .await?;

    Ok(SubmissionReceipt {
        course_id,
        assignment_id,
        workflow_state: created.workflow_state,
        submission_type: created.submission_type,
        submitted_at: created.submitted_at,
        attempt: created.attempt,
        late: created.late,
        preview_url: created.preview_url,
    })
}

/// GET one assignment with `include[]=submission` so preflight can see whether
/// the user already has a submission.
async fn show_with_submission(
    client: &CoolClient,
    course_id: i64,
    assignment_id: i64,
) -> Result<Assignment> {
    let path = format!("/api/v1/courses/{course_id}/assignments/{assignment_id}");
    let query = [("include[]", "submission")];
    Ok(client.get(&path, Some(&query)).await?)
}

/// Pure risk assessment: given a fetched assignment and the intended content,
/// enumerate every condition that should block or warn the submission.
fn assess(
    course_id: i64,
    assignment_id: i64,
    content: &SubmitContent,
    a: &Assignment,
) -> SubmitPreflight {
    let want = content.submission_type();
    let now = Utc::now();
    let mut risks = Vec::new();

    // Hard: the assignment doesn't accept this submission type.
    let accepted = a.submission_types.clone().unwrap_or_default();
    if !accepted.iter().any(|t| t == want) {
        risks.push(SubmitRisk {
            code: "type_mismatch".into(),
            severity: RiskSeverity::Hard,
            message: format!(
                "Assignment accepts {accepted:?}, not `{want}` — Canvas will reject this submission."
            ),
        });
    }

    // Hard: locked for this user.
    if a.locked_for_user == Some(true) {
        let why = a
            .lock_explanation
            .clone()
            .unwrap_or_else(|| "assignment is locked".into());
        risks.push(SubmitRisk {
            code: "locked".into(),
            severity: RiskSeverity::Hard,
            message: format!("Assignment is locked for you: {why}"),
        });
    }

    // Hard: not yet open.
    if let Some(unlock) = a.unlock_at {
        if unlock > now {
            risks.push(SubmitRisk {
                code: "not_yet_unlocked".into(),
                severity: RiskSeverity::Hard,
                message: format!(
                    "Assignment unlocks at {} — submissions aren't open yet.",
                    unlock.to_rfc3339()
                ),
            });
        }
    }

    // Hard: past the lock date — Canvas refuses submissions after `lock_at`.
    if let Some(lock) = a.lock_at {
        if lock < now {
            risks.push(SubmitRisk {
                code: "past_lock_date".into(),
                severity: RiskSeverity::Hard,
                message: format!("Submissions closed at {} (lock date passed).", lock.to_rfc3339()),
            });
        }
    }

    // Hard: file extension(s) Canvas is configured to reject.
    if let SubmitContent::Files(paths) = content {
        let allowed = a.allowed_extensions.clone().unwrap_or_default();
        if !allowed.is_empty() {
            for p in paths {
                let ext = p.extension().and_then(|e| e.to_str());
                let ok = ext
                    .map(|e| allowed.iter().any(|a| a.eq_ignore_ascii_case(e)))
                    .unwrap_or(false);
                if !ok {
                    risks.push(SubmitRisk {
                        code: "disallowed_extension".into(),
                        severity: RiskSeverity::Hard,
                        message: format!(
                            "`{}` has an extension Canvas won't accept here (allowed: {allowed:?}).",
                            p.display()
                        ),
                    });
                }
            }
        }
    }

    // Existing-submission state — drives the `overwrites_existing` soft risk
    // and the `attempts_exhausted` hard risk.
    let existing = a.submission.as_ref();
    let has_existing = existing
        .map(|s| {
            s.submitted_at.is_some()
                || s.workflow_state.as_deref().is_some_and(|w| w != "unsubmitted")
        })
        .unwrap_or(false);

    // Hard: attempts exhausted (`allowed_attempts = -1` means unlimited).
    if let Some(max) = a.allowed_attempts {
        if max > 0 {
            let used = existing.and_then(|s| s.attempt).unwrap_or(0);
            if used >= max {
                risks.push(SubmitRisk {
                    code: "attempts_exhausted".into(),
                    severity: RiskSeverity::Hard,
                    message: format!("All {max} allowed attempt(s) already used."),
                });
            }
        }
    }

    // Soft: past due — still submittable, but Canvas flags it late.
    if let Some(due) = a.due_at {
        if due < now {
            risks.push(SubmitRisk {
                code: "past_due".into(),
                severity: RiskSeverity::Soft,
                message: format!(
                    "Past due ({}) — the submission will be marked late.",
                    due.to_rfc3339()
                ),
            });
        }
    }

    // Soft: re-submitting over an existing submission.
    if has_existing {
        let attempt = existing.and_then(|s| s.attempt).unwrap_or(0);
        risks.push(SubmitRisk {
            code: "overwrites_existing".into(),
            severity: RiskSeverity::Soft,
            message: format!(
                "You already submitted (attempt {attempt}). This adds a new attempt; the \
                 previous one stays in Canvas's history but is no longer the active submission."
            ),
        });
    }

    SubmitPreflight {
        course_id,
        assignment_id,
        assignment_name: a.name.clone().unwrap_or_default(),
        submission_type: want.to_string(),
        due_at: a.due_at.map(|t| t.to_rfc3339()),
        lock_at: a.lock_at.map(|t| t.to_rfc3339()),
        has_existing_submission: has_existing,
        risks,
    }
}

/// Apply the `write_level` policy to the preflight risks. Callers handle
/// `None` (refused before preflight) and `Unguarded` (preflight skipped), so
/// `level` here is always `Safe` or `Guarded`:
///
///   Safe    — any risk aborts; `i_understand` is powerless.
///   Guarded — "will-fail" (Hard) risks abort; "danger" (Soft) risks abort
///             unless `i_understand` is set.
fn enforce_gate(risks: &[SubmitRisk], level: WriteLevel, i_understand: bool) -> Result<()> {
    let bullets = |sev: RiskSeverity| -> Vec<String> {
        risks
            .iter()
            .filter(|r| r.severity == sev)
            .map(|r| format!("  - [{}] {}", r.code, r.message))
            .collect()
    };

    // "Will-fail" risks — Canvas would reject these. Abort at both safe and guarded.
    let hard = bullets(RiskSeverity::Hard);
    if !hard.is_empty() {
        anyhow::bail!(
            "Submission refused — {} blocking issue(s) Canvas would reject:\n{}",
            hard.len(),
            hard.join("\n")
        );
    }

    // "Danger" risks — would succeed, but risky. Cleared only at `guarded` + i_understand.
    let soft = bullets(RiskSeverity::Soft);
    if !soft.is_empty() {
        let cleared = level == WriteLevel::Guarded && i_understand;
        if !cleared {
            let hint = if level == WriteLevel::Safe {
                "write_level is `safe`, which blocks every risky submission — \
                 raise it to `guarded` and pass i_understand to proceed"
            } else {
                "pass i_understand to proceed"
            };
            anyhow::bail!(
                "Submission held — {} risk(s) ({hint}):\n{}",
                soft.len(),
                soft.join("\n")
            );
        }
    }
    Ok(())
}

/// Run the Canvas file-upload protocol for each path, returning the file IDs
/// in submission order:
///   1. POST .../submissions/self/files → upload token
///   2. PUT the bytes to the S3-style upload URL (via `execute_upload`)
async fn upload_files(
    client: &CoolClient,
    course_id: i64,
    assignment_id: i64,
    paths: &[PathBuf],
) -> Result<Vec<i64>> {
    let mut ids = Vec::with_capacity(paths.len());
    for path in paths {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let file_size = std::fs::metadata(path)?.len();
        let step1 = serde_json::json!({ "name": file_name, "size": file_size });

        let token: cool_api::upload::UploadToken = client
            .post(
                &format!(
                    "/api/v1/courses/{course_id}/assignments/{assignment_id}/submissions/self/files"
                ),
                &step1,
            )
            .await?;

        let file = cool_api::upload::execute_upload(client, &token, path).await?;
        let id = file.id.ok_or_else(|| {
            anyhow::anyhow!(
                "Upload of {} succeeded but Canvas returned no file ID",
                path.display()
            )
        })?;
        ids.push(id);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── enforce_gate: the write_level × risk-severity safety matrix ──────────

    fn risk(code: &str, severity: RiskSeverity) -> SubmitRisk {
        SubmitRisk {
            code: code.into(),
            severity,
            message: String::new(),
        }
    }

    #[test]
    fn gate_clean_submission_passes() {
        // No risks → through at both safe and guarded, with or without i_understand.
        for level in [WriteLevel::Safe, WriteLevel::Guarded] {
            assert!(enforce_gate(&[], level, false).is_ok());
            assert!(enforce_gate(&[], level, true).is_ok());
        }
    }

    #[test]
    fn gate_hard_risk_always_aborts() {
        let risks = [risk("type_mismatch", RiskSeverity::Hard)];
        for level in [WriteLevel::Safe, WriteLevel::Guarded] {
            for i_understand in [false, true] {
                assert!(
                    enforce_gate(&risks, level, i_understand).is_err(),
                    "hard risk must abort at {level:?} i_understand={i_understand}"
                );
            }
        }
    }

    #[test]
    fn gate_safe_blocks_soft_risk_even_with_i_understand() {
        let risks = [risk("past_due", RiskSeverity::Soft)];
        assert!(enforce_gate(&risks, WriteLevel::Safe, false).is_err());
        assert!(
            enforce_gate(&risks, WriteLevel::Safe, true).is_err(),
            "`safe` must ignore i_understand for soft risks"
        );
    }

    #[test]
    fn gate_guarded_soft_risk_needs_i_understand() {
        let risks = [risk("overwrites_existing", RiskSeverity::Soft)];
        assert!(enforce_gate(&risks, WriteLevel::Guarded, false).is_err());
        assert!(enforce_gate(&risks, WriteLevel::Guarded, true).is_ok());
    }

    // ── assess: risk detection from a fetched Assignment ─────────────────────

    fn assignment(value: serde_json::Value) -> Assignment {
        serde_json::from_value(value).expect("test assignment must deserialize")
    }

    fn one_pdf() -> SubmitContent {
        SubmitContent::Files(vec![PathBuf::from("x.pdf")])
    }

    fn has(pf: &SubmitPreflight, code: &str, severity: RiskSeverity) -> bool {
        pf.risks
            .iter()
            .any(|r| r.code == code && r.severity == severity)
    }

    #[test]
    fn assess_type_mismatch_when_content_type_unaccepted() {
        let a = assignment(json!({ "submission_types": ["online_upload"] }));
        // text body against an upload-only assignment
        let pf = assess(1, 2, &SubmitContent::Text("hi".into()), &a);
        assert!(has(&pf, "type_mismatch", RiskSeverity::Hard));
        // ...but a file upload is accepted — no type_mismatch
        let pf = assess(1, 2, &one_pdf(), &a);
        assert!(!pf.risks.iter().any(|r| r.code == "type_mismatch"));
    }

    #[test]
    fn assess_flags_locked_assignment() {
        let a = assignment(json!({
            "submission_types": ["online_upload"],
            "locked_for_user": true,
        }));
        assert!(has(&assess(1, 2, &one_pdf(), &a), "locked", RiskSeverity::Hard));
    }

    #[test]
    fn assess_past_due_is_soft_future_due_is_clean() {
        let past = assignment(json!({
            "submission_types": ["online_upload"],
            "due_at": "2000-01-01T00:00:00Z",
        }));
        assert!(has(&assess(1, 2, &one_pdf(), &past), "past_due", RiskSeverity::Soft));

        let future = assignment(json!({
            "submission_types": ["online_upload"],
            "due_at": "2099-01-01T00:00:00Z",
        }));
        let pf = assess(1, 2, &one_pdf(), &future);
        assert!(pf.risks.is_empty(), "clean future-dated assignment: {:?}", pf.risks);
    }

    #[test]
    fn assess_flags_past_lock_date() {
        let a = assignment(json!({
            "submission_types": ["online_upload"],
            "lock_at": "2000-01-01T00:00:00Z",
        }));
        assert!(has(
            &assess(1, 2, &one_pdf(), &a),
            "past_lock_date",
            RiskSeverity::Hard
        ));
    }

    #[test]
    fn assess_flags_existing_submission_as_soft() {
        let a = assignment(json!({
            "submission_types": ["online_upload"],
            "submission": { "workflow_state": "submitted", "attempt": 1 },
        }));
        let pf = assess(1, 2, &one_pdf(), &a);
        assert!(pf.has_existing_submission);
        assert!(has(&pf, "overwrites_existing", RiskSeverity::Soft));
    }

    #[test]
    fn assess_flags_disallowed_extension() {
        let a = assignment(json!({
            "submission_types": ["online_upload"],
            "allowed_extensions": ["pdf"],
        }));
        let docx = SubmitContent::Files(vec![PathBuf::from("essay.docx")]);
        assert!(has(
            &assess(1, 2, &docx, &a),
            "disallowed_extension",
            RiskSeverity::Hard
        ));
        // an allowed extension does not flag
        assert!(!assess(1, 2, &one_pdf(), &a)
            .risks
            .iter()
            .any(|r| r.code == "disallowed_extension"));
    }

    #[test]
    fn assess_flags_attempts_exhausted() {
        let a = assignment(json!({
            "submission_types": ["online_upload"],
            "allowed_attempts": 2,
            "submission": { "workflow_state": "submitted", "attempt": 2 },
        }));
        assert!(has(
            &assess(1, 2, &one_pdf(), &a),
            "attempts_exhausted",
            RiskSeverity::Hard
        ));
    }

    #[test]
    fn submit_content_maps_to_canvas_type() {
        assert_eq!(SubmitContent::Files(vec![]).submission_type(), "online_upload");
        assert_eq!(
            SubmitContent::Text(String::new()).submission_type(),
            "online_text_entry"
        );
    }
}
