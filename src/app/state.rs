use ratatui::widgets::ListState;
use std::collections::HashSet;
use tokio::sync::mpsc;

use super::Action;
use crate::logs::CompiledFilter;
use crate::types::{ArcLogEntry, ContextInfo, DeploymentInfo, NamespaceInfo, PodInfo, TimeRange};

/// Cache for filtered log results to avoid re-filtering on every render
#[derive(Default)]
pub struct FilterCache {
    /// Cached filter pattern (None = no text filter)
    cached_filter_pattern: Option<String>,
    /// Cached case sensitivity setting
    cached_case_insensitive: bool,
    /// Cached JSON visible keys
    cached_json_keys: HashSet<String>,
    /// Buffer entry count when cache was built
    cached_log_count: usize,
    /// The cached filtered entries
    pub cached_entries: Vec<ArcLogEntry>,
    /// Whether cache is valid
    pub is_valid: bool,
}

impl FilterCache {
    /// Check if cache needs to be invalidated based on current state
    pub fn needs_refresh(
        &self,
        filter: Option<&CompiledFilter>,
        case_insensitive: bool,
        json_keys: &HashSet<String>,
        current_log_count: usize,
    ) -> bool {
        if !self.is_valid {
            return true;
        }

        // Check if log count changed (new logs arrived)
        if self.cached_log_count != current_log_count {
            return true;
        }

        // Check if filter changed
        let current_pattern = filter.map(|f| f.pattern().to_string());
        if self.cached_filter_pattern != current_pattern {
            return true;
        }

        // Check if case sensitivity changed
        if self.cached_case_insensitive != case_insensitive {
            return true;
        }

        // Check if JSON keys changed
        if self.cached_json_keys != *json_keys {
            return true;
        }

        false
    }

    /// Update the cache with new filtered results
    pub fn update(
        &mut self,
        filter: Option<&CompiledFilter>,
        case_insensitive: bool,
        json_keys: &HashSet<String>,
        log_count: usize,
        entries: Vec<ArcLogEntry>,
    ) {
        self.cached_filter_pattern = filter.map(|f| f.pattern().to_string());
        self.cached_case_insensitive = case_insensitive;
        self.cached_json_keys = json_keys.clone();
        self.cached_log_count = log_count;
        self.cached_entries = entries;
        self.is_valid = true;
    }
}

/// Screen enumeration
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    ContextSelect,
    NamespaceSelect,
    DeploymentSelect,
    LogViewer,
}

/// UI-specific transient state
#[allow(dead_code)]
pub struct UiState {
    /// Is command palette open?
    pub command_palette_open: bool,

    /// Is search/filter bar active?
    pub search_active: bool,

    /// Current search input text
    pub search_input: String,

    /// Is help overlay visible?
    pub help_visible: bool,

    /// List state for selection screens
    pub list_state: ListState,

    /// Error message to display (if any)
    pub error_message: Option<String>,

    // Log viewer specific state
    /// Scroll position in log viewer
    pub log_scroll: usize,

    /// Auto-scroll enabled (follow mode)?
    pub auto_scroll: bool,

    /// Show timestamps in log viewer?
    pub show_timestamps: bool,

    /// Show pod names in log viewer?
    pub show_pod_names: bool,

    /// JSON pretty-print enabled?
    pub json_pretty_print: bool,

    /// Currently active filter (None = show all)
    pub active_filter: Option<CompiledFilter>,

    /// Filter input error message (e.g., invalid regex)
    pub filter_error: Option<String>,

    /// Case insensitive search?
    pub filter_case_insensitive: bool,

    /// Show statistics panel?
    pub stats_visible: bool,

    /// JSON key filter mode active?
    pub json_key_filter_active: bool,

    /// Selected JSON keys to display (empty = show all)
    pub json_visible_keys: std::collections::HashSet<String>,

    /// All discovered JSON keys from logs
    pub json_available_keys: Vec<String>,

    /// Current selection in key filter (index into filtered list)
    pub json_key_selection: usize,

    /// Search input for filtering keys
    pub json_key_search: String,

    /// Scroll offset for key list viewport
    pub json_key_scroll: usize,

    /// Selected time range for log filtering
    pub time_range: TimeRange,

    /// Show timestamps in local time (vs UTC)
    pub use_local_time: bool,

    /// Cache for filtered log results
    pub filter_cache: FilterCache,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            command_palette_open: false,
            search_active: false,
            search_input: String::new(),
            help_visible: false,
            list_state: ListState::default(),
            error_message: None,
            // Log viewer defaults
            log_scroll: 0,
            auto_scroll: true,
            show_timestamps: true,
            show_pod_names: true,
            json_pretty_print: false,
            // Filter defaults
            active_filter: None,
            filter_error: None,
            filter_case_insensitive: true,
            // Stats panel
            stats_visible: false,
            // JSON key filter
            json_key_filter_active: false,
            json_visible_keys: std::collections::HashSet::new(),
            json_available_keys: Vec::new(),
            json_key_selection: 0,
            json_key_search: String::new(),
            json_key_scroll: 0,
            // Time range
            time_range: TimeRange::default(),
            // Local time display (default to local time for better UX)
            use_local_time: true,
            // Filter cache
            filter_cache: FilterCache::default(),
        }
    }
}

/// Global application state
pub struct AppState {
    /// Current screen being displayed
    pub current_screen: Screen,

    /// Navigation stack for back navigation
    pub screen_stack: Vec<Screen>,

    /// Available Kubernetes contexts
    pub contexts: Vec<ContextInfo>,

    /// Selected Kubernetes context
    pub selected_context: Option<String>,

    /// Available namespaces
    pub namespaces: Vec<NamespaceInfo>,

    /// Selected namespace
    pub selected_namespace: Option<String>,

    /// Available deployments
    pub deployments: Vec<DeploymentInfo>,

    /// Selected deployment
    pub selected_deployment: Option<String>,

    /// Pods belonging to the selected deployment
    pub pods: Vec<PodInfo>,

    /// UI state
    pub ui_state: UiState,

    /// Whether app should quit
    pub should_quit: bool,

    /// Channel sender for async actions
    pub action_tx: mpsc::UnboundedSender<Action>,

    /// Dirty flag for rendering - only render when true
    pub render_dirty: bool,

    /// Last known log count for change detection
    pub last_log_count: usize,
}

impl AppState {
    pub fn new(action_tx: mpsc::UnboundedSender<Action>) -> Self {
        let mut ui_state = UiState::default();
        ui_state.list_state.select(Some(0));

        Self {
            current_screen: Screen::ContextSelect,
            screen_stack: Vec::new(),
            contexts: Vec::new(),
            selected_context: None,
            namespaces: Vec::new(),
            selected_namespace: None,
            deployments: Vec::new(),
            selected_deployment: None,
            pods: Vec::new(),
            ui_state,
            should_quit: false,
            action_tx,
            render_dirty: true, // Start dirty to ensure initial render
            last_log_count: 0,
        }
    }

    /// Navigate to a new screen, pushing current to stack
    pub fn navigate_to(&mut self, screen: Screen) {
        self.screen_stack.push(self.current_screen.clone());
        self.current_screen = screen;
        self.ui_state.list_state.select(Some(0));
    }

    /// Go back to previous screen
    pub fn go_back(&mut self) -> bool {
        if let Some(prev_screen) = self.screen_stack.pop() {
            self.current_screen = prev_screen;
            self.ui_state.list_state.select(Some(0));
            true
        } else {
            false
        }
    }

    /// Get the current list length based on screen
    pub fn current_list_len(&self) -> usize {
        match self.current_screen {
            Screen::ContextSelect => self.contexts.len(),
            Screen::NamespaceSelect => self.namespaces.len(),
            Screen::DeploymentSelect => self.deployments.len(),
            Screen::LogViewer => 0,
        }
    }

    /// Move selection up
    pub fn list_up(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }

        let i = match self.ui_state.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.ui_state.list_state.select(Some(i));
    }

    /// Move selection down
    pub fn list_down(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }

        let i = match self.ui_state.list_state.selected() {
            Some(i) => {
                if i >= len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.ui_state.list_state.select(Some(i));
    }

    /// Get currently selected index
    pub fn selected_index(&self) -> Option<usize> {
        self.ui_state.list_state.selected()
    }

    /// Show an error message
    pub fn show_error(&mut self, msg: String) {
        self.ui_state.error_message = Some(msg);
    }

    /// Dismiss the error message
    pub fn dismiss_error(&mut self) {
        self.ui_state.error_message = None;
    }

    /// Start search/filter input mode
    pub fn start_search(&mut self) {
        self.ui_state.search_active = true;
        self.ui_state.search_input.clear();
        self.ui_state.filter_error = None;
    }

    /// Cancel search/filter input and clear filter
    pub fn cancel_search(&mut self) {
        self.ui_state.search_active = false;
        self.ui_state.search_input.clear();
        self.ui_state.active_filter = None;
        self.ui_state.filter_error = None;
    }

    /// Apply the current search input as a filter
    pub fn apply_filter(&mut self) {
        self.ui_state.search_active = false;
        self.ui_state.filter_error = None;

        if self.ui_state.search_input.is_empty() {
            self.ui_state.active_filter = None;
            return;
        }

        let result = if self.ui_state.filter_case_insensitive {
            CompiledFilter::new_case_insensitive(&self.ui_state.search_input)
        } else {
            CompiledFilter::new(&self.ui_state.search_input)
        };

        match result {
            Ok(filter) => {
                self.ui_state.active_filter = Some(filter);
            }
            Err(e) => {
                self.ui_state.filter_error = Some(format!("Invalid regex: {}", e));
                self.ui_state.search_active = true; // Keep input open to fix
            }
        }
    }

    /// Clear the active filter
    pub fn clear_filter(&mut self) {
        self.ui_state.active_filter = None;
        self.ui_state.search_input.clear();
        self.ui_state.filter_error = None;
    }

    /// Add a character to search input
    pub fn search_input_char(&mut self, c: char) {
        self.ui_state.search_input.push(c);
    }

    /// Remove last character from search input
    pub fn search_input_backspace(&mut self) {
        self.ui_state.search_input.pop();
    }
}
