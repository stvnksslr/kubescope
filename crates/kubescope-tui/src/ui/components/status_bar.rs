use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::Widget,
};

use crate::ui::Theme;

/// Status bar showing keyboard shortcuts
pub struct StatusBar<'a> {
    hints: Vec<(&'a str, &'a str)>,
    right_text: Option<String>,
}

impl<'a> StatusBar<'a> {
    pub fn new() -> Self {
        Self {
            hints: Vec::new(),
            right_text: None,
        }
    }

    /// Add keyboard hints as (key, description) pairs
    pub fn hints<I>(mut self, hints: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        self.hints = hints.into_iter().collect();
        self
    }

    /// Set text to display on the right side
    pub fn right<S: Into<String>>(mut self, text: S) -> Self {
        self.right_text = Some(text.into());
        self
    }
}

impl Default for StatusBar<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background
        buf.set_style(area, Theme::status_bar());

        // Build hints
        let mut spans = Vec::new();
        for (i, (key, desc)) in self.hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", Theme::status_bar()));
            }
            spans.push(Span::styled(format!("[{}]", key), Theme::status_bar_key()));
            spans.push(Span::styled(format!(" {}", desc), Theme::status_bar()));
        }

        let line = Line::from(spans);
        let line_width = line.width() as u16;

        // Render hints on the left
        buf.set_line(area.x + 1, area.y, &line, area.width.saturating_sub(2));

        // Render right text if present
        if let Some(right) = self.right_text {
            let right_span = Span::styled(&right, Theme::status_bar());
            let right_x = area.x + area.width.saturating_sub(right.len() as u16 + 2);
            if right_x > area.x + line_width + 2 {
                buf.set_span(right_x, area.y, &right_span, right.len() as u16);
            }
        }
    }
}

/// Default hints for list navigation screens
pub fn list_nav_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("↑/k", "Up"),
        ("↓/j", "Down"),
        ("Enter", "Select"),
        ("Esc", "Back"),
        ("q", "Quit"),
    ]
}
