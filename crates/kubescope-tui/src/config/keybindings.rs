use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

use crate::app::Action;

/// A key combination
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    pub fn new(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }

    pub fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::CONTROL,
        }
    }

    #[allow(dead_code)]
    pub fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::SHIFT,
        }
    }

    pub fn from_event(event: &KeyEvent) -> Self {
        Self {
            code: event.code,
            modifiers: event.modifiers,
        }
    }
}

/// Context for keybindings
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyContext {
    Global,
    ListNavigation,
    LogViewer,
    FilterInput,
    CommandPalette,
    JsonKeyFilter,
}

/// Keybinding configuration
pub struct KeyBindings {
    bindings: HashMap<KeyContext, HashMap<KeyBinding, Action>>,
}

impl KeyBindings {
    pub fn new() -> Self {
        let mut bindings = HashMap::new();

        // Global bindings
        let mut global = HashMap::new();
        global.insert(
            KeyBinding::new(KeyCode::Char(' ')),
            Action::ToggleCommandPalette,
        );
        global.insert(KeyBinding::new(KeyCode::Char('?')), Action::ToggleHelp);
        global.insert(KeyBinding::new(KeyCode::Esc), Action::GoBack);
        global.insert(KeyBinding::ctrl(KeyCode::Char('c')), Action::Quit);
        global.insert(KeyBinding::new(KeyCode::Char('q')), Action::Quit);
        bindings.insert(KeyContext::Global, global);

        // List navigation bindings
        let mut list_nav = HashMap::new();
        list_nav.insert(KeyBinding::new(KeyCode::Char('j')), Action::ListDown);
        list_nav.insert(KeyBinding::new(KeyCode::Down), Action::ListDown);
        list_nav.insert(KeyBinding::new(KeyCode::Char('k')), Action::ListUp);
        list_nav.insert(KeyBinding::new(KeyCode::Up), Action::ListUp);
        list_nav.insert(KeyBinding::new(KeyCode::Enter), Action::ListSelect);
        list_nav.insert(KeyBinding::new(KeyCode::Char('/')), Action::OpenSearch);
        bindings.insert(KeyContext::ListNavigation, list_nav);

        // Log viewer bindings - less-like navigation
        let mut log_viewer = HashMap::new();
        // Line navigation
        log_viewer.insert(KeyBinding::new(KeyCode::Char('j')), Action::ScrollDown(1));
        log_viewer.insert(KeyBinding::new(KeyCode::Down), Action::ScrollDown(1));
        log_viewer.insert(KeyBinding::new(KeyCode::Enter), Action::ScrollDown(1));
        log_viewer.insert(KeyBinding::new(KeyCode::Char('k')), Action::ScrollUp(1));
        log_viewer.insert(KeyBinding::new(KeyCode::Up), Action::ScrollUp(1));
        // Page navigation (less-style)
        log_viewer.insert(KeyBinding::ctrl(KeyCode::Char('f')), Action::PageDown);
        log_viewer.insert(KeyBinding::ctrl(KeyCode::Char('b')), Action::PageUp);
        log_viewer.insert(KeyBinding::ctrl(KeyCode::Char('d')), Action::PageDown);
        log_viewer.insert(KeyBinding::ctrl(KeyCode::Char('u')), Action::PageUp);
        log_viewer.insert(KeyBinding::new(KeyCode::PageDown), Action::PageDown);
        log_viewer.insert(KeyBinding::new(KeyCode::PageUp), Action::PageUp);
        // Top/bottom navigation (less-style)
        log_viewer.insert(KeyBinding::new(KeyCode::Char('g')), Action::ScrollToTop);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('G')), Action::ScrollToBottom);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('<')), Action::ScrollToTop);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('>')), Action::ScrollToBottom);
        log_viewer.insert(KeyBinding::new(KeyCode::Home), Action::ScrollToTop);
        log_viewer.insert(KeyBinding::new(KeyCode::End), Action::ScrollToBottom);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('f')), Action::ToggleAutoScroll);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('t')), Action::ToggleTimestamps);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('T')), Action::ToggleLocalTime);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('p')), Action::TogglePodNames);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('J')), Action::ToggleJsonPrettyPrint);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('c')), Action::ClearLogs);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('/')), Action::OpenSearch);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('n')), Action::ClearFilter);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('i')), Action::ToggleCaseSensitive);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('s')), Action::ToggleStats);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('e')), Action::ExportLogs);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('K')), Action::ToggleJsonKeyFilter);
        log_viewer.insert(KeyBinding::new(KeyCode::Char('r')), Action::CycleTimeRange);
        log_viewer.insert(KeyBinding::shift(KeyCode::Char('R')), Action::CycleTimeRangeBack);
        bindings.insert(KeyContext::LogViewer, log_viewer);

        // JSON key filter bindings
        let mut json_keys = HashMap::new();
        json_keys.insert(KeyBinding::new(KeyCode::Up), Action::JsonKeyUp);
        json_keys.insert(KeyBinding::new(KeyCode::Down), Action::JsonKeyDown);
        json_keys.insert(KeyBinding::ctrl(KeyCode::Char('p')), Action::JsonKeyUp);
        json_keys.insert(KeyBinding::ctrl(KeyCode::Char('n')), Action::JsonKeyDown);
        json_keys.insert(KeyBinding::new(KeyCode::Tab), Action::JsonKeyToggle);
        json_keys.insert(KeyBinding::new(KeyCode::Enter), Action::JsonKeySelectPattern);
        json_keys.insert(KeyBinding::ctrl(KeyCode::Char('a')), Action::JsonKeySelectAll);
        json_keys.insert(KeyBinding::ctrl(KeyCode::Char('x')), Action::JsonKeyClearAll);
        json_keys.insert(KeyBinding::new(KeyCode::Esc), Action::ToggleJsonKeyFilter);
        json_keys.insert(KeyBinding::shift(KeyCode::Char('K')), Action::ToggleJsonKeyFilter);
        json_keys.insert(KeyBinding::new(KeyCode::Backspace), Action::JsonKeyBackspace);
        json_keys.insert(KeyBinding::ctrl(KeyCode::Char('u')), Action::JsonKeyClearSearch);
        bindings.insert(KeyContext::JsonKeyFilter, json_keys);

        // Filter input bindings (when search bar is active)
        let mut filter_input = HashMap::new();
        filter_input.insert(KeyBinding::new(KeyCode::Enter), Action::ApplyFilter);
        filter_input.insert(KeyBinding::new(KeyCode::Esc), Action::CloseSearch);
        filter_input.insert(KeyBinding::new(KeyCode::Backspace), Action::SearchBackspace);
        filter_input.insert(KeyBinding::ctrl(KeyCode::Char('u')), Action::SearchClear);
        filter_input.insert(KeyBinding::ctrl(KeyCode::Char('c')), Action::CloseSearch);
        bindings.insert(KeyContext::FilterInput, filter_input);

        // Command palette bindings
        let mut palette = HashMap::new();
        palette.insert(KeyBinding::new(KeyCode::Up), Action::PaletteUp);
        palette.insert(KeyBinding::new(KeyCode::Down), Action::PaletteDown);
        palette.insert(KeyBinding::new(KeyCode::Char('k')), Action::PaletteUp);
        palette.insert(KeyBinding::new(KeyCode::Char('j')), Action::PaletteDown);
        palette.insert(KeyBinding::ctrl(KeyCode::Char('p')), Action::PaletteUp);
        palette.insert(KeyBinding::ctrl(KeyCode::Char('n')), Action::PaletteDown);
        palette.insert(KeyBinding::new(KeyCode::Enter), Action::PaletteSelect);
        palette.insert(KeyBinding::new(KeyCode::Esc), Action::PaletteClose);
        palette.insert(KeyBinding::new(KeyCode::Backspace), Action::PaletteBackspace);
        palette.insert(KeyBinding::ctrl(KeyCode::Char('c')), Action::PaletteClose);
        bindings.insert(KeyContext::CommandPalette, palette);

        Self { bindings }
    }

    /// Look up action for key event in given context
    pub fn get_action(&self, context: KeyContext, key: &KeyEvent) -> Option<Action> {
        let binding = KeyBinding::from_event(key);

        // First check context-specific bindings
        if let Some(context_bindings) = self.bindings.get(&context) {
            if let Some(action) = context_bindings.get(&binding) {
                return Some(action.clone());
            }
        }

        // Fall back to global bindings
        self.bindings
            .get(&KeyContext::Global)?
            .get(&binding)
            .cloned()
    }

    /// Handle key event in filter input mode
    /// Returns Some(Action) for special keys, None for regular character input
    pub fn get_filter_input_action(&self, key: &KeyEvent) -> Option<Action> {
        let binding = KeyBinding::from_event(key);

        // Check filter input bindings first
        if let Some(filter_bindings) = self.bindings.get(&KeyContext::FilterInput) {
            if let Some(action) = filter_bindings.get(&binding) {
                return Some(action.clone());
            }
        }

        // For regular characters, return SearchInput action
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                return Some(Action::SearchInput(c));
            }
        }

        None
    }

    /// Handle key event in command palette mode
    pub fn get_palette_action(&self, key: &KeyEvent) -> Option<Action> {
        let binding = KeyBinding::from_event(key);

        // Check palette bindings first
        if let Some(palette_bindings) = self.bindings.get(&KeyContext::CommandPalette) {
            if let Some(action) = palette_bindings.get(&binding) {
                return Some(action.clone());
            }
        }

        // For regular characters, return PaletteInput action
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                return Some(Action::PaletteInput(c));
            }
        }

        None
    }

    /// Handle key event in JSON key filter mode
    pub fn get_json_key_filter_action(&self, key: &KeyEvent) -> Option<Action> {
        let binding = KeyBinding::from_event(key);

        // Check JSON key filter bindings first
        if let Some(json_bindings) = self.bindings.get(&KeyContext::JsonKeyFilter) {
            if let Some(action) = json_bindings.get(&binding) {
                return Some(action.clone());
            }
        }

        // For regular characters, return JsonKeyInput action for search
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                return Some(Action::JsonKeyInput(c));
            }
        }

        None
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new()
    }
}
