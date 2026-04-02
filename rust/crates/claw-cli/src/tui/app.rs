//! Application state and logic for the TUI

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

use api::ProviderKind;
use commands::slash_command_specs;
use runtime::{
    PermissionMode, PermissionPromptDecision, PermissionRequest, Session as RuntimeSession,
    TokenUsage,
};

use super::events::{PermissionRequestEvent, StreamEvent, TurnFinishedEvent};
use crate::render::ColorTheme;

/// Application mode - determines what's currently displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal chat mode - message list + input
    Normal,
    /// Command palette is open (Ctrl+K)
    CommandPalette,
    /// Model selection dialog (Ctrl+O)
    ModelSelect,
    /// Session switcher
    SessionSelect,
    /// Help overlay
    Help,
    /// Confirmation dialog (e.g., for tool execution)
    Confirm,
}

/// Input mode for the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode (vim-style, for navigation)
    #[default]
    Normal,
    /// Insert mode (typing)
    Insert,
    /// Visual mode (selection)
    Visual,
}

/// A message in the conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Instant,
    pub tool_use: Option<ToolUseInfo>,
    pub is_streaming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ToolUseInfo {
    pub name: String,
    pub status: ToolStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// Confirmation dialog state
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub selected: bool, // true = confirm, false = cancel
}

/// Session info for session list
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub path: PathBuf,
    pub message_count: usize,
    pub modified: std::time::SystemTime,
}

/// Main application state
pub struct App {
    // Core state
    pub mode: AppMode,
    pub input_mode: InputMode,
    pub should_quit: bool,

    // Model and settings
    pub model: String,
    pub permission_mode: PermissionMode,
    pub provider_override: Option<ProviderKind>,

    // Messages
    pub messages: VecDeque<Message>,
    pub message_scroll: usize,
    pub selected_message: Option<usize>,

    // Input
    pub input: String,
    pub input_cursor: usize,
    pub input_history: Vec<String>,
    pub input_history_index: Option<usize>,

    // Command palette
    pub command_input: String,
    pub command_cursor: usize,
    pub command_results: Vec<String>,
    pub command_selected: usize,

    // Model selection
    pub available_models: Vec<String>,
    pub model_selected: usize,

    // Session management
    pub session_id: String,
    pub session_path: PathBuf,
    pub sessions: Vec<SessionInfo>,
    pub session_selected: usize,

    // Confirmation dialog
    pub confirm_dialog: Option<ConfirmDialog>,

    // Usage tracking
    pub usage: TokenUsage,
    pub cumulative_usage: TokenUsage,
    pub turn_count: u32,
    pub estimated_context_tokens: usize,

    // UI state
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub theme: ColorTheme,

    // Status
    pub status_message: Option<(String, Instant)>,
    pub is_loading: bool,
    pub loading_phase: String,

    // Git info
    pub git_branch: Option<String>,

    // Working directory
    pub cwd: PathBuf,

    // Event sender for runtime callbacks
    pub event_tx: Option<mpsc::Sender<super::events::Event>>,

    // Current pending tool calls (id -> name)
    pub pending_tools: HashMap<String, String>,
    pub pending_tool_messages: HashMap<String, usize>,

    // Session-scoped permission allow-list (tool names)
    pub permission_allowlist: Option<Arc<Mutex<HashSet<String>>>>,

    // Pending slash command to execute in runtime layer
    pub pending_slash_command: Option<String>,

    // Runtime turn status
    pub pending_turn_input: Option<String>,
    pub awaiting_permission: bool,
    pub pending_permission: Option<PermissionRequest>,
    pub permission_response_tx: Option<mpsc::Sender<PermissionPromptDecision>>,
}

impl App {
    pub fn new(
        model: String,
        permission_mode: PermissionMode,
        provider_override: Option<ProviderKind>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let git_branch = get_git_branch();
        let session_id = generate_session_id();
        let session_path = get_session_path(&session_id)?;

        let (terminal_width, terminal_height) = crossterm::terminal::size().unwrap_or((80, 24));

        Ok(Self {
            mode: AppMode::Normal,
            input_mode: InputMode::Insert, // Start in insert mode for immediate typing
            should_quit: false,

            model,
            permission_mode,
            provider_override,

            messages: VecDeque::new(),
            message_scroll: 0,
            selected_message: None,

            input: String::new(),
            input_cursor: 0,
            input_history: Vec::new(),
            input_history_index: None,

            command_input: String::new(),
            command_cursor: 0,
            command_results: get_all_commands(),
            command_selected: 0,

            available_models: vec![
                "claude-sonnet-4".to_string(),
                "claude-opus-4".to_string(),
                "claude-haiku-4".to_string(),
            ],
            model_selected: 0,

            session_id,
            session_path,
            sessions: Vec::new(),
            session_selected: 0,

            confirm_dialog: None,

            usage: TokenUsage::default(),
            cumulative_usage: TokenUsage::default(),
            turn_count: 0,
            estimated_context_tokens: 0,

            terminal_width,
            terminal_height,
            theme: ColorTheme::default(),

            status_message: None,
            is_loading: false,
            loading_phase: String::new(),

            git_branch,
            cwd,

            event_tx: None,
            pending_tools: HashMap::new(),
            pending_tool_messages: HashMap::new(),
            permission_allowlist: None,
            pending_slash_command: None,
            pending_turn_input: None,
            awaiting_permission: false,
            pending_permission: None,
            permission_response_tx: None,
        })
    }

    /// Set the event sender for streaming events
    pub fn set_event_sender(&mut self, tx: mpsc::Sender<super::events::Event>) {
        self.event_tx = Some(tx);
    }

    pub fn set_permission_allowlist(&mut self, allowlist: Arc<Mutex<HashSet<String>>>) {
        self.permission_allowlist = Some(allowlist);
    }

    pub fn load_sessions(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let sessions_dir = PathBuf::from(home).join(".claw").join("sessions");
        if !sessions_dir.exists() {
            self.sessions.clear();
            self.session_selected = 0;
            return Ok(());
        }

        let mut sessions = Vec::new();
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
            let metadata = entry.metadata()?;
            let modified = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string();
            let message_count = RuntimeSession::load_from_path(&path)
                .map(|session| session.messages.len())
                .unwrap_or(0);
            sessions.push(SessionInfo {
                id,
                path,
                message_count,
                modified,
            });
        }

        sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
        self.sessions = sessions;
        self.session_selected = 0;
        Ok(())
    }

    /// Handle tick event (called periodically for animations, etc.)
    pub fn on_tick(&mut self) {
        // Clear expired status messages
        if let Some((_, time)) = &self.status_message {
            if time.elapsed().as_secs() > 5 {
                self.status_message = None;
            }
        }
    }

    /// Handle key events, returns true if app should quit
    pub fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Global shortcuts that work in any mode
        match (key.modifiers, key.code) {
            // Quit
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.is_loading {
                    // Cancel current operation
                    self.is_loading = false;
                    self.set_status("Cancelled");
                } else if self.mode != AppMode::Normal {
                    // Close overlay
                    self.mode = AppMode::Normal;
                } else {
                    // Quit
                    self.should_quit = true;
                    return Ok(true);
                }
            }
            // Command palette
            (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
                self.open_command_palette();
            }
            // Model select
            (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
                self.open_model_select();
            }
            // Help
            (KeyModifiers::CONTROL, KeyCode::Char('?')) | (_, KeyCode::F(1)) => {
                self.mode = AppMode::Help;
            }
            // Session picker
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                if let Err(error) = self.load_sessions() {
                    self.set_status(&format!("Failed to load sessions: {error}"));
                } else {
                    self.mode = AppMode::SessionSelect;
                }
            }
            _ => {
                // Mode-specific handling
                match self.mode {
                    AppMode::Normal => self.handle_normal_mode_key(key)?,
                    AppMode::CommandPalette => self.handle_command_palette_key(key)?,
                    AppMode::ModelSelect => self.handle_model_select_key(key)?,
                    AppMode::SessionSelect => self.handle_session_select_key(key)?,
                    AppMode::Help => self.handle_help_key(key)?,
                    AppMode::Confirm => self.handle_confirm_key(key)?,
                }
            }
        }

        Ok(self.should_quit)
    }

    fn handle_normal_mode_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match self.input_mode {
            InputMode::Normal => {
                match key.code {
                    // Enter insert mode
                    KeyCode::Char('i') => self.input_mode = InputMode::Insert,
                    KeyCode::Char('a') => {
                        self.input_mode = InputMode::Insert;
                        self.move_cursor_right();
                    }
                    KeyCode::Char('A') => {
                        self.input_mode = InputMode::Insert;
                        self.input_cursor = self.input.len();
                    }
                    KeyCode::Char('I') => {
                        self.input_mode = InputMode::Insert;
                        self.input_cursor = 0;
                    }
                    // Navigation
                    KeyCode::Char('h') | KeyCode::Left => self.move_cursor_left(),
                    KeyCode::Char('l') | KeyCode::Right => self.move_cursor_right(),
                    KeyCode::Char('j') | KeyCode::Down => self.scroll_messages_down(),
                    KeyCode::Char('k') | KeyCode::Up => self.scroll_messages_up(),
                    KeyCode::Char('g') => self.scroll_to_top(),
                    KeyCode::Char('G') => self.scroll_to_bottom(),
                    // Delete
                    KeyCode::Char('x') => self.delete_char_at_cursor(),
                    KeyCode::Char('d') => {
                        // dd to clear line - simplified for now
                        self.input.clear();
                        self.input_cursor = 0;
                    }
                    // Quit
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                    }
                    _ => {}
                }
            }
            InputMode::Insert => {
                match (key.modifiers, key.code) {
                    // Exit insert mode
                    (_, KeyCode::Esc) => {
                        self.input_mode = InputMode::Normal;
                        if self.input_cursor > 0 {
                            self.input_cursor -= 1;
                        }
                    }
                    // Submit input
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        self.submit_input()?;
                    }
                    // Shift+Enter for newline
                    (KeyModifiers::SHIFT, KeyCode::Enter) => {
                        self.insert_char('\n');
                    }
                    // Backspace
                    (_, KeyCode::Backspace) => {
                        self.delete_char_before_cursor();
                    }
                    // Delete
                    (_, KeyCode::Delete) => {
                        self.delete_char_at_cursor();
                    }
                    // Navigation
                    (_, KeyCode::Left) => self.move_cursor_left(),
                    (_, KeyCode::Right) => self.move_cursor_right(),
                    (_, KeyCode::Up) => self.history_prev(),
                    (_, KeyCode::Down) => self.history_next(),
                    (_, KeyCode::Home) => self.input_cursor = 0,
                    (_, KeyCode::End) => self.input_cursor = self.input.len(),
                    // Regular character input
                    (_, KeyCode::Char(c)) => {
                        self.insert_char(c);
                    }
                    (_, KeyCode::Tab) => {
                        // Tab completion could go here
                        self.insert_char('\t');
                    }
                    _ => {}
                }
            }
            InputMode::Visual => {
                // Visual mode not fully implemented yet
                if key.code == crossterm::event::KeyCode::Esc {
                    self.input_mode = InputMode::Normal;
                }
            }
        }

        Ok(())
    }

    fn handle_command_palette_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
                self.command_input.clear();
                self.command_cursor = 0;
            }
            KeyCode::Enter => {
                self.execute_selected_command()?;
                self.mode = AppMode::Normal;
                self.command_input.clear();
                self.command_cursor = 0;
            }
            KeyCode::Up => {
                if self.command_selected > 0 {
                    self.command_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.command_selected < self.command_results.len().saturating_sub(1) {
                    self.command_selected += 1;
                }
            }
            KeyCode::Backspace => {
                if self.command_cursor > 0 {
                    self.command_input.remove(self.command_cursor - 1);
                    self.command_cursor -= 1;
                    self.filter_commands();
                }
            }
            KeyCode::Char(c) => {
                self.command_input.insert(self.command_cursor, c);
                self.command_cursor += 1;
                self.filter_commands();
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_model_select_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                if let Some(model) = self.available_models.get(self.model_selected) {
                    self.model = model.clone();
                    self.set_status(&format!("Model changed to {}", self.model));
                }
                self.mode = AppMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.model_selected > 0 {
                    self.model_selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.model_selected < self.available_models.len().saturating_sub(1) {
                    self.model_selected += 1;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_session_select_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                // Load selected session
                if let Some(session) = self.sessions.get(self.session_selected) {
                    self.pending_slash_command = Some(format!("/session switch {}", session.id));
                    self.set_status(&format!("Switching to session {}...", session.id));
                }
                self.mode = AppMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.session_selected > 0 {
                    self.session_selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.session_selected < self.sessions.len().saturating_sub(1) {
                    self.session_selected += 1;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_help_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::event::KeyCode;

        if let Some(dialog) = &mut self.confirm_dialog {
            match key.code {
                KeyCode::Esc => {
                    if let Some(tx) = self.permission_response_tx.take() {
                        let reason = self.pending_permission.as_ref().map_or_else(
                            || "tool denied by user".to_string(),
                            |request| {
                                format!(
                                    "tool '{}' denied by user approval prompt",
                                    request.tool_name
                                )
                            },
                        );
                        let _ = tx.send(PermissionPromptDecision::Deny { reason });
                    }
                    self.awaiting_permission = false;
                    self.pending_permission = None;
                    self.confirm_dialog = None;
                    self.mode = AppMode::Normal;
                }
                KeyCode::Enter => {
                    let confirmed = dialog.selected;
                    self.confirm_dialog = None;
                    self.mode = AppMode::Normal;
                    if confirmed {
                        if let Some(tx) = self.permission_response_tx.take() {
                            let _ = tx.send(PermissionPromptDecision::Allow);
                        }
                        self.awaiting_permission = false;
                        self.pending_permission = None;
                        self.set_status("Confirmed");
                    } else {
                        if let Some(tx) = self.permission_response_tx.take() {
                            let reason = self.pending_permission.as_ref().map_or_else(
                                || "tool denied by user".to_string(),
                                |request| {
                                    format!(
                                        "tool '{}' denied by user approval prompt",
                                        request.tool_name
                                    )
                                },
                            );
                            let _ = tx.send(PermissionPromptDecision::Deny { reason });
                        }
                        self.awaiting_permission = false;
                        self.pending_permission = None;
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    dialog.selected = true;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    dialog.selected = false;
                }
                KeyCode::Tab => {
                    dialog.selected = !dialog.selected;
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if let Some(tx) = self.permission_response_tx.take() {
                        let _ = tx.send(PermissionPromptDecision::Allow);
                    }
                    if let Some(allowlist) = &self.permission_allowlist {
                        if let Some(request) = &self.pending_permission {
                            if let Ok(mut set) = allowlist.lock() {
                                set.insert(request.tool_name.clone());
                            }
                        }
                    }
                    self.awaiting_permission = false;
                    self.pending_permission = None;
                    self.confirm_dialog = None;
                    self.mode = AppMode::Normal;
                    self.set_status("Approved for this session");
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(tx) = self.permission_response_tx.take() {
                        let _ = tx.send(PermissionPromptDecision::Allow);
                    }
                    self.awaiting_permission = false;
                    self.pending_permission = None;
                    self.confirm_dialog = None;
                    self.mode = AppMode::Normal;
                    self.set_status("Confirmed");
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    if let Some(tx) = self.permission_response_tx.take() {
                        let reason = self.pending_permission.as_ref().map_or_else(
                            || "tool denied by user".to_string(),
                            |request| {
                                format!(
                                    "tool '{}' denied by user approval prompt",
                                    request.tool_name
                                )
                            },
                        );
                        let _ = tx.send(PermissionPromptDecision::Deny { reason });
                    }
                    self.awaiting_permission = false;
                    self.pending_permission = None;
                    self.confirm_dialog = None;
                    self.mode = AppMode::Normal;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) {
        // Mouse handling could go here
        // For now, we'll focus on keyboard navigation
    }

    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    // Input manipulation helpers
    fn insert_char(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    fn delete_char_before_cursor(&mut self) {
        if self.input_cursor > 0 {
            let prev_char_boundary = self.input[..self.input_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev_char_boundary);
            self.input_cursor = prev_char_boundary;
        }
    }

    fn delete_char_at_cursor(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input.remove(self.input_cursor);
        }
    }

    fn move_cursor_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input[..self.input_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    fn move_cursor_right(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input_cursor = self.input[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input.len());
        }
    }

    fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        let new_index = match self.input_history_index {
            Some(i) if i > 0 => Some(i - 1),
            Some(i) => Some(i),
            None => Some(self.input_history.len() - 1),
        };

        if let Some(i) = new_index {
            self.input_history_index = Some(i);
            self.input = self.input_history[i].clone();
            self.input_cursor = self.input.len();
        }
    }

    fn history_next(&mut self) {
        if let Some(i) = self.input_history_index {
            if i < self.input_history.len() - 1 {
                self.input_history_index = Some(i + 1);
                self.input = self.input_history[i + 1].clone();
                self.input_cursor = self.input.len();
            } else {
                self.input_history_index = None;
                self.input.clear();
                self.input_cursor = 0;
            }
        }
    }

    fn scroll_messages_up(&mut self) {
        if self.message_scroll > 0 {
            self.message_scroll -= 1;
        }
    }

    fn scroll_messages_down(&mut self) {
        let max_scroll = self.messages.len().saturating_sub(1);
        if self.message_scroll < max_scroll {
            self.message_scroll += 1;
        }
    }

    fn scroll_to_top(&mut self) {
        self.message_scroll = 0;
    }

    fn scroll_to_bottom(&mut self) {
        self.message_scroll = self.messages.len().saturating_sub(1);
    }

    pub fn scroll_to_bottom_for_system(&mut self) {
        self.scroll_to_bottom();
    }

    fn submit_input(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return Ok(());
        }

        // Check for slash commands - defer execution to runtime layer
        if input.starts_with('/') {
            self.pending_slash_command = Some(input);
            self.input.clear();
            self.input_cursor = 0;
            return Ok(());
        }

        // Add to history
        self.input_history.push(input.clone());
        self.input_history_index = None;

        // Add user message to display
        self.messages.push_back(Message {
            role: MessageRole::User,
            content: input.clone(),
            timestamp: Instant::now(),
            tool_use: None,
            is_streaming: false,
        });

        // Clear input
        self.input.clear();
        self.input_cursor = 0;

        // Start loading and streaming
        self.is_loading = true;
        self.loading_phase = "Thinking...".to_string();
        self.pending_turn_input = Some(input);

        // Start streaming assistant message
        self.start_streaming_message();

        // Scroll to show latest
        self.scroll_to_bottom();

        Ok(())
    }

    /// Handle a streaming event from the API
    pub fn handle_stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta(text) => {
                self.loading_phase = "Writing...".to_string();
                self.update_streaming_message(&text);
            }
            StreamEvent::ToolUseStart { id, name } => {
                self.loading_phase = format!("Running {}...", name);
                self.pending_tools.insert(id.clone(), name.clone());
                self.messages.push_back(Message {
                    role: MessageRole::Tool,
                    content: String::new(),
                    timestamp: Instant::now(),
                    tool_use: Some(ToolUseInfo {
                        name,
                        status: ToolStatus::Running,
                        detail: None,
                    }),
                    is_streaming: false,
                });
                let idx = self.messages.len().saturating_sub(1);
                self.pending_tool_messages.insert(id, idx);
                self.scroll_to_bottom();
            }
            StreamEvent::ToolUseInput { id, input: _input } => {
                if let Some(name) = self.pending_tools.get(&id) {
                    // Update status to show tool is running
                    self.loading_phase = format!("Running {}...", name);
                }
            }
            StreamEvent::Usage(usage) => {
                // Update usage tracking
                self.usage = usage;
                self.cumulative_usage.input_tokens += usage.input_tokens;
                self.cumulative_usage.output_tokens += usage.output_tokens;
                self.cumulative_usage.cache_creation_input_tokens +=
                    usage.cache_creation_input_tokens;
                self.cumulative_usage.cache_read_input_tokens += usage.cache_read_input_tokens;
            }
            StreamEvent::Done => {
                self.pending_tools.clear();
                for (_, idx) in self.pending_tool_messages.drain() {
                    if let Some(message) = self.messages.get_mut(idx) {
                        if let Some(tool) = &mut message.tool_use {
                            tool.status = ToolStatus::Success;
                            if tool.detail.is_none() {
                                tool.detail = Some("completed".to_string());
                            }
                        }
                    }
                }
                self.loading_phase.clear();
                self.finish_streaming();
            }
            StreamEvent::Error(error) => {
                self.pending_tools.clear();
                for (_, idx) in self.pending_tool_messages.drain() {
                    if let Some(message) = self.messages.get_mut(idx) {
                        if let Some(tool) = &mut message.tool_use {
                            tool.status = ToolStatus::Error;
                            tool.detail = Some(error.clone());
                        }
                    }
                }
                self.loading_phase.clear();
                self.finish_streaming();
                self.set_status(&format!("Error: {}", error));
            }
        }
    }

    pub fn handle_permission_request(&mut self, event: PermissionRequestEvent) {
        self.awaiting_permission = true;
        self.pending_permission = Some(event.request.clone());
        self.permission_response_tx = Some(event.response_tx);
        self.mode = AppMode::Confirm;
        self.confirm_dialog = Some(ConfirmDialog {
            title: "Permission required".to_string(),
            message: format!(
                "Tool: {}\nCurrent: {}\nRequired: {}\nInput: {}\n\n[y] allow once  [a] always for session  [n] deny",
                event.request.tool_name,
                event.request.current_mode.as_str(),
                event.request.required_mode.as_str(),
                event.request.input
            ),
            confirm_label: "Allow".to_string(),
            cancel_label: "Deny".to_string(),
            selected: true,
        });
    }

    pub fn handle_turn_finished(&mut self, event: TurnFinishedEvent) {
        self.turn_count = event.turns;
        self.usage = event.latest_usage;
        self.cumulative_usage = event.cumulative_usage;
        self.estimated_context_tokens = event.estimated_tokens;
        self.pending_turn_input = None;

        if let Some(last) = self.messages.back_mut() {
            if last.role == MessageRole::Assistant && !event.final_text.is_empty() {
                last.content = event.final_text;
            }
        }
    }

    fn open_command_palette(&mut self) {
        self.mode = AppMode::CommandPalette;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_results = get_all_commands();
        self.command_selected = 0;
    }

    fn open_model_select(&mut self) {
        self.mode = AppMode::ModelSelect;
        // Find current model in list
        self.model_selected = self
            .available_models
            .iter()
            .position(|m| m == &self.model)
            .unwrap_or(0);
    }

    fn filter_commands(&mut self) {
        let query = self.command_input.to_lowercase();
        self.command_results = get_all_commands()
            .into_iter()
            .filter(|cmd| cmd.to_lowercase().contains(&query))
            .collect();
        self.command_selected = 0;
    }

    fn execute_selected_command(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(command) = self.command_results.get(self.command_selected) {
            let command = format!("/{}", command.split_whitespace().next().unwrap_or(""));
            self.pending_slash_command = Some(command);
            self.input.clear();
            self.input_cursor = 0;
        }
        Ok(())
    }

    pub fn set_status(&mut self, message: &str) {
        self.status_message = Some((message.to_string(), Instant::now()));
    }

    /// Update the last message (for streaming)
    pub fn update_streaming_message(&mut self, content: &str) {
        if let Some(last) = self.messages.back_mut() {
            if last.role == MessageRole::Assistant && last.is_streaming {
                last.content.push_str(content);
            }
        }
    }

    /// Start a streaming assistant message
    pub fn start_streaming_message(&mut self) {
        self.messages.push_back(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            timestamp: Instant::now(),
            tool_use: None,
            is_streaming: true,
        });
        self.scroll_to_bottom();
    }

    /// Finish streaming
    pub fn finish_streaming(&mut self) {
        if let Some(last) = self.messages.back_mut() {
            last.is_streaming = false;
        }
        self.is_loading = false;
    }
}

// Helper functions

fn get_git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD")
}

fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("session-{:x}", timestamp)
}

fn get_session_path(session_id: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let sessions_dir = PathBuf::from(home).join(".claw").join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;
    Ok(sessions_dir.join(format!("{}.json", session_id)))
}

fn get_all_commands() -> Vec<String> {
    let mut commands = slash_command_specs()
        .iter()
        .map(|spec| {
            let command = spec
                .argument_hint
                .map(|hint| format!("{} {}", spec.name, hint))
                .unwrap_or_else(|| spec.name.to_string());
            format!("{} - {}", command, spec.summary)
        })
        .collect::<Vec<_>>();
    commands.sort();
    commands
}

#[cfg(test)]
mod tests {
    use super::get_all_commands;

    #[test]
    fn command_palette_uses_slash_specs() {
        let commands = get_all_commands();
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|entry| entry.starts_with("status - ")));
        assert!(commands
            .iter()
            .any(|entry| entry.starts_with("copy [last|code|all] - ")));
    }

    #[test]
    fn command_palette_entries_are_sorted() {
        let commands = get_all_commands();
        let mut sorted = commands.clone();
        sorted.sort();
        assert_eq!(commands, sorted);
    }
}
