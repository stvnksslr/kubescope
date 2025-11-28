use std::fs::File;
use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;

use kubescope_k8s::{DeploymentInfo, KubeClient, NamespaceInfo, PodInfo};
use kubescope_logs::{LogBuffer, LogEntry, LogStreamManager};
use kubescope_tui::{
    collect_json_keys, log_viewer_commands, Action, AppState, Command, CommandPalette,
    CommandPaletteState, ContextSelectScreen, DeploymentSelectScreen, Event, EventHandler,
    HelpOverlay, JsonKeyFilter, KeyBindings, KeyContext, LogViewerScreen, NamespaceSelectScreen,
    Screen, Tui,
};

/// Kubescope - A terminal UI for viewing Kubernetes deployment logs
#[derive(Parser, Debug)]
#[command(name = "kubescope")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Kubernetes context name (optional, will prompt if not provided)
    #[arg(value_name = "CONTEXT")]
    context: Option<String>,

    /// Namespace (optional, requires context)
    #[arg(value_name = "NAMESPACE")]
    namespace: Option<String>,

    /// Deployment name (optional, requires context and namespace)
    #[arg(value_name = "DEPLOYMENT")]
    deployment: Option<String>,

    /// Buffer size for log entries
    #[arg(long, default_value = "10000")]
    buffer_size: usize,

    /// Number of historical log lines to fetch per pod
    #[arg(long, default_value = "100")]
    tail_lines: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing for debugging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Run the application
    let result = run_app(args).await;

    // Handle any errors
    if let Err(e) = &result {
        eprintln!("Error: {:#}", e);
    }

    result
}

/// Internal actions for async operations
enum InternalAction {
    LoadNamespaces(String),
    LoadDeployments(String),
    LoadPods(String, DeploymentInfo),
    NamespacesLoaded(Vec<NamespaceInfo>),
    DeploymentsLoaded(Vec<DeploymentInfo>),
    PodsLoaded(Vec<PodInfo>),
    StartLogStreaming,
    StopLogStreaming,
    RestartLogStreaming,
    Error(String),
}

async fn run_app(args: Args) -> Result<()> {
    // Create action channels
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let (internal_tx, mut internal_rx) = mpsc::unbounded_channel::<InternalAction>();
    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<LogEntry>();

    // Initialize state
    let mut state = AppState::new(action_tx.clone());

    // Load kubeconfig and contexts
    let kube_client = KubeClient::new().await?;
    state.contexts = kube_client.get_contexts();

    // Track the active K8s client for the selected context
    let mut active_client: Option<kube::Client> = None;

    // Log buffer and stream manager
    let log_buffer = LogBuffer::new(args.buffer_size);
    let mut stream_manager = LogStreamManager::new();

    // Initialize TUI
    let mut tui = Tui::new()?;

    // Initialize event handler
    let mut events = EventHandler::new(Duration::from_millis(100));

    // Initialize keybindings
    let keybindings = KeyBindings::new();

    // Command palette
    let mut palette_state = CommandPaletteState::default();
    let commands = log_viewer_commands();

    // Handle CLI arguments for direct navigation
    if let Some(context_name) = &args.context {
        // Validate context exists
        if !state.contexts.iter().any(|c| &c.name == context_name) {
            anyhow::bail!("Context '{}' not found in kubeconfig", context_name);
        }

        // Connect to context and load namespaces
        let client = kube_client.client_for_context(context_name).await?;
        let namespaces = kube_client.get_namespaces(&client).await?;

        state.selected_context = Some(context_name.clone());
        state.namespaces = namespaces;
        active_client = Some(client.clone());

        if let Some(namespace_name) = &args.namespace {
            // Validate namespace exists
            if !state.namespaces.iter().any(|n| &n.name == namespace_name) {
                anyhow::bail!(
                    "Namespace '{}' not found in context '{}'",
                    namespace_name,
                    context_name
                );
            }

            // Load deployments
            let deployments = kube_client.get_deployments(&client, namespace_name).await?;
            state.selected_namespace = Some(namespace_name.clone());
            state.deployments = deployments;
            state.screen_stack.push(Screen::ContextSelect);

            if let Some(deployment_name) = &args.deployment {
                // Validate deployment exists
                let deployment = state
                    .deployments
                    .iter()
                    .find(|d| &d.name == deployment_name)
                    .cloned();

                if let Some(deployment) = deployment {
                    // Load pods and go directly to log viewer
                    let pods = kube_client
                        .get_pods_for_deployment(&client, namespace_name, &deployment)
                        .await?;

                    state.selected_deployment = Some(deployment_name.clone());
                    state.pods = pods;
                    state.screen_stack.push(Screen::NamespaceSelect);
                    state.screen_stack.push(Screen::DeploymentSelect);
                    state.current_screen = Screen::LogViewer;

                    // Start log streaming
                    log_buffer.clear();
                    let since_seconds = state.ui_state.time_range.as_seconds();
                    stream_manager.start_streams(
                        client,
                        namespace_name,
                        &state.pods,
                        log_tx.clone(),
                        Some(args.tail_lines),
                        since_seconds,
                    );
                } else {
                    anyhow::bail!(
                        "Deployment '{}' not found in namespace '{}'",
                        deployment_name,
                        namespace_name
                    );
                }
            } else {
                // Go to deployment select
                state.screen_stack.push(Screen::NamespaceSelect);
                state.current_screen = Screen::DeploymentSelect;
            }
        } else {
            // Go to namespace select
            state.screen_stack.push(Screen::ContextSelect);
            state.current_screen = Screen::NamespaceSelect;
        }
    }

    // Initial render
    render(&mut tui, &mut state, &log_buffer, &mut palette_state, &commands)?;

    // Main event loop
    loop {
        tokio::select! {
            // Handle terminal events
            Some(event) = events.next() => {
                match event {
                    Event::Key(key) => {
                        // Check if command palette is open
                        if palette_state.visible {
                            if let Some(action) = keybindings.get_palette_action(&key) {
                                let _ = action_tx.send(action);
                            }
                        // Check if JSON key filter is open
                        } else if state.ui_state.json_key_filter_active && state.current_screen == Screen::LogViewer {
                            if let Some(action) = keybindings.get_json_key_filter_action(&key) {
                                let _ = action_tx.send(action);
                            }
                        // Check if we're in filter input mode
                        } else if state.ui_state.search_active && state.current_screen == Screen::LogViewer {
                            if let Some(action) = keybindings.get_filter_input_action(&key) {
                                let _ = action_tx.send(action);
                            }
                        } else {
                            let context = match state.current_screen {
                                Screen::ContextSelect |
                                Screen::NamespaceSelect |
                                Screen::DeploymentSelect => KeyContext::ListNavigation,
                                Screen::LogViewer => KeyContext::LogViewer,
                            };

                            if let Some(action) = keybindings.get_action(context, &key) {
                                let _ = action_tx.send(action);
                            }
                        }
                    }
                    Event::Tick => {
                        // Re-render on tick to show new logs
                        if state.current_screen == Screen::LogViewer {
                            // Just trigger a render
                        }
                    }
                    Event::Resize(_, _) => {
                        let _ = action_tx.send(Action::Render);
                    }
                    Event::Error(e) => {
                        state.show_error(e);
                    }
                }
            }

            // Handle incoming log entries
            Some(entry) = log_rx.recv() => {
                log_buffer.push(entry);
            }

            // Handle user actions
            Some(action) = action_rx.recv() => {
                handle_action(&mut state, &internal_tx, &log_buffer, &mut palette_state, &commands, action);
            }

            // Handle internal async actions
            Some(internal) = internal_rx.recv() => {
                match internal {
                    InternalAction::LoadNamespaces(context_name) => {
                        match kube_client.client_for_context(&context_name).await {
                            Ok(client) => {
                                match kube_client.get_namespaces(&client).await {
                                    Ok(namespaces) => {
                                        active_client = Some(client);
                                        let _ = internal_tx.send(InternalAction::NamespacesLoaded(namespaces));
                                    }
                                    Err(e) => {
                                        let _ = internal_tx.send(InternalAction::Error(
                                            format!("Failed to load namespaces: {}", e)
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = internal_tx.send(InternalAction::Error(
                                    format!("Failed to connect to cluster: {}", e)
                                ));
                            }
                        }
                    }

                    InternalAction::LoadDeployments(namespace) => {
                        if let Some(client) = &active_client {
                            match kube_client.get_deployments(client, &namespace).await {
                                Ok(deployments) => {
                                    let _ = internal_tx.send(InternalAction::DeploymentsLoaded(deployments));
                                }
                                Err(e) => {
                                    let _ = internal_tx.send(InternalAction::Error(
                                        format!("Failed to load deployments: {}", e)
                                    ));
                                }
                            }
                        }
                    }

                    InternalAction::LoadPods(namespace, deployment) => {
                        if let Some(client) = &active_client {
                            match kube_client.get_pods_for_deployment(client, &namespace, &deployment).await {
                                Ok(pods) => {
                                    let _ = internal_tx.send(InternalAction::PodsLoaded(pods));
                                }
                                Err(e) => {
                                    let _ = internal_tx.send(InternalAction::Error(
                                        format!("Failed to load pods: {}", e)
                                    ));
                                }
                            }
                        }
                    }

                    InternalAction::NamespacesLoaded(namespaces) => {
                        state.namespaces = namespaces;
                        state.navigate_to(Screen::NamespaceSelect);
                    }

                    InternalAction::DeploymentsLoaded(deployments) => {
                        state.deployments = deployments;
                        state.navigate_to(Screen::DeploymentSelect);
                    }

                    InternalAction::PodsLoaded(pods) => {
                        state.pods = pods;
                        state.navigate_to(Screen::LogViewer);
                        // Start log streaming
                        let _ = internal_tx.send(InternalAction::StartLogStreaming);
                    }

                    InternalAction::StartLogStreaming => {
                        if let Some(client) = &active_client {
                            if let Some(namespace) = &state.selected_namespace {
                                // Clear previous logs
                                log_buffer.clear();
                                // Reset scroll and enable auto-scroll
                                state.ui_state.log_scroll = 0;
                                state.ui_state.auto_scroll = true;
                                // Get time range
                                let since_seconds = state.ui_state.time_range.as_seconds();
                                // Start streaming
                                stream_manager.start_streams(
                                    client.clone(),
                                    namespace,
                                    &state.pods,
                                    log_tx.clone(),
                                    Some(args.tail_lines),
                                    since_seconds,
                                );
                            }
                        }
                    }

                    InternalAction::RestartLogStreaming => {
                        if let Some(client) = &active_client {
                            if let Some(namespace) = &state.selected_namespace {
                                // Stop current streams
                                stream_manager.stop();
                                // Clear logs for fresh start with new time range
                                log_buffer.clear();
                                state.ui_state.log_scroll = 0;
                                state.ui_state.auto_scroll = true;
                                // Get time range
                                let since_seconds = state.ui_state.time_range.as_seconds();
                                // Restart streaming with new time range
                                stream_manager.start_streams(
                                    client.clone(),
                                    namespace,
                                    &state.pods,
                                    log_tx.clone(),
                                    Some(args.tail_lines),
                                    since_seconds,
                                );
                            }
                        }
                    }

                    InternalAction::StopLogStreaming => {
                        stream_manager.stop();
                    }

                    InternalAction::Error(msg) => {
                        state.show_error(msg);
                    }
                }
            }
        }

        if state.should_quit {
            break;
        }

        render(&mut tui, &mut state, &log_buffer, &mut palette_state, &commands)?;
    }

    // Cleanup
    stream_manager.stop();
    events.shutdown();
    tui.restore()?;

    Ok(())
}

fn handle_action(
    state: &mut AppState,
    internal_tx: &mpsc::UnboundedSender<InternalAction>,
    log_buffer: &LogBuffer,
    palette_state: &mut CommandPaletteState,
    commands: &[Command],
    action: Action,
) {
    match action {
        Action::Quit => {
            let _ = internal_tx.send(InternalAction::StopLogStreaming);
            state.should_quit = true;
        }
        Action::GoBack => {
            // Stop streaming if leaving log viewer
            if state.current_screen == Screen::LogViewer {
                let _ = internal_tx.send(InternalAction::StopLogStreaming);
                // Clear all filter state
                state.ui_state.json_visible_keys.clear();
                state.ui_state.json_available_keys.clear();
                state.ui_state.json_key_filter_active = false;
                state.ui_state.json_key_search.clear();
                state.ui_state.active_filter = None;
                state.ui_state.search_input.clear();
                state.ui_state.filter_error = None;
            }
            if !state.go_back() {
                state.should_quit = true;
            }
        }
        Action::Navigate(screen) => {
            state.navigate_to(screen);
        }
        Action::ListUp => {
            state.list_up();
        }
        Action::ListDown => {
            state.list_down();
        }
        Action::ListSelect => {
            handle_list_select(state, internal_tx);
        }
        Action::SelectContext(name) => {
            state.selected_context = Some(name.clone());
            let _ = internal_tx.send(InternalAction::LoadNamespaces(name));
        }
        Action::SelectNamespace(name) => {
            state.selected_namespace = Some(name.clone());
            let _ = internal_tx.send(InternalAction::LoadDeployments(name));
        }
        Action::SelectDeployment(name) => {
            state.selected_deployment = Some(name.clone());
            // Clear all filter state for new deployment
            state.ui_state.json_visible_keys.clear();
            state.ui_state.json_available_keys.clear();
            state.ui_state.json_key_filter_active = false;
            state.ui_state.json_key_search.clear();
            state.ui_state.active_filter = None;
            state.ui_state.search_input.clear();
            state.ui_state.filter_error = None;
            if let Some(namespace) = &state.selected_namespace {
                if let Some(deployment) = state.deployments.iter().find(|d| d.name == name) {
                    let _ = internal_tx.send(InternalAction::LoadPods(
                        namespace.clone(),
                        deployment.clone(),
                    ));
                }
            }
        }

        // Log viewer actions
        Action::ScrollUp(n) => {
            state.ui_state.auto_scroll = false;
            state.ui_state.log_scroll = state.ui_state.log_scroll.saturating_sub(n);
        }
        Action::ScrollDown(n) => {
            state.ui_state.auto_scroll = false;
            // Don't cap here - let render_logs clamp to the actual filtered count
            state.ui_state.log_scroll = state.ui_state.log_scroll.saturating_add(n);
        }
        Action::PageUp => {
            state.ui_state.auto_scroll = false;
            state.ui_state.log_scroll = state.ui_state.log_scroll.saturating_sub(20);
        }
        Action::PageDown => {
            state.ui_state.auto_scroll = false;
            // Don't cap here - let render_logs clamp to the actual filtered count
            state.ui_state.log_scroll = state.ui_state.log_scroll.saturating_add(20);
        }
        Action::ScrollToTop => {
            state.ui_state.auto_scroll = false;
            state.ui_state.log_scroll = 0;
        }
        Action::ScrollToBottom => {
            state.ui_state.auto_scroll = false;
            // Set to max value - render_logs will clamp to actual bottom
            state.ui_state.log_scroll = usize::MAX;
        }
        Action::ToggleAutoScroll => {
            state.ui_state.auto_scroll = !state.ui_state.auto_scroll;
        }
        Action::ToggleTimestamps => {
            state.ui_state.show_timestamps = !state.ui_state.show_timestamps;
        }
        Action::ToggleLocalTime => {
            state.ui_state.use_local_time = !state.ui_state.use_local_time;
        }
        Action::TogglePodNames => {
            state.ui_state.show_pod_names = !state.ui_state.show_pod_names;
        }
        Action::ToggleJsonPrettyPrint => {
            state.ui_state.json_pretty_print = !state.ui_state.json_pretty_print;
        }
        Action::ToggleStats => {
            state.ui_state.stats_visible = !state.ui_state.stats_visible;
        }
        Action::ClearLogs => {
            log_buffer.clear();
            state.ui_state.log_scroll = 0;
        }
        Action::ExportLogs => {
            let deployment = state.selected_deployment.as_deref().unwrap_or("logs");
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let filename = format!("{}_{}.log", deployment, timestamp);

            match export_logs_to_file(&filename, log_buffer, state) {
                Ok(count) => {
                    state.show_error(format!("Exported {} logs to {}", count, filename));
                }
                Err(e) => {
                    state.show_error(format!("Export failed: {}", e));
                }
            }
        }

        Action::CycleTimeRange => {
            state.ui_state.time_range = state.ui_state.time_range.next();
            if state.current_screen == Screen::LogViewer {
                let _ = internal_tx.send(InternalAction::RestartLogStreaming);
            }
        }
        Action::CycleTimeRangeBack => {
            state.ui_state.time_range = state.ui_state.time_range.prev();
            if state.current_screen == Screen::LogViewer {
                let _ = internal_tx.send(InternalAction::RestartLogStreaming);
            }
        }

        Action::ShowError(msg) => {
            state.show_error(msg);
        }
        Action::DismissError => {
            state.dismiss_error();
        }
        Action::ToggleHelp => {
            state.ui_state.help_visible = !state.ui_state.help_visible;
        }
        Action::ToggleCommandPalette => {
            if palette_state.visible {
                palette_state.close();
            } else {
                palette_state.open(commands);
            }
        }

        // Command palette actions
        Action::PaletteUp => {
            palette_state.move_up();
        }
        Action::PaletteDown => {
            palette_state.move_down();
        }
        Action::PaletteInput(c) => {
            palette_state.input_char(c, commands);
        }
        Action::PaletteBackspace => {
            palette_state.input_backspace(commands);
        }
        Action::PaletteClose => {
            palette_state.close();
        }
        Action::PaletteSelect => {
            if let Some(cmd) = palette_state.selected_command(commands) {
                let action = cmd.action.clone();
                palette_state.close();
                // Recursively handle the selected action
                handle_action(state, internal_tx, log_buffer, palette_state, commands, action);
                return;
            }
        }

        // Filter/Search actions
        Action::OpenSearch => {
            state.start_search();
        }
        Action::CloseSearch => {
            state.cancel_search();
        }
        Action::SearchInput(c) => {
            state.search_input_char(c);
        }
        Action::SearchBackspace => {
            state.search_input_backspace();
        }
        Action::SearchClear => {
            state.ui_state.search_input.clear();
        }
        Action::ApplyFilter => {
            state.apply_filter();
            // Reset scroll to top when applying filter
            state.ui_state.log_scroll = 0;
        }
        Action::ClearFilter => {
            state.clear_filter();
        }
        Action::ToggleCaseSensitive => {
            state.ui_state.filter_case_insensitive = !state.ui_state.filter_case_insensitive;
            // Re-apply filter with new case sensitivity if active
            if state.ui_state.active_filter.is_some() {
                state.apply_filter();
            }
        }

        // JSON key filter actions
        Action::ToggleJsonKeyFilter => {
            if state.ui_state.json_key_filter_active {
                state.ui_state.json_key_filter_active = false;
                state.ui_state.json_key_search.clear();
            } else {
                // Collect available keys from logs
                state.ui_state.json_available_keys = collect_json_keys(log_buffer);
                state.ui_state.json_key_selection = 0;
                state.ui_state.json_key_scroll = 0;
                state.ui_state.json_key_search.clear();
                state.ui_state.json_key_filter_active = true;
            }
        }
        Action::JsonKeyUp => {
            if state.ui_state.json_key_selection > 0 {
                state.ui_state.json_key_selection -= 1;
                // Adjust scroll to keep selection visible
                if state.ui_state.json_key_selection < state.ui_state.json_key_scroll {
                    state.ui_state.json_key_scroll = state.ui_state.json_key_selection;
                }
            }
        }
        Action::JsonKeyDown => {
            let filtered = get_filtered_json_keys(state);
            let max = filtered.len().saturating_sub(1);
            if state.ui_state.json_key_selection < max {
                state.ui_state.json_key_selection += 1;
            }
        }
        Action::JsonKeyToggle => {
            let filtered = get_filtered_json_keys(state);
            if let Some(key) = filtered.get(state.ui_state.json_key_selection) {
                let key = key.clone();
                if state.ui_state.json_visible_keys.contains(&key) {
                    state.ui_state.json_visible_keys.remove(&key);
                } else {
                    state.ui_state.json_visible_keys.insert(key);
                }
            }
        }
        Action::JsonKeySelectAll => {
            // Select all visible (filtered) keys
            let filtered = get_filtered_json_keys(state);
            for key in filtered {
                state.ui_state.json_visible_keys.insert(key);
            }
        }
        Action::JsonKeyClearAll => {
            // Clear all selections (shows all when empty)
            state.ui_state.json_visible_keys.clear();
        }
        Action::JsonKeyInput(c) => {
            state.ui_state.json_key_search.push(c);
            state.ui_state.json_key_selection = 0;
            state.ui_state.json_key_scroll = 0;
        }
        Action::JsonKeyBackspace => {
            state.ui_state.json_key_search.pop();
            state.ui_state.json_key_selection = 0;
            state.ui_state.json_key_scroll = 0;
        }
        Action::JsonKeyClearSearch => {
            state.ui_state.json_key_search.clear();
            state.ui_state.json_key_selection = 0;
            state.ui_state.json_key_scroll = 0;
        }
        Action::JsonKeySelectPattern => {
            // Select all keys matching current search pattern
            let search = state.ui_state.json_key_search.to_lowercase();
            if !search.is_empty() {
                for key in &state.ui_state.json_available_keys {
                    if key.to_lowercase().contains(&search) {
                        state.ui_state.json_visible_keys.insert(key.clone());
                    }
                }
            }
        }

        Action::RefreshContexts
        | Action::RefreshNamespaces
        | Action::RefreshDeployments
        | Action::Tick
        | Action::Render => {
            // No-op for now
        }
    }
}

fn handle_list_select(state: &mut AppState, internal_tx: &mpsc::UnboundedSender<InternalAction>) {
    match state.current_screen {
        Screen::ContextSelect => {
            if let Some(idx) = state.selected_index() {
                if let Some(ctx) = state.contexts.get(idx) {
                    let name = ctx.name.clone();
                    let _ = state.action_tx.send(Action::SelectContext(name));
                }
            }
        }
        Screen::NamespaceSelect => {
            if let Some(idx) = state.selected_index() {
                if let Some(ns) = state.namespaces.get(idx) {
                    let name = ns.name.clone();
                    let _ = state.action_tx.send(Action::SelectNamespace(name));
                }
            }
        }
        Screen::DeploymentSelect => {
            if let Some(idx) = state.selected_index() {
                if let Some(deploy) = state.deployments.get(idx) {
                    let name = deploy.name.clone();
                    let _ = state.action_tx.send(Action::SelectDeployment(name));
                }
            }
        }
        Screen::LogViewer => {
            // No selection in log viewer
        }
    }
    let _ = internal_tx;
}

fn render(
    tui: &mut Tui,
    state: &mut AppState,
    log_buffer: &LogBuffer,
    palette_state: &mut CommandPaletteState,
    commands: &[Command],
) -> Result<()> {
    tui.terminal().draw(|frame| {
        match state.current_screen {
            Screen::ContextSelect => {
                ContextSelectScreen::render(frame, state);
            }
            Screen::NamespaceSelect => {
                NamespaceSelectScreen::render(frame, state);
            }
            Screen::DeploymentSelect => {
                DeploymentSelectScreen::render(frame, state);
            }
            Screen::LogViewer => {
                LogViewerScreen::render(frame, state, log_buffer);
            }
        }

        // Render JSON key filter overlay if visible
        if state.ui_state.json_key_filter_active {
            JsonKeyFilter::render(frame, state);
        }

        // Render command palette overlay if visible
        if palette_state.visible {
            CommandPalette::render(frame, palette_state, commands);
        }

        // Render help overlay if visible
        if state.ui_state.help_visible {
            HelpOverlay::render(frame);
        }
    })?;

    Ok(())
}

/// Get filtered JSON keys based on search input
fn get_filtered_json_keys(state: &AppState) -> Vec<String> {
    let search = state.ui_state.json_key_search.to_lowercase();
    if search.is_empty() {
        state.ui_state.json_available_keys.clone()
    } else {
        state.ui_state.json_available_keys
            .iter()
            .filter(|k| k.to_lowercase().contains(&search))
            .cloned()
            .collect()
    }
}

fn export_logs_to_file(filename: &str, log_buffer: &LogBuffer, state: &AppState) -> Result<usize> {
    let mut file = File::create(filename)?;
    let logs = log_buffer.all();

    // Apply text filter if active
    let text_filtered: Vec<_> = if let Some(filter) = &state.ui_state.active_filter {
        logs.iter().filter(|e| filter.matches(e)).collect()
    } else {
        logs.iter().collect()
    };

    // Apply JSON key filter if active
    let filtered: Vec<_> = if !state.ui_state.json_visible_keys.is_empty() {
        text_filtered
            .into_iter()
            .filter(|e| {
                if let Some(fields) = &e.fields {
                    fields.keys().any(|k| state.ui_state.json_visible_keys.contains(k))
                } else {
                    false
                }
            })
            .collect()
    } else {
        text_filtered
    };

    for entry in &filtered {
        let ts = entry
            .timestamp
            .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_default();

        writeln!(
            file,
            "{} [{}] {} | {}",
            ts,
            entry.level.as_str(),
            entry.pod_name,
            entry.raw
        )?;
    }

    Ok(filtered.len())
}
