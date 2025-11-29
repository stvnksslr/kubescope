use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::AppState,
    ui::{
        Layout, Theme,
        components::{ListSelector, ListSelectorExt, StatusBar, list_nav_hints},
    },
};

/// Deployment selection screen
pub struct DeploymentSelectScreen;

impl DeploymentSelectScreen {
    pub fn render(frame: &mut Frame, state: &mut AppState) {
        let area = frame.area();
        let (header_area, content_area, status_area) = Layout::main(area);

        // Render header
        Self::render_header(frame, header_area, state);

        // Render deployment list
        Self::render_list(frame, content_area, state);

        // Render status bar
        Self::render_status_bar(frame, status_area, state);
    }

    fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
        let context_name = state.selected_context.as_deref().unwrap_or("unknown");
        let namespace = state.selected_namespace.as_deref().unwrap_or("unknown");

        let title = Line::from(vec![
            Span::styled("kubescope", Theme::title()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(context_name, Theme::text()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled(namespace, Theme::text_highlight()),
            Span::styled(" │ ", Theme::text_dim()),
            Span::styled("Select Deployment", Theme::text()),
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
            .deployments
            .iter()
            .map(|deploy| {
                let display = format!(
                    "{} ({}/{})",
                    deploy.name, deploy.ready_replicas, deploy.replicas
                );
                // Highlight if all replicas are ready
                let is_healthy = deploy.ready_replicas == deploy.replicas && deploy.replicas > 0;
                (display, is_healthy)
            })
            .collect();

        let selector = ListSelector::new(" Deployments ").items(items);

        frame.render_list_selector(list_area, selector, &mut state.ui_state.list_state);
    }

    fn render_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
        let deploy_count = format!("{} deployments", state.deployments.len());

        let status = StatusBar::new().hints(list_nav_hints()).right(deploy_count);

        frame.render_widget(status, area);
    }
}
