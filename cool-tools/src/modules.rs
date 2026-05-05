//! Modules — list and show, with the Canvas-shaped item structs the standard
//! `/modules?include[]=items` response uses (which differs from the generated
//! `ModuleItem` struct, modeled after the CoursePace API).

use anyhow::Result;
use serde::{Deserialize, Serialize};

use cool_api::client::PaginatedResponse;
use cool_api::CoolClient;

use crate::types::{ModuleDetail, ModuleItem as ContractModuleItem, ModuleSummary};

/// Module with items as returned by `GET /courses/:id/modules?include[]=items`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanvasModule {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub position: Option<i64>,
    #[serde(default)]
    pub items: Option<Vec<CanvasModuleItem>>,
}

/// Item inside a `CanvasModule`. `type` covers File / Page / Assignment /
/// Discussion / Quiz / ExternalUrl / ExternalTool / SubHeader.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanvasModuleItem {
    pub id: Option<i64>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub html_url: Option<String>,
    pub position: Option<i64>,
    pub indent: Option<i64>,
    pub content_id: Option<i64>,
}

/// List modules for a course, including their items.
///
/// `include` defaults to `["items"]`; pass `&["items", "content_details"]` for
/// richer file metadata.
pub async fn list_with_items(
    client: &CoolClient,
    course_id: i64,
    include: &[&str],
) -> Result<Vec<CanvasModule>> {
    let include_str = if include.is_empty() {
        "items".to_string()
    } else {
        include.join(",")
    };
    let query = [("include[]", include_str.as_str()), ("per_page", "50")];

    let mut all: Vec<CanvasModule> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.unwrap_or_else(|| format!("/api/v1/courses/{}/modules", course_id));
        let page: PaginatedResponse<CanvasModule> =
            client.get_paginated(&path, Some(&query)).await?;
        all.extend(page.items);
        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }
    Ok(all)
}

pub async fn show_with_items(
    client: &CoolClient,
    course_id: i64,
    module_id: &str,
) -> Result<CanvasModule> {
    let query = [("include[]", "items"), ("include[]", "content_details")];
    let module: CanvasModule = client
        .get(
            &format!("/api/v1/courses/{}/modules/{}", course_id, module_id),
            Some(&query),
        )
        .await?;
    Ok(module)
}

// ────────────────────────────────────────────────────────────────────────────
// Contract-shape adapters
// ────────────────────────────────────────────────────────────────────────────

/// List modules (without items) for a course. Paginated, items not included.
pub async fn list_summaries(
    client: &CoolClient,
    course_id: i64,
) -> Result<Vec<ModuleSummary>> {
    let query = [("per_page", "50")];

    let mut all: Vec<ModuleSummary> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.unwrap_or_else(|| format!("/api/v1/courses/{}/modules", course_id));
        let page: PaginatedResponse<serde_json::Value> =
            client.get_paginated(&path, Some(&query)).await?;
        for raw in page.items {
            let Some(id) = raw.get("id").and_then(|v| v.as_i64()) else {
                continue;
            };
            all.push(ModuleSummary {
                id,
                course_id,
                name: raw
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                position: raw.get("position").and_then(|v| v.as_i64()),
                items_count: raw.get("items_count").and_then(|v| v.as_i64()),
            });
        }
        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }
    Ok(all)
}

fn item_to_contract(i: &CanvasModuleItem) -> Option<ContractModuleItem> {
    Some(ContractModuleItem {
        id: i.id?,
        title: i.title.clone().unwrap_or_default(),
        item_type: i.item_type.clone().unwrap_or_default(),
        content_id: i.content_id,
        url: i.html_url.clone(),
        position: i.position,
        indent: i.indent,
    })
}

pub async fn show_detail(
    client: &CoolClient,
    course_id: i64,
    module_id: i64,
) -> Result<ModuleDetail> {
    let module_id_str = module_id.to_string();
    let m = show_with_items(client, course_id, &module_id_str).await?;
    Ok(ModuleDetail {
        id: m.id.unwrap_or(module_id),
        course_id,
        name: m.name.unwrap_or_default(),
        position: m.position,
        items: m
            .items
            .unwrap_or_default()
            .iter()
            .filter_map(item_to_contract)
            .collect(),
    })
}
