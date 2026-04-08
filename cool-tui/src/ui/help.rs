use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::theme::THEME;
use crate::ui::components::centered_rect;

pub fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(70, 80, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(" Keyboard Shortcuts ", THEME.title_style()))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let sections = vec![
        (
            "Global",
            vec![
                ("Ctrl+C", "Quit"),
                ("Ctrl+P", "Quick search / fuzzy finder"),
                ("?", "Toggle this help screen"),
            ],
        ),
        (
            "Dashboard",
            vec![
                ("j / k", "Navigate down / up"),
                ("h / l", "Switch panel focus"),
                ("Tab", "Switch panels"),
                ("Enter", "Open selected course"),
                ("g / G", "Go to first / last item"),
                ("r", "Refresh all data"),
                ("o", "Open selected in browser"),
                ("/", "Search"),
                ("q", "Quit"),
            ],
        ),
        (
            "Course Detail",
            vec![
                ("1-8", "Jump to tab by number"),
                ("a", "Assignments tab"),
                ("n", "Announcements tab"),
                ("d", "Discussions tab"),
                ("f", "Files tab"),
                ("m", "Modules tab"),
                ("p", "Pages tab"),
                ("Tab / Shift+Tab", "Cycle through tabs"),
                ("j / k", "Navigate items"),
                ("Enter", "Open detail / Enter folder"),
                ("Backspace / Esc", "Go back"),
                ("Ctrl+S", "Download selected file"),
                ("o", "Open in browser"),
                ("r", "Refresh"),
            ],
        ),
        (
            "Assignment Detail",
            vec![
                ("j / k", "Scroll description"),
                ("o", "Open in browser"),
                ("Esc", "Close"),
            ],
        ),
        (
            "File Preview",
            vec![
                ("d / Enter", "Download file"),
                ("o", "Open URL in browser"),
                ("Esc", "Close"),
            ],
        ),
        (
            "Search",
            vec![
                ("Type", "Filter results"),
                ("Up / Down", "Navigate results"),
                ("Enter", "Open selected"),
                ("Esc", "Close search"),
            ],
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();

    for (section_title, bindings) in &sections {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {section_title}"),
            Style::default()
                .fg(THEME.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )));
        lines.push(Line::from(""));

        for (key, desc) in bindings {
            lines.push(Line::from(vec![
                Span::styled(format!("    {key:>16}"), THEME.key_hint_style()),
                Span::styled(format!("   {desc}"), THEME.key_desc_style()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press Esc or ? to close",
        THEME.muted_style(),
    )));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(THEME.surface)),
        inner,
    );
}
