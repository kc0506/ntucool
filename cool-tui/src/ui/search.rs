use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::SearchState;
use crate::theme::THEME;
use crate::ui::components::centered_rect;

pub fn draw_search(frame: &mut Frame, area: Rect, state: &SearchState) {
    let popup = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(" Quick Open ", THEME.title_style()))
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search input
            Constraint::Length(1), // result count
            Constraint::Min(0),    // results
        ])
        .split(inner);

    // Search input
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(THEME.border_focused_style())
        .style(Style::default().bg(THEME.surface_bright));

    let input_inner = input_block.inner(chunks[0]);
    frame.render_widget(input_block, chunks[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" > ", Style::default().fg(THEME.accent)),
            Span::styled(&state.query, Style::default().fg(THEME.fg)),
            Span::styled("|", Style::default().fg(THEME.accent)),
        ])),
        input_inner,
    );

    // Result count
    let count_text = if state.query.is_empty() {
        format!("  {} items", state.results.len())
    } else {
        format!("  {} matches", state.results.len())
    };
    frame.render_widget(
        Paragraph::new(count_text).style(THEME.muted_style()),
        chunks[1],
    );

    // Results list
    if state.results.is_empty() {
        frame.render_widget(
            Paragraph::new("  No results found")
                .style(THEME.muted_style())
                .alignment(Alignment::Center),
            chunks[2],
        );
        return;
    }

    let items: Vec<ListItem> = state
        .results
        .iter()
        .enumerate()
        .take(20) // Limit display
        .map(|(i, r)| {
            let is_selected = i == state.selected;

            let category_style = match r.category.as_str() {
                "Course" => Style::default().fg(THEME.accent),
                "Assignment" => Style::default().fg(THEME.warning),
                "File" => Style::default().fg(THEME.secondary),
                _ => THEME.muted_style(),
            };

            let line = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    if is_selected {
                        Style::default().fg(THEME.accent)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("[{}] ", r.category), category_style),
                Span::styled(
                    &r.label,
                    if is_selected {
                        Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    },
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    let list = List::new(items).highlight_style(Style::default().bg(THEME.selection));

    frame.render_stateful_widget(list, chunks[2], &mut list_state);
}
