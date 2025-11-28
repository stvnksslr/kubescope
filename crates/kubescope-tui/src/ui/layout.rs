use ratatui::layout::{Constraint, Direction, Layout as RatatuiLayout, Rect};

/// Layout helper for consistent screen layouts
pub struct Layout;

impl Layout {
    /// Create the main layout with header, content, and status bar
    pub fn main(area: Rect) -> (Rect, Rect, Rect) {
        let chunks = RatatuiLayout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Content
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        (chunks[0], chunks[1], chunks[2])
    }

    /// Create a centered content area (for selection screens)
    pub fn centered_list(area: Rect, width_percent: u16) -> Rect {
        let horizontal = RatatuiLayout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - width_percent) / 2),
                Constraint::Percentage(width_percent),
                Constraint::Percentage((100 - width_percent) / 2),
            ])
            .split(area);

        let vertical = RatatuiLayout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(horizontal[1]);

        vertical[1]
    }

    /// Create layout for log viewer with optional stats sidebar
    pub fn log_viewer(area: Rect, show_stats: bool) -> (Rect, Option<Rect>) {
        if show_stats {
            let chunks = RatatuiLayout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(10), // Stats sidebar
                    Constraint::Min(1),     // Log content
                ])
                .split(area);
            (chunks[1], Some(chunks[0]))
        } else {
            (area, None)
        }
    }
}
