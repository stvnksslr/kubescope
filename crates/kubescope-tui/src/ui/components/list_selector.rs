use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget},
};

use crate::ui::Theme;

/// A generic list selector component
pub struct ListSelector<'a> {
    items: Vec<ListItem<'a>>,
    title: &'a str,
    highlight_symbol: &'a str,
}

impl<'a> ListSelector<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            items: Vec::new(),
            title,
            highlight_symbol: "▶ ",
        }
    }

    /// Add items from an iterator of (display_text, is_current) tuples
    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = (S, bool)>,
        S: Into<String>,
    {
        self.items = items
            .into_iter()
            .map(|(text, is_current)| {
                let text = text.into();
                let style = if is_current {
                    Theme::list_item_current()
                } else {
                    Theme::list_item()
                };

                let content = if is_current {
                    let display = format!("{} (current)", text);
                    Line::from(Span::styled(display, style))
                } else {
                    Line::from(Span::styled(text, style))
                };

                ListItem::new(content)
            })
            .collect();
        self
    }

    /// Set the highlight symbol
    #[allow(dead_code)]
    pub fn highlight_symbol(mut self, symbol: &'a str) -> Self {
        self.highlight_symbol = symbol;
        self
    }
}

impl StatefulWidget for ListSelector<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border_focused())
            .title(Span::styled(self.title, Theme::title()));

        let list = List::new(self.items)
            .block(block)
            .highlight_style(Theme::list_item_selected())
            .highlight_symbol(self.highlight_symbol);

        StatefulWidget::render(list, area, buf, state);
    }
}

/// Extension trait to render ListSelector more easily
pub trait ListSelectorExt {
    fn render_list_selector(&mut self, area: Rect, selector: ListSelector, state: &mut ListState);
}

impl ListSelectorExt for ratatui::Frame<'_> {
    fn render_list_selector(&mut self, area: Rect, selector: ListSelector, state: &mut ListState) {
        self.render_stateful_widget(selector, area, state);
    }
}
