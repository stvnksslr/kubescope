use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout as RatatuiLayout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::app::AppState;
use crate::logs::LogBuffer;
use crate::types::{ArcLogEntry, LogEntry, LogLevel};
use crate::ui::Theme;

/// Log viewer screen
pub struct LogViewerScreen;

impl LogViewerScreen {
    pub fn render(
        frame: &mut Frame,
        state: &mut AppState,
        log_buffer: &LogBuffer,
        dropped_count: u64,
    ) {
        let area = frame.area();

        // Determine if we need the filter bar
        let show_filter_bar = state.ui_state.search_active
            || state.ui_state.active_filter.is_some()
            || state.ui_state.filter_error.is_some();

        // Build constraints based on what's visible
        let mut constraints = vec![Constraint::Length(3)]; // Header always

        if state.ui_state.stats_visible {
            constraints.push(Constraint::Length(3)); // Stats bar
        }
        if show_filter_bar {
            constraints.push(Constraint::Length(3)); // Filter bar
        }
        constraints.push(Constraint::Min(1)); // Logs
        constraints.push(Constraint::Length(1)); // Status bar

        let chunks = RatatuiLayout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let mut idx = 0;

        // Header
        Self::render_header(frame, chunks[idx], state);
        idx += 1;

        // Stats bar (if visible)
        if state.ui_state.stats_visible {
            Self::render_stats_bar(frame, chunks[idx], log_buffer);
            idx += 1;
        }

        // Filter bar (if visible)
        if show_filter_bar {
            Self::render_filter_bar(frame, chunks[idx], state);
            idx += 1;
        }

        // Logs
        Self::render_logs(frame, chunks[idx], state, log_buffer);
        idx += 1;

        // Status bar
        Self::render_status_bar(frame, chunks[idx], state, log_buffer, dropped_count);
    }

    fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
        let context_name = state.selected_context.as_deref().unwrap_or("?");
        let namespace = state.selected_namespace.as_deref().unwrap_or("?");
        let deployment = state.selected_deployment.as_deref().unwrap_or("?");
        let pod_count = state.pods.len();
        let time_range = state.ui_state.time_range.label();

        let title = Line::from(vec![
            Span::styled("kubescope", Theme::title()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(context_name, Theme::text()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(namespace, Theme::text()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(deployment, Theme::text_highlight()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(format!("{} pods", pod_count), Theme::text()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(
                format!("⏱ {}", time_range),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let header = Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border()),
        );

        frame.render_widget(header, area);
    }

    fn render_filter_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let mut spans = vec![];

        // Prompt
        if state.ui_state.search_active {
            spans.push(Span::styled(
                " /",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(" Filter: ", Theme::text_dim()));
        }

        // Input or current filter pattern
        let pattern = if state.ui_state.search_active {
            &state.ui_state.search_input
        } else if let Some(filter) = &state.ui_state.active_filter {
            filter.pattern()
        } else {
            ""
        };

        spans.push(Span::styled(pattern.to_string(), Theme::text_highlight()));

        // Cursor when active
        if state.ui_state.search_active {
            spans.push(Span::styled(
                "█",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            ));
        }

        // Error message
        if let Some(err) = &state.ui_state.filter_error {
            spans.push(Span::styled(" ", Theme::text()));
            spans.push(Span::styled(
                format!("⚠ {}", err),
                Style::default().fg(Color::Red),
            ));
        }

        // Case sensitivity indicator
        if state.ui_state.active_filter.is_some() || state.ui_state.search_active {
            spans.push(Span::styled("  ", Theme::text()));
            let case_text = if state.ui_state.filter_case_insensitive {
                "[i] case-insensitive"
            } else {
                "[I] case-sensitive"
            };
            spans.push(Span::styled(case_text, Theme::text_dim()));
        }

        // Hints
        if state.ui_state.search_active {
            spans.push(Span::styled(
                "  [Enter] Apply  [Esc] Cancel",
                Theme::text_dim(),
            ));
        } else if state.ui_state.active_filter.is_some() {
            spans.push(Span::styled("  [n] Clear  [/] Edit", Theme::text_dim()));
        }

        let filter_bar = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if state.ui_state.search_active {
                    Style::default().fg(Color::Yellow)
                } else if state.ui_state.filter_error.is_some() {
                    Style::default().fg(Color::Red)
                } else {
                    Theme::border()
                })
                .title(Span::styled(" Search/Filter ", Theme::title())),
        );

        frame.render_widget(filter_bar, area);
    }

    fn render_logs(frame: &mut Frame, area: Rect, state: &mut AppState, log_buffer: &LogBuffer) {
        let current_log_count = log_buffer.len();

        // Check if we need to refresh the filter cache
        let needs_refresh = state.ui_state.filter_cache.needs_refresh(
            state.ui_state.active_filter.as_ref(),
            state.ui_state.filter_case_insensitive,
            &state.ui_state.json_visible_keys,
            current_log_count,
        );

        // Only recompute filtered logs when cache is invalid
        if needs_refresh {
            let all_logs = log_buffer.all();

            // Apply text filter if active (Arc clones are cheap)
            let text_filtered: Vec<ArcLogEntry> =
                if let Some(filter) = &state.ui_state.active_filter {
                    all_logs.into_iter().filter(|e| filter.matches(e)).collect()
                } else {
                    all_logs
                };

            // Apply JSON key filter if active (only show entries with selected keys)
            let filtered_logs: Vec<ArcLogEntry> = if !state.ui_state.json_visible_keys.is_empty() {
                text_filtered
                    .into_iter()
                    .filter(|e| {
                        // Keep entry if it has any of the selected keys
                        if let Some(fields) = &e.fields {
                            fields
                                .keys()
                                .any(|k| state.ui_state.json_visible_keys.contains(k))
                        } else {
                            false // No fields = no match when filtering
                        }
                    })
                    .collect()
            } else {
                text_filtered
            };

            // Update the cache
            state.ui_state.filter_cache.update(
                state.ui_state.active_filter.as_ref(),
                state.ui_state.filter_case_insensitive,
                &state.ui_state.json_visible_keys,
                current_log_count,
                filtered_logs,
            );
        }

        let total_logs = state.ui_state.filter_cache.cached_entries.len();

        // Calculate visible area (accounting for border)
        let inner_height = area.height.saturating_sub(2) as usize;

        // Auto-scroll: if at bottom, stay at bottom
        if state.ui_state.auto_scroll && total_logs > 0 {
            state.ui_state.log_scroll = total_logs.saturating_sub(inner_height);
        }

        // Clamp scroll position
        let max_scroll = total_logs.saturating_sub(inner_height);
        if state.ui_state.log_scroll > max_scroll {
            state.ui_state.log_scroll = max_scroll;
        }

        // Get visible logs from cache (viewport-first: skip/take from cached results)
        let visible_logs: Vec<ArcLogEntry> = state
            .ui_state
            .filter_cache
            .cached_entries
            .iter()
            .skip(state.ui_state.log_scroll)
            .take(inner_height)
            .cloned()
            .collect();

        // Build log lines with highlighting
        let lines: Vec<Line> = visible_logs
            .iter()
            .map(|entry| Self::format_log_line(entry, state))
            .collect();

        // Title shows filter status
        let title = if state.ui_state.active_filter.is_some()
            || !state.ui_state.json_visible_keys.is_empty()
        {
            format!(" Logs ({} matching) ", total_logs)
        } else {
            format!(" Logs ({}) ", total_logs)
        };

        let logs_widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(Span::styled(title, Theme::title())),
        );

        frame.render_widget(logs_widget, area);

        // Render scrollbar
        if total_logs > inner_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            // Calculate scrollbar position as percentage of scrollable range
            let scroll_position = state.ui_state.log_scroll.min(max_scroll);
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(max_scroll)
                .position(scroll_position);

            frame.render_stateful_widget(
                scrollbar,
                area.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }

    fn render_stats_bar(frame: &mut Frame, area: Rect, log_buffer: &LogBuffer) {
        let counts = log_buffer.level_counts();
        let total = counts.total();

        // Build horizontal stats display
        let mut spans = vec![Span::styled(" ", Theme::text())];

        // Fatal (only if > 0)
        if counts.fatal > 0 {
            spans.push(Span::styled(
                "FTL:",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!("{} ", counts.fatal), Theme::text()));
        }

        // Error
        spans.push(Span::styled(
            "ERR:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!("{} ", counts.error), Theme::text()));

        // Warn
        spans.push(Span::styled(
            "WRN:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!("{} ", counts.warn), Theme::text()));

        // Info
        spans.push(Span::styled(
            "INF:",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!("{} ", counts.info), Theme::text()));

        // Debug
        spans.push(Span::styled(
            "DBG:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!("{} ", counts.debug), Theme::text()));

        // Trace (only if > 0)
        if counts.trace > 0 {
            spans.push(Span::styled(
                "TRC:",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!("{} ", counts.trace), Theme::text()));
        }

        // Separator and total
        spans.push(Span::styled("│ ", Theme::text_dim()));
        spans.push(Span::styled("Total:", Theme::text_dim()));
        spans.push(Span::styled(format!("{}", total), Theme::text()));

        let stats_widget = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(Span::styled(" Stats ", Theme::title())),
        );

        frame.render_widget(stats_widget, area);
    }

    fn format_log_line<'a>(entry: &'a LogEntry, state: &AppState) -> Line<'a> {
        let mut spans = Vec::new();

        // Line number (compact)
        spans.push(Span::styled(format!("{:>5}", entry.id), Theme::text_dim()));

        // Timestamp (if enabled and available)
        if state.ui_state.show_timestamps
            && let Some(ts) = &entry.timestamp
        {
            let time_str = if state.ui_state.use_local_time {
                ts.with_timezone(&Local).format("%H:%M:%S").to_string()
            } else {
                ts.format("%H:%M:%S").to_string()
            };
            spans.push(Span::styled(format!(" {}", time_str), Theme::text_dim()));
        }

        // Pod name (if enabled)
        if state.ui_state.show_pod_names {
            spans.push(Span::styled(
                format!(" {:>10}", entry.short_pod_name()),
                Style::default().fg(pod_color(&entry.pod_name)),
            ));
        }

        // Log level (fixed width)
        spans.push(Span::styled(
            format!(" {:>3}", entry.level.as_str()),
            Style::default()
                .fg(entry.level.color())
                .add_modifier(Modifier::BOLD),
        ));

        // Separator
        spans.push(Span::styled(" │ ", Theme::text_dim()));

        // Message content - handle JSON colorization
        if state.ui_state.json_pretty_print && entry.is_json {
            // Get JSON content (remove timestamp prefix if present)
            let json_str = if entry.timestamp.is_some() && entry.raw.len() > 31 {
                &entry.raw[31..]
            } else {
                &entry.raw
            };

            // Colorize the JSON with optional key filtering (pass pre-parsed fields to avoid re-parsing)
            let json_spans = colorize_json(
                json_str,
                &state.ui_state.json_visible_keys,
                entry.fields.as_ref(),
            );
            spans.extend(json_spans);
        } else {
            // Regular message handling
            let message = if entry.timestamp.is_some() && entry.raw.len() > 31 {
                entry.raw[31..].to_string()
            } else {
                entry.raw.clone()
            };

            // Truncate message if too long
            let max_msg_len = 200;
            let display_msg = if message.len() > max_msg_len {
                format!("{}...", &message[..max_msg_len])
            } else {
                message
            };

            // Apply search highlighting if filter is active
            if let Some(filter) = &state.ui_state.active_filter {
                let matches = filter.find_matches(&display_msg);
                if !matches.is_empty() {
                    // Build spans with highlighted matches
                    let base_style = level_text_style(entry.level);
                    let highlight_style = Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);

                    let mut last_end = 0;
                    for (start, end) in matches {
                        // Add text before match
                        if start > last_end {
                            spans.push(Span::styled(
                                display_msg[last_end..start].to_string(),
                                base_style,
                            ));
                        }
                        // Add highlighted match
                        spans.push(Span::styled(
                            display_msg[start..end].to_string(),
                            highlight_style,
                        ));
                        last_end = end;
                    }
                    // Add remaining text after last match
                    if last_end < display_msg.len() {
                        spans.push(Span::styled(
                            display_msg[last_end..].to_string(),
                            base_style,
                        ));
                    }
                } else {
                    spans.push(Span::styled(display_msg, level_text_style(entry.level)));
                }
            } else {
                spans.push(Span::styled(display_msg, level_text_style(entry.level)));
            }
        }

        Line::from(spans)
    }

    fn render_status_bar(
        frame: &mut Frame,
        area: Rect,
        state: &AppState,
        log_buffer: &LogBuffer,
        dropped_count: u64,
    ) {
        let counts = log_buffer.level_counts();
        let total = counts.total();

        let mut spans = vec![
            Span::styled(" ", Theme::status_bar()),
            // Keyboard hints
            Span::styled("[", Theme::status_bar()),
            Span::styled("Space", Theme::status_bar_key()),
            Span::styled("]Cmd ", Theme::status_bar()),
            Span::styled("[", Theme::status_bar()),
            Span::styled("/", Theme::status_bar_key()),
            Span::styled("]Filter ", Theme::status_bar()),
            Span::styled("[", Theme::status_bar()),
            Span::styled("r", Theme::status_bar_key()),
            Span::styled("]", Theme::status_bar()),
            Span::styled(
                format!("[{}]", state.ui_state.time_range.label()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Theme::status_bar()),
            Span::styled("[", Theme::status_bar()),
            Span::styled("e", Theme::status_bar_key()),
            Span::styled("]Export ", Theme::status_bar()),
            Span::styled("[", Theme::status_bar()),
            Span::styled("?", Theme::status_bar_key()),
            Span::styled("]Help ", Theme::status_bar()),
            Span::styled("[", Theme::status_bar()),
            Span::styled("Esc", Theme::status_bar_key()),
            Span::styled("]Back", Theme::status_bar()),
        ];

        // Show dropped logs warning if any
        if dropped_count > 0 {
            spans.push(Span::styled(" ", Theme::status_bar()));
            spans.push(Span::styled(
                format!("[{}dropped]", dropped_count),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }

        // Right side: log counts
        let right_text = format!(
            "E:{} W:{} I:{} | {} logs {}",
            counts.error + counts.fatal,
            counts.warn,
            counts.info,
            total,
            if state.ui_state.auto_scroll {
                "▼"
            } else {
                " "
            }
        );

        // Calculate padding
        let left_width: usize = spans.iter().map(|s| s.content.len()).sum();
        let right_width = right_text.len();
        let padding = (area.width as usize).saturating_sub(left_width + right_width + 1);

        spans.push(Span::styled(" ".repeat(padding), Theme::status_bar()));
        spans.push(Span::styled(right_text, Theme::status_bar()));

        let status = Paragraph::new(Line::from(spans)).style(Theme::status_bar());

        frame.render_widget(status, area);
    }
}

/// Get a consistent color for a pod name
fn pod_color(pod_name: &str) -> ratatui::style::Color {
    use ratatui::style::Color;

    // Hash the pod name to get a consistent color
    let hash: u32 = pod_name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(b as u32));

    let colors = [
        Color::Cyan,
        Color::Magenta,
        Color::Blue,
        Color::Yellow,
        Color::Green,
        Color::Red,
        Color::LightCyan,
        Color::LightMagenta,
    ];

    colors[(hash as usize) % colors.len()]
}

/// Get text style based on log level
fn level_text_style(level: LogLevel) -> Style {
    match level {
        LogLevel::Error | LogLevel::Fatal => Style::default().fg(ratatui::style::Color::Red),
        LogLevel::Warn => Style::default().fg(ratatui::style::Color::Yellow),
        _ => Style::default().fg(ratatui::style::Color::White),
    }
}

/// Colorize JSON string into styled spans with optional key filtering
/// Uses pre-parsed fields when available to avoid re-parsing JSON
fn colorize_json(
    json_str: &str,
    visible_keys: &std::collections::HashSet<String>,
    parsed_fields: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> Vec<Span<'static>> {
    // If we have key filters and pre-parsed fields, use them to avoid re-parsing
    if !visible_keys.is_empty() {
        if let Some(fields) = parsed_fields {
            // Use pre-parsed fields - much faster than re-parsing
            let filtered: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .filter(|(k, _)| visible_keys.contains(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            if filtered.is_empty() {
                // No matching keys, show empty object
                return vec![Span::styled("{}", Style::default().fg(Color::White))];
            }

            let filtered_str = serde_json::to_string(&serde_json::Value::Object(filtered))
                .unwrap_or_else(|_| json_str.to_string());
            return colorize_json_inner(&filtered_str);
        }

        // Fallback: parse JSON if fields not pre-parsed (shouldn't happen for JSON logs)
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str)
            && let serde_json::Value::Object(map) = parsed
        {
            let filtered: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter(|(k, _)| visible_keys.contains(k))
                .collect();
            let filtered_str = serde_json::to_string(&serde_json::Value::Object(filtered))
                .unwrap_or_else(|_| json_str.to_string());
            return colorize_json_inner(&filtered_str);
        }
    }

    colorize_json_inner(json_str)
}

/// Inner JSON colorization function
fn colorize_json_inner(json_str: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = json_str.chars().peekable();
    let mut current = String::new();

    // JSON syntax colors
    let brace_style = Style::default().fg(Color::White);
    let key_style = Style::default().fg(Color::Cyan);
    let string_style = Style::default().fg(Color::Green);
    let number_style = Style::default().fg(Color::Yellow);
    let bool_style = Style::default().fg(Color::Magenta);
    let null_style = Style::default().fg(Color::Red);
    let punct_style = Style::default().fg(Color::DarkGray);

    let max_len = 300; // Limit total length
    let mut total_len = 0;

    while let Some(c) = chars.next() {
        if total_len >= max_len {
            spans.push(Span::styled("...", punct_style));
            break;
        }

        match c {
            '{' | '}' | '[' | ']' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), punct_style));
                    total_len += current.len();
                    current.clear();
                }
                spans.push(Span::styled(c.to_string(), brace_style));
                total_len += 1;
            }
            ':' | ',' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), punct_style));
                    total_len += current.len();
                    current.clear();
                }
                spans.push(Span::styled(c.to_string(), punct_style));
                total_len += 1;
            }
            '"' => {
                // Parse string
                let mut s = String::from("\"");
                let mut is_key = false;

                // Check if this might be a key (look back for { or ,)
                let trimmed =
                    json_str[..json_str.len().saturating_sub(chars.clone().count() + 1)].trim_end();
                if trimmed.ends_with('{') || trimmed.ends_with(',') {
                    is_key = true;
                }

                while let Some(sc) = chars.next() {
                    s.push(sc);
                    if sc == '"' {
                        break;
                    }
                    if sc == '\\'
                        && let Some(escaped) = chars.next()
                    {
                        s.push(escaped);
                    }
                }

                let style = if is_key { key_style } else { string_style };
                spans.push(Span::styled(s.clone(), style));
                total_len += s.len();
            }
            't' | 'f' => {
                // Check for true/false
                let mut word = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_alphabetic() {
                        word.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if word == "true" || word == "false" {
                    spans.push(Span::styled(word.clone(), bool_style));
                } else {
                    spans.push(Span::styled(word.clone(), punct_style));
                }
                total_len += word.len();
            }
            'n' => {
                // Check for null
                let mut word = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_alphabetic() {
                        word.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if word == "null" {
                    spans.push(Span::styled(word.clone(), null_style));
                } else {
                    spans.push(Span::styled(word.clone(), punct_style));
                }
                total_len += word.len();
            }
            '0'..='9' | '-' | '.' => {
                // Parse number
                let mut num = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit()
                        || next == '.'
                        || next == 'e'
                        || next == 'E'
                        || next == '+'
                        || next == '-'
                    {
                        num.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                spans.push(Span::styled(num.clone(), number_style));
                total_len += num.len();
            }
            ' ' | '\n' | '\r' | '\t' => {
                // Collapse whitespace to single space
                if !current.is_empty()
                    || spans
                        .last()
                        .map(|s| !s.content.ends_with(' '))
                        .unwrap_or(true)
                {
                    current.push(' ');
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, punct_style));
    }

    spans
}
