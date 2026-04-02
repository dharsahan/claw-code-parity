//! Full TUI implementation using ratatui - OpenCode style
//!
//! This module provides a complete terminal user interface for claw-cli,
//! similar to OpenCode's Bubble Tea TUI in Go.

mod app;
mod events;
mod ui;
mod widgets;

pub use app::App;
pub use events::{Event, EventHandler, StreamEvent};

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::env;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use api::{
    AuthSource, ContentBlockDelta, MessageRequest, MessageResponse,
    OutputContentBlock, ProviderClient, ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice,
};
use commands::{
    handle_agents_slash_command, handle_plugins_slash_command, handle_skills_slash_command,
    SlashCommand,
};
use plugins::{PluginManager, PluginManagerConfig};
use runtime::{
    load_system_prompt, ApiRequest, AssistantEvent, CompactionConfig, ConfigLoader, ContentBlock,
    ConversationRuntime, MessageRole, PermissionMode, PermissionPromptDecision,
    PermissionPrompter, Session, TokenUsage, ToolError, ToolExecutor, UsageTracker,
};
use tools::GlobalToolRegistry;

use crate::{
    convert_messages, extract_code_blocks, format_compact_report, format_cost_report,
    format_model_report, format_model_switch_report, format_permissions_report,
    format_permissions_switch_report, format_status_report, format_tool_call_start,
    format_tool_result, max_tokens_for_model, permission_mode_from_label, permission_policy,
    render_config_report, render_diff_report, render_memory_report, render_session_list,
    resolve_model_alias, resolve_startup_auth_source as resolve_startup_runtime_auth,
    StatusUsage,
};

type AllowedToolSet = BTreeSet<String>;

const DEFAULT_DATE: &str = "2026-03-31";

/// Initialize the terminal for TUI mode
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restore the terminal to its original state
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Run the TUI application
pub async fn run(
    model: String,
    permission_mode: runtime::PermissionMode,
    provider_override: Option<api::ProviderKind>,
) -> io::Result<()> {
    let mut terminal = init_terminal()?;

    let app_result = App::new(model.clone(), permission_mode, provider_override).and_then(|mut app| {
        let event_handler = EventHandler::new(100);
        app.set_event_sender(event_handler.sender());
        let permission_allowlist = Arc::new(std::sync::Mutex::new(HashSet::new()));
        app.set_permission_allowlist(permission_allowlist.clone());
        let runtime = TuiRuntime::new(
            model,
            None,
            permission_mode,
            provider_override,
            event_handler.sender(),
            permission_allowlist,
        )
        .map_err(|error| io::Error::other(error.to_string()))?;
        app.session_path = runtime.session_path.clone();
        app.session_id = runtime
            .session_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("session")
            .to_string();
        run_app_async(&mut terminal, &mut app, event_handler, runtime)
    });

    restore_terminal(&mut terminal)?;

    if let Err(err) = app_result {
        eprintln!("Error: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}

/// Shared state between main loop and async streaming task
struct StreamingState {
    /// Whether a streaming request is in progress
    is_streaming: AtomicBool,
}

fn run_app_async(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    mut event_handler: EventHandler,
    runtime: TuiRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a tokio runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    // Shared state for coordinating streaming
    let streaming_state = Arc::new(StreamingState {
        is_streaming: AtomicBool::new(false),
    });

    const STREAM_BATCH_INTERVAL: Duration = Duration::from_millis(16);
    const STREAM_BATCH_MAX: usize = 96;
    let mut stream_batch: VecDeque<StreamEvent> = VecDeque::new();
    let mut last_stream_flush = Instant::now();

    // Get sender for injecting stream events
    let stream_tx = event_handler.sender();
    let runtime_state = Arc::new(std::sync::Mutex::new(runtime));

    loop {
        if !stream_batch.is_empty() && last_stream_flush.elapsed() >= STREAM_BATCH_INTERVAL {
            flush_stream_batch(app, &mut stream_batch);
            last_stream_flush = Instant::now();
        }

        terminal.draw(|frame| ui::render(frame, app))?;

        // Check if we need to start a runtime turn
        if app.is_loading
            && app.pending_turn_input.is_some()
            && !streaming_state.is_streaming.load(Ordering::SeqCst)
        {
            streaming_state.is_streaming.store(true, Ordering::SeqCst);
            let input = app.pending_turn_input.clone().unwrap_or_default();
            let tx = stream_tx.clone();
            let state = streaming_state.clone();
            let runtime_state = runtime_state.clone();

            rt.spawn_blocking(move || {
                let result = {
                    let mut runtime = runtime_state.lock().expect("tui runtime lock poisoned");
                    runtime.run_turn(&input, tx.clone())
                };
                state.is_streaming.store(false, Ordering::SeqCst);

                match result {
                    Ok(turn) => {
                        let _ = tx.send(Event::Stream(StreamEvent::Done));
                        let _ = tx.send(Event::TurnFinished(turn));
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Stream(StreamEvent::Error(e.to_string())));
                    }
                }
            });
        }

        match event_handler.next()? {
            Event::Tick => {
                app.on_tick();
            }
            Event::Key(key_event) => {
                if !stream_batch.is_empty() {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }

                if app.handle_key_event(key_event)? {
                    if let Ok(runtime) = runtime_state.lock() {
                        let _ = runtime.persist_session();
                    }
                    break;
                }

                if app.mode == app::AppMode::Normal {
                    if let Some(raw_command) = app.pending_slash_command.take() {
                        let result = {
                            let mut runtime =
                                runtime_state.lock().expect("tui runtime lock poisoned");
                            let parsed = SlashCommand::parse(&raw_command).unwrap_or_else(|| {
                                let name = raw_command
                                    .trim()
                                    .trim_start_matches('/')
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("unknown")
                                    .to_string();
                                SlashCommand::Unknown(name)
                            });
                            runtime.handle_slash_command(
                                parsed,
                                stream_tx.clone(),
                                app.permission_mode,
                            )
                        };
                        match result {
                            Ok(output) => {
                                if !output.trim().is_empty() {
                                    app.messages.push_back(app::Message {
                                        role: app::MessageRole::System,
                                        content: output,
                                        timestamp: std::time::Instant::now(),
                                        tool_use: None,
                                        is_streaming: false,
                                    });
                                    app.scroll_to_bottom_for_system();
                                }
                                app.input.clear();
                                app.input_cursor = 0;
                                if let Ok(runtime) = runtime_state.lock() {
                                    app.model = runtime.model.clone();
                                    app.permission_mode = runtime.permission_mode;
                                    app.session_path = runtime.session_path.clone();
                                    app.session_id = runtime
                                        .session_path
                                        .file_stem()
                                        .and_then(|value| value.to_str())
                                        .unwrap_or("session")
                                        .to_string();
                                    app.turn_count = runtime.runtime.usage().turns();
                                    app.usage = runtime.runtime.usage().current_turn_usage();
                                    app.cumulative_usage = runtime.runtime.usage().cumulative_usage();
                                    app.estimated_context_tokens = runtime.runtime.estimated_tokens();
                                }
                            }
                            Err(error) => {
                                app.set_status(&format!("Command failed: {error}"));
                            }
                        }
                    }
                }

                if app.model
                    != runtime_state
                        .lock()
                        .expect("tui runtime lock poisoned")
                        .model
                {
                    let update_result = {
                        let mut runtime = runtime_state.lock().expect("tui runtime lock poisoned");
                        runtime.set_model(&app.model, stream_tx.clone())
                    };
                    if let Err(error) = update_result {
                        app.set_status(&format!("Model switch failed: {error}"));
                    }
                }
            }
            Event::Mouse(mouse_event) => {
                if !stream_batch.is_empty() {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }
                app.handle_mouse_event(mouse_event);
            }
            Event::Resize(width, height) => {
                if !stream_batch.is_empty() {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }
                app.handle_resize(width, height);
            }
            Event::Stream(stream_event) => {
                let force_flush = !matches!(stream_event, StreamEvent::TextDelta(_));
                stream_batch.push_back(stream_event);
                if force_flush
                    || stream_batch.len() >= STREAM_BATCH_MAX
                    || last_stream_flush.elapsed() >= STREAM_BATCH_INTERVAL
                {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }
            }
            Event::PermissionRequest(request_event) => {
                if !stream_batch.is_empty() {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }
                app.handle_permission_request(request_event);
            }
            Event::TurnFinished(turn_event) => {
                if !stream_batch.is_empty() {
                    flush_stream_batch(app, &mut stream_batch);
                    last_stream_flush = Instant::now();
                }
                app.handle_turn_finished(turn_event);
            }
        }
    }

    Ok(())
}

fn flush_stream_batch(app: &mut App, stream_batch: &mut VecDeque<StreamEvent>) {
    while let Some(event) = stream_batch.pop_front() {
        app.handle_stream_event(event);
    }
}

struct TuiRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    enable_tools: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    stream_tx: std::sync::mpsc::Sender<Event>,
}

impl TuiRuntimeClient {
    fn new(
        model: String,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        provider_override: Option<ProviderKind>,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let default_auth = resolve_tui_auth_source().ok();
        let client = ProviderClient::from_model_with_override_and_auth(
            &model,
            provider_override,
            default_auth,
        )?;
        Ok(Self {
            runtime: tokio::runtime::Runtime::new()?,
            client,
            model,
            enable_tools,
            allowed_tools,
            tool_registry,
            stream_tx,
        })
    }
}

fn resolve_tui_auth_source() -> Result<AuthSource, Box<dyn std::error::Error>> {
    Ok(resolve_startup_runtime_auth(|| {
        let cwd = env::current_dir().map_err(api::ApiError::from)?;
        let config = ConfigLoader::default_for(&cwd).load().map_err(|error| {
            api::ApiError::Auth(format!("failed to load runtime OAuth config: {error}"))
        })?;
        Ok(config.oauth().cloned())
    })?)
}

impl runtime::ApiClient for TuiRuntimeClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, runtime::RuntimeError> {
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: self
                .enable_tools
                .then(|| self.tool_registry.definitions(self.allowed_tools.as_ref())),
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            stream: true,
        };

        self.runtime.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|error| runtime::RuntimeError::new(error.to_string()))?;
            let mut events = Vec::new();
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut saw_stop = false;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|error| runtime::RuntimeError::new(error.to_string()))?
            {
                match event {
                    ApiStreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            push_output_block_tui(
                                block,
                                &self.stream_tx,
                                &mut events,
                                &mut pending_tool,
                                true,
                            )?;
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => {
                        push_output_block_tui(
                            start.content_block,
                            &self.stream_tx,
                            &mut events,
                            &mut pending_tool,
                            true,
                        )?;
                    }
                    ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                let _ = self
                                    .stream_tx
                                    .send(Event::Stream(StreamEvent::TextDelta(text.clone())));
                                events.push(AssistantEvent::TextDelta(text));
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = &mut pending_tool {
                                input.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { .. }
                        | ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    ApiStreamEvent::ContentBlockStop(_) => {
                        if let Some((id, name, input)) = pending_tool.take() {
                            let _ = self.stream_tx.send(Event::Stream(StreamEvent::ToolUseInput {
                                id: id.clone(),
                                input: input.clone(),
                            }));
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    ApiStreamEvent::MessageDelta(delta) => {
                        let usage = TokenUsage {
                            input_tokens: delta.usage.input_tokens,
                            output_tokens: delta.usage.output_tokens,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: 0,
                        };
                        let _ = self.stream_tx.send(Event::Stream(StreamEvent::Usage(usage)));
                        events.push(AssistantEvent::Usage(usage));
                    }
                    ApiStreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        events.push(AssistantEvent::MessageStop);
                    }
                }
            }

            if !saw_stop
                && events.iter().any(|event| {
                    matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                        || matches!(event, AssistantEvent::ToolUse { .. })
                })
            {
                events.push(AssistantEvent::MessageStop);
            }

            if events
                .iter()
                .any(|event| matches!(event, AssistantEvent::MessageStop))
            {
                return Ok(events);
            }

            let response = self
                .client
                .send_message(&MessageRequest {
                    stream: false,
                    ..message_request.clone()
                })
                .await
                .map_err(|error| runtime::RuntimeError::new(error.to_string()))?;
            response_to_events_tui(response, &self.stream_tx)
        })
    }
}

fn push_output_block_tui(
    block: OutputContentBlock,
    tx: &std::sync::mpsc::Sender<Event>,
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    streaming_tool_input: bool,
) -> Result<(), runtime::RuntimeError> {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let _ = tx.send(Event::Stream(StreamEvent::TextDelta(text.clone())));
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let _ = tx.send(Event::Stream(StreamEvent::ToolUseStart {
                id: id.clone(),
                name: name.clone(),
            }));
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
    }
    Ok(())
}

fn response_to_events_tui(
    response: MessageResponse,
    tx: &std::sync::mpsc::Sender<Event>,
) -> Result<Vec<AssistantEvent>, runtime::RuntimeError> {
    let mut events = Vec::new();
    let mut pending_tool = None;

    for block in response.content {
        push_output_block_tui(block, tx, &mut events, &mut pending_tool, false)?;
        if let Some((id, name, input)) = pending_tool.take() {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    let usage = TokenUsage {
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        cache_creation_input_tokens: response.usage.cache_creation_input_tokens,
        cache_read_input_tokens: response.usage.cache_read_input_tokens,
    };
    let _ = tx.send(Event::Stream(StreamEvent::Usage(usage)));
    events.push(AssistantEvent::Usage(usage));
    events.push(AssistantEvent::MessageStop);
    Ok(events)
}

#[derive(Clone)]
struct TuiToolExecutor {
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    stream_tx: std::sync::mpsc::Sender<Event>,
}

impl TuiToolExecutor {
    fn new(
        allowed_tools: Option<AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Self {
        Self {
            allowed_tools,
            tool_registry,
            stream_tx,
        }
    }
}

impl ToolExecutor for TuiToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }

        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;

        let start = format_tool_call_start(tool_name, input);
        let _ = self
            .stream_tx
            .send(Event::Stream(StreamEvent::TextDelta(format!("\n{start}\n"))));

        match self.tool_registry.execute(tool_name, &value) {
            Ok(output) => {
                let rendered = format_tool_result(tool_name, &output, false);
                let _ = self
                    .stream_tx
                    .send(Event::Stream(StreamEvent::TextDelta(format!("\n{rendered}\n"))));
                Ok(output)
            }
            Err(error) => {
                let rendered = format_tool_result(tool_name, &error, true);
                let _ = self
                    .stream_tx
                    .send(Event::Stream(StreamEvent::TextDelta(format!("\n{rendered}\n"))));
                Err(ToolError::new(error))
            }
        }
    }
}

#[derive(Clone)]
struct TuiPermissionPrompter {
    tx: std::sync::mpsc::Sender<Event>,
    allowlist: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl TuiPermissionPrompter {
    fn new(tx: std::sync::mpsc::Sender<Event>, allowlist: Arc<std::sync::Mutex<HashSet<String>>>) -> Self {
        Self { tx, allowlist }
    }
}

impl PermissionPrompter for TuiPermissionPrompter {
    fn decide(&mut self, request: &runtime::PermissionRequest) -> runtime::PermissionPromptDecision {
        if let Ok(set) = self.allowlist.lock() {
            if set.contains(&request.tool_name) {
                return PermissionPromptDecision::Allow;
            }
        }

        let (decision_tx, decision_rx) = std::sync::mpsc::channel();
        let request_event = events::PermissionRequestEvent {
            request: request.clone(),
            response_tx: decision_tx,
        };
        if self.tx.send(Event::PermissionRequest(request_event)).is_err() {
            return runtime::PermissionPromptDecision::Deny {
                reason: "permission prompt unavailable".to_string(),
            };
        }
        match decision_rx.recv() {
            Ok(decision) => decision,
            Err(error) => runtime::PermissionPromptDecision::Deny {
                reason: format!("permission prompt failed: {error}"),
            },
        }
    }
}

struct TuiRuntime {
    model: String,
    system_prompt: Vec<String>,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    provider_override: Option<ProviderKind>,
    runtime: ConversationRuntime<TuiRuntimeClient, TuiToolExecutor>,
    session_path: PathBuf,
    permission_allowlist: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl TuiRuntime {
    fn new(
        model: String,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
        provider_override: Option<ProviderKind>,
        stream_tx: std::sync::mpsc::Sender<Event>,
        permission_allowlist: Arc<std::sync::Mutex<HashSet<String>>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let system_prompt = build_system_prompt()?;
        let session_path = create_session_path()?;
        let (feature_config, tool_registry) = build_runtime_plugin_state()?;
        let policy = permission_policy(permission_mode, &tool_registry);
        let runtime = ConversationRuntime::new_with_features(
            Session::new(),
            TuiRuntimeClient::new(
                model.clone(),
                true,
                allowed_tools.clone(),
                tool_registry.clone(),
                provider_override,
                stream_tx.clone(),
            )?,
            TuiToolExecutor::new(allowed_tools.clone(), tool_registry, stream_tx),
            policy,
            system_prompt.clone(),
            feature_config,
        );
        let runtime = Self {
            model,
            system_prompt,
            allowed_tools,
            permission_mode,
            provider_override,
            runtime,
            session_path,
            permission_allowlist,
        };
        runtime.persist_session()?;
        Ok(runtime)
    }

    fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session_path)?;
        Ok(())
    }

    fn rebuild_runtime(
        &mut self,
        session: Session,
        usage: UsageTracker,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (feature_config, tool_registry) = build_runtime_plugin_state()?;
        let policy = permission_policy(self.permission_mode, &tool_registry);
        let runtime = ConversationRuntime::new_with_features(
            session,
            TuiRuntimeClient::new(
                self.model.clone(),
                true,
                self.allowed_tools.clone(),
                tool_registry.clone(),
                self.provider_override,
                stream_tx.clone(),
            )?,
            TuiToolExecutor::new(self.allowed_tools.clone(), tool_registry.clone(), stream_tx),
            policy,
            self.system_prompt.clone(),
            feature_config,
        );
        self.runtime = runtime;
        let _ = usage;
        Ok(())
    }

    fn set_model(
        &mut self,
        model: &str,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let next = resolve_model_alias(model).to_string();
        if self.model == next {
            return Ok(());
        }
        let session = self.runtime.session().clone();
        let usage = UsageTracker::from_session(&session);
        self.model = next;
        self.rebuild_runtime(session, usage, stream_tx)
    }

    fn set_permission_mode(
        &mut self,
        mode: PermissionMode,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.permission_mode = mode;
        let session = self.runtime.session().clone();
        let usage = UsageTracker::from_session(&session);
        self.rebuild_runtime(session, usage, stream_tx)
    }

    fn clear_session(
        &mut self,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut set) = self.permission_allowlist.lock() {
            set.clear();
        }
        self.session_path = create_session_path()?;
        self.rebuild_runtime(Session::new(), UsageTracker::new(), stream_tx)?;
        self.persist_session()?;
        Ok(())
    }

    fn run_turn(
        &mut self,
        input: &str,
        stream_tx: std::sync::mpsc::Sender<Event>,
    ) -> Result<events::TurnFinishedEvent, Box<dyn std::error::Error>> {
        let mut prompter = TuiPermissionPrompter::new(stream_tx, self.permission_allowlist.clone());
        let summary = self.runtime.run_turn(input, Some(&mut prompter))?;
        self.persist_session()?;
        Ok(events::TurnFinishedEvent {
            final_text: final_assistant_text(&summary),
            latest_usage: self.runtime.usage().current_turn_usage(),
            cumulative_usage: self.runtime.usage().cumulative_usage(),
            turns: self.runtime.usage().turns(),
            estimated_tokens: self.runtime.estimated_tokens(),
        })
    }

    fn handle_slash_command(
        &mut self,
        command: SlashCommand,
        stream_tx: std::sync::mpsc::Sender<Event>,
        current_permission_mode: PermissionMode,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match command {
            SlashCommand::Help => Ok(crate::render_repl_help()),
            SlashCommand::Status => {
                let cumulative = self.runtime.usage().cumulative_usage();
                let latest = self.runtime.usage().current_turn_usage();
                Ok(format_status_report(
                    &self.model,
                    StatusUsage {
                        message_count: self.runtime.session().messages.len(),
                        turns: self.runtime.usage().turns(),
                        latest,
                        cumulative,
                        estimated_tokens: self.runtime.estimated_tokens(),
                    },
                    current_permission_mode.as_str(),
                    &crate::status_context(Some(&self.session_path))?,
                ))
            }
            SlashCommand::Compact => {
                let result = self.runtime.compact(CompactionConfig::default());
                let removed = result.removed_message_count;
                let kept = result.compacted_session.messages.len();
                let skipped = removed == 0;
                let usage = UsageTracker::from_session(&result.compacted_session);
                self.rebuild_runtime(result.compacted_session, usage, stream_tx)?;
                self.persist_session()?;
                Ok(format_compact_report(removed, kept, skipped))
            }
            SlashCommand::Model { model } => {
                let Some(model) = model else {
                    return Ok(format_model_report(
                        &self.model,
                        self.runtime.session().messages.len(),
                        self.runtime.usage().turns(),
                    ));
                };
                let previous = self.model.clone();
                self.set_model(&model, stream_tx)?;
                Ok(format_model_switch_report(
                    &previous,
                    &self.model,
                    self.runtime.session().messages.len(),
                ))
            }
            SlashCommand::Permissions { mode } => {
                let Some(mode) = mode else {
                    return Ok(format_permissions_report(current_permission_mode.as_str()));
                };
                let normalized = crate::normalize_permission_mode(&mode).ok_or_else(|| {
                    format!(
                        "unsupported permission mode '{mode}'. Use read-only, workspace-write, or danger-full-access."
                    )
                })?;
                let next = permission_mode_from_label(normalized);
                self.set_permission_mode(next, stream_tx)?;
                Ok(format_permissions_switch_report(
                    current_permission_mode.as_str(),
                    normalized,
                ))
            }
            SlashCommand::Clear { confirm } => {
                if !confirm {
                    return Ok(
                        "clear: confirmation required; run /clear --confirm to start a fresh session."
                            .to_string(),
                    );
                }
                self.clear_session(stream_tx)?;
                Ok("Session cleared".to_string())
            }
            SlashCommand::Cost => Ok(format_cost_report(self.runtime.usage().cumulative_usage())),
            SlashCommand::Resume { session_path } => {
                let Some(session_ref) = session_path else {
                    return Ok("Usage: /resume <session-path>".to_string());
                };
                let session_path = resolve_session_reference(&session_ref)?;
                let session = Session::load_from_path(&session_path)?;
                let usage = UsageTracker::from_session(&session);
                if let Ok(mut set) = self.permission_allowlist.lock() {
                    set.clear();
                }
                self.session_path = session_path;
                self.rebuild_runtime(session, usage, stream_tx)?;
                Ok(format!(
                    "Session resumed\n  Session file     {}\n  Messages         {}\n  Turns            {}",
                    self.session_path.display(),
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                ))
            }
            SlashCommand::Config { section } => render_config_report(section.as_deref()),
            SlashCommand::Memory => render_memory_report(),
            SlashCommand::Diff => render_diff_report(),
            SlashCommand::Version => Ok(crate::render_version_report()),
            SlashCommand::Export { path } => {
                let export_path = crate::resolve_export_path(path.as_deref(), self.runtime.session())?;
                std::fs::write(&export_path, crate::render_export_text(self.runtime.session()))?;
                Ok(format!(
                    "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
                    export_path.display(),
                    self.runtime.session().messages.len(),
                ))
            }
            SlashCommand::Session { action, target } => match action.as_deref() {
                None | Some("list") => Ok(render_session_list(
                    self.session_path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default(),
                )?),
                Some("switch") => {
                    let Some(target) = target.as_deref() else {
                        return Ok("Usage: /session switch <session-id>".to_string());
                    };
                    let session_path = resolve_session_reference(target)?;
                    let session = Session::load_from_path(&session_path)?;
                    let usage = UsageTracker::from_session(&session);
                    if let Ok(mut set) = self.permission_allowlist.lock() {
                        set.clear();
                    }
                    self.session_path = session_path;
                    self.rebuild_runtime(session, usage, stream_tx)?;
                    Ok(format!(
                        "Session switched\n  File             {}\n  Messages         {}",
                        self.session_path.display(),
                        self.runtime.session().messages.len(),
                    ))
                }
                Some(other) => Ok(format!(
                    "Unknown /session action '{other}'. Use /session list or /session switch <session-id>."
                )),
            },
            SlashCommand::Plugins { action, target } => {
                let cwd = env::current_dir()?;
                let loader = ConfigLoader::default_for(&cwd);
                let runtime_config = loader.load()?;
                let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
                let result = handle_plugins_slash_command(action.as_deref(), target.as_deref(), &mut manager)?;
                if result.reload_runtime {
                    let session = self.runtime.session().clone();
                    let usage = UsageTracker::from_session(&session);
                    self.rebuild_runtime(session, usage, stream_tx)?;
                }
                Ok(result.message)
            }
            SlashCommand::Agents { args } => {
                let cwd = env::current_dir()?;
                Ok(handle_agents_slash_command(args.as_deref(), &cwd)?)
            }
            SlashCommand::Skills { args } => {
                let cwd = env::current_dir()?;
                Ok(handle_skills_slash_command(args.as_deref(), &cwd)?)
            }
            SlashCommand::Copy { target } => {
                let content = format_copy_content(self.runtime.session(), target.as_deref())?;
                if content.trim().is_empty() {
                    return Ok("Copy\n  Result           skipped\n  Reason           no content to copy".to_string());
                }
                match crate::copy_to_clipboard(&content) {
                    Ok(()) => {
                        let lines = content.lines().count();
                        let chars = content.len();
                        Ok(format!(
                            "Copy\n  Result           copied to clipboard\n  Lines            {lines}\n  Characters       {chars}"
                        ))
                    }
                    Err(error) => Ok(format!(
                        "Copy\n  Result           failed\n  Error            {error}\n  Hint             install xclip, xsel, or wl-copy"
                    )),
                }
            }
            SlashCommand::Theme { name } => {
                let available_themes = ["default", "claude", "monokai", "nord", "light"];
                match name.as_deref() {
                    None | Some("list") => Ok(format!(
                        "Themes\n  Available themes:\n    {}\n\n  Usage: /theme <name>",
                        available_themes.join("\n    ")
                    )),
                    Some(theme_name) if available_themes.contains(&theme_name) => Ok(format!(
                        "Theme\n  Result           theme changed\n  Theme            {theme_name}\n  Note             restart REPL or use next turn to see changes"
                    )),
                    Some(theme_name) => Ok(format!(
                        "Theme\n  Result           not found\n  Theme            {theme_name}\n  Available        {}",
                        available_themes.join(", ")
                    )),
                }
            }
            SlashCommand::Bughunter { .. }
            | SlashCommand::Branch { .. }
            | SlashCommand::Worktree { .. }
            | SlashCommand::CommitPushPr { .. }
            | SlashCommand::Commit
            | SlashCommand::Pr { .. }
            | SlashCommand::Issue { .. }
            | SlashCommand::Ultraplan { .. }
            | SlashCommand::Teleport { .. }
            | SlashCommand::DebugToolCall
            | SlashCommand::Init => {
                Ok("Not yet wired in TUI. Use non-TUI REPL for this command.".to_string())
            }
            SlashCommand::Unknown(name) => Ok(format!("unknown slash command: /{name}")),
        }
    }
}

fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn build_system_prompt() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(load_system_prompt(
        env::current_dir()?,
        DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )?)
}

fn build_runtime_plugin_state(
) -> Result<(runtime::RuntimeFeatureConfig, GlobalToolRegistry), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    let plugin_manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let tool_registry = GlobalToolRegistry::with_plugin_tools(plugin_manager.aggregated_tools()?)?;
    Ok((runtime_config.feature_config().clone(), tool_registry))
}

fn build_plugin_manager(
    cwd: &std::path::Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

fn resolve_plugin_path(
    cwd: &std::path::Path,
    config_home: &std::path::Path,
    value: &str,
) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

fn create_session_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let sessions_dir = PathBuf::from(home).join(".claw").join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let pid = std::process::id();
    Ok(sessions_dir.join(format!("session-{timestamp:x}-{pid:x}.json")))
}

fn resolve_session_reference(session_ref: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let candidate = PathBuf::from(session_ref);
    if candidate.exists() {
        return Ok(std::fs::canonicalize(candidate)?);
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let sessions_dir = PathBuf::from(home).join(".claw").join("sessions");
    let normalized = if session_ref.ends_with(".json") {
        session_ref.to_string()
    } else {
        format!("{session_ref}.json")
    };
    let direct = sessions_dir.join(&normalized);
    if direct.exists() {
        return Ok(direct);
    }
    for entry in std::fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if id.starts_with(session_ref) || id.ends_with(session_ref) {
            return Ok(path);
        }
    }
    Err(format!("session '{session_ref}' not found").into())
}

fn format_copy_content(session: &Session, target: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    match target {
        None | Some("last") => session
            .messages
            .iter()
            .rev()
            .find(|msg| msg.role == MessageRole::Assistant)
            .map(|msg| {
                msg.blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .ok_or_else(|| "No assistant message found in session".into()),
        Some("code") => session
            .messages
            .iter()
            .rev()
            .find(|msg| msg.role == MessageRole::Assistant)
            .map(|msg| {
                msg.blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(extract_code_blocks(text)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .ok_or_else(|| "No assistant message found in session".into()),
        Some("all") => Ok(session
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                    MessageRole::Tool => "Tool",
                };
                let text = msg
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("## {role}\n\n{text}")
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")),
        Some(other) => Err(format!(
            "Unknown /copy target '{other}'. Use /copy [last|code|all]."
        )
        .into()),
    }
}
