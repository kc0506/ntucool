//! Announcements — Canvas exposes them via `discussion_topics` filtered by
//! announcement flag, and via the global `/announcements` endpoint scoped by
//! `context_codes[]=course_<id>`.

use anyhow::Result;

use cool_api::client::PaginatedResponse;
pub use cool_api::generated::models::DiscussionTopic;
use cool_api::CoolClient;

use crate::attachments;
use crate::text;
use crate::types::{AnnouncementDetail, AnnouncementSummary};

/// List announcements for one course (most recent first, per Canvas default).
///
/// Manual pagination with tuple query params: `ListAnnouncementsParams` has
/// `context_codes: Vec<String>` which `serde_urlencoded` cannot serialize.
pub async fn list(client: &CoolClient, course_id: i64) -> Result<Vec<DiscussionTopic>> {
    let context_code = format!("course_{}", course_id);
    let query = [
        ("context_codes[]", context_code.as_str()),
        ("per_page", "50"),
    ];

    let mut out: Vec<DiscussionTopic> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.as_deref().unwrap_or("/api/v1/announcements");
        let page: PaginatedResponse<DiscussionTopic> =
            client.get_paginated(path, Some(&query)).await?;
        out.extend(page.items);
        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }
    Ok(out)
}

pub async fn show(
    client: &CoolClient,
    course_id: i64,
    topic_id: &str,
) -> Result<DiscussionTopic> {
    let topic: DiscussionTopic = client
        .get(
            &format!(
                "/api/v1/courses/{}/discussion_topics/{}",
                course_id, topic_id
            ),
            None::<&()>,
        )
        .await?;
    Ok(topic)
}

// ────────────────────────────────────────────────────────────────────────────
// Contract-shape adapters
// ────────────────────────────────────────────────────────────────────────────

fn topic_to_summary(t: &DiscussionTopic, course_id: i64) -> Option<AnnouncementSummary> {
    Some(AnnouncementSummary {
        id: t.id?,
        course_id,
        title: t.title.clone().unwrap_or_default(),
        posted_at: t.posted_at.map(|t| t.to_rfc3339()),
        html_url: t.html_url.clone(),
    })
}

/// List announcements across one or more courses, optionally newer than `since`.
/// Empty `course_ids` is honoured by fetching the user's currently-active
/// enrolments (Canvas `/announcements` itself requires at least one
/// `context_codes[]` argument, so the expansion happens client-side).
pub async fn list_summaries(
    client: &CoolClient,
    course_ids: &[i64],
    since: Option<&str>,
) -> Result<Vec<AnnouncementSummary>> {
    let resolved: Vec<i64> = if course_ids.is_empty() {
        crate::courses::list_active(client)
            .await?
            .into_iter()
            .filter_map(|c| c.id)
            .collect()
    } else {
        course_ids.to_vec()
    };

    let mut out: Vec<AnnouncementSummary> = Vec::new();
    for &cid in &resolved {
        let topics = list(client, cid).await?;
        for t in &topics {
            if let Some(threshold) = since {
                if let Some(posted) = t.posted_at {
                    if posted.to_rfc3339().as_str() < threshold {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            if let Some(s) = topic_to_summary(t, cid) {
                out.push(s);
            }
        }
    }
    Ok(out)
}

pub async fn get_detail(
    client: &CoolClient,
    course_id: i64,
    topic_id: i64,
    with_html: bool,
) -> Result<AnnouncementDetail> {
    let topic_id_str = topic_id.to_string();
    let t = show(client, course_id, &topic_id_str).await?;
    let raw_html = t.message.as_deref();
    let body_md = raw_html.map(text::html_to_md).unwrap_or_default();
    let body_html = if with_html { raw_html.map(str::to_string) } else { None };
    let references = raw_html.map(attachments::extract_references).unwrap_or_default();
    Ok(AnnouncementDetail {
        id: t.id.unwrap_or(topic_id),
        course_id,
        title: t.title.unwrap_or_default(),
        body_md,
        body_html,
        posted_at: t.posted_at.map(|t| t.to_rfc3339()),
        html_url: t.html_url,
        references,
    })
}
