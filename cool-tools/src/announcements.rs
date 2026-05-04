//! Announcements — Canvas exposes them via `discussion_topics` filtered by
//! announcement flag, and via the global `/announcements` endpoint scoped by
//! `context_codes[]=course_<id>`.

use anyhow::Result;

use cool_api::client::PaginatedResponse;
pub use cool_api::generated::models::DiscussionTopic;
use cool_api::CoolClient;

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
