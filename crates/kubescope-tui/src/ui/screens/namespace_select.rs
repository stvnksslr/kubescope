use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{
    app::AppState,
    ui::{
        components::{list_nav_hints, ListSelector, ListSelectorExt, StatusBar},
        Layout, Theme,
    },
};

/// Namespace selection screen
pub struct NamespaceSelectScreen;

impl NamespaceSelectScreen {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = frame.area();
        let (header_area, content_area, status_area) = Layout::main(area);

        // Render header
        Self::render_header(frame, header_area, state);

        // Render namespace list
        Self::render_list(frame, content_area, state);

        // Render status bar
        Self::render_status_bar(frame, status_area, state);
    }

    fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
        let context_name = state
            .selected_context
            .as_deref()
            .unwrap_or("unknown");

        let title = Line::from(vec![
            Span::styled("kubescope", Theme::title()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(context_name, Theme::text_highlight()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled("Select Namespace", Theme::text()),
        ]);

        let header = Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border()),
        );

        frame.render_widget(header, area);
    }

    fn render_list(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let list_area = Layout::centered_list(area, 80);

        let items: Vec<(String, bool)> = state
            .namespaces
            .iter()
            .map(|ns| {
                let display = format!("{} ({})", ns.name, ns.status);
                (display, false)
            })
            .collect();

        let selector = ListSelector::new(" Namespaces ").items(items);

        frame.render_list_selector(list_area, selector, &mut state.ui_state.list_state);
    }

    fn render_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let ns_count = format!("{} namespaces", state.namespaces.len());

        let status = StatusBar::new().hints(list_nav_hints()).right(ns_count);

        frame.render_widget(status, area);
    }
}
