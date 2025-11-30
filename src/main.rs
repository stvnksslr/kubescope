use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

mod app;
mod config;
mod k8s;
mod logs;
mod tui;
mod types;
mod ui;

use app::{Action, AppState, Screen};
use config::{KeyBindings, KeyContext};
use k8s::KubeClient;
use logs::{CompiledFilter, LogBuffer, LogStreamManager};
use tui::{Event, EventHandler, Tui};
use types::{DeploymentInfo, LogEntry, NamespaceInfo, PodInfo};
use ui::components::{
    Command, CommandPalette, CommandPaletteState, HelpOverlay, JsonKeyFilter, collect_json_keys,
    log_viewer_commands,
};
use ui::screens::{
    ContextSelectScreen, DeploymentSelectScreen, LogViewerScreen, NamespaceSelectScreen,
};

/// Configuration file structure for .kubescope
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Config {
    /// Kubernetes context name
    context: Option<String>,
    /// Namespace
    namespace: Option<String>,
    /// Deployment name
    deployment: Option<String>,
    /// Filter pattern (regex)
    filter: Option<String>,
    /// Case insensitive filter matching
    #[serde(default)]
    ignore_case: bool,
    /// Invert filter match
    #[serde(default)]
    invert_match: bool,
    /// Buffer size for log entries
    buffer_size: Option<usize>,
    /// Number of historical log lines to fetch per pod
    tail_lines: Option<i64>,
}

impl Config {
    /// Load config from .kubescope file in current directory
    fn load() -> Option<Self> {
        let path = PathBuf::from(".kubescope");
        if path.exists() {
            let content = std::fs::read_to_string(&path).ok()?;
            toml::from_str(&content).ok()
        } else {
            None
        }
    }

    /// Save config to .kubescope file
    fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(".kubescope", content)?;
        Ok(())
    }
}

/// Kubescope - A terminal UI for viewing Kubernetes deployment logs
#[derive(Parser, Debug)]
#[command(name = "kubescope")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Kubernetes context name (optional, will prompt if not provided)
    #[arg(value_name = "CONTEXT", global = true)]
    context: Option<String>,

    /// Namespace (optional, requires context)
    #[arg(value_name = "NAMESPACE", global = true)]
    namespace: Option<String>,

    /// Deployment name (optional, requires context and namespace)
    #[arg(value_name = "DEPLOYMENT", global = true)]
    deployment: Option<String>,

    /// Buffer size for log entries
    #[arg(long, default_value = "10000", global = true)]
    buffer_size: usize,

    /// Number of historical log lines to fetch per pod
    #[arg(long, default_value = "100", global = true)]
    tail_lines: i64,

    /// Filter pattern (regex) to pre-populate log filter
    #[arg(short = 'e', long = "filter", global = true)]
    filter: Option<String>,

    /// Case insensitive filter matching
    #[arg(short = 'i', long = "ignore-case", global = true)]
    ignore_case: bool,

    /// Invert filter match (show non-matching lines)
    #[arg(short = 'v', long = "invert-match", global = true)]
    invert_match: bool,

    /// Ignore .kubescope config file
    #[arg(long, global = true)]
    no_config: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a .kubescope configuration file in the current directory
    Init,
}

/// Resolved arguments after merging CLI args and config file
struct Args {
    context: Option<String>,
    namespace: Option<String>,
    deployment: Option<String>,
    buffer_size: usize,
    tail_lines: i64,
    filter: Option<String>,
    ignore_case: bool,
    invert_match: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing for debugging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Handle subcommands
    if let Some(Commands::Init) = cli.command {
        return run_init().await;
    }

    // Load config file if present and not disabled
    let config = if cli.no_config { None } else { Config::load() };

    // Merge CLI args with config file (CLI takes precedence)
    let args = Args {
        context: cli
            .context
            .or_else(|| config.as_ref().and_then(|c| c.context.clone())),
        namespace: cli
            .namespace
            .or_else(|| config.as_ref().and_then(|c| c.namespace.clone())),
        deployment: cli
            .deployment
            .or_else(|| config.as_ref().and_then(|c| c.deployment.clone())),
        buffer_size: config
            .as_ref()
            .and_then(|c| c.buffer_size)
            .unwrap_or(cli.buffer_size),
        tail_lines: config
            .as_ref()
            .and_then(|c| c.tail_lines)
            .unwrap_or(cli.tail_lines),
        filter: cli
            .filter
            .or_else(|| config.as_ref().and_then(|c| c.filter.clone())),
        ignore_case: cli.ignore_case || config.as_ref().is_some_and(|c| c.ignore_case),
        invert_match: cli.invert_match || config.as_ref().is_some_and(|c| c.invert_match),
    };

    // Run the application
    let result = run_app(args).await;

    // Handle any errors
    if let Err(e) = &result {
        eprintln!("Error: {:#}", e);
    }

    result
}

/// Run the init command to create a .kubescope configuration file
async fn run_init() -> Result<()> {
    use std::io::{self, BufRead};

    println!("Initializing .kubescope configuration file...\n");

    // Check if .kubescope already exists
    if PathBuf::from(".kubescope").exists() {
        print!("A .kubescope file already exists. Overwrite? [y/N]: ");
        std::io::Write::flush(&mut io::stdout())?;

        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;

        if !line.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Load kubeconfig to get available contexts
    let kube_client = KubeClient::new().await?;
    let contexts = kube_client.get_contexts();

    if contexts.is_empty() {
        anyhow::bail!("No Kubernetes contexts found in kubeconfig");
    }

    let stdin = io::stdin();
    let mut config = Config::default();

    // Select context
    println!("Available contexts:");
    for (i, ctx) in contexts.iter().enumerate() {
        let current = if ctx.is_current { " (current)" } else { "" };
        println!("  {}. {}{}", i + 1, ctx.name, current);
    }
    print!("\nSelect context number (or press Enter to skip): ");
    std::io::Write::flush(&mut io::stdout())?;

    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    let line = line.trim();

    if !line.is_empty()
        && let Ok(idx) = line.parse::<usize>()
        && idx > 0
        && idx <= contexts.len()
    {
        let context_name = contexts[idx - 1].name.clone();
        config.context = Some(context_name.clone());

        // Connect to context and get namespaces
        println!("\nLoading namespaces for '{}'...", context_name);
        let client = kube_client.client_for_context(&context_name).await?;
        let namespaces = kube_client.get_namespaces(&client).await?;

        if !namespaces.is_empty() {
            println!("\nAvailable namespaces:");
            for (i, ns) in namespaces.iter().enumerate() {
                println!("  {}. {}", i + 1, ns.name);
            }
            print!("\nSelect namespace number (or press Enter to skip): ");
            std::io::Write::flush(&mut io::stdout())?;

            let mut line = String::new();
            stdin.lock().read_line(&mut line)?;
            let line = line.trim();

            if !line.is_empty()
                && let Ok(idx) = line.parse::<usize>()
                && idx > 0
                && idx <= namespaces.len()
            {
                let namespace_name = namespaces[idx - 1].name.clone();
                config.namespace = Some(namespace_name.clone());

                // Get deployments
                println!("\nLoading deployments for '{}'...", namespace_name);
                let deployments = kube_client
                    .get_deployments(&client, &namespace_name)
                    .await?;

                if !deployments.is_empty() {
                    println!("\nAvailable deployments:");
                    for (i, deploy) in deployments.iter().enumerate() {
                        println!(
                            "  {}. {} ({}/{} ready)",
                            i + 1,
                            deploy.name,
                            deploy.ready_replicas,
                            deploy.replicas
                        );
                    }
                    print!("\nSelect deployment number (or press Enter to skip): ");
                    std::io::Write::flush(&mut io::stdout())?;

                    let mut line = String::new();
                    stdin.lock().read_line(&mut line)?;
                    let line = line.trim();

                    if !line.is_empty()
                        && let Ok(idx) = line.parse::<usize>()
                        && idx > 0
                        && idx <= deployments.len()
                    {
                        config.deployment = Some(deployments[idx - 1].name.clone());
                    }
                } else {
                    println!("No deployments found in namespace.");
                }
            }
        } else {
            println!("No namespaces found.");
        }
    }

    // Filter pattern
    print!("\nFilter pattern (regex, press Enter to skip): ");
    std::io::Write::flush(&mut io::stdout())?;

    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    let filter = line.trim();
    if !filter.is_empty() {
        // Validate the filter pattern
        match CompiledFilter::new(filter) {
            Ok(_) => {
                config.filter = Some(filter.to_string());

                // Case insensitive?
                print!("Case insensitive matching? [y/N]: ");
                std::io::Write::flush(&mut io::stdout())?;

                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                if line.trim().eq_ignore_ascii_case("y") {
                    config.ignore_case = true;
                }

                // Invert match?
                print!("Invert match (show non-matching lines)? [y/N]: ");
                std::io::Write::flush(&mut io::stdout())?;

                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                if line.trim().eq_ignore_ascii_case("y") {
                    config.invert_match = true;
                }
            }
            Err(e) => {
                println!("Invalid filter pattern: {}. Skipping filter.", e);
            }
        }
    }

    // Save config
    config.save()?;

    println!("\nConfiguration saved to .kubescope");
    println!("\nConfiguration:");
    if let Some(ctx) = &config.context {
        println!("  context: {}", ctx);
    }
    if let Some(ns) = &config.namespace {
        println!("  namespace: {}", ns);
    }
    if let Some(deploy) = &config.deployment {
        println!("  deployment: {}", deploy);
    }
    if let Some(filter) = &config.filter {
        println!("  filter: {}", filter);
        if config.ignore_case {
            println!("  ignore_case: true");
        }
        if config.invert_match {
            println!("  invert_match: true");
        }
    }

    println!("\nRun 'kubescope' to start with this configuration.");
    println!("Use --no-config to ignore this file.");

    Ok(())
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
    // Validate filter pattern early (before any expensive initialization)
    if let Some(filter_pattern) = &args.filter {
        let test_result = if args.ignore_case {
            CompiledFilter::new_case_insensitive(filter_pattern)
        } else {
            CompiledFilter::new(filter_pattern)
        };
        if let Err(e) = test_result {
            anyhow::bail!("Invalid filter pattern '{}': {}", filter_pattern, e);
        }
    }

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

    // Apply CLI filter if provided (already validated at startup)
    if let Some(filter_pattern) = &args.filter {
        let mut filter = if args.ignore_case {
            CompiledFilter::new_case_insensitive(filter_pattern)
        } else {
            CompiledFilter::new(filter_pattern)
        }
        .expect("Filter pattern was validated at startup");

        if args.invert_match {
            filter = filter.inverted();
        }
        state.ui_state.active_filter = Some(filter);
        state.ui_state.search_input = filter_pattern.clone();
        state.ui_state.filter_case_insensitive = args.ignore_case;
    }

    // Initial render
    render(
        &mut tui,
        &mut state,
        &log_buffer,
        &mut palette_state,
        &commands,
    )?;

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
                        if let Some(client) = &active_client
                            && let Some(namespace) = &state.selected_namespace {
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

                    InternalAction::RestartLogStreaming => {
                        if let Some(client) = &active_client
                            && let Some(namespace) = &state.selected_namespace {
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

        render(
            &mut tui,
            &mut state,
            &log_buffer,
            &mut palette_state,
            &commands,
        )?;
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
            if let Some(namespace) = &state.selected_namespace
                && let Some(deployment) = state.deployments.iter().find(|d| d.name == name)
            {
                let _ = internal_tx.send(InternalAction::LoadPods(
                    namespace.clone(),
                    deployment.clone(),
                ));
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
                handle_action(
                    state,
                    internal_tx,
                    log_buffer,
                    palette_state,
                    commands,
                    action,
                );
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
            if let Some(idx) = state.selected_index()
                && let Some(ctx) = state.contexts.get(idx)
            {
                let name = ctx.name.clone();
                let _ = state.action_tx.send(Action::SelectContext(name));
            }
        }
        Screen::NamespaceSelect => {
            if let Some(idx) = state.selected_index()
                && let Some(ns) = state.namespaces.get(idx)
            {
                let name = ns.name.clone();
                let _ = state.action_tx.send(Action::SelectNamespace(name));
            }
        }
        Screen::DeploymentSelect => {
            if let Some(idx) = state.selected_index()
                && let Some(deploy) = state.deployments.get(idx)
            {
                let name = deploy.name.clone();
                let _ = state.action_tx.send(Action::SelectDeployment(name));
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
        state
            .ui_state
            .json_available_keys
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
                    fields
                        .keys()
                        .any(|k| state.ui_state.json_visible_keys.contains(k))
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
