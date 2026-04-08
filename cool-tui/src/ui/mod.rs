mod components;
mod course;
mod dashboard;
mod help;
mod login;
mod search;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Modal, Screen};
use crate::theme::THEME;

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Background fill
    frame.render_widget(
        Block::default().style(THEME.style()),
        size,
    );

    // Layout: header + body + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(0),    // body
            Constraint::Length(1), // status bar
        ])
        .split(size);

    // Header
    draw_header(frame, chunks[0], app);

    // Body
    match &app.screen {
        Screen::Login => login::draw_login(frame, chunks[1], app),
        Screen::Dashboard => dashboard::draw_dashboard(frame, chunks[1], app),
        Screen::CourseDetail(cid) => course::draw_course(frame, chunks[1], app, *cid),
    }

    // Status bar
    draw_status_bar(frame, chunks[2], app);

    // Modal overlay
    if let Some(modal) = &app.modal {
        draw_modal(frame, size, app, modal);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let header_style = Style::default()
        .fg(THEME.bg)
        .bg(THEME.accent)
        .add_modifier(Modifier::BOLD);

    let user_info = app
        .current_user
        .as_ref()
        .and_then(|u| u.name.clone())
        .unwrap_or_default();

    let title = match &app.screen {
        Screen::Login => " NTU COOL ".to_string(),
        Screen::Dashboard => format!(
            " NTU COOL  |  Dashboard  {}",
            if user_info.is_empty() {
                String::new()
            } else {
                format!(" |  {user_info} ")
            }
        ),
        Screen::CourseDetail(cid) => {
            let course_name = app
                .course_views
                .get(cid)
                .map(|v| v.course_name.as_str())
                .unwrap_or("Course");
            format!(" NTU COOL  |  {course_name} ")
        }
    };

    let notif_badge = if app.unread_count > 0 {
        format!("  [{} unread]  ", app.unread_count)
    } else {
        String::new()
    };

    let right = format!("{notif_badge}Ctrl+P: Search  ?: Help ");

    let available = area.width as usize;
    let left_len = title.len();
    let right_len = right.len();

    let padding = if available > left_len + right_len {
        " ".repeat(available - left_len - right_len)
    } else {
        String::new()
    };

    let header_text = format!("{title}{padding}{right}");

    frame.render_widget(
        Paragraph::new(header_text).style(header_style),
        area,
    );
}

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let style = THEME.status_bar_style();

    let status = app
        .status_message
        .as_deref()
        .unwrap_or("");

    let sync_info = app
        .last_sync
        .map(|t| {
            let elapsed = chrono::Utc::now() - t;
            if elapsed.num_seconds() < 60 {
                "synced just now".to_string()
            } else if elapsed.num_minutes() < 60 {
                format!("synced {}m ago", elapsed.num_minutes())
            } else {
                format!("synced {}h ago", elapsed.num_hours())
            }
        })
        .unwrap_or_default();

    let loading_indicator = if app.loading { " [loading...] " } else { "" };

    let left = format!(" {status}{loading_indicator}");
    let right = format!("{sync_info} ");

    let available = area.width as usize;
    let padding = if available > left.len() + right.len() {
        " ".repeat(available - left.len() - right.len())
    } else {
        String::new()
    };

    frame.render_widget(
        Paragraph::new(format!("{left}{padding}{right}")).style(style),
        area,
    );
}

fn draw_modal(frame: &mut Frame, area: Rect, app: &App, modal: &Modal) {
    match modal {
        Modal::Help => help::draw_help(frame, area),
        Modal::Search(state) => search::draw_search(frame, area, state),
        Modal::AssignmentDetail(state) => {
            course::draw_assignment_detail(frame, area, app, state);
        }
        Modal::TopicDetail(state) => {
            course::draw_topic_detail(frame, area, app, state);
        }
        Modal::FilePreview(state) => {
            course::draw_file_preview(frame, area, app, state);
        }
        Modal::Confirm(state) => {
            components::draw_confirm(frame, area, state);
        }
        Modal::Notification => {
            draw_notifications(frame, area, app);
        }
    }
}

fn draw_notifications(frame: &mut Frame, area: Rect, app: &App) {
    let popup = components::centered_rect(60, 70, area);

    frame.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" Notifications ")
        .title_style(THEME.title_style())
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if app.notifications.is_empty() {
        let msg = Paragraph::new("No notifications")
            .style(THEME.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
    } else {
        let items: Vec<Line> = app
            .notifications
            .iter()
            .flat_map(|n| {
                let style = if n.read {
                    THEME.muted_style()
                } else {
                    Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD)
                };
                vec![
                    Line::from(Span::styled(&n.title, style)),
                    Line::from(Span::styled(
                        format!(
                            "  {} | {}",
                            n.course_name.as_deref().unwrap_or("System"),
                            n.timestamp.format("%m/%d %H:%M")
                        ),
                        THEME.muted_style(),
                    )),
                    Line::from(""),
                ]
            })
            .collect();

        frame.render_widget(
            Paragraph::new(items).style(Style::default().bg(THEME.surface)),
            inner,
        );
    }
}
