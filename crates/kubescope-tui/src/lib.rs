//! TUI components for kubescope
//!
//! This crate provides the terminal user interface for kubescope,
//! including state management, keybindings, event handling, and UI components.

pub mod app;
pub mod config;
pub mod tui;
pub mod ui;

pub use app::{Action, AppState, Screen, UiState};
pub use config::{KeyBinding, KeyBindings, KeyContext};
pub use tui::{Event, EventHandler, Tui};
pub use ui::components::{
    Command, CommandPalette, CommandPaletteState, HelpOverlay, JsonKeyFilter, ListSelector,
    ListSelectorExt, StatusBar, collect_json_keys, list_nav_hints, log_viewer_commands,
};
pub use ui::screens::{
    ContextSelectScreen, DeploymentSelectScreen, LogViewerScreen, NamespaceSelectScreen,
};
pub use ui::{Layout, Theme};
