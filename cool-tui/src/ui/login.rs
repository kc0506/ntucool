use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme::THEME;
use crate::ui::components::centered_rect;

pub fn draw_login(frame: &mut Frame, area: Rect, app: &App) {
    let popup = centered_rect(50, 50, area);

    // Outer box
    let block = Block::default()
        .title(" NTU COOL Login ")
        .title_style(THEME.title_style())
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // logo
            Constraint::Length(2), // spacer
            Constraint::Length(1), // username label
            Constraint::Length(3), // username input
            Constraint::Length(1), // password label
            Constraint::Length(3), // password input
            Constraint::Length(2), // spacer
            Constraint::Length(1), // login button / status
            Constraint::Length(2), // error
            Constraint::Min(0),    // spacer
        ])
        .split(inner);

    // Logo / Title
    let logo = vec![
        Line::from(Span::styled(
            "  NTU COOL Terminal",
            Style::default()
                .fg(THEME.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  National Taiwan University",
            THEME.muted_style(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(logo).alignment(Alignment::Center),
        chunks[0],
    );

    // Username
    let username_style = if app.login_field == 0 {
        THEME.border_focused_style()
    } else {
        THEME.border_style()
    };
    frame.render_widget(
        Paragraph::new("  Username (Student ID)")
            .style(Style::default().fg(THEME.muted)),
        chunks[2],
    );
    let username_block = Block::default()
        .borders(Borders::ALL)
        .border_style(username_style)
        .style(Style::default().bg(THEME.surface_bright));
    let username_inner = username_block.inner(chunks[3]);
    frame.render_widget(username_block, chunks[3]);

    let cursor_char = if app.login_field == 0 { "|" } else { "" };
    frame.render_widget(
        Paragraph::new(format!(" {}{}", app.login_username, cursor_char))
            .style(Style::default().fg(THEME.fg)),
        username_inner,
    );

    // Password
    let password_style = if app.login_field == 1 {
        THEME.border_focused_style()
    } else {
        THEME.border_style()
    };
    frame.render_widget(
        Paragraph::new("  Password")
            .style(Style::default().fg(THEME.muted)),
        chunks[4],
    );
    let password_block = Block::default()
        .borders(Borders::ALL)
        .border_style(password_style)
        .style(Style::default().bg(THEME.surface_bright));
    let password_inner = password_block.inner(chunks[5]);
    frame.render_widget(password_block, chunks[5]);

    let masked: String = "*".repeat(app.login_password.len());
    let cursor_char = if app.login_field == 1 { "|" } else { "" };
    frame.render_widget(
        Paragraph::new(format!(" {masked}{cursor_char}"))
            .style(Style::default().fg(THEME.fg)),
        password_inner,
    );

    // Login button / status
    if app.login_loading {
        frame.render_widget(
            Paragraph::new("  Authenticating via SAML...")
                .style(Style::default().fg(THEME.warning))
                .alignment(Alignment::Center),
            chunks[7],
        );
    } else {
        frame.render_widget(
            Paragraph::new("  Press Enter to login  |  Tab to switch fields  |  Esc to quit")
                .style(THEME.muted_style())
                .alignment(Alignment::Center),
            chunks[7],
        );
    }

    // Error message
    if let Some(err) = &app.login_error {
        frame.render_widget(
            Paragraph::new(format!("  {err}"))
                .style(Style::default().fg(THEME.error))
                .alignment(Alignment::Center),
            chunks[8],
        );
    }
}
