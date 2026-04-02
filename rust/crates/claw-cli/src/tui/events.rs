//! Event handling for the TUI

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use runtime::{PermissionPromptDecision, PermissionRequest, TokenUsage};

/// Streaming events from the API
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta from assistant
    TextDelta(String),
    /// Tool use started
    ToolUseStart { id: String, name: String },
    /// Tool use input (accumulated)
    ToolUseInput { id: String, input: String },
    /// Usage information
    Usage(TokenUsage),
    /// Stream completed
    Done,
    /// Stream error
    Error(String),
}

#[derive(Debug)]
pub struct PermissionRequestEvent {
    pub request: PermissionRequest,
    pub response_tx: mpsc::Sender<PermissionPromptDecision>,
}

#[derive(Debug)]
pub struct TurnFinishedEvent {
    pub final_text: String,
    pub latest_usage: TokenUsage,
    pub cumulative_usage: TokenUsage,
    pub turns: u32,
    pub estimated_tokens: usize,
}

/// TUI events
#[derive(Debug)]
pub enum Event {
    /// Terminal tick (for animations, etc.)
    Tick,
    /// Key press
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Streaming event from API
    Stream(StreamEvent),
    /// Permission approval request from runtime
    PermissionRequest(PermissionRequestEvent),
    /// Turn finished from runtime
    TurnFinished(TurnFinishedEvent),
}

/// Handles terminal events
pub struct EventHandler {
    /// Event receiver
    rx: mpsc::Receiver<Event>,
    /// Event sender - used to inject streaming events
    tx: mpsc::Sender<Event>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate in milliseconds
    pub fn new(tick_rate_ms: u64) -> Self {
        let tick_rate = Duration::from_millis(tick_rate_ms);
        let (tx, rx) = mpsc::channel();
        let event_tx = tx.clone();

        thread::spawn(move || {
            loop {
                // Poll for events with timeout
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) => {
                            if event_tx.send(Event::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Mouse(mouse)) => {
                            if event_tx.send(Event::Mouse(mouse)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Resize(width, height)) => {
                            if event_tx.send(Event::Resize(width, height)).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {} // Ignore other events
                        Err(_) => break,
                    }
                } else {
                    // Timeout - send tick
                    if event_tx.send(Event::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        Self { rx, tx }
    }

    /// Get a clone of the sender for injecting events (e.g., from streaming API)
    pub fn sender(&self) -> mpsc::Sender<Event> {
        self.tx.clone()
    }

    /// Get the next event, blocking until one is available
    pub fn next(&mut self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Try to get the next event without blocking
    #[allow(dead_code)]
    pub fn try_next(&mut self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}
