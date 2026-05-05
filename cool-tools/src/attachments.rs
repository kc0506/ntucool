//! Extract file references from Canvas-rendered HTML.
//!
//! Canvas embeds attachments inside `description` / `message` HTML as
//! `<a class="instructure_file_link" href=".../files/{id}">name</a>`. There is
//! no separate typed `attachments` field on Assignment / DiscussionTopic /
//! Page, so callers that want a structured list have to mine the HTML.

use scraper::{Html, Selector};

use crate::types::AttachmentRef;

/// Extract every `<a href=".../files/{id}">` reference from an HTML fragment.
/// Drops links whose file_id can't be parsed as i64.
pub fn extract_attachments(html: &str) -> Vec<AttachmentRef> {
    let document = Html::parse_fragment(html);
    let a_selector = Selector::parse("a").expect("static CSS selector 'a' is always valid");

    let mut out: Vec<AttachmentRef> = Vec::new();
    for el in document.select(&a_selector) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        if !href.contains("/files/") {
            continue;
        }
        let id_str = href
            .split("/files/")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.split('?').next());
        let Some(id) = id_str.and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let name = el.text().collect::<String>();
        let name = if name.trim().is_empty() {
            href.split('/').last().unwrap_or("file").to_string()
        } else {
            name.trim().to_string()
        };
        out.push(AttachmentRef {
            id,
            name,
            url: href.to_string(),
        });
    }
    out
}
