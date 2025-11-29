use ratatui::style::{Color, Modifier, Style};

/// Color theme for the application
pub struct Theme;

impl Theme {
    // Base colors
    pub const BG: Color = Color::Reset;
    pub const FG: Color = Color::White;
    pub const FG_DIM: Color = Color::DarkGray;

    // Accent colors
    pub const PRIMARY: Color = Color::Cyan;
    pub const SECONDARY: Color = Color::Blue;
    pub const HIGHLIGHT: Color = Color::Yellow;

    // Status colors
    pub const SUCCESS: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const ERROR: Color = Color::Red;

    // Log level colors
    pub const LOG_TRACE: Color = Color::DarkGray;
    pub const LOG_DEBUG: Color = Color::Cyan;
    pub const LOG_INFO: Color = Color::Green;
    pub const LOG_WARN: Color = Color::Yellow;
    pub const LOG_ERROR: Color = Color::Red;
    pub const LOG_FATAL: Color = Color::Magenta;

    // Border styles
    pub fn border() -> Style {
        Style::default().fg(Self::FG_DIM)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Self::PRIMARY)
    }

    // Text styles
    pub fn title() -> Style {
        Style::default()
            .fg(Self::PRIMARY)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text() -> Style {
        Style::default().fg(Self::FG)
    }

    pub fn text_dim() -> Style {
        Style::default().fg(Self::FG_DIM)
    }

    pub fn text_highlight() -> Style {
        Style::default()
            .fg(Self::HIGHLIGHT)
            .add_modifier(Modifier::BOLD)
    }

    // List styles
    pub fn list_item() -> Style {
        Style::default().fg(Self::FG)
    }

    pub fn list_item_selected() -> Style {
        Style::default()
            .fg(Self::BG)
            .bg(Self::PRIMARY)
            .add_modifier(Modifier::BOLD)
    }

    pub fn list_item_current() -> Style {
        Style::default()
            .fg(Self::SUCCESS)
            .add_modifier(Modifier::BOLD)
    }

    // Status bar
    pub fn status_bar() -> Style {
        Style::default().fg(Self::FG_DIM).bg(Color::DarkGray)
    }

    pub fn status_bar_key() -> Style {
        Style::default()
            .fg(Self::HIGHLIGHT)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    }

    // Error
    pub fn error() -> Style {
        Style::default()
            .fg(Self::ERROR)
            .add_modifier(Modifier::BOLD)
    }
}
