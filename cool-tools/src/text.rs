//! HTML → plain text / markdown. Shared by assignments / announcements / discussions.

use scraper::Html;

/// Convert Canvas-rendered HTML to GitHub-flavored markdown via `htmd`. Used as
/// the AI-facing default — fewer tokens than HTML, more structure than plain
/// text. Falls back to the original HTML if conversion errors out (htmd
/// occasionally trips on malformed fragments — better to surface raw than
/// drop content).
pub fn html_to_md(html: &str) -> String {
    htmd::convert(html).unwrap_or_else(|_| html.to_string())
}

pub fn html_to_text(html: &str) -> String {
    let document = Html::parse_fragment(html);
    let mut result = String::new();
    extract_text(&document.tree.root(), &mut result);
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result
}

fn extract_text(node: &ego_tree::NodeRef<scraper::Node>, out: &mut String) {
    for child in node.children() {
        match child.value() {
            scraper::Node::Text(text) => out.push_str(text),
            scraper::Node::Element(el) => {
                let tag = el.name();
                let is_block = matches!(
                    tag,
                    "p" | "div"
                        | "br"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "li"
                        | "tr"
                        | "blockquote"
                        | "pre"
                );
                if tag == "br" {
                    out.push('\n');
                }
                if is_block && tag != "br" && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                extract_text(&child, out);
                if is_block && tag != "br" {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
}
