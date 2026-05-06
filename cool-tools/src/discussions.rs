//! Discussion topics (course-scoped).

use anyhow::Result;
use futures::StreamExt;

use cool_api::generated::endpoints;
pub use cool_api::generated::models::DiscussionTopic;
use cool_api::generated::params::ListDiscussionTopicsCoursesParams;
use cool_api::CoolClient;

use crate::attachments;
use crate::text;
use crate::types::{DiscussionDetail, DiscussionEntry, DiscussionSummary};

pub async fn list(client: &CoolClient, course_id: &str) -> Result<Vec<DiscussionTopic>> {
    let params = ListDiscussionTopicsCoursesParams {
        include: None,
        order_by: None,
        scope: None,
        only_announcements: None,
        filter_by: None,
        search_term: None,
        exclude_context_module_locked_topics: None,
    };

    let mut out: Vec<DiscussionTopic> = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_discussion_topics_courses(
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

fn topic_to_summary(t: &DiscussionTopic, course_id: i64) -> Option<DiscussionSummary> {
    Some(DiscussionSummary {
        id: t.id?,
        course_id,
        title: t.title.clone().unwrap_or_default(),
        posted_at: t.posted_at.map(|t| t.to_rfc3339()),
        author_name: t.user_name.clone(),
        html_url: t.html_url.clone(),
    })
}

pub async fn list_summaries(
    client: &CoolClient,
    course_id: i64,
) -> Result<Vec<DiscussionSummary>> {
    let course_id_str = course_id.to_string();
    let topics = list(client, &course_id_str).await?;
    Ok(topics
        .iter()
        .filter_map(|t| topic_to_summary(t, course_id))
        .collect())
}

/// Get one discussion topic. When `with_entries == true`, also fetch the
/// top-level entries via `/courses/:cid/discussion_topics/:tid/entries`.
pub async fn get_detail(
    client: &CoolClient,
    course_id: i64,
    topic_id: i64,
    with_entries: bool,
    with_html: bool,
) -> Result<DiscussionDetail> {
    let topic_id_str = topic_id.to_string();
    let course_id_str = course_id.to_string();
    let t = show(client, &course_id_str, &topic_id_str).await?;

    let entries = if with_entries {
        fetch_entries(client, course_id, topic_id, with_html)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let raw_html = t.message.as_deref();
    let message_md = raw_html.map(text::html_to_md).unwrap_or_default();
    let message_html = if with_html { raw_html.map(str::to_string) } else { None };
    let references = raw_html.map(attachments::extract_references).unwrap_or_default();

    Ok(DiscussionDetail {
        id: t.id.unwrap_or(topic_id),
        course_id,
        title: t.title.unwrap_or_default(),
        message_md,
        message_html,
        posted_at: t.posted_at.map(|t| t.to_rfc3339()),
        author_name: t.user_name,
        html_url: t.html_url,
        references,
        entries,
    })
}

/// Fetch top-level entries for a topic.
///
/// The generated `list_topic_entries_courses` endpoint returns `Result<()>`
/// (ignores the response body — generation bug), so we hand-roll a paginated
/// untyped GET and pluck the fields we need.
async fn fetch_entries(
    client: &CoolClient,
    course_id: i64,
    topic_id: i64,
    with_html: bool,
) -> Result<Vec<DiscussionEntry>> {
    use cool_api::client::PaginatedResponse;

    let mut entries: Vec<DiscussionEntry> = Vec::new();
    let mut next_url: Option<String> = None;
    let initial = format!(
        "/api/v1/courses/{}/discussion_topics/{}/entries",
        course_id, topic_id
    );

    loop {
        let path = next_url.as_deref().unwrap_or(&initial);
        let page: PaginatedResponse<serde_json::Value> =
            client.get_paginated(path, None::<&()>).await?;

        for raw in page.items {
            let Some(id) = raw.get("id").and_then(|v| v.as_i64()) else {
                continue;
            };
            let raw_msg = raw.get("message").and_then(|v| v.as_str());
            entries.push(DiscussionEntry {
                id,
                author_name: raw
                    .get("user_name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                message_md: raw_msg.map(text::html_to_md).unwrap_or_default(),
                message_html: if with_html { raw_msg.map(str::to_string) } else { None },
                posted_at: raw
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            });
        }
        match page.next_url {
            Some(u) => next_url = Some(u),
            None => break,
        }
    }
    Ok(entries)
}
