use std::cmp::Ordering;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

use super::theme;

/// A reusable full-screen fuzzy-select widget backed by ratatui.
pub struct FuzzySelect<'a, T> {
    items: &'a [T],
    display: Box<dyn Fn(&T, u16) -> String + 'a>,
    filter: Box<dyn Fn(&T, &str) -> bool + 'a>,
    sort_modes: Vec<(&'static str, Box<dyn Fn(&T, &T) -> Ordering + 'a>)>,
    prompt: &'a str,
}

impl<'a, T> FuzzySelect<'a, T> {
    pub fn new(
        items: &'a [T],
        display: impl Fn(&T, u16) -> String + 'a,
        filter: impl Fn(&T, &str) -> bool + 'a,
    ) -> Self {
        Self {
            items,
            display: Box::new(display),
            filter: Box::new(filter),
            sort_modes: Vec::new(),
            prompt: ">",
        }
    }

    pub fn with_sort_modes(
        mut self,
        modes: Vec<(&'static str, Box<dyn Fn(&T, &T) -> Ordering + 'a>)>,
    ) -> Self {
        self.sort_modes = modes;
        self
    }

    pub fn with_prompt(mut self, p: &'a str) -> Self {
        self.prompt = p;
        self
    }

    /// Run the interactive selection using the given terminal.
    pub fn run(
        &self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<Option<usize>> {
        let mut filtered: Vec<usize> = (0..self.items.len()).collect();
        let mut sort_idx: usize = 0;
        let mut query = String::new();
        let mut selected: usize = 0;
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        if !self.sort_modes.is_empty() {
            let cmp = &self.sort_modes[sort_idx].1;
            filtered.sort_by(|&a, &b| cmp(&self.items[a], &self.items[b]));
        }

        loop {
            terminal.draw(|frame| {
                let area = frame.area();

                // Outer block with rounded borders
                let outer = Block::default()
                    .title(Span::styled(
                        format!(" {} ", self.prompt),
                        Style::default()
                            .fg(theme::ORANGE)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::BLUE_DIM));
                let inner = outer.inner(area);
                frame.render_widget(outer, area);

                let chunks = Layout::vertical([
                    Constraint::Length(1), // search bar
                    Constraint::Length(1), // separator
                    Constraint::Min(1),    // list
                    Constraint::Length(1), // status bar
                ])
                .split(inner);

                self.render_filter(frame, chunks[0], &query);
                // Thin separator line
                let sep = Paragraph::new(Line::styled(
                    "─".repeat(chunks[1].width as usize),
                    Style::default().fg(theme::BLUE_DIM),
                ));
                frame.render_widget(sep, chunks[1]);
                self.render_list(frame, chunks[2], &filtered, &mut list_state);
                self.render_status(frame, chunks[3], &filtered, sort_idx);
            })?;

            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if filtered.is_empty() {
                            return Ok(None);
                        }
                        return Ok(Some(filtered[selected]));
                    }
                    KeyCode::Char(c) => {
                        query.push(c);
                        self.refilter(&mut filtered, &query, &sort_idx);
                        selected = 0;
                        list_state.select(Some(0));
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        self.refilter(&mut filtered, &query, &sort_idx);
                        selected = 0;
                        list_state.select(Some(0));
                    }
                    KeyCode::Down => {
                        if !filtered.is_empty() {
                            selected = (selected + 1) % filtered.len();
                            list_state.select(Some(selected));
                        }
                    }
                    KeyCode::Up => {
                        if !filtered.is_empty() {
                            if selected == 0 {
                                selected = filtered.len() - 1;
                            } else {
                                selected -= 1;
                            }
                            list_state.select(Some(selected));
                        }
                    }
                    KeyCode::Tab => {
                        if !self.sort_modes.is_empty() {
                            sort_idx = (sort_idx + 1) % self.sort_modes.len();
                            let cmp = &self.sort_modes[sort_idx].1;
                            filtered.sort_by(|&a, &b| cmp(&self.items[a], &self.items[b]));
                            selected = 0;
                            list_state.select(Some(0));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn refilter(&self, filtered: &mut Vec<usize>, query: &str, sort_idx: &usize) {
        filtered.clear();
        for (i, item) in self.items.iter().enumerate() {
            if query.is_empty() || (self.filter)(item, query) {
                filtered.push(i);
            }
        }
        if !self.sort_modes.is_empty() {
            let cmp = &self.sort_modes[*sort_idx].1;
            filtered.sort_by(|&a, &b| cmp(&self.items[a], &self.items[b]));
        }
    }

    fn render_filter(&self, frame: &mut ratatui::Frame, area: Rect, query: &str) {
        let text = Line::from(vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(theme::ORANGE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(query, Style::default().fg(theme::FG)),
            Span::styled(
                "▏",
                Style::default()
                    .fg(theme::ORANGE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);
        frame.render_widget(Paragraph::new(text), area);
    }

    fn render_list(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        filtered: &[usize],
        list_state: &mut ListState,
    ) {
        let width = area.width.saturating_sub(4);
        let items: Vec<ListItem> = filtered
            .iter()
            .map(|&idx| {
                let text = (self.display)(&self.items[idx], width);
                ListItem::new(Line::styled(text, Style::default().fg(theme::FG)))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(theme::ORANGE)
                    .bg(theme::HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        frame.render_stateful_widget(list, area, list_state);
    }

    fn render_status(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        filtered: &[usize],
        sort_idx: usize,
    ) {
        let mut spans = vec![
            Span::styled(
                format!(" {}/{}", filtered.len(), self.items.len()),
                Style::default().fg(theme::MUTED),
            ),
        ];

        if !self.sort_modes.is_empty() {
            spans.push(Span::styled("  ", Style::default()));
            spans.push(Span::styled(
                "Tab",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {}", self.sort_modes[sort_idx].0),
                Style::default().fg(theme::MUTED),
            ));
        }

        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            "Enter",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" select", Style::default().fg(theme::MUTED)));
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            "Esc",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" back", Style::default().fg(theme::MUTED)));

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
