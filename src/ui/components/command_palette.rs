use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::Action;

/// A command that can be executed from the palette
#[derive(Clone)]
pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub key_hint: &'static str,
    pub action: Action,
}

/// Command palette state
pub struct CommandPaletteState {
    pub visible: bool,
    pub search_input: String,
    pub list_state: ListState,
    pub filtered_indices: Vec<usize>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            visible: false,
            search_input: String::new(),
            list_state,
            filtered_indices: Vec::new(),
        }
    }
}

impl CommandPaletteState {
    pub fn open(&mut self, commands: &[Command]) {
        self.visible = true;
        self.search_input.clear();
        self.list_state.select(Some(0));
        self.update_filtered(commands);
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.search_input.clear();
    }

    pub fn update_filtered(&mut self, commands: &[Command]) {
        let query = self.search_input.to_lowercase();
        self.filtered_indices = commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if query.is_empty() {
                    true
                } else {
                    cmd.name.to_lowercase().contains(&query)
                        || cmd.description.to_lowercase().contains(&query)
                }
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection if out of bounds
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            let current = self.list_state.selected().unwrap_or(0);
            if current >= self.filtered_indices.len() {
                self.list_state.select(Some(0));
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_indices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn move_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_indices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn selected_command<'a>(&self, commands: &'a [Command]) -> Option<&'a Command> {
        let selected_idx = self.list_state.selected()?;
        let cmd_idx = self.filtered_indices.get(selected_idx)?;
        commands.get(*cmd_idx)
    }

    pub fn input_char(&mut self, c: char, commands: &[Command]) {
        self.search_input.push(c);
        self.update_filtered(commands);
    }

    pub fn input_backspace(&mut self, commands: &[Command]) {
        self.search_input.pop();
        self.update_filtered(commands);
    }
}

/// Command palette widget
pub struct CommandPalette;

impl CommandPalette {
    pub fn render(frame: &mut Frame, state: &mut CommandPaletteState, commands: &[Command]) {
        let area = frame.area();

        // Center the palette
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 16.min(area.height.saturating_sub(4));

        let popup_area = centered_rect(popup_width, popup_height, area);

        // Clear the background
        frame.render_widget(Clear, popup_area);

        // Split into search input and list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search input
                Constraint::Min(1),    // Command list
            ])
            .split(popup_area);

        // Render search input
        let search_text = if state.search_input.is_empty() {
            vec![Span::styled(
                "Type to filter...",
                Style::default().fg(Color::DarkGray),
            )]
        } else {
            vec![
                Span::styled(&state.search_input, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ]
        };

        let search_widget = Paragraph::new(Line::from(search_text)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(Span::styled(
                    " Command Palette ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
        );
        frame.render_widget(search_widget, chunks[0]);

        // Build list items
        let items: Vec<ListItem> = state
            .filtered_indices
            .iter()
            .map(|&idx| {
                let cmd = &commands[idx];
                let line = Line::from(vec![
                    Span::styled(
                        format!("{:<20}", cmd.name),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(cmd.description, Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("  {}", cmd.key_hint),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        frame.render_stateful_widget(list, chunks[1], &mut state.list_state);
    }
}

/// Helper to create a centered rect
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// Get log viewer commands
pub fn log_viewer_commands() -> Vec<Command> {
    vec![
        Command {
            name: "Toggle Follow",
            description: "Auto-scroll to new logs",
            key_hint: "f",
            action: Action::ToggleAutoScroll,
        },
        Command {
            name: "Toggle Timestamps",
            description: "Show/hide timestamps",
            key_hint: "t",
            action: Action::ToggleTimestamps,
        },
        Command {
            name: "Toggle Local Time",
            description: "Switch local/UTC time",
            key_hint: "T",
            action: Action::ToggleLocalTime,
        },
        Command {
            name: "Toggle Pod Names",
            description: "Show/hide pod names",
            key_hint: "p",
            action: Action::TogglePodNames,
        },
        Command {
            name: "Toggle JSON",
            description: "Pretty print JSON logs",
            key_hint: "J",
            action: Action::ToggleJsonPrettyPrint,
        },
        Command {
            name: "JSON Key Filter",
            description: "Filter by JSON keys",
            key_hint: "K",
            action: Action::ToggleJsonKeyFilter,
        },
        Command {
            name: "Toggle Stats",
            description: "Show/hide stats bar",
            key_hint: "s",
            action: Action::ToggleStats,
        },
        Command {
            name: "Cycle Time Range",
            description: "Change log time window",
            key_hint: "r",
            action: Action::CycleTimeRange,
        },
        Command {
            name: "Search/Filter",
            description: "Filter logs with regex",
            key_hint: "/",
            action: Action::OpenSearch,
        },
        Command {
            name: "Clear Filter",
            description: "Remove active filter",
            key_hint: "n",
            action: Action::ClearFilter,
        },
        Command {
            name: "Toggle Case Sensitive",
            description: "Case sensitive search",
            key_hint: "i",
            action: Action::ToggleCaseSensitive,
        },
        Command {
            name: "Clear Logs",
            description: "Clear all log entries",
            key_hint: "c",
            action: Action::ClearLogs,
        },
        Command {
            name: "Export Logs",
            description: "Save logs to file",
            key_hint: "e",
            action: Action::ExportLogs,
        },
        Command {
            name: "Show Help",
            description: "Display keybindings",
            key_hint: "?",
            action: Action::ToggleHelp,
        },
        Command {
            name: "Scroll to Top",
            description: "Jump to first log",
            key_hint: "g",
            action: Action::ScrollToTop,
        },
        Command {
            name: "Scroll to Bottom",
            description: "Jump to latest log",
            key_hint: "G",
            action: Action::ScrollToBottom,
        },
        Command {
            name: "Go Back",
            description: "Return to deployment list",
            key_hint: "Esc",
            action: Action::GoBack,
        },
        Command {
            name: "Quit",
            description: "Exit kubescope",
            key_hint: "q",
            action: Action::Quit,
        },
    ]
}
