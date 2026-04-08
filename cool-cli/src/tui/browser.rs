use std::cmp::Ordering;
use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Terminal,
};
use scraper::{Html, Selector};

use cool_api::generated::endpoints;
use cool_api::generated::models::{Assignment, Course};
use cool_api::generated::params::ListAssignmentsAssignmentsParams;

use super::fuzzy_select::FuzzySelect;
use super::theme;
use crate::commands::assignment::html_to_text;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

struct FileLink {
    name: String,
    #[allow(dead_code)]
    url: String,
    file_id: Option<String>,
}

fn extract_file_links(description: &str) -> Vec<FileLink> {
    let document = Html::parse_fragment(description);
    let a_selector = Selector::parse("a").expect("static CSS selector 'a' is always valid");
    let mut links = Vec::new();

    for element in document.select(&a_selector) {
        if let Some(href) = element.value().attr("href") {
            if href.contains("/files/") {
                let name = element.text().collect::<String>();
                let name = if name.trim().is_empty() {
                    href.split('/').last().unwrap_or("file").to_string()
                } else {
                    name.trim().to_string()
                };

                let file_id = href
                    .split("/files/")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .and_then(|s| s.split('?').next())
                    .map(|s| s.to_string());

                links.push(FileLink {
                    name,
                    url: href.to_string(),
                    file_id,
                });
            }
        }
    }

    links
}

// ── Content format ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContentFormat {
    None,
    Json,
    Markdown,
    Html,
}

impl ContentFormat {
    const ALL: [ContentFormat; 4] = [
        ContentFormat::None,
        ContentFormat::Json,
        ContentFormat::Markdown,
        ContentFormat::Html,
    ];

    fn label(self) -> &'static str {
        match self {
            ContentFormat::None => "None",
            ContentFormat::Json => "JSON",
            ContentFormat::Markdown => "Markdown",
            ContentFormat::Html => "HTML",
        }
    }
}

// ── Export form state ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormFocus {
    Format(usize),
    File(usize),
    OutputDir,
    Download,
}

struct ExportFormState {
    name: String,
    due: String,
    points: String,
    description_text: String,
    description_scroll: u16,
    file_links: Vec<FileLink>,

    selected_format: ContentFormat,
    file_selected: Vec<bool>,
    output_dir: String,
    focus: FormFocus,
    status_msg: Option<(String, bool)>, // (message, is_success)

    focus_items: Vec<FormFocus>,
}

impl ExportFormState {
    fn new(assignment: &Assignment) -> Self {
        let name = assignment
            .name
            .as_deref()
            .unwrap_or("(unknown)")
            .to_string();
        let due = assignment
            .due_at
            .map(|d| d.format("%m/%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        let points = assignment
            .points_possible
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());
        let description_text = assignment
            .description
            .as_deref()
            .map(|d| html_to_text(d))
            .unwrap_or_default();
        let file_links = assignment
            .description
            .as_deref()
            .map(extract_file_links)
            .unwrap_or_default();
        let file_selected = vec![false; file_links.len()];

        let mut state = ExportFormState {
            name,
            due,
            points,
            description_text,
            description_scroll: 0,
            file_links,
            selected_format: ContentFormat::None,
            file_selected,
            output_dir: "./".to_string(),
            focus: FormFocus::Format(0),
            status_msg: None,
            focus_items: Vec::new(),
        };
        state.rebuild_focus_items();
        state
    }

    fn rebuild_focus_items(&mut self) {
        let mut items = Vec::new();
        for i in 0..ContentFormat::ALL.len() {
            items.push(FormFocus::Format(i));
        }
        for i in 0..self.file_links.len() {
            items.push(FormFocus::File(i));
        }
        items.push(FormFocus::OutputDir);
        items.push(FormFocus::Download);
        self.focus_items = items;
    }

    fn focus_index(&self) -> usize {
        self.focus_items
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0)
    }

    fn move_focus(&mut self, delta: isize) {
        if self.focus_items.is_empty() {
            return;
        }
        let cur = self.focus_index() as isize;
        let len = self.focus_items.len() as isize;
        let next = ((cur + delta) % len + len) % len;
        self.focus = self.focus_items[next as usize];
    }

    fn handle_space(&mut self) {
        match self.focus {
            FormFocus::Format(i) => {
                self.selected_format = ContentFormat::ALL[i];
            }
            FormFocus::File(i) => {
                self.file_selected[i] = !self.file_selected[i];
            }
            _ => {}
        }
    }
}

// ── Styled helpers ───────────────────────────────────────────────────────────

fn key_hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", label),
            Style::default().fg(theme::MUTED),
        ),
    ]
}

fn section_title(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {}", title),
        Style::default()
            .fg(theme::BLUE)
            .add_modifier(Modifier::BOLD),
    ))
}

// ── Main entry ───────────────────────────────────────────────────────────────

pub async fn run_browser(client: &cool_api::CoolClient) -> Result<()> {
    let (mut terminal, _guard) = super::setup_terminal()?;

    loop {
        let courses = crate::commands::course::fetch_courses_cached_pub(client).await?;
        if courses.is_empty() {
            anyhow::bail!("No courses found.");
        }

        let course = match select_course(&mut terminal, &courses)? {
            Some(c) => c,
            None => return Ok(()),
        };

        let course_id = course
            .id
            .ok_or_else(|| anyhow::anyhow!("Selected course has no ID"))?;

        'assignment_loop: loop {
            let assignments = fetch_assignments(client, course_id).await?;
            if assignments.is_empty() {
                eprintln!("No assignments found for this course.");
                break 'assignment_loop;
            }

            let assignment = match select_assignment(&mut terminal, &assignments)? {
                Some(a) => a,
                None => break 'assignment_loop,
            };

            match show_export_form(&mut terminal, client, assignment, course_id).await? {
                DetailAction::Back => continue 'assignment_loop,
                DetailAction::Quit => return Ok(()),
            }
        }
    }
}

// ── Phase 1 & 2: FuzzySelect wrappers ───────────────────────────────────────

fn select_course<'a>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    courses: &'a [Course],
) -> io::Result<Option<&'a Course>> {
    let selector = FuzzySelect::new(
        courses,
        |c, width| {
            let name = c.name.as_deref().unwrap_or("(unknown)");
            let code = c.course_code.as_deref().unwrap_or("");
            let max_name = (width as usize).saturating_sub(code.len() + 4);
            let truncated = truncate_str(name, max_name);
            format!("{:<width$}  {}", truncated, code, width = max_name)
        },
        |c, query| {
            let q = query.to_lowercase();
            let name_match = c
                .name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false);
            let code_match = c
                .course_code
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false);
            name_match || code_match
        },
    )
    .with_prompt("Select Course");

    match selector.run(terminal)? {
        Some(idx) => Ok(Some(&courses[idx])),
        None => Ok(None),
    }
}

fn select_assignment<'a>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    assignments: &'a [Assignment],
) -> io::Result<Option<&'a Assignment>> {
    let selector = FuzzySelect::new(
        assignments,
        |a, width| {
            let name = a.name.as_deref().unwrap_or("(unknown)");
            let due = a
                .due_at
                .map(|d| d.format("%m/%d %H:%M").to_string())
                .unwrap_or_else(|| "-".to_string());
            let pts = a
                .points_possible
                .map(|p| format!("{p}pts"))
                .unwrap_or_else(|| "-".to_string());
            let suffix = format!("  {}  {}", due, pts);
            let max_name = (width as usize).saturating_sub(suffix.len() + 2);
            let truncated = truncate_str(name, max_name);
            format!("{:<width$}{}", truncated, suffix, width = max_name)
        },
        |a, query| {
            let q = query.to_lowercase();
            a.name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false)
        },
    )
    .with_prompt("Select Assignment")
    .with_sort_modes(vec![
        (
            "Due ↑",
            Box::new(|a: &Assignment, b: &Assignment| {
                let da = a.due_at;
                let db = b.due_at;
                match (da, db) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }),
        ),
        (
            "Due ↓",
            Box::new(|a: &Assignment, b: &Assignment| {
                let da = a.due_at;
                let db = b.due_at;
                match (da, db) {
                    (Some(a), Some(b)) => b.cmp(&a),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }),
        ),
    ]);

    match selector.run(terminal)? {
        Some(idx) => Ok(Some(&assignments[idx])),
        None => Ok(None),
    }
}

async fn fetch_assignments(
    client: &cool_api::CoolClient,
    course_id: i64,
) -> Result<Vec<Assignment>> {
    let cid = course_id.to_string();
    let params = ListAssignmentsAssignmentsParams {
        include: None,
        search_term: None,
        override_assignment_dates: None,
        needs_grading_count_by_section: None,
        bucket: None,
        assignment_ids: None,
        order_by: None,
        post_to_sis: None,
        new_quizzes: None,
    };

    let mut assignments = Vec::new();
    let mut stream = std::pin::pin!(endpoints::list_assignments_assignments(
        client, &cid, &params
    ));
    while let Some(item) = stream.next().await {
        assignments.push(item?);
    }
    Ok(assignments)
}

// ── Phase 3: Export form ─────────────────────────────────────────────────────

enum DetailAction {
    Back,
    Quit,
}

async fn show_export_form(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: &cool_api::CoolClient,
    assignment: &Assignment,
    course_id: i64,
) -> Result<DetailAction> {
    let mut state = ExportFormState::new(assignment);

    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            // Main layout: two panels + bottom hint bar
            let main_chunks = Layout::vertical([
                Constraint::Min(1),    // panels
                Constraint::Length(1), // hint bar
            ])
            .split(area);

            let panel_chunks = Layout::horizontal([
                Constraint::Percentage(55),
                Constraint::Percentage(45),
            ])
            .split(main_chunks[0]);

            render_left_panel(frame, panel_chunks[0], &state);
            render_right_panel(frame, panel_chunks[1], &state);
            render_hint_bar(frame, main_chunks[1]);
        })?;

        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            state.status_msg = None;

            match code {
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(DetailAction::Quit);
                }
                KeyCode::Char('q') => return Ok(DetailAction::Quit),
                KeyCode::Esc => return Ok(DetailAction::Back),
                KeyCode::Char('j') => {
                    state.description_scroll = state.description_scroll.saturating_add(1);
                }
                KeyCode::Char('k') => {
                    state.description_scroll = state.description_scroll.saturating_sub(1);
                }
                KeyCode::Down => {
                    state.move_focus(1);
                }
                KeyCode::Up => {
                    state.move_focus(-1);
                }
                KeyCode::Char(' ') => {
                    state.handle_space();
                }
                KeyCode::Char(c) if matches!(state.focus, FormFocus::OutputDir) => {
                    state.output_dir.push(c);
                }
                KeyCode::Backspace if matches!(state.focus, FormFocus::OutputDir) => {
                    state.output_dir.pop();
                }
                KeyCode::Enter => {
                    let msg = execute_export(client, assignment, &state, course_id).await;
                    let is_success = !msg.contains("failed") && !msg.contains("Nothing");
                    state.status_msg = Some((msg, is_success));
                }
                _ => {}
            }
        }
    }
}

// ── Left panel: metadata + description ───────────────────────────────────────

fn render_left_panel(frame: &mut ratatui::Frame, area: Rect, state: &ExportFormState) {
    let block = Block::default()
        .title(Span::styled(
            " Assignment ",
            Style::default()
                .fg(theme::ORANGE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE_DIM));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(4), // metadata
        Constraint::Length(1), // separator
        Constraint::Min(1),    // description
    ])
    .split(inner);

    // Metadata with colored keys
    let meta_style_key = Style::default()
        .fg(theme::BLUE)
        .add_modifier(Modifier::BOLD);
    let meta_style_val = Style::default().fg(theme::FG);

    let meta = vec![
        Line::from(vec![
            Span::styled("  Name   ", meta_style_key),
            Span::styled(&state.name, meta_style_val),
        ]),
        Line::from(vec![
            Span::styled("  Due    ", meta_style_key),
            Span::styled(&state.due, meta_style_val),
        ]),
        Line::from(vec![
            Span::styled("  Points ", meta_style_key),
            Span::styled(&state.points, meta_style_val),
        ]),
        Line::raw(""),
    ];
    frame.render_widget(Paragraph::new(meta), chunks[0]);

    // Separator
    let sep = Paragraph::new(Line::styled(
        " ".to_string() + &"─".repeat(chunks[1].width.saturating_sub(2) as usize),
        Style::default().fg(theme::BLUE_DIM),
    ));
    frame.render_widget(sep, chunks[1]);

    // Description
    let desc_text = if state.description_text.is_empty() {
        "  (no description)".to_string()
    } else {
        state
            .description_text
            .lines()
            .map(|l| format!("  {}", l))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let paragraph = Paragraph::new(desc_text)
        .style(Style::default().fg(theme::FG))
        .wrap(Wrap { trim: false })
        .scroll((state.description_scroll, 0));
    frame.render_widget(paragraph, chunks[2]);
}

// ── Right panel: export form ─────────────────────────────────────────────────

fn render_right_panel(frame: &mut ratatui::Frame, area: Rect, state: &ExportFormState) {
    let block = Block::default()
        .title(Span::styled(
            " Export ",
            Style::default()
                .fg(theme::ORANGE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE_DIM));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Content format section
    lines.push(section_title("Content Format"));
    for (i, fmt) in ContentFormat::ALL.iter().enumerate() {
        let selected = *fmt == state.selected_format;
        let focused = state.focus == FormFocus::Format(i);

        let (bullet, bullet_style) = if selected {
            ("◉ ", Style::default().fg(theme::ORANGE))
        } else {
            ("○ ", Style::default().fg(theme::MUTED))
        };

        let label_style = if focused {
            Style::default()
                .fg(theme::ORANGE)
                .bg(theme::HIGHLIGHT_BG)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            Style::default().fg(theme::FG)
        } else {
            Style::default().fg(theme::MUTED)
        };

        let row_bg = if focused {
            Style::default().bg(theme::HIGHLIGHT_BG)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::styled(if focused { " ▸ " } else { "   " }, row_bg.fg(theme::ORANGE)),
            Span::styled(bullet, if focused { bullet_style.bg(theme::HIGHLIGHT_BG) } else { bullet_style }),
            Span::styled(fmt.label(), label_style),
        ]));
    }

    lines.push(Line::raw(""));

    // Files section
    lines.push(section_title("Files"));
    if state.file_links.is_empty() {
        lines.push(Line::styled(
            "   (none)",
            Style::default().fg(theme::MUTED),
        ));
    } else {
        for (i, link) in state.file_links.iter().enumerate() {
            let checked = state.file_selected[i];
            let focused = state.focus == FormFocus::File(i);

            let (check, check_style) = if checked {
                ("■ ", Style::default().fg(theme::GREEN))
            } else {
                ("□ ", Style::default().fg(theme::MUTED))
            };

            let label_style = if focused {
                Style::default()
                    .fg(theme::ORANGE)
                    .bg(theme::HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else if checked {
                Style::default().fg(theme::FG)
            } else {
                Style::default().fg(theme::MUTED)
            };

            let row_bg = if focused {
                Style::default().bg(theme::HIGHLIGHT_BG)
            } else {
                Style::default()
            };

            lines.push(Line::from(vec![
                Span::styled(
                    if focused { " ▸ " } else { "   " },
                    row_bg.fg(theme::ORANGE),
                ),
                Span::styled(
                    check,
                    if focused {
                        check_style.bg(theme::HIGHLIGHT_BG)
                    } else {
                        check_style
                    },
                ),
                Span::styled(link.name.clone(), label_style),
            ]));
        }
    }

    lines.push(Line::raw(""));

    // Output dir
    lines.push(section_title("Output Directory"));
    let dir_focused = state.focus == FormFocus::OutputDir;
    if dir_focused {
        lines.push(Line::from(vec![
            Span::styled(" ▸ ", Style::default().fg(theme::ORANGE).bg(theme::HIGHLIGHT_BG)),
            Span::styled(
                &state.output_dir,
                Style::default()
                    .fg(theme::ORANGE)
                    .bg(theme::HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "▏",
                Style::default()
                    .fg(theme::ORANGE)
                    .bg(theme::HIGHLIGHT_BG)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(&state.output_dir, Style::default().fg(theme::FG)),
        ]));
    }

    lines.push(Line::raw(""));

    // Download button
    let btn_focused = state.focus == FormFocus::Download;
    if btn_focused {
        lines.push(Line::from(vec![
            Span::styled(
                " ▸ ⏎ Download ",
                Style::default()
                    .fg(theme::ORANGE)
                    .bg(theme::HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                "⏎ Download",
                Style::default().fg(theme::MUTED),
            ),
        ]));
    }

    // Status message
    if let Some((ref msg, is_success)) = state.status_msg {
        lines.push(Line::raw(""));
        let color = if is_success {
            theme::GREEN
        } else {
            theme::RED
        };
        let icon = if is_success { "✓ " } else { "✗ " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}{}", icon, msg),
                Style::default().fg(color),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Bottom hint bar ──────────────────────────────────────────────────────────

fn render_hint_bar(frame: &mut ratatui::Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Style::default()));
    spans.extend(key_hint("↑↓", "navigate"));
    spans.extend(key_hint("Space", "toggle"));
    spans.extend(key_hint("j/k", "scroll desc"));
    spans.extend(key_hint("Enter", "download"));
    spans.extend(key_hint("Esc", "back"));
    spans.extend(key_hint("q", "quit"));

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

// ── Download execution ───────────────────────────────────────────────────────

async fn execute_export(
    client: &cool_api::CoolClient,
    assignment: &Assignment,
    state: &ExportFormState,
    course_id: i64,
) -> String {
    let mut messages = Vec::new();

    if state.selected_format != ContentFormat::None {
        let msg = export_content(
            assignment,
            &state.description_text,
            state.selected_format,
            &state.output_dir,
        )
        .await;
        messages.push(msg);
    }

    for (i, link) in state.file_links.iter().enumerate() {
        if state.file_selected[i] {
            let msg = download_file(client, link, course_id, &state.output_dir).await;
            messages.push(msg);
        }
    }

    if messages.is_empty() {
        "Nothing to export — select a format or files first.".to_string()
    } else {
        messages.join(" | ")
    }
}

async fn export_content(
    assignment: &Assignment,
    description_text: &str,
    format: ContentFormat,
    output_dir: &str,
) -> String {
    let name = assignment.name.as_deref().unwrap_or("assignment");
    let due = assignment
        .due_at
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());
    let pts = assignment
        .points_possible
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string());

    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let (filename, content) = match format {
        ContentFormat::Json => {
            let content = serde_json::to_string_pretty(assignment).unwrap_or_default();
            (format!("{}.json", safe_name), content)
        }
        ContentFormat::Markdown => {
            let content = format!(
                "# {}\n\n- **Due:** {}\n- **Points:** {}\n\n---\n\n{}",
                name, due, pts, description_text
            );
            (format!("{}.md", safe_name), content)
        }
        ContentFormat::Html => {
            let raw_html = assignment.description.as_deref().unwrap_or("");
            let content = format!(
                "<html><head><meta charset=\"utf-8\"><title>{}</title></head><body>\
                 <h1>{}</h1><p><b>Due:</b> {} | <b>Points:</b> {}</p><hr/>{}</body></html>",
                name, name, due, pts, raw_html
            );
            (format!("{}.html", safe_name), content)
        }
        ContentFormat::None => return String::new(),
    };

    let path = std::path::Path::new(output_dir).join(&filename);

    if let Some(parent) = path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return format!("Failed to create dir: {e}");
        }
    }

    match tokio::fs::write(&path, &content).await {
        Ok(_) => format!("Exported to {}", path.display()),
        Err(e) => format!("Export failed: {e}"),
    }
}

async fn download_file(
    client: &cool_api::CoolClient,
    link: &FileLink,
    course_id: i64,
    output_dir: &str,
) -> String {
    let Some(ref file_id) = link.file_id else {
        return format!("No file ID for: {}", link.name);
    };

    let file_result: Result<cool_api::generated::models::File, _> = client
        .get(
            &format!("/api/v1/courses/{}/files/{}", course_id, file_id),
            None::<&()>,
        )
        .await;

    let file = match file_result {
        Ok(f) => f,
        Err(e) => return format!("Failed to get file info: {e}"),
    };

    let dest_name = file
        .display_name
        .as_deref()
        .or(file.filename.as_deref())
        .unwrap_or(&link.name);

    let dest_path = std::path::Path::new(output_dir).join(dest_name);
    let dest_str = dest_path.to_string_lossy().to_string();

    match cool_api::download::download_file(client, &file, &dest_str).await {
        Ok(bytes) => format!("Downloaded {} ({} bytes)", dest_name, bytes),
        Err(e) => format!("Download failed: {e}"),
    }
}
