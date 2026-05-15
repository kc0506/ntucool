//! Contract types — the public, AI-facing shape of every tool's I/O.
//!
//! These structs are the contract documented in `docs/TOOLS.md`. They are
//! **deliberately trimmed** — Canvas API responses carry 25+ fields per object,
//! 90% of which are noise for an AI consumer. Adding a field here is a
//! deliberate API decision; raw Canvas types stay inside `cool-api` /
//! `cool-tools` internals and never leak through MCP.
//!
//! Round-trip note: every type derives `Serialize`/`Deserialize`/`JsonSchema`
//! so cool-mcp can hand them straight to the rmcp `Parameters<>` extractor and
//! `json_result()`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// Profile
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSummary {
    pub id: i64,
    pub name: String,
    pub login_id: Option<String>,
    pub primary_email: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// User (any user, including non-self — for resolving teacher_id, classmates, etc.)
// ────────────────────────────────────────────────────────────────────────────

/// Public-ish view of a Canvas user fetched via `/api/v1/users/:id`.
///
/// `login_id` and `email` are typically `None` for non-self users at student
/// privilege level — Canvas only exposes them to admins/teachers or the user
/// themselves. Use `whoami` (returns `ProfileSummary`) for the richer self
/// view, which goes through `/users/self/profile` and surfaces `primary_email`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserSummary {
    pub id: i64,
    pub name: String,
    /// Short display name (often "first last" without honorifics). Useful for
    /// rendering author lines.
    pub short_name: Option<String>,
    /// "last, first" format Canvas uses for sorting.
    pub sortable_name: Option<String>,
    pub login_id: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Course
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CourseSummary {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    /// Human-readable term name when known (e.g. "112-1"). `None` if Canvas did
    /// not include term data on this listing.
    pub term: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolveMatch {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    /// Fuzzy match confidence 0.0..=1.0. 1.0 = exact id, ~0.9 = exact substring,
    /// lower = partial. Numeric IDs always score 1.0 even without API hit.
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CourseDetail {
    pub id: i64,
    pub name: String,
    pub course_code: Option<String>,
    pub term: Option<String>,
    /// Course-level window. Often null when the term itself bounds the course
    /// (`term.start_at` / `term.end_at` apply); both surfaced when present.
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub term_start_at: Option<String>,
    pub term_end_at: Option<String>,
    /// IANA time zone string (e.g. "Asia/Taipei"). Useful when interpreting
    /// `due_at` / `posted_at` for users outside the host TZ. Canvas does not
    /// expose a per-course weekly meeting time, so callers wanting class
    /// schedule should parse the syllabus.
    pub time_zone: Option<String>,
    /// Canvas syllabus body (HTML). Caller decides whether to convert to text.
    pub syllabus_html: Option<String>,
    pub teachers: Vec<TeacherSummary>,
    pub default_view: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeacherSummary {
    pub id: i64,
    pub name: String,
}

// ────────────────────────────────────────────────────────────────────────────
// File
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileSummary {
    pub id: i64,
    pub display_name: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileMetadata {
    pub id: i64,
    pub display_name: String,
    pub size_bytes: i64,
    pub mime_type: Option<String>,
    pub updated_at: Option<String>,
    /// Canvas-signed download URL. Time-limited; treat as opaque.
    pub url: Option<String>,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FolderListing {
    pub course_id: i64,
    /// Path the listing represents, "/" for root.
    pub path: String,
    pub folders: Vec<FolderSummary>,
    pub files: Vec<FileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FolderSummary {
    pub id: i64,
    pub name: String,
    pub files_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DownloadResult {
    pub file_id: i64,
    pub dest_path: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FilesFetchResult {
    pub file_id: i64,
    pub display_name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    /// URI to access the file's bytes. Scheme depends on the cool-mcp
    /// instance's serve mode:
    ///   stdio mode → `file:///abs/path` to a server-internal cache file
    ///   http mode  → `https://host/cache/<token>.ext` (not implemented yet)
    /// Pass to user, or read via this URI directly. Cache-controlled — repeat
    /// calls reuse the same URI while Canvas's `updated_at` is unchanged.
    pub uri: String,
    /// ISO-8601 expiry. `None` for `file://` (no per-URI expiry; subject
    /// only to cache eviction). Always set for HTTP URIs.
    pub expires_at: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Assignment
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssignmentSummary {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub due_at: Option<String>,
    pub points_possible: Option<f64>,
    /// Canvas bucket on listing context (`upcoming` / `past` / etc), if filtered.
    pub bucket: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssignmentDetail {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    /// Markdown version of Canvas assignment description (HTML→Markdown via htmd).
    /// `None` when Canvas returned no description at all (vs an empty string,
    /// which means the description is intentionally blank).
    pub description_md: Option<String>,
    /// Raw Canvas description HTML, populated only when caller asked for it
    /// (`with_html=true` on the MCP tool). Lets the AI see the original markup
    /// when the markdown rendering loses something it cares about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
    pub due_at: Option<String>,
    pub points_possible: Option<f64>,
    pub submission_types: Vec<String>,
    pub html_url: Option<String>,
    /// Rubric criteria when set. Empty Vec when no rubric.
    pub rubric: Vec<RubricCriterion>,
    /// Canvas-internal references mined from the description HTML — files,
    /// pages, other assignments, discussions, modules. Each variant carries
    /// the IDs needed to call the matching `*_get` / `files_fetch` tool.
    pub references: Vec<CanvasRef>,
}

/// Canvas-internal link discovered in HTML content.
///
/// Tagged union so the AI can dispatch on `kind` without parsing URLs:
/// File → `files_fetch` / `files_get_metadata`
/// Page → `pages_get`
/// Assignment → `assignments_get`
/// DiscussionTopic → `discussions_get` / `announcements_get`
/// Module → `modules_get`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum CanvasRef {
    File {
        id: i64,
        /// Visible link text (usually the original filename).
        name: String,
        /// Original HTML `href`, kept for context — prefer `id` when calling tools.
        href: String,
    },
    Page {
        course_id: i64,
        /// Canvas page URL slug (page primary key).
        slug: String,
        name: String,
        href: String,
    },
    Assignment {
        course_id: i64,
        id: i64,
        name: String,
        href: String,
    },
    DiscussionTopic {
        course_id: i64,
        id: i64,
        name: String,
        href: String,
    },
    Module {
        course_id: i64,
        id: i64,
        name: String,
        href: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RubricCriterion {
    pub description: String,
    pub points: f64,
    pub long_description: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Announcement
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementSummary {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub posted_at: Option<String>,
    /// Display name of whoever posted. Canvas exposes this on the topic
    /// listing; previously only DiscussionSummary surfaced it.
    pub author_name: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementDetail {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    /// Markdown body (HTML→Markdown via htmd).
    pub body_md: String,
    /// Raw Canvas HTML body, populated only when `with_html=true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub posted_at: Option<String>,
    pub author_name: Option<String>,
    pub html_url: Option<String>,
    /// Canvas-internal references mined from the announcement body HTML.
    pub references: Vec<CanvasRef>,
}

// ────────────────────────────────────────────────────────────────────────────
// Module
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleSummary {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub position: Option<i64>,
    pub items_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleDetail {
    pub id: i64,
    pub course_id: i64,
    pub name: String,
    pub position: Option<i64>,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleItem {
    pub id: i64,
    pub title: String,
    /// File / Page / Discussion / Assignment / Quiz / ExternalUrl / ExternalTool / SubHeader.
    pub item_type: String,
    /// Resource ID for File/Page/etc; `None` for SubHeader and ExternalUrl.
    pub content_id: Option<i64>,
    pub url: Option<String>,
    pub position: Option<i64>,
    pub indent: Option<i64>,
}

// ────────────────────────────────────────────────────────────────────────────
// Discussion
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionSummary {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    pub posted_at: Option<String>,
    pub author_name: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionDetail {
    pub id: i64,
    pub course_id: i64,
    pub title: String,
    /// Markdown body (HTML→Markdown via htmd).
    pub message_md: String,
    /// Raw HTML body, populated only when `with_html=true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_html: Option<String>,
    pub posted_at: Option<String>,
    pub author_name: Option<String>,
    pub html_url: Option<String>,
    /// Canvas-internal references mined from the topic body HTML.
    pub references: Vec<CanvasRef>,
    /// Top-level entries (first level of replies). Empty when caller did not
    /// request `with_entries`.
    pub entries: Vec<DiscussionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscussionEntry {
    pub id: i64,
    pub author_name: Option<String>,
    /// Markdown body for this entry.
    pub message_md: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_html: Option<String>,
    pub posted_at: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// PDF text extraction & content search
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PdfExtractResult {
    pub file_id: i64,
    pub display_name: String,
    /// Total page count of the original document, even when only a subset is returned.
    pub page_count: usize,
    /// Per-page text in document order. May contain fewer entries than
    /// `page_count` when caller scoped to a subrange.
    pub pages: Vec<PdfPage>,
    /// True when the extractor returned no text at all (likely an image-only or
    /// encrypted PDF). Distinct from "pages exist but contain only whitespace".
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PdfPage {
    /// 1-indexed page number from the original document.
    pub page_no: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PdfSearchHit {
    pub file_id: i64,
    pub display_name: String,
    /// 1-indexed page where the match occurred.
    pub page: usize,
    /// Single-line excerpt around the match. Whitespace collapsed; ~80 chars
    /// of context on each side of the matched span.
    pub snippet: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Submissions (mine) & grades
// ────────────────────────────────────────────────────────────────────────────

/// One of the logged-in user's submissions for a single assignment. Returned by
/// `submissions_mine`. Built from `/api/v1/courses/:cid/students/submissions`
/// with `include[]=assignment` so `assignment_name` and `points_possible` come
/// in the same request.
///
/// `score` is the numeric points earned; `grade` is the rendered grade
/// (letter / pass-fail / percentage / numeric-as-string) — Canvas decides
/// representation per assignment grading_type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmissionMine {
    pub course_id: i64,
    pub assignment_id: i64,
    pub assignment_name: Option<String>,
    pub points_possible: Option<f64>,
    pub score: Option<f64>,
    pub grade: Option<String>,
    /// `submitted` / `unsubmitted` / `graded` / `pending_review`.
    pub workflow_state: Option<String>,
    pub submitted_at: Option<String>,
    pub graded_at: Option<String>,
    pub late: Option<bool>,
    pub missing: Option<bool>,
    pub excused: Option<bool>,
}

/// Per-course grade summary. Returned by `grades_get`. Built from
/// `/api/v1/users/self/enrollments` (StudentEnrollment rows) where
/// `grades.current_*` and `grades.final_*` are Canvas's authoritative numbers.
///
/// `current_*` reflects only graded assignments to date; `final_*` treats
/// ungraded assignments as zeros (i.e., projected final if you submit
/// nothing else). Both can be `None` when the course hides grades or
/// there are no graded assignments yet.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CourseGrade {
    pub course_id: i64,
    pub course_name: Option<String>,
    pub current_grade: Option<String>,
    pub current_score: Option<f64>,
    pub final_grade: Option<String>,
    pub final_score: Option<f64>,
    /// Canvas link to the gradebook page. Open in a browser for the full breakdown.
    pub html_url: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Assignment submission (write path)
// ────────────────────────────────────────────────────────────────────────────

/// Severity of a pre-submit risk. `Hard` risks abort the submission outright;
/// `Soft` risks proceed only when the caller explicitly acknowledges them
/// (`i_understand` in cool-tools / cool-mcp, the interactive prompt in the CLI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Hard,
    Soft,
}

/// One condition flagged by the pre-submit safety check (`assignments::preflight`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitRisk {
    /// Stable machine code so callers can branch without parsing `message`:
    /// `type_mismatch`, `locked`, `not_yet_unlocked`, `past_lock_date`,
    /// `disallowed_extension`, `attempts_exhausted` (all `Hard`);
    /// `past_due`, `overwrites_existing` (`Soft`).
    pub code: String,
    pub severity: RiskSeverity,
    /// Human-readable explanation, safe to show the user verbatim.
    pub message: String,
}

/// Result of `assignments::preflight` — what a submission *would* look like and
/// every risk attached to it, computed without submitting anything.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitPreflight {
    pub course_id: i64,
    pub assignment_id: i64,
    pub assignment_name: String,
    /// Canvas `submission_type` that would be used: `online_upload` (files) or
    /// `online_text_entry` (text).
    pub submission_type: String,
    pub due_at: Option<String>,
    pub lock_at: Option<String>,
    /// Whether the user already has a submission. Re-submitting adds a new
    /// attempt; the previous one stays in Canvas's history.
    pub has_existing_submission: bool,
    /// Every flagged risk. Empty = clean. Any `Hard` entry means `submit` refuses.
    pub risks: Vec<SubmitRisk>,
}

/// Receipt for a completed submission, built from the `Submission` object
/// Canvas returns from `POST .../submissions`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmissionReceipt {
    pub course_id: i64,
    pub assignment_id: i64,
    /// `submitted` / `pending_review` / `graded` / …
    pub workflow_state: Option<String>,
    pub submission_type: Option<String>,
    pub submitted_at: Option<String>,
    /// Attempt number Canvas recorded — re-submissions increment this.
    pub attempt: Option<i64>,
    /// True when Canvas marked the submission late (turned in past `due_at`).
    pub late: Option<bool>,
    /// Canvas URL to view the submission in a browser.
    pub preview_url: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Page (Canvas wiki)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageSummary {
    pub course_id: i64,
    /// URL slug — Canvas's primary key for pages within a course.
    pub url: String,
    pub title: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageDetail {
    pub course_id: i64,
    pub url: String,
    pub title: String,
    /// Markdown body (HTML→Markdown via htmd).
    pub body_md: String,
    /// Raw HTML body, populated only when `with_html=true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub updated_at: Option<String>,
    pub html_url: Option<String>,
    /// Canvas-internal references mined from the page body HTML.
    pub references: Vec<CanvasRef>,
}
