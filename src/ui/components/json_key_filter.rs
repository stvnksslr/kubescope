use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::AppState;
use crate::logs::LogBuffer;

/// JSON key filter overlay - handles high cardinality key sets
pub struct JsonKeyFilter;

impl JsonKeyFilter {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = frame.area();

        // Larger popup for better usability
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 30.min(area.height.saturating_sub(4));

        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(x, y, popup_width, popup_height);

        // Clear the background
        frame.render_widget(Clear, popup_area);

        // Get filtered keys
        let search = state.ui_state.json_key_search.to_lowercase();
        let filtered_keys: Vec<&String> = if search.is_empty() {
            state.ui_state.json_available_keys.iter().collect()
        } else {
            state
                .ui_state
                .json_available_keys
                .iter()
                .filter(|k| k.to_lowercase().contains(&search))
                .collect()
        };

        let total_keys = state.ui_state.json_available_keys.len();
        let filtered_count = filtered_keys.len();
        let selected_count = state.ui_state.json_visible_keys.len();

        // Calculate viewport
        let header_lines = 3; // Search bar + separator + column headers
        let footer_lines = 2; // Help text
        let viewport_height =
            (popup_height as usize).saturating_sub(header_lines + footer_lines + 2); // -2 for borders

        // Adjust scroll to keep selection visible
        if state.ui_state.json_key_selection >= state.ui_state.json_key_scroll + viewport_height {
            state.ui_state.json_key_scroll = state
                .ui_state
                .json_key_selection
                .saturating_sub(viewport_height - 1);
        }
        if state.ui_state.json_key_selection < state.ui_state.json_key_scroll {
            state.ui_state.json_key_scroll = state.ui_state.json_key_selection;
        }

        // Clamp selection to valid range
        if !filtered_keys.is_empty() && state.ui_state.json_key_selection >= filtered_keys.len() {
            state.ui_state.json_key_selection = filtered_keys.len() - 1;
        }

        // Build content lines
        let mut lines = Vec::new();

        // Search input line
        let search_line = Line::from(vec![
            Span::styled(" Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                &state.ui_state.json_key_search,
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled(
                format!(
                    "  ({}/{} keys, {} selected)",
                    filtered_count, total_keys, selected_count
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        lines.push(search_line);

        // Separator
        lines.push(Line::from(Span::styled(
            "─".repeat(popup_width.saturating_sub(2) as usize),
            Style::default().fg(Color::DarkGray),
        )));

        // Visible keys in viewport
        let visible_keys: Vec<_> = filtered_keys
            .iter()
            .skip(state.ui_state.json_key_scroll)
            .take(viewport_height)
            .enumerate()
            .collect();

        for (viewport_idx, key) in visible_keys {
            let actual_idx = state.ui_state.json_key_scroll + viewport_idx;
            let is_cursor = actual_idx == state.ui_state.json_key_selection;
            let is_selected = state.ui_state.json_visible_keys.is_empty()
                || state.ui_state.json_visible_keys.contains(*key);

            let checkbox = if is_selected { "[✓]" } else { "[ ]" };
            let cursor = if is_cursor { "▸" } else { " " };

            let line_style = if is_cursor {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let checkbox_style = if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let key_style = if is_cursor {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };

            // Truncate key if too long
            let max_key_len = (popup_width as usize).saturating_sub(10);
            let display_key = if key.len() > max_key_len {
                format!("{}...", &key[..max_key_len.saturating_sub(3)])
            } else {
                (*key).clone()
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {}", cursor), line_style),
                Span::styled(format!("{} ", checkbox), checkbox_style),
                Span::styled(display_key, key_style),
            ]));
        }

        // Pad with empty lines if needed
        while lines.len() < header_lines + viewport_height {
            lines.push(Line::from(""));
        }

        // Scroll indicator
        if filtered_keys.len() > viewport_height {
            let scroll_info = format!(
                " [{}-{} of {}]",
                state.ui_state.json_key_scroll + 1,
                (state.ui_state.json_key_scroll + viewport_height).min(filtered_keys.len()),
                filtered_keys.len()
            );
            lines.push(Line::from(Span::styled(
                scroll_info,
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(""));
        }

        // Help text
        lines.push(Line::from(vec![
            Span::styled(" [Tab]", Style::default().fg(Color::Yellow)),
            Span::styled("Toggle ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::styled("Select matching ", Style::default().fg(Color::DarkGray)),
            Span::styled("[^A]", Style::default().fg(Color::Yellow)),
            Span::styled("All ", Style::default().fg(Color::DarkGray)),
            Span::styled("[^X]", Style::default().fg(Color::Yellow)),
            Span::styled("Clear ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled("Close", Style::default().fg(Color::DarkGray)),
        ]));

        // Title with selection status
        let title = if state.ui_state.json_visible_keys.is_empty() {
            " JSON Keys (showing all) "
        } else {
            " JSON Keys (filtered) "
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
        );

        frame.render_widget(paragraph, popup_area);
    }
}

/// Collect all unique JSON keys from log entries
/// Uses incrementally maintained key set from the buffer (O(1) vs O(n*m))
pub fn collect_json_keys(log_buffer: &LogBuffer) -> Vec<String> {
    log_buffer.json_keys()
}
