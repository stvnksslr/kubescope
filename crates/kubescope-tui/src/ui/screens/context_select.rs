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

/// Context selection screen
pub struct ContextSelectScreen;

impl ContextSelectScreen {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = frame.area();
        let (header_area, content_area, status_area) = Layout::main(area);

        // Render header
        Self::render_header(frame, header_area);

        // Render context list
        Self::render_list(frame, content_area, state);

        // Render status bar
        Self::render_status_bar(frame, status_area, state);
    }

    fn render_header(frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            Span::styled("kubescope", Theme::title()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled("Select Context", Theme::text()),
        ]);

        let header = Paragraph::new(title)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Theme::border()),
            );

        frame.render_widget(header, area);
    }

    fn render_list(frame: &mut Frame, area: Rect, state: &mut AppState) {
        let list_area = Layout::centered_list(area, 80);

        let items: Vec<(String, bool)> = state
            .contexts
            .iter()
            .map(|ctx| {
                let display = if let Some(ns) = &ctx.namespace {
                    format!("{} (namespace: {})", ctx.name, ns)
                } else {
                    ctx.name.clone()
                };
                (display, ctx.is_current)
            })
            .collect();

        let selector = ListSelector::new(" Kubernetes Contexts ").items(items);

        frame.render_list_selector(list_area, selector, &mut state.ui_state.list_state);
    }

    fn render_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let context_count = format!("{} contexts", state.contexts.len());

        let status = StatusBar::new()
            .hints(list_nav_hints())
            .right(context_count);

        frame.render_widget(status, area);
    }
}
