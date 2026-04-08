use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::ConfirmState;
use crate::theme::THEME;

/// Create a centered rectangle using percentage of parent area
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Create a fixed-size centered rect
pub fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Draw a confirmation dialog
pub fn draw_confirm(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    let popup = centered_rect_fixed(50, 7, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Confirm ")
        .title_style(THEME.title_style())
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // message
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
        ])
        .split(inner);

    // Message
    frame.render_widget(
        Paragraph::new(&*state.message)
            .style(Style::default().fg(THEME.fg))
            .alignment(Alignment::Center),
        chunks[0],
    );

    // Buttons
    let yes_style = if state.selected_yes {
        Style::default()
            .fg(THEME.bg)
            .bg(THEME.success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME.muted)
    };
    let no_style = if !state.selected_yes {
        Style::default()
            .fg(THEME.bg)
            .bg(THEME.error)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME.muted)
    };

    let buttons = Line::from(vec![
        Span::raw("    "),
        Span::styled("  Yes  ", yes_style),
        Span::raw("    "),
        Span::styled("  No  ", no_style),
        Span::raw("    "),
    ]);

    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        chunks[2],
    );
}

/// Render a loading spinner text
pub fn loading_text(msg: &str) -> Paragraph<'_> {
    Paragraph::new(msg)
        .style(THEME.muted_style())
        .alignment(Alignment::Center)
}

/// Format file size in human-readable form
pub fn format_size(bytes: Option<i64>) -> String {
    match bytes {
        None => "-".to_string(),
        Some(b) if b < 1024 => format!("{b} B"),
        Some(b) if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        Some(b) if b < 1024 * 1024 * 1024 => {
            format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
        }
        Some(b) => format!("{:.1} GB", b as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

/// Format a chrono datetime as a relative/short string
pub fn format_datetime(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now - *dt;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_hours() < 1 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d ago", diff.num_days())
    } else {
        dt.format("%m/%d").to_string()
    }
}

/// Format a due date with urgency
pub fn format_due_date(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = *dt - now;

    if diff.num_seconds() < 0 {
        let past = now - *dt;
        format!("{} overdue", format_duration_short(past))
    } else if diff.num_hours() < 24 {
        format!("in {}h {}m", diff.num_hours(), diff.num_minutes() % 60)
    } else if diff.num_days() < 7 {
        format!("in {}d", diff.num_days())
    } else {
        dt.format("%m/%d %H:%M").to_string()
    }
}

fn format_duration_short(d: chrono::Duration) -> String {
    if d.num_days() > 0 {
        format!("{}d", d.num_days())
    } else if d.num_hours() > 0 {
        format!("{}h", d.num_hours())
    } else {
        format!("{}m", d.num_minutes())
    }
}

/// Strip HTML tags from a string, preserving some structure
pub fn strip_html(html: &str) -> String {
    // Replace block elements with newlines
    let html = html.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "  - ")
        .replace("</h1>", "\n")
        .replace("</h2>", "\n")
        .replace("</h3>", "\n")
        .replace("</h4>", "\n")
        .replace("<hr>", "\n────────────────────\n")
        .replace("<hr/>", "\n────────────────────\n")
        .replace("</tr>", "\n")
        .replace("</td>", " | ")
        .replace("</th>", " | ");

    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(&html, "");
    let decoded = html_escape::decode_html_entities(&text).to_string();

    // Collapse multiple blank lines
    let re_blank = regex::Regex::new(r"\n{3,}").unwrap();
    re_blank.replace_all(&decoded, "\n\n").trim().to_string()
}

/// Truncate a string to max_len characters with ellipsis
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Create a progress bar string
pub fn progress_bar(progress: f64, width: usize) -> String {
    let filled = (progress * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}
