//! PDF text extraction + content search.
//!
//! Two-layer cache:
//!   * Bytes cache (owned by `files::cache_or_download`) — the original PDF.
//!   * Text cache  (owned by this module) — JSON sidecar listing per-page text,
//!     keyed by the same `(file_id, updated_at_unix)` so it stays in lockstep
//!     with the bytes cache.
//!
//! Search is naive: we extract every PDF in the course on first call (slow),
//! cache the text, then run a case-insensitive substring scan on subsequent
//! calls (fast). No inverted index yet — adequate up to a few hundred PDFs;
//! revisit if a course's `pdf_search` start blowing through MCP timeouts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use cool_api::client::PaginatedResponse;
use cool_api::generated::models::File;
use cool_api::CoolClient;

use crate::files;
use crate::types::{PdfExtractResult, PdfPage, PdfSearchHit};

// ────────────────────────────────────────────────────────────────────────────
// Page range parsing
// ────────────────────────────────────────────────────────────────────────────

/// Inclusive page selector. `None` on either bound means "open" — `extract`
/// applies these as filters over the full page list.
#[derive(Debug, Clone, Default)]
pub struct PageRange {
    pub start: Option<usize>,
    pub end: Option<usize>,
}

impl PageRange {
    /// Accepted forms: "all" (or empty), "5", "5-10". 1-indexed.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("all") {
            return Ok(Self::default());
        }
        if let Some((a, b)) = s.split_once('-') {
            let start: usize = a.trim().parse().context("invalid start in range")?;
            let end: usize = b.trim().parse().context("invalid end in range")?;
            if start == 0 || end < start {
                anyhow::bail!("invalid page range \"{s}\": expected 1-indexed N or N-M with N<=M");
            }
            return Ok(Self {
                start: Some(start),
                end: Some(end),
            });
        }
        let n: usize = s.parse().context("invalid page number")?;
        if n == 0 {
            anyhow::bail!("page numbers are 1-indexed");
        }
        Ok(Self {
            start: Some(n),
            end: Some(n),
        })
    }

    fn contains(&self, page_no: usize) -> bool {
        let lo = self.start.unwrap_or(1);
        let hi = self.end.unwrap_or(usize::MAX);
        page_no >= lo && page_no <= hi
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Text cache (per-file JSON sidecar)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedExtraction {
    display_name: String,
    pages: Vec<String>,
}

fn text_cache_path(text_cache_dir: &Path, file_id: i64, updated_at_unix: i64) -> PathBuf {
    text_cache_dir.join(format!("{file_id}-{updated_at_unix}.json"))
}

async fn read_text_cache(p: &Path) -> Option<CachedExtraction> {
    let bytes = tokio::fs::read(p).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn write_text_cache(p: &Path, c: &CachedExtraction) -> Result<()> {
    if let Some(parent) = p.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let bytes = serde_json::to_vec(c)?;
    tokio::fs::write(p, &bytes).await.context("write text cache")?;
    Ok(())
}

/// Get `(display_name, pages)` for a file, downloading and extracting on first
/// call and reading the JSON sidecar on subsequent ones. Both caches keyed by
/// `(file_id, updated_at_unix)` so a Canvas re-upload invalidates everything.
pub async fn extract_pages(
    client: &CoolClient,
    file_id: i64,
    cache_dir: &Path,
    text_cache_dir: &Path,
) -> Result<(String, Vec<String>)> {
    let cached = files::cache_or_download(client, file_id, cache_dir).await?;
    let text_path = text_cache_path(text_cache_dir, file_id, cached.updated_at_unix);

    if let Some(c) = read_text_cache(&text_path).await {
        return Ok((c.display_name, c.pages));
    }

    let path_for_extract = cached.path.clone();
    // pdf-extract is sync + can spend seconds on a large PDF — keep it off the
    // tokio worker pool. spawn_blocking also catches panics from the parser.
    // Use the per-page API: extract_text() concatenates without page markers,
    // so we'd lose the page boundaries pdf_search needs.
    let pages = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        pdf_extract::extract_text_by_pages(&path_for_extract).context("pdf-extract failed")
    })
    .await
    .context("pdf-extract task panicked")??;

    let entry = CachedExtraction {
        display_name: cached.display_name.clone(),
        pages,
    };
    write_text_cache(&text_path, &entry).await.ok();
    Ok((entry.display_name, entry.pages))
}

// ────────────────────────────────────────────────────────────────────────────
// Public contract-shape API
// ────────────────────────────────────────────────────────────────────────────

pub async fn extract(
    client: &CoolClient,
    file_id: i64,
    range: Option<&PageRange>,
    cache_dir: &Path,
    text_cache_dir: &Path,
) -> Result<PdfExtractResult> {
    let (display_name, pages) = extract_pages(client, file_id, cache_dir, text_cache_dir).await?;
    let page_count = pages.len();
    let total_chars: usize = pages.iter().map(|p| p.len()).sum();
    let empty = total_chars == 0;

    let pages_out: Vec<PdfPage> = pages
        .into_iter()
        .enumerate()
        .map(|(i, t)| (i + 1, t))
        .filter(|(n, _)| range.map_or(true, |r| r.contains(*n)))
        .map(|(n, t)| PdfPage { page_no: n, text: t })
        .collect();

    Ok(PdfExtractResult {
        file_id,
        display_name,
        page_count,
        pages: pages_out,
        empty,
    })
}

/// Search every PDF in `course_id` for a case-insensitive substring `query`.
///
/// First call extracts (and caches) every PDF — can take a long time on a
/// course with many large files. Subsequent searches reuse the JSON sidecars
/// so they're sub-second. Files that fail to parse are silently skipped (the
/// AI client is told via `tracing::warn!`, but the search still completes).
pub async fn search_in_course(
    client: &CoolClient,
    course_id: i64,
    query: &str,
    max_results: usize,
    cache_dir: &Path,
    text_cache_dir: &Path,
) -> Result<Vec<PdfSearchHit>> {
    if query.trim().is_empty() {
        anyhow::bail!("query is empty");
    }
    let pdfs = list_pdf_files(client, course_id).await?;
    let needle = query.to_lowercase();

    let mut hits: Vec<PdfSearchHit> = Vec::new();
    for f in pdfs {
        if hits.len() >= max_results {
            break;
        }
        let Some(file_id) = f.id else { continue };
        let fallback_name = f
            .display_name
            .clone()
            .or_else(|| f.filename.clone())
            .unwrap_or_default();

        let (display, pages) = match extract_pages(client, file_id, cache_dir, text_cache_dir).await
        {
            Ok((name, pages)) => {
                let display = if name.is_empty() { fallback_name } else { name };
                (display, pages)
            }
            Err(e) => {
                eprintln!("pdf_search: extract file_id={file_id} failed: {e:#}");
                continue;
            }
        };

        for (i, page_text) in pages.iter().enumerate() {
            if hits.len() >= max_results {
                break;
            }
            let lower = page_text.to_lowercase();
            if let Some(off) = lower.find(&needle) {
                let snippet = make_snippet(page_text, off, query.len());
                hits.push(PdfSearchHit {
                    file_id,
                    display_name: display.clone(),
                    page: i + 1,
                    snippet,
                });
            }
        }
    }
    Ok(hits)
}

async fn list_pdf_files(client: &CoolClient, course_id: i64) -> Result<Vec<File>> {
    let path = format!("/api/v1/courses/{}/files", course_id);
    let query: [(&str, &str); 2] = [("content_types[]", "application/pdf"), ("per_page", "50")];

    let mut all: Vec<File> = Vec::new();
    let mut next_url: Option<String> = None;
    loop {
        let p = next_url.as_deref().unwrap_or(&path);
        let page: PaginatedResponse<File> = client.get_paginated(p, Some(&query)).await?;
        all.extend(page.items);
        match page.next_url {
            Some(u) => next_url = Some(u),
            None => break,
        }
    }
    Ok(all)
}

fn make_snippet(page_text: &str, byte_off: usize, match_len: usize) -> String {
    let lo = byte_off.saturating_sub(80);
    let hi = (byte_off + match_len + 80).min(page_text.len());
    let lo = next_char_boundary_back(page_text, lo);
    let hi = next_char_boundary_fwd(page_text, hi);
    let raw = &page_text[lo..hi];
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn next_char_boundary_back(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary_fwd(s: &str, mut i: usize) -> usize {
    let len = s.len();
    while i < len && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}
