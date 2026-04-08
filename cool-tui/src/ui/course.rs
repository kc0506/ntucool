use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs,
};

use crate::app::{App, AssignmentDetailState, CourseTab, CourseViewState, FilePreviewState, TopicDetailState};
use crate::theme::{Urgency, THEME};
use crate::ui::components::{
    centered_rect, format_datetime, format_due_date, format_size, loading_text, strip_html,
};

pub fn draw_course(frame: &mut Frame, area: Rect, app: &App, course_id: i64) {
    let view = match app.course_views.get(&course_id) {
        Some(v) => v,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // tab bar
            Constraint::Min(0),    // content
        ])
        .split(area);

    // Tab bar
    draw_tab_bar(frame, chunks[0], view);

    // Content
    if view.loading {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(THEME.border_style())
            .style(Style::default().bg(THEME.surface));
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);
        frame.render_widget(loading_text("Loading course data..."), inner);
        return;
    }

    match view.active_tab {
        CourseTab::Assignments => draw_assignments(frame, chunks[1], view),
        CourseTab::Announcements => draw_announcements(frame, chunks[1], view),
        CourseTab::Discussions => draw_discussions(frame, chunks[1], view),
        CourseTab::Files => draw_files(frame, chunks[1], view),
        CourseTab::Modules => draw_modules(frame, chunks[1], view),
        CourseTab::Pages => draw_pages(frame, chunks[1], view),
        CourseTab::Quizzes => draw_quizzes(frame, chunks[1], view),
        CourseTab::Grades => draw_grades(frame, chunks[1]),
    }
}

fn draw_tab_bar(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let tabs: Vec<Line> = CourseTab::all()
        .iter()
        .map(|tab| {
            let style = if *tab == view.active_tab {
                THEME.active_tab_style()
            } else {
                THEME.inactive_tab_style()
            };
            Line::from(Span::styled(
                format!(" {} {} ", tab.icon(), tab.label()),
                style,
            ))
        })
        .collect();

    let tabs_widget = Tabs::new(tabs)
        .style(Style::default().bg(THEME.surface))
        .highlight_style(THEME.active_tab_style())
        .select(
            CourseTab::all()
                .iter()
                .position(|t| *t == view.active_tab)
                .unwrap_or(0),
        )
        .divider(Span::styled(" | ", THEME.muted_style()));

    frame.render_widget(tabs_widget, area);
}

// ─── Assignments ───

fn draw_assignments(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Assignments ({}) ", view.assignments.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.assignments.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No assignments").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .assignments
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = a.name.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();

            let (due_text, urgency) = if let Some(due) = &a.due_at {
                let now = chrono::Utc::now();
                let hours = (*due - now).num_hours();
                let u = if *due < now {
                    Urgency::None
                } else {
                    Urgency::from_hours(hours)
                };
                (format_due_date(due), u)
            } else {
                ("No due date".to_string(), Urgency::None)
            };

            let submitted = a.has_submitted_submissions.unwrap_or(false);
            let points = a
                .points_possible
                .map(|p| format!("{p} pts"))
                .unwrap_or_default();

            let name_str = name.to_string();
            let submitted_str = if submitted { " [submitted]".to_string() } else { String::new() };
            let points_str = if !points.is_empty() { format!("  |  {points}") } else { String::new() };

            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { " > ".to_string() } else { "   ".to_string() },
                    if is_selected {
                        Style::default().fg(THEME.accent)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    name_str,
                    if is_selected {
                        Style::default().fg(THEME.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
                Span::styled(submitted_str, Style::default().fg(THEME.success)),
            ]);

            let line2 = Line::from(vec![
                Span::raw("     "),
                Span::styled(due_text, THEME.urgency_style(urgency)),
                Span::styled(points_str, THEME.muted_style()),
            ]);

            ListItem::new(vec![line1, line2, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Announcements ───

fn draw_announcements(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Announcements ({}) ", view.announcements.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.announcements.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No announcements").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .announcements
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let title = a.title.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();

            let posted = a
                .posted_at
                .as_ref()
                .map(|d| format_datetime(d))
                .unwrap_or_default();

            let author = a.user_name.as_deref().unwrap_or("Unknown");

            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.primary)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    title,
                    if is_selected {
                        Style::default().fg(THEME.primary).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
            ]);

            let line2 = Line::from(vec![
                Span::raw("     "),
                Span::styled(author, THEME.muted_style()),
                Span::styled("  |  ", THEME.muted_style()),
                Span::styled(posted, THEME.muted_style()),
            ]);

            ListItem::new(vec![line1, line2, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Discussions ───

fn draw_discussions(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Discussions ({}) ", view.discussions.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.discussions.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No discussions").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .discussions
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let title = d.title.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();

            let reply_count = d.discussion_subentry_count.unwrap_or(0);
            let unread = d.unread_count.unwrap_or(0);
            let posted = d
                .posted_at
                .as_ref()
                .map(|dt| format_datetime(dt))
                .unwrap_or_default();

            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.warning)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    title,
                    if is_selected {
                        Style::default().fg(THEME.warning).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
                if unread > 0 {
                    Span::styled(
                        format!(" ({unread} new)"),
                        Style::default().fg(THEME.accent),
                    )
                } else {
                    Span::raw("")
                },
            ]);

            let line2 = Line::from(vec![
                Span::raw("     "),
                Span::styled(format!("{reply_count} replies"), THEME.muted_style()),
                Span::styled("  |  ", THEME.muted_style()),
                Span::styled(posted, THEME.muted_style()),
            ]);

            ListItem::new(vec![line1, line2, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Files ───

fn draw_files(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let total = view.folders.len() + view.files.len();

    // Breadcrumb
    let breadcrumb = if view.folder_path.is_empty() {
        "/ (root)".to_string()
    } else {
        let parts: Vec<&str> = view.folder_path.iter().map(|(_, n)| n.as_str()).collect();
        format!("/ {}", parts.join(" / "))
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                format!(" Files ({total}) ", ),
                THEME.title_style(),
            ),
            Span::styled(
                format!(" {breadcrumb} "),
                THEME.muted_style(),
            ),
        ]))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if total == 0 {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("Empty folder").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();

    // Back navigation if in subfolder
    if !view.folder_path.is_empty() {
        items.push(ListItem::new(Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(".. (back)", Style::default().fg(THEME.muted)),
        ])));
    }

    // Folders first
    for (i, folder) in view.folders.iter().enumerate() {
        let name = folder.name.as_deref().unwrap_or("Unnamed");
        let is_selected = i == view.selected();

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                if is_selected { " > " } else { "   " },
                if is_selected {
                    Style::default().fg(THEME.accent)
                } else {
                    Style::default()
                },
            ),
            Span::styled("[DIR] ", Style::default().fg(THEME.accent)),
            Span::styled(
                name,
                if is_selected {
                    Style::default().fg(THEME.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.fg)
                },
            ),
        ])));
    }

    // Files
    for (i, file) in view.files.iter().enumerate() {
        let name = file
            .display_name
            .as_deref()
            .or(file.filename.as_deref())
            .unwrap_or("Unnamed");
        let size = format_size(file.size);
        let idx = view.folders.len() + i;
        let is_selected = idx == view.selected();

        let icon = match file.mime_class.as_deref() {
            Some("pdf") => "[PDF]",
            Some("image") => "[IMG]",
            Some("video") => "[VID]",
            Some("audio") => "[AUD]",
            Some("doc") => "[DOC]",
            Some("ppt") => "[PPT]",
            Some("xls") => "[XLS]",
            Some("zip") => "[ZIP]",
            Some("code") => "[COD]",
            _ => "[FIL]",
        };

        let modified = file
            .updated_at
            .as_ref()
            .or(file.created_at.as_ref())
            .map(|d| format_datetime(d))
            .unwrap_or_default();

        let line = Line::from(vec![
            Span::styled(
                if is_selected { " > " } else { "   " },
                if is_selected {
                    Style::default().fg(THEME.secondary)
                } else {
                    Style::default()
                },
            ),
            Span::styled(format!("{icon} "), Style::default().fg(THEME.muted)),
            Span::styled(
                name,
                if is_selected {
                    Style::default().fg(THEME.secondary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.fg)
                },
            ),
            Span::styled(format!("  {size}"), THEME.muted_style()),
            Span::styled(format!("  {modified}"), THEME.muted_style()),
        ]);

        items.push(ListItem::new(line));
    }

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Modules ───

fn draw_modules(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Modules ({}) ", view.modules.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.modules.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No modules").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .modules
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let name = m.name.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();
            let state_icon = "[ ]";

            let items_count = m
                .items
                .as_ref()
                .map(|v| v.len())
                .unwrap_or(0);

            let line = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.accent)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    format!("{state_icon} "),
                    THEME.muted_style(),
                ),
                Span::styled(
                    name,
                    if is_selected {
                        Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
                Span::styled(
                    format!("  ({items_count} items)"),
                    THEME.muted_style(),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Pages ───

fn draw_pages(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Pages ({}) ", view.pages.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.pages.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No pages").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let title = p.title.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();

            let updated = p
                .updated_at
                .as_ref()
                .map(|d| format_datetime(d))
                .unwrap_or_default();

            let published = p.published.unwrap_or(false);

            let line = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.accent)
                    } else {
                        Style::default()
                    },
                ),
                if !published {
                    Span::styled("[draft] ", Style::default().fg(THEME.warning))
                } else {
                    Span::raw("")
                },
                Span::styled(
                    title,
                    if is_selected {
                        Style::default().fg(THEME.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
                Span::styled(format!("  {updated}"), THEME.muted_style()),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Quizzes ───

fn draw_quizzes(frame: &mut Frame, area: Rect, view: &CourseViewState) {
    let block = Block::default()
        .title(Span::styled(
            format!(" Quizzes ({}) ", view.quizzes.len()),
            THEME.title_style(),
        ))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    if view.quizzes.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No quizzes").style(THEME.muted_style()).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = view
        .quizzes
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let title = q.title.as_deref().unwrap_or("Untitled");
            let is_selected = i == view.selected();

            let due = q
                .due_at
                .as_ref()
                .map(|d| format_due_date(d))
                .unwrap_or("No due date".to_string());

            let points = q
                .points_possible
                .map(|p| format!("{p} pts"))
                .unwrap_or_default();

            let quiz_type = q.quiz_type.as_deref().unwrap_or("quiz");

            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.accent)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    title,
                    if is_selected {
                        Style::default().fg(THEME.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
            ]);

            let line2 = Line::from(vec![
                Span::raw("     "),
                Span::styled(format!("[{quiz_type}]"), THEME.muted_style()),
                Span::styled(format!("  {due}"), THEME.muted_style()),
                if !points.is_empty() {
                    Span::styled(format!("  |  {points}"), THEME.muted_style())
                } else {
                    Span::raw("")
                },
            ]);

            ListItem::new(vec![line1, line2, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(view.selected()));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, area, &mut list_state);
}

// ─── Grades ───

fn draw_grades(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(Span::styled(" Grades ", THEME.title_style()))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(
        Paragraph::new("Grades view coming soon\n\nUse 'o' to open grades in browser")
            .style(THEME.muted_style())
            .alignment(Alignment::Center),
        inner,
    );
}

// ─── Assignment Detail Modal ───

pub fn draw_assignment_detail(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    state: &AssignmentDetailState,
) {
    let popup = centered_rect(80, 80, area);
    frame.render_widget(Clear, popup);

    let view = match app.course_views.get(&state.course_id) {
        Some(v) => v,
        None => return,
    };

    let assignment = match view.assignments.get(state.assignment_idx) {
        Some(a) => a,
        None => return,
    };

    let name = assignment.name.as_deref().unwrap_or("Untitled");

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" Assignment: ", THEME.muted_style()),
            Span::styled(name, THEME.title_style()),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // Layout inside
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4), // metadata
            Constraint::Length(1), // separator
            Constraint::Min(0),    // description
            Constraint::Length(2), // footer
        ])
        .split(inner);

    // Metadata
    let due_text = assignment
        .due_at
        .as_ref()
        .map(|d| format_due_date(d))
        .unwrap_or("No due date".to_string());

    let points = assignment
        .points_possible
        .map(|p| format!("{p} pts"))
        .unwrap_or("No points".to_string());

    let submitted = if assignment.has_submitted_submissions.unwrap_or(false) {
        "Submitted"
    } else {
        "Not submitted"
    };

    let submission_types = assignment
        .submission_types
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let meta_lines = vec![
        Line::from(vec![
            Span::styled("  Due: ", THEME.key_hint_style()),
            Span::styled(&due_text, Style::default().fg(THEME.fg)),
            Span::styled("    Points: ", THEME.key_hint_style()),
            Span::styled(&points, Style::default().fg(THEME.fg)),
        ]),
        Line::from(vec![
            Span::styled("  Status: ", THEME.key_hint_style()),
            Span::styled(
                submitted,
                if submitted == "Submitted" {
                    Style::default().fg(THEME.success)
                } else {
                    Style::default().fg(THEME.warning)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("  Type: ", THEME.key_hint_style()),
            Span::styled(&submission_types, THEME.muted_style()),
        ]),
    ];

    frame.render_widget(Paragraph::new(meta_lines), chunks[0]);

    // Separator
    frame.render_widget(
        Paragraph::new("─".repeat(chunks[1].width as usize)).style(THEME.border_style()),
        chunks[1],
    );

    // Description
    let description = assignment
        .description
        .as_deref()
        .map(strip_html)
        .unwrap_or("No description available.".to_string());

    let wrapped = textwrap::wrap(&description, chunks[2].width as usize - 4);
    let desc_lines: Vec<Line> = wrapped
        .iter()
        .map(|l| Line::from(format!("  {l}")))
        .collect();

    // Apply scroll
    let visible_lines = chunks[2].height as usize;
    let scroll = state.scroll as usize;
    let display_lines: Vec<Line> = desc_lines
        .into_iter()
        .skip(scroll)
        .take(visible_lines)
        .collect();

    frame.render_widget(
        Paragraph::new(display_lines).style(Style::default().fg(THEME.fg)),
        chunks[2],
    );

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Esc", THEME.key_hint_style()),
            Span::styled(" close  ", THEME.key_desc_style()),
            Span::styled("j/k", THEME.key_hint_style()),
            Span::styled(" scroll  ", THEME.key_desc_style()),
            Span::styled("o", THEME.key_hint_style()),
            Span::styled(" open in browser", THEME.key_desc_style()),
        ])),
        chunks[3],
    );
}

// ─── File Preview Modal ───

pub fn draw_file_preview(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    state: &FilePreviewState,
) {
    let popup = centered_rect(60, 40, area);
    frame.render_widget(Clear, popup);

    let view = match app.course_views.get(&state.course_id) {
        Some(v) => v,
        None => return,
    };

    let file = match view.files.get(state.file_idx) {
        Some(f) => f,
        None => return,
    };

    let name = file
        .display_name
        .as_deref()
        .or(file.filename.as_deref())
        .unwrap_or("Unknown");

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(" File: ", THEME.muted_style()),
            Span::styled(name, THEME.title_style()),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(5), // metadata
            Constraint::Min(0),    // spacer
            Constraint::Length(2), // footer
        ])
        .split(inner);

    let size = format_size(file.size);
    let mime = file
        .content_type
        .as_deref()
        .or(file.mime_class.as_deref())
        .unwrap_or("Unknown");
    let modified = file
        .updated_at
        .as_ref()
        .map(|d| format_datetime(d))
        .unwrap_or_default();

    let meta = vec![
        Line::from(vec![
            Span::styled("  Size: ", THEME.key_hint_style()),
            Span::styled(&size, Style::default().fg(THEME.fg)),
        ]),
        Line::from(vec![
            Span::styled("  Type: ", THEME.key_hint_style()),
            Span::styled(mime, Style::default().fg(THEME.fg)),
        ]),
        Line::from(vec![
            Span::styled("  Modified: ", THEME.key_hint_style()),
            Span::styled(&modified, Style::default().fg(THEME.fg)),
        ]),
    ];

    frame.render_widget(Paragraph::new(meta), chunks[0]);

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  d/Enter", THEME.key_hint_style()),
            Span::styled(" download  ", THEME.key_desc_style()),
            Span::styled("o", THEME.key_hint_style()),
            Span::styled(" open URL  ", THEME.key_desc_style()),
            Span::styled("Esc", THEME.key_hint_style()),
            Span::styled(" close", THEME.key_desc_style()),
        ])),
        chunks[2],
    );
}

// ─── Topic Detail Modal (Announcements & Discussions) ───

pub fn draw_topic_detail(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    state: &TopicDetailState,
) {
    let popup = centered_rect(80, 80, area);
    frame.render_widget(Clear, popup);

    let view = match app.course_views.get(&state.course_id) {
        Some(v) => v,
        None => return,
    };

    let topics = if state.is_announcement {
        &view.announcements
    } else {
        &view.discussions
    };

    let topic = match topics.get(state.topic_idx) {
        Some(t) => t,
        None => return,
    };

    let title = topic.title.as_deref().unwrap_or("Untitled");
    let kind = if state.is_announcement {
        "Announcement"
    } else {
        "Discussion"
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::styled(format!(" {kind}: "), THEME.muted_style()),
            Span::styled(title, THEME.title_style()),
            Span::raw(" "),
        ]))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // metadata
            Constraint::Length(1), // separator
            Constraint::Min(0),    // body
            Constraint::Length(2), // footer
        ])
        .split(inner);

    // Metadata
    let author = topic.user_name.as_deref().unwrap_or("Unknown");
    let posted = topic
        .posted_at
        .as_ref()
        .map(|d| format_datetime(d))
        .unwrap_or_default();

    let reply_count = topic.discussion_subentry_count.unwrap_or(0);
    let unread = topic.unread_count.unwrap_or(0);

    let meta_lines = vec![
        Line::from(vec![
            Span::styled("  Author: ", THEME.key_hint_style()),
            Span::styled(author, Style::default().fg(THEME.fg)),
            Span::styled("    Posted: ", THEME.key_hint_style()),
            Span::styled(posted, Style::default().fg(THEME.fg)),
        ]),
        Line::from(vec![
            Span::styled("  Replies: ", THEME.key_hint_style()),
            Span::styled(
                reply_count.to_string(),
                Style::default().fg(THEME.fg),
            ),
            if unread > 0 {
                Span::styled(
                    format!("  ({unread} unread)"),
                    Style::default().fg(THEME.accent),
                )
            } else {
                Span::raw("")
            },
        ]),
    ];
    frame.render_widget(Paragraph::new(meta_lines), chunks[0]);

    // Separator
    frame.render_widget(
        Paragraph::new("─".repeat(chunks[1].width as usize)).style(THEME.border_style()),
        chunks[1],
    );

    // Body
    let body = topic
        .message
        .as_deref()
        .map(strip_html)
        .unwrap_or("No content.".to_string());

    let wrapped = textwrap::wrap(&body, chunks[2].width as usize - 4);
    let body_lines: Vec<Line> = wrapped
        .iter()
        .map(|l| Line::from(format!("  {l}")))
        .collect();

    let visible_lines = chunks[2].height as usize;
    let scroll = state.scroll as usize;
    let display_lines: Vec<Line> = body_lines
        .into_iter()
        .skip(scroll)
        .take(visible_lines)
        .collect();

    frame.render_widget(
        Paragraph::new(display_lines).style(Style::default().fg(THEME.fg)),
        chunks[2],
    );

    // Footer
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Esc", THEME.key_hint_style()),
            Span::styled(" close  ", THEME.key_desc_style()),
            Span::styled("j/k", THEME.key_hint_style()),
            Span::styled(" scroll  ", THEME.key_desc_style()),
            Span::styled("o", THEME.key_hint_style()),
            Span::styled(" open in browser", THEME.key_desc_style()),
        ])),
        chunks[3],
    );
}
