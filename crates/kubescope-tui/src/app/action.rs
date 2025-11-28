use crate::app::Screen;

/// All possible actions in the application (command pattern)
#[derive(Clone, Debug)]
pub enum Action {
    // Navigation
    Navigate(Screen),
    GoBack,
    Quit,

    // Selection
    SelectContext(String),
    SelectNamespace(String),
    SelectDeployment(String),

    // UI toggles
    ToggleCommandPalette,
    ToggleHelp,

    // Command palette
    PaletteUp,
    PaletteDown,
    PaletteSelect,
    PaletteInput(char),
    PaletteBackspace,
    PaletteClose,

    // List navigation
    ListUp,
    ListDown,
    ListSelect,

    // Search/Filter in lists
    OpenSearch,
    CloseSearch,
    SearchInput(char),
    SearchBackspace,
    SearchClear,

    // Filter in log viewer
    ApplyFilter,
    ClearFilter,
    ToggleCaseSensitive,

    // Refresh
    RefreshContexts,
    RefreshNamespaces,
    RefreshDeployments,

    // Log viewer actions
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToTop,
    ScrollToBottom,
    PageUp,
    PageDown,
    ToggleAutoScroll,
    ToggleTimestamps,
    ToggleLocalTime,
    TogglePodNames,
    ToggleJsonPrettyPrint,
    ToggleStats,
    ToggleJsonKeyFilter,
    JsonKeyUp,
    JsonKeyDown,
    JsonKeyToggle,
    JsonKeySelectAll,
    JsonKeyClearAll,
    JsonKeyInput(char),
    JsonKeyBackspace,
    JsonKeyClearSearch,
    JsonKeySelectPattern,
    ClearLogs,
    ExportLogs,

    // Time range
    CycleTimeRange,
    CycleTimeRangeBack,

    // Error handling
    ShowError(String),
    DismissError,

    // Tick (for periodic updates)
    Tick,

    // Render request
    Render,
}
