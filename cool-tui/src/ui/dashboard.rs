use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, DashboardFocus};
use crate::theme::{course_color, Urgency, THEME};
use crate::ui::components::{format_due_date, loading_text};

pub fn draw_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    // Show splash screen while loading
    if app.loading && app.courses.is_empty() {
        draw_splash(frame, area);
        return;
    }

    // Two-column layout: courses (left) | deadlines + info (right)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(area);

    draw_courses_panel(frame, columns[0], app);

    // Right column: deadlines (top) + quick actions (bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(65),
            Constraint::Percentage(35),
        ])
        .split(columns[1]);

    draw_deadlines_panel(frame, right_chunks[0], app);
    draw_quick_actions(frame, right_chunks[1], app);
}

fn draw_courses_panel(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.dashboard_focus == DashboardFocus::Courses;
    let border_style = if is_focused {
        THEME.border_focused_style()
    } else {
        THEME.border_style()
    };

    let title_spans = vec![
        Span::styled(" Courses ", THEME.title_style()),
        Span::styled(
            format!("({}) ", app.courses.len()),
            THEME.muted_style(),
        ),
    ];

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(THEME.surface));

    if app.loading && app.courses.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(loading_text("Loading courses..."), inner);
        return;
    }

    let items: Vec<ListItem> = app
        .courses
        .iter()
        .enumerate()
        .map(|(i, course)| {
            let name = course.name.as_deref().unwrap_or("Untitled");
            let code = course.course_code.as_deref().unwrap_or("");
            let is_selected = i == app.course_selected && is_focused;
            let color = course_color(i);

            let line = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.muted)
                    },
                ),
                Span::styled(
                    "█ ",
                    Style::default().fg(color),
                ),
                Span::styled(
                    name,
                    if is_selected {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
            ]);

            let code_line = Line::from(vec![
                Span::raw("     "),
                Span::styled(code, THEME.muted_style()),
            ]);

            ListItem::new(vec![line, code_line, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    if is_focused && !app.courses.is_empty() {
        list_state.select(Some(app.course_selected));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(THEME.selection)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_deadlines_panel(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.dashboard_focus == DashboardFocus::Deadlines;
    let border_style = if is_focused {
        THEME.border_focused_style()
    } else {
        THEME.border_style()
    };

    let title_spans = vec![
        Span::styled(" Upcoming Deadlines ", THEME.title_style()),
        Span::styled(
            format!("({}) ", app.deadlines.len()),
            THEME.muted_style(),
        ),
    ];

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(THEME.surface));

    if app.loading && app.deadlines.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(loading_text("Loading deadlines..."), inner);
        return;
    }

    if app.deadlines.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("No upcoming deadlines! You're all caught up.")
                .style(Style::default().fg(THEME.success))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .deadlines
        .iter()
        .enumerate()
        .map(|(i, deadline)| {
            let now = chrono::Utc::now();
            let hours_left = (deadline.due_at - now).num_hours();
            let urgency = Urgency::from_hours(hours_left);
            let is_selected = i == app.deadline_selected && is_focused;

            let urgency_icon = urgency.icon().to_string();
            let urgency_style = THEME.urgency_style(urgency);
            let due_text = format_due_date(&deadline.due_at);

            let submitted_marker = if deadline.submitted { " [done]".to_string() } else { String::new() };
            let name = deadline.assignment_name.clone();
            let course = deadline.course_name.clone();
            let pts_str = deadline.points_possible.map(|p| format!("  ({p} pts)")).unwrap_or_default();

            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { " > ".to_string() } else { "   ".to_string() },
                    if is_selected {
                        Style::default().fg(THEME.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(urgency_icon, urgency_style),
                Span::raw(" "),
                Span::styled(
                    name,
                    if is_selected {
                        urgency_style.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
                Span::styled(submitted_marker, Style::default().fg(THEME.success)),
            ]);

            let line2 = Line::from(vec![
                Span::raw("      "),
                Span::styled(course, THEME.muted_style()),
                Span::styled("  |  ", THEME.muted_style()),
                Span::styled(due_text, urgency_style),
                Span::styled(pts_str, THEME.muted_style()),
            ]);

            ListItem::new(vec![line1, line2, Line::from("")])
        })
        .collect();

    let mut list_state = ListState::default();
    if is_focused && !app.deadlines.is_empty() {
        list_state.select(Some(app.deadline_selected));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(THEME.selection)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_splash(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(THEME.bg));
    frame.render_widget(block, area);

    let ascii_art = vec![
        "",
        "",
        "    _   _ _____ _   _    ____ ___   ___  _     ",
        "   | \\ | |_   _| | | |  / ___/ _ \\ / _ \\| |    ",
        "   |  \\| | | | | | | | | |  | | | | | | | |    ",
        "   | |\\  | | | | |_| | | |__| |_| | |_| | |___ ",
        "   |_| \\_| |_|  \\___/   \\____\\___/ \\___/|_____|",
        "",
        "              Terminal User Interface",
        "",
        "            Loading your courses...",
        "",
    ];

    let lines: Vec<Line> = ascii_art
        .iter()
        .map(|l| {
            Line::from(Span::styled(
                *l,
                Style::default().fg(THEME.accent),
            ))
        })
        .collect();

    let y_offset = area.height.saturating_sub(lines.len() as u16) / 2;
    let splash_area = Rect::new(
        area.x,
        area.y + y_offset,
        area.width,
        lines.len() as u16,
    );

    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        splash_area,
    );
}

fn draw_quick_actions(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(Span::styled(" Quick Actions ", THEME.title_style()))
        .borders(Borders::ALL)
        .border_style(THEME.border_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let actions = vec![
        ("Enter", "Open selected course"),
        ("j/k", "Navigate up/down"),
        ("h/l", "Switch panels"),
        ("Tab", "Switch panel focus"),
        ("/", "Search"),
        ("Ctrl+P", "Quick open"),
        ("r", "Refresh"),
        ("o", "Open in browser"),
        ("?", "Show all shortcuts"),
        ("q", "Quit"),
    ];

    let lines: Vec<Line> = actions
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("  {key:>8}"), THEME.key_hint_style()),
                Span::styled(format!("  {desc}"), THEME.key_desc_style()),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}
