use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Help overlay showing keybindings
pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame) {
        let area = frame.area();

        // Center the help popup
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 24.min(area.height.saturating_sub(4));

        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        // Clear the background
        frame.render_widget(Clear, popup_area);

        let help_text = vec![
            Line::from(Span::styled(
                "Keybindings",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Navigation", Style::default().fg(Color::Yellow)),
            ]),
            Self::key_line("j/↓", "Scroll down"),
            Self::key_line("k/↑", "Scroll up"),
            Self::key_line("Ctrl+d", "Page down"),
            Self::key_line("Ctrl+u", "Page up"),
            Self::key_line("g", "Go to top"),
            Self::key_line("G", "Go to bottom"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Display", Style::default().fg(Color::Yellow)),
            ]),
            Self::key_line("f", "Toggle follow mode"),
            Self::key_line("t", "Toggle timestamps"),
            Self::key_line("p", "Toggle pod names"),
            Self::key_line("J", "Toggle JSON pretty print"),
            Self::key_line("s", "Toggle stats bar"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Actions", Style::default().fg(Color::Yellow)),
            ]),
            Self::key_line("Space", "Command palette"),
            Self::key_line("/", "Search/filter logs"),
            Self::key_line("n", "Clear filter"),
            Self::key_line("c", "Clear logs"),
            Self::key_line("e", "Export logs to file"),
            Self::key_line("?", "Toggle this help"),
            Self::key_line("Esc", "Go back"),
            Self::key_line("q", "Quit"),
        ];

        let help_widget = Paragraph::new(help_text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    " Help ",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )),
        );

        frame.render_widget(help_widget, popup_area);
    }

    fn key_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
        Line::from(vec![
            Span::styled(format!("  {:>8}", key), Style::default().fg(Color::Green)),
            Span::styled(format!("  {}", desc), Style::default().fg(Color::White)),
        ])
    }
}
