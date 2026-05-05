//! Canvas wiki pages (course-scoped).
//!
//! Canvas's primary key for pages is the URL slug, not the numeric `page_id` —
//! that's what every other Canvas surface (web UI, module item, deep-link)
//! references. We honour that convention.

use anyhow::Result;
use futures::StreamExt;

use cool_api::generated::endpoints;
use cool_api::generated::models::Page as CanvasPage;
use cool_api::generated::params::ListPagesCoursesParams;
use cool_api::CoolClient;

use crate::text;
use crate::types::{PageDetail, PageSummary};

pub async fn list_summaries(
    client: &CoolClient,
    course_id: i64,
) -> Result<Vec<PageSummary>> {
    let params = ListPagesCoursesParams {
        sort: None,
        order: None,
        search_term: None,
        published: None,
        include: None,
    };

    let course_id_str = course_id.to_string();
    let mut out: Vec<PageSummary> = Vec::new();
    let mut stream =
        std::pin::pin!(endpoints::list_pages_courses(client, &course_id_str, &params));
    while let Some(item) = stream.next().await {
        let page: CanvasPage = item?;
        out.push(PageSummary {
            course_id,
            url: page.url.unwrap_or_default(),
            title: page.title.unwrap_or_default(),
            updated_at: page.updated_at.map(|t| t.to_rfc3339()),
        });
    }
    Ok(out)
}

pub async fn get_detail(
    client: &CoolClient,
    course_id: i64,
    url_or_id: &str,
) -> Result<PageDetail> {
    let course_id_str = course_id.to_string();
    let page = endpoints::show_page_courses(client, &course_id_str, url_or_id).await?;
    Ok(PageDetail {
        course_id,
        url: page.url.unwrap_or_else(|| url_or_id.to_string()),
        title: page.title.unwrap_or_default(),
        body_text: page.body.as_deref().map(text::html_to_text).unwrap_or_default(),
        updated_at: page.updated_at.map(|t| t.to_rfc3339()),
        html_url: None,
    })
}
