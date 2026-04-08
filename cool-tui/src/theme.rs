use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub primary: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub muted: Color,
    pub surface: Color,
    pub surface_bright: Color,
    pub border: Color,
    pub border_focused: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub selection: Color,
    pub search_match: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(22, 22, 30),
            fg: Color::Rgb(205, 214, 244),
            accent: Color::Rgb(137, 180, 250),      // Blue
            accent_dim: Color::Rgb(88, 91, 112),
            primary: Color::Rgb(203, 166, 247),      // Mauve
            secondary: Color::Rgb(148, 226, 213),     // Teal
            success: Color::Rgb(166, 227, 161),       // Green
            warning: Color::Rgb(249, 226, 175),       // Yellow
            error: Color::Rgb(243, 139, 168),         // Red
            muted: Color::Rgb(108, 112, 134),
            surface: Color::Rgb(30, 30, 46),
            surface_bright: Color::Rgb(49, 50, 68),
            border: Color::Rgb(69, 71, 90),
            border_focused: Color::Rgb(137, 180, 250),
            tab_active: Color::Rgb(137, 180, 250),
            tab_inactive: Color::Rgb(88, 91, 112),
            selection: Color::Rgb(49, 50, 68),
            search_match: Color::Rgb(249, 226, 175),
        }
    }

    pub fn style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .fg(self.fg)
            .bg(self.selection)
            .add_modifier(Modifier::BOLD)
    }

    pub fn active_tab_style(&self) -> Style {
        Style::default()
            .fg(self.bg)
            .bg(self.tab_active)
            .add_modifier(Modifier::BOLD)
    }

    pub fn inactive_tab_style(&self) -> Style {
        Style::default()
            .fg(self.tab_inactive)
            .bg(self.surface)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn border_focused_style(&self) -> Style {
        Style::default().fg(self.border_focused)
    }

    pub fn urgency_style(&self, urgency: Urgency) -> Style {
        match urgency {
            Urgency::Urgent => Style::default().fg(self.error).add_modifier(Modifier::BOLD),
            Urgency::Soon => Style::default().fg(self.warning),
            Urgency::Later => Style::default().fg(self.success),
            Urgency::None => Style::default().fg(self.muted),
        }
    }

    pub fn status_bar_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.surface_bright)
    }

    pub fn key_hint_style(&self) -> Style {
        Style::default()
            .fg(self.warning)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_desc_style(&self) -> Style {
        Style::default().fg(self.muted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Urgent, // < 24h
    Soon,   // < 3 days
    Later,  // > 3 days
    None,   // no due date
}

impl Urgency {
    pub fn from_hours(hours: i64) -> Self {
        if hours < 0 {
            Self::None
        } else if hours < 24 {
            Self::Urgent
        } else if hours < 72 {
            Self::Soon
        } else {
            Self::Later
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::Urgent => "!!",
            Self::Soon => "! ",
            Self::Later => "  ",
            Self::None => "  ",
        }
    }
}

pub static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(Theme::dark);

/// Colors assigned to courses for visual distinction
const COURSE_COLORS: &[Color] = &[
    Color::Rgb(137, 180, 250), // Blue
    Color::Rgb(203, 166, 247), // Mauve
    Color::Rgb(148, 226, 213), // Teal
    Color::Rgb(166, 227, 161), // Green
    Color::Rgb(250, 179, 135), // Peach
    Color::Rgb(245, 194, 231), // Pink
    Color::Rgb(249, 226, 175), // Yellow
    Color::Rgb(180, 190, 254), // Lavender
    Color::Rgb(242, 205, 205), // Flamingo
    Color::Rgb(148, 226, 213), // Teal variant
];

pub fn course_color(index: usize) -> Color {
    COURSE_COLORS[index % COURSE_COLORS.len()]
}
