//! Discussion topics (course-scoped).

use anyhow::Result;
use futures::StreamExt;

use cool_api::generated::endpoints;
pub use cool_api::generated::models::DiscussionTopic;
use cool_api::generated::params::ListDiscussionTopicsCoursesParams;
use cool_api::CoolClient;

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
