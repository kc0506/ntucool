//! Mine Canvas-internal references out of an HTML fragment.
//!
//! Canvas embeds links to resources inside `description` / `message` HTML —
//! files (`/files/{id}` or `/courses/{cid}/files/{id}`), wiki pages
//! (`/courses/{cid}/pages/{slug}`), other assignments, discussion topics,
//! modules. There is no separate typed `attachments` field on Assignment /
//! DiscussionTopic / Page, so callers that want a structured list have to
//! mine the HTML themselves.

use scraper::{Html, Selector};

use crate::types::CanvasRef;

/// Extract every Canvas-internal reference (`<a href>`) from an HTML fragment.
///
/// Recognises all of:
///   `/files/{id}`
///   `/courses/{cid}/files/{id}`
///   `/courses/{cid}/pages/{slug}`
///   `/courses/{cid}/assignments/{id}`
///   `/courses/{cid}/discussion_topics/{id}`
///   `/courses/{cid}/modules/{id}`
///
/// Other links (mailto, external sites) are skipped.
pub fn extract_references(html: &str) -> Vec<CanvasRef> {
    let document = Html::parse_fragment(html);
    let a_selector = Selector::parse("a").expect("static CSS selector 'a' is always valid");

    let mut out: Vec<CanvasRef> = Vec::new();
    for el in document.select(&a_selector) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let name = link_text(&el);
        if let Some(r) = parse_canvas_ref(href, &name) {
            out.push(r);
        }
    }
    out
}

fn link_text(el: &scraper::ElementRef) -> String {
    let raw = el.text().collect::<String>();
    let trimmed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed
}

/// Try every Canvas URL shape we recognise; return the first match.
fn parse_canvas_ref(href: &str, link_text: &str) -> Option<CanvasRef> {
    // Strip query/hash and any host prefix; keep only the path portion.
    let path = strip_to_path(href);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // /files/{id}
    if let [.., "files", id] = segs.as_slice() {
        if !path_starts_with_courses(&segs) {
            if let Ok(id) = id.parse::<i64>() {
                return Some(CanvasRef::File {
                    id,
                    name: name_or_fallback(link_text, id.to_string()),
                    href: href.to_string(),
                });
            }
        }
    }

    // /courses/{cid}/files/{id}
    if let Some((cid, "files", id)) = course_scoped_pair(&segs) {
        if let Ok(id) = id.parse::<i64>() {
            return Some(CanvasRef::File {
                id,
                name: name_or_fallback(link_text, id.to_string()),
                href: href.to_string(),
            });
        }
        let _ = cid;
    }

    // /courses/{cid}/pages/{slug}
    if let Some((cid, "pages", slug)) = course_scoped_pair(&segs) {
        return Some(CanvasRef::Page {
            course_id: cid,
            slug: slug.to_string(),
            name: name_or_fallback(link_text, slug.to_string()),
            href: href.to_string(),
        });
    }

    // /courses/{cid}/assignments/{id}
    if let Some((cid, "assignments", id)) = course_scoped_pair(&segs) {
        if let Ok(id) = id.parse::<i64>() {
            return Some(CanvasRef::Assignment {
                course_id: cid,
                id,
                name: name_or_fallback(link_text, id.to_string()),
                href: href.to_string(),
            });
        }
    }

    // /courses/{cid}/discussion_topics/{id}
    if let Some((cid, "discussion_topics", id)) = course_scoped_pair(&segs) {
        if let Ok(id) = id.parse::<i64>() {
            return Some(CanvasRef::DiscussionTopic {
                course_id: cid,
                id,
                name: name_or_fallback(link_text, id.to_string()),
                href: href.to_string(),
            });
        }
    }

    // /courses/{cid}/modules/{id}
    if let Some((cid, "modules", id)) = course_scoped_pair(&segs) {
        if let Ok(id) = id.parse::<i64>() {
            return Some(CanvasRef::Module {
                course_id: cid,
                id,
                name: name_or_fallback(link_text, id.to_string()),
                href: href.to_string(),
            });
        }
    }

    None
}

/// Match `["courses", "<cid>", "<kind>", "<id_or_slug>", ...]`. Returns
/// `(cid, kind, last_seg)` where `last_seg` is the segment immediately
/// after `kind` (id or slug), so callers don't trip on Canvas's trailing
/// `/edit`, `/preview`, etc.
fn course_scoped_pair<'a>(segs: &[&'a str]) -> Option<(i64, &'a str, &'a str)> {
    if segs.len() < 4 {
        return None;
    }
    if segs[0] != "courses" {
        return None;
    }
    let cid: i64 = segs[1].parse().ok()?;
    Some((cid, segs[2], segs[3]))
}

fn path_starts_with_courses(segs: &[&str]) -> bool {
    segs.first().copied() == Some("courses")
}

fn strip_to_path(href: &str) -> String {
    // Drop scheme://host
    let stripped = href.split_once("://").map(|(_, rest)| rest).unwrap_or(href);
    let path = stripped
        .split_once('/')
        .map(|(_, rest)| format!("/{}", rest))
        .unwrap_or_else(|| stripped.to_string());
    // Drop ?query and #hash
    let path = path.split_once('?').map(|(p, _)| p.to_string()).unwrap_or(path);
    let path = path.split_once('#').map(|(p, _)| p.to_string()).unwrap_or(path);
    path
}

fn name_or_fallback(text: &str, fallback: String) -> String {
    if text.trim().is_empty() {
        fallback
    } else {
        text.trim().to_string()
    }
}

/// Backward-compatibility helper for callers (and tests) that only care about
/// File references. Returns just the File-kind references as plain (id, name, href).
pub fn extract_file_refs(html: &str) -> Vec<CanvasRef> {
    extract_references(html)
        .into_iter()
        .filter(|r| matches!(r, CanvasRef::File { .. }))
        .collect()
}
