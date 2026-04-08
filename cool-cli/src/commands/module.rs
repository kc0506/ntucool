use anyhow::Result;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};
use serde::{Deserialize, Serialize};

use crate::output::OutputFormat;

// Custom structs instead of generated models because the generated `ModuleItem`
// is modeled after the CoursePace API context (fields: `module_item_type`,
// `assignment_title`, `assignment_link`), not the standard Canvas module items
// returned by `GET /courses/:id/modules?include[]=items` which uses `title`,
// `type`, `html_url`. Deserializing the standard response into the generated
// struct would silently drop all useful fields.

/// Module with items as returned by Canvas API with include=items.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanvasModule {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub position: Option<i64>,
    #[serde(default)]
    pub items: Option<Vec<CanvasModuleItem>>,
}

/// Module item as returned by Canvas API (inside module.items).
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

#[derive(Subcommand)]
pub enum ModuleCommand {
    /// List modules for a course
    List(ModuleListArgs),
    /// Show module item details
    Show(ModuleShowArgs),
}

#[derive(Parser)]
pub struct ModuleListArgs {
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

#[derive(Parser)]
pub struct ModuleShowArgs {
    /// Module ID
    pub id: String,
    /// Course ID or name
    #[arg(short, long)]
    pub course: String,
}

pub async fn run(cmd: ModuleCommand, opts: &super::GlobalOpts) -> Result<()> {
    let client = super::get_client()?;
    let fmt = OutputFormat::from_flag(opts.json);

    match cmd {
        ModuleCommand::List(args) => list(&client, &args, fmt).await,
        ModuleCommand::Show(args) => show(&client, &args, fmt).await,
    }
}

async fn fetch_modules_paginated(
    client: &cool_api::CoolClient,
    course_id: i64,
    include: &[&str],
) -> Result<Vec<CanvasModule>> {
    let include_str = include.join(",");
    let query = [("include[]", include_str.as_str()), ("per_page", "50")];

    let mut all_modules: Vec<CanvasModule> = Vec::new();
    let mut next_url: Option<String> = None;

    loop {
        let path = next_url.unwrap_or_else(|| {
            format!("/api/v1/courses/{}/modules", course_id)
        });

        let page: cool_api::client::PaginatedResponse<CanvasModule> =
            client.get_paginated(&path, Some(&query)).await?;
        all_modules.extend(page.items);

        match page.next_url {
            Some(url) => next_url = Some(url),
            None => break,
        }
    }

    Ok(all_modules)
}

async fn list(
    client: &cool_api::CoolClient,
    args: &ModuleListArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;
    let modules = fetch_modules_paginated(client, course_id, &["items"]).await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&modules)?);
        }
        OutputFormat::Table => {
            for m in &modules {
                let name = m.name.as_deref().unwrap_or("(unnamed module)");
                println!("Module: \"{}\"", name);

                match &m.items {
                    Some(items) if !items.is_empty() => {
                        for item in items {
                            let type_icon = match item
                                .item_type
                                .as_deref()
                                .unwrap_or("")
                            {
                                "File" => "📁",
                                "Page" => "📄",
                                "Assignment" => "📝",
                                "Discussion" => "💬",
                                "Quiz" => "❓",
                                "ExternalUrl" | "ExternalTool" => "🔗",
                                "SubHeader" => "──",
                                _ => "  ",
                            };
                            let title =
                                item.title.as_deref().unwrap_or("(untitled)");
                            let itype =
                                item.item_type.as_deref().unwrap_or("Unknown");
                            println!("  {} {}: {}", type_icon, itype, title);
                        }
                    }
                    _ => {
                        println!("  (no items)");
                    }
                }
                println!();
            }
        }
    }

    Ok(())
}

async fn show(
    client: &cool_api::CoolClient,
    args: &ModuleShowArgs,
    fmt: OutputFormat,
) -> Result<()> {
    let course_id = super::course::resolve_course(client, &args.course).await?;

    let query = [
        ("include[]", "items"),
        ("include[]", "content_details"),
    ];
    let module: CanvasModule = client
        .get(
            &format!("/api/v1/courses/{}/modules/{}", course_id, args.id),
            Some(&query),
        )
        .await?;

    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&module)?);
        }
        OutputFormat::Table => {
            println!(
                "Module: {}",
                module.name.as_deref().unwrap_or("(unnamed)")
            );
            println!(
                "ID:     {}",
                module.id.map(|id| id.to_string()).unwrap_or_default()
            );
            println!();

            match &module.items {
                Some(items) if !items.is_empty() => {
                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL_CONDENSED);
                    table.set_header(vec!["Type", "Title", "URL"]);

                    for item in items {
                        table.add_row(vec![
                            item.item_type.clone().unwrap_or_default(),
                            item.title.clone().unwrap_or_default(),
                            item.html_url.clone().unwrap_or_else(|| "-".to_string()),
                        ]);
                    }

                    println!("{table}");
                }
                _ => {
                    println!("(no items)");
                }
            }
        }
    }

    Ok(())
}
