#![allow(dead_code)] // Some components are for future use

use std::fmt::Write as FmtWrite;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossterm::cursor::{MoveToColumn, RestorePosition, SavePosition};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor, Stylize};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

/// Color theme for terminal rendering - OpenCode style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTheme {
    pub heading: Color,
    pub emphasis: Color,
    pub strong: Color,
    pub inline_code: Color,
    pub link: Color,
    pub quote: Color,
    pub table_border: Color,
    pub code_block_border: Color,
    pub spinner_active: Color,
    pub spinner_done: Color,
    pub spinner_failed: Color,
    pub prompt: Color,
    pub prompt_symbol: Color,
    pub dim: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub tool_border: Color,
    pub tool_name: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            heading: Color::Cyan,
            emphasis: Color::Magenta,
            strong: Color::Yellow,
            inline_code: Color::Green,
            link: Color::Blue,
            quote: Color::DarkGrey,
            table_border: Color::DarkCyan,
            code_block_border: Color::DarkGrey,
            spinner_active: Color::Rgb {
                r: 99,
                g: 102,
                b: 241,
            }, // Indigo
            spinner_done: Color::Rgb {
                r: 34,
                g: 197,
                b: 94,
            }, // Green
            spinner_failed: Color::Rgb {
                r: 239,
                g: 68,
                b: 68,
            }, // Red
            prompt: Color::White,
            prompt_symbol: Color::Rgb {
                r: 99,
                g: 102,
                b: 241,
            }, // Indigo
            dim: Color::DarkGrey,
            accent: Color::Rgb {
                r: 139,
                g: 92,
                b: 246,
            }, // Violet
            success: Color::Rgb {
                r: 34,
                g: 197,
                b: 94,
            }, // Green
            warning: Color::Rgb {
                r: 234,
                g: 179,
                b: 8,
            }, // Yellow
            error: Color::Rgb {
                r: 239,
                g: 68,
                b: 68,
            }, // Red
            info: Color::Rgb {
                r: 59,
                g: 130,
                b: 246,
            }, // Blue
            tool_border: Color::Rgb {
                r: 75,
                g: 85,
                b: 99,
            }, // Gray
            tool_name: Color::Rgb {
                r: 14,
                g: 165,
                b: 233,
            }, // Sky
        }
    }
}

impl ColorTheme {
    /// Claude Code inspired dark theme
    #[must_use]
    pub fn claude() -> Self {
        Self {
            heading: Color::Rgb {
                r: 236,
                g: 72,
                b: 153,
            }, // Pink
            emphasis: Color::Rgb {
                r: 168,
                g: 85,
                b: 247,
            }, // Purple
            strong: Color::Rgb {
                r: 251,
                g: 191,
                b: 36,
            }, // Amber
            inline_code: Color::Rgb {
                r: 52,
                g: 211,
                b: 153,
            }, // Emerald
            link: Color::Rgb {
                r: 59,
                g: 130,
                b: 246,
            }, // Blue
            quote: Color::Rgb {
                r: 107,
                g: 114,
                b: 128,
            }, // Gray
            table_border: Color::Rgb {
                r: 55,
                g: 65,
                b: 81,
            }, // Gray-700
            code_block_border: Color::Rgb {
                r: 55,
                g: 65,
                b: 81,
            },
            spinner_active: Color::Rgb {
                r: 236,
                g: 72,
                b: 153,
            }, // Pink (Claude brand)
            spinner_done: Color::Rgb {
                r: 34,
                g: 197,
                b: 94,
            }, // Green
            spinner_failed: Color::Rgb {
                r: 239,
                g: 68,
                b: 68,
            }, // Red
            prompt: Color::White,
            prompt_symbol: Color::Rgb {
                r: 236,
                g: 72,
                b: 153,
            }, // Pink
            dim: Color::Rgb {
                r: 107,
                g: 114,
                b: 128,
            }, // Gray
            accent: Color::Rgb {
                r: 236,
                g: 72,
                b: 153,
            }, // Pink
            success: Color::Rgb {
                r: 34,
                g: 197,
                b: 94,
            },
            warning: Color::Rgb {
                r: 234,
                g: 179,
                b: 8,
            },
            error: Color::Rgb {
                r: 239,
                g: 68,
                b: 68,
            },
            info: Color::Rgb {
                r: 59,
                g: 130,
                b: 246,
            },
            tool_border: Color::Rgb {
                r: 55,
                g: 65,
                b: 81,
            },
            tool_name: Color::Rgb {
                r: 236,
                g: 72,
                b: 153,
            },
        }
    }

    /// Monokai-inspired dark theme
    #[must_use]
    pub fn monokai() -> Self {
        Self {
            heading: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            }, // Monokai pink
            emphasis: Color::Rgb {
                r: 174,
                g: 129,
                b: 255,
            }, // Monokai purple
            strong: Color::Rgb {
                r: 230,
                g: 219,
                b: 116,
            }, // Monokai yellow
            inline_code: Color::Rgb {
                r: 166,
                g: 226,
                b: 46,
            }, // Monokai green
            link: Color::Rgb {
                r: 102,
                g: 217,
                b: 239,
            }, // Monokai cyan
            quote: Color::Rgb {
                r: 117,
                g: 113,
                b: 94,
            }, // Monokai comment
            table_border: Color::Rgb {
                r: 73,
                g: 72,
                b: 62,
            },
            code_block_border: Color::Rgb {
                r: 73,
                g: 72,
                b: 62,
            },
            spinner_active: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            },
            spinner_done: Color::Rgb {
                r: 166,
                g: 226,
                b: 46,
            },
            spinner_failed: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            },
            prompt: Color::Rgb {
                r: 248,
                g: 248,
                b: 242,
            }, // Monokai foreground
            prompt_symbol: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            },
            dim: Color::Rgb {
                r: 117,
                g: 113,
                b: 94,
            },
            accent: Color::Rgb {
                r: 102,
                g: 217,
                b: 239,
            },
            success: Color::Rgb {
                r: 166,
                g: 226,
                b: 46,
            },
            warning: Color::Rgb {
                r: 230,
                g: 219,
                b: 116,
            },
            error: Color::Rgb {
                r: 249,
                g: 38,
                b: 114,
            },
            info: Color::Rgb {
                r: 102,
                g: 217,
                b: 239,
            },
            tool_border: Color::Rgb {
                r: 73,
                g: 72,
                b: 62,
            },
            tool_name: Color::Rgb {
                r: 102,
                g: 217,
                b: 239,
            },
        }
    }

    /// Nord-inspired cold theme
    #[must_use]
    pub fn nord() -> Self {
        Self {
            heading: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            }, // Nord frost
            emphasis: Color::Rgb {
                r: 180,
                g: 142,
                b: 173,
            }, // Nord aurora purple
            strong: Color::Rgb {
                r: 235,
                g: 203,
                b: 139,
            }, // Nord aurora yellow
            inline_code: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            }, // Nord aurora green
            link: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            }, // Nord frost blue
            quote: Color::Rgb {
                r: 76,
                g: 86,
                b: 106,
            }, // Nord polar night
            table_border: Color::Rgb {
                r: 67,
                g: 76,
                b: 94,
            },
            code_block_border: Color::Rgb {
                r: 67,
                g: 76,
                b: 94,
            },
            spinner_active: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            spinner_done: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            },
            spinner_failed: Color::Rgb {
                r: 191,
                g: 97,
                b: 106,
            },
            prompt: Color::Rgb {
                r: 236,
                g: 239,
                b: 244,
            }, // Nord snow storm
            prompt_symbol: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            dim: Color::Rgb {
                r: 76,
                g: 86,
                b: 106,
            },
            accent: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
            success: Color::Rgb {
                r: 163,
                g: 190,
                b: 140,
            },
            warning: Color::Rgb {
                r: 235,
                g: 203,
                b: 139,
            },
            error: Color::Rgb {
                r: 191,
                g: 97,
                b: 106,
            },
            info: Color::Rgb {
                r: 129,
                g: 161,
                b: 193,
            },
            tool_border: Color::Rgb {
                r: 67,
                g: 76,
                b: 94,
            },
            tool_name: Color::Rgb {
                r: 136,
                g: 192,
                b: 208,
            },
        }
    }

    /// Light theme for bright terminals
    #[must_use]
    pub fn light() -> Self {
        Self {
            heading: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            }, // Blue
            emphasis: Color::Rgb {
                r: 128,
                g: 0,
                b: 128,
            }, // Purple
            strong: Color::Rgb {
                r: 153,
                g: 102,
                b: 0,
            }, // Brown/amber
            inline_code: Color::Rgb { r: 0, g: 128, b: 0 }, // Green
            link: Color::Rgb { r: 0, g: 0, b: 204 },        // Blue
            quote: Color::Rgb {
                r: 102,
                g: 102,
                b: 102,
            }, // Gray
            table_border: Color::Rgb {
                r: 180,
                g: 180,
                b: 180,
            },
            code_block_border: Color::Rgb {
                r: 200,
                g: 200,
                b: 200,
            },
            spinner_active: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            },
            spinner_done: Color::Rgb { r: 0, g: 128, b: 0 },
            spinner_failed: Color::Rgb { r: 204, g: 0, b: 0 },
            prompt: Color::Rgb {
                r: 30,
                g: 30,
                b: 30,
            }, // Near black
            prompt_symbol: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            },
            dim: Color::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
            accent: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            },
            success: Color::Rgb { r: 0, g: 128, b: 0 },
            warning: Color::Rgb {
                r: 204,
                g: 153,
                b: 0,
            },
            error: Color::Rgb { r: 204, g: 0, b: 0 },
            info: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            },
            tool_border: Color::Rgb {
                r: 180,
                g: 180,
                b: 180,
            },
            tool_name: Color::Rgb {
                r: 0,
                g: 102,
                b: 204,
            },
        }
    }

    /// Get a theme by name
    #[must_use]
    pub fn by_name(name: &str) -> Self {
        match name {
            "claude" => Self::claude(),
            "monokai" => Self::monokai(),
            "nord" => Self::nord(),
            "light" => Self::light(),
            _ => Self::default(),
        }
    }
}

/// Live token counter for real-time display during streaming
#[derive(Debug, Clone)]
pub struct TokenCounter {
    input_tokens: Arc<AtomicU64>,
    output_tokens: Arc<AtomicU64>,
    start_time: Instant,
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            input_tokens: Arc::new(AtomicU64::new(0)),
            output_tokens: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        }
    }

    pub fn set_input(&self, tokens: u64) {
        self.input_tokens.store(tokens, Ordering::Relaxed);
    }

    pub fn add_output(&self, tokens: u64) {
        self.output_tokens.fetch_add(tokens, Ordering::Relaxed);
    }

    #[must_use]
    pub fn input(&self) -> u64 {
        self.input_tokens.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn output(&self) -> u64 {
        self.output_tokens.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    #[must_use]
    pub fn format_compact(&self) -> String {
        let input = self.input();
        let output = self.output();
        if input == 0 && output == 0 {
            return String::new();
        }
        format!(
            "{}↓ {}↑",
            format_token_count(input),
            format_token_count(output)
        )
    }

    /// Calculate estimated cost in USD based on Claude Sonnet pricing
    /// Input: $3/MTok, Output: $15/MTok (as of 2024)
    #[must_use]
    pub fn estimate_cost(&self, model: &str) -> f64 {
        let input = self.input();
        let output = self.output();

        // Pricing per million tokens (MTok) in USD
        let (input_price, output_price) = get_model_pricing(model);

        let input_cost = (input as f64 / 1_000_000.0) * input_price;
        let output_cost = (output as f64 / 1_000_000.0) * output_price;

        input_cost + output_cost
    }

    /// Format cost as a compact dollar string
    #[must_use]
    pub fn format_cost(&self, model: &str) -> String {
        let cost = self.estimate_cost(model);
        if cost < 0.001 {
            return String::new();
        }
        if cost < 0.01 {
            format!("${:.3}", cost)
        } else if cost < 1.0 {
            format!("${:.2}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    /// Format compact with cost estimate
    #[must_use]
    pub fn format_with_cost(&self, model: &str) -> String {
        let tokens = self.format_compact();
        let cost = self.format_cost(model);

        if tokens.is_empty() {
            return String::new();
        }

        if cost.is_empty() {
            tokens
        } else {
            format!("{tokens} {cost}")
        }
    }
}

/// Get pricing per million tokens (input, output) for a model
fn get_model_pricing(model: &str) -> (f64, f64) {
    // Claude pricing as of late 2024/early 2025
    // Format: (input $/MTok, output $/MTok)
    if model.contains("opus") {
        (15.0, 75.0) // Claude Opus
    } else if model.contains("haiku") {
        (0.25, 1.25) // Claude Haiku
    } else {
        (3.0, 15.0) // Claude Sonnet (default)
    }
}

fn format_token_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Status bar at the bottom of the terminal
#[derive(Debug)]
pub struct StatusBar {
    model: String,
    session_id: String,
    token_counter: TokenCounter,
    context_usage: Option<ContextUsage>,
    theme: ColorTheme,
}

/// Context window usage information
#[derive(Debug, Clone, Copy)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
}

impl ContextUsage {
    #[must_use]
    pub fn new(used: u64, max: u64) -> Self {
        Self {
            used_tokens: used,
            max_tokens: max,
        }
    }

    #[must_use]
    pub fn percentage(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        (self.used_tokens as f64 / self.max_tokens as f64) * 100.0
    }

    /// Format as compact percentage with color coding
    #[must_use]
    pub fn format_compact(&self) -> String {
        let pct = self.percentage();
        let color = if pct >= 90.0 {
            "\x1b[31m" // Red - critical
        } else if pct >= 75.0 {
            "\x1b[33m" // Yellow - warning
        } else {
            "\x1b[32m" // Green - ok
        };
        format!("{}ctx:{:.0}%\x1b[0m", color, pct)
    }

    /// Format with bar visualization
    #[must_use]
    pub fn format_with_bar(&self, width: usize) -> String {
        let pct = self.percentage();
        let filled = ((pct / 100.0) * width as f64).round() as usize;
        let empty = width.saturating_sub(filled);

        let color = if pct >= 90.0 {
            "\x1b[31m"
        } else if pct >= 75.0 {
            "\x1b[33m"
        } else {
            "\x1b[32m"
        };

        format!(
            "{}[{}{}]\x1b[0m {:.0}%",
            color,
            "█".repeat(filled),
            "░".repeat(empty),
            pct
        )
    }
}

impl StatusBar {
    #[must_use]
    pub fn new(model: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            session_id: session_id.into(),
            token_counter: TokenCounter::new(),
            context_usage: None,
            theme: ColorTheme::default(),
        }
    }

    /// Set the context window usage
    pub fn set_context_usage(&mut self, usage: ContextUsage) {
        self.context_usage = Some(usage);
    }

    #[must_use]
    pub fn token_counter(&self) -> &TokenCounter {
        &self.token_counter
    }

    pub fn render(&self, out: &mut impl Write) -> io::Result<()> {
        let (width, _) = terminal::size().unwrap_or((80, 24));
        let width = width as usize;

        // Build status components
        let model_str = format!(" {} ", self.model);
        let tokens_with_cost = self.token_counter.format_with_cost(&self.model);
        let elapsed = format!("{:.1}s", self.token_counter.elapsed_secs());

        // Build right side with context usage if available
        let mut right_parts = Vec::new();

        if let Some(ctx) = &self.context_usage {
            right_parts.push(ctx.format_compact());
        }

        if !tokens_with_cost.is_empty() {
            right_parts.push(tokens_with_cost);
        }

        right_parts.push(elapsed);

        let right_side = right_parts.join(" | ");

        let padding = width.saturating_sub(model_str.len() + right_side.len() + 4);
        let padding_str = " ".repeat(padding);

        queue!(
            out,
            SavePosition,
            MoveToColumn(0),
            SetForegroundColor(self.theme.dim),
            Print("─".repeat(width)),
            Print("\n"),
            SetForegroundColor(self.theme.accent),
            Print(&model_str),
            ResetColor,
            SetForegroundColor(self.theme.dim),
            Print(&padding_str),
            Print(&right_side),
            Print(" "),
            ResetColor,
            RestorePosition
        )?;
        out.flush()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Spinner {
    frame_index: usize,
}

impl Spinner {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(
        &mut self,
        label: &str,
        theme: &ColorTheme,
        out: &mut impl Write,
    ) -> io::Result<()> {
        let frame = Self::FRAMES[self.frame_index % Self::FRAMES.len()];
        self.frame_index += 1;
        queue!(
            out,
            SavePosition,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(theme.spinner_active),
            Print(format!("{frame} {label}")),
            ResetColor,
            RestorePosition
        )?;
        out.flush()
    }

    pub fn finish(
        &mut self,
        label: &str,
        theme: &ColorTheme,
        out: &mut impl Write,
    ) -> io::Result<()> {
        self.frame_index = 0;
        execute!(
            out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(theme.spinner_done),
            Print(format!("✔ {label}\n")),
            ResetColor
        )?;
        out.flush()
    }

    pub fn fail(
        &mut self,
        label: &str,
        theme: &ColorTheme,
        out: &mut impl Write,
    ) -> io::Result<()> {
        self.frame_index = 0;
        execute!(
            out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(theme.spinner_failed),
            Print(format!("✘ {label}\n")),
            ResetColor
        )?;
        out.flush()
    }
}

/// Enhanced progress indicator with multiple phases - OpenCode style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Thinking,
    Generating,
    ToolUse,
    Reading,
    Writing,
    Searching,
    Executing,
}

impl ProgressPhase {
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::Thinking => "◐",
            Self::Generating => "✦",
            Self::ToolUse => "⚡",
            Self::Reading => "📖",
            Self::Writing => "✏️",
            Self::Searching => "🔍",
            Self::Executing => "▶",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Thinking => "Thinking",
            Self::Generating => "Generating",
            Self::ToolUse => "Using tool",
            Self::Reading => "Reading",
            Self::Writing => "Writing",
            Self::Searching => "Searching",
            Self::Executing => "Executing",
        }
    }
}

/// Progress indicator with phase tracking and token display
#[derive(Debug)]
pub struct ProgressIndicator {
    spinner: Spinner,
    phase: ProgressPhase,
    detail: Option<String>,
    token_counter: Option<TokenCounter>,
    start_time: Instant,
    theme: ColorTheme,
}

impl Default for ProgressIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressIndicator {
    const THINKING_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
    const TOOL_FRAMES: [&str; 4] = ["⚡", "⚡", "✦", "✦"];

    #[must_use]
    pub fn new() -> Self {
        Self {
            spinner: Spinner::new(),
            phase: ProgressPhase::Thinking,
            detail: None,
            token_counter: None,
            start_time: Instant::now(),
            theme: ColorTheme::default(),
        }
    }

    pub fn with_token_counter(mut self, counter: TokenCounter) -> Self {
        self.token_counter = Some(counter);
        self
    }

    pub fn set_phase(&mut self, phase: ProgressPhase) {
        self.phase = phase;
        self.detail = None;
    }

    pub fn set_phase_with_detail(&mut self, phase: ProgressPhase, detail: impl Into<String>) {
        self.phase = phase;
        self.detail = Some(detail.into());
    }

    pub fn tick(&mut self, out: &mut impl Write) -> io::Result<()> {
        let frame_idx = self.spinner.frame_index;
        self.spinner.frame_index += 1;

        let frame = match self.phase {
            ProgressPhase::Thinking | ProgressPhase::Generating => {
                Self::THINKING_FRAMES[frame_idx % Self::THINKING_FRAMES.len()]
            }
            ProgressPhase::ToolUse | ProgressPhase::Executing => {
                Self::TOOL_FRAMES[frame_idx % Self::TOOL_FRAMES.len()]
            }
            _ => self.phase.icon(),
        };

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let elapsed_str = format!("{elapsed:.1}s");

        let mut label = self.phase.label().to_string();
        if let Some(detail) = &self.detail {
            label = format!("{} {}", label, detail);
        }

        let token_info = self
            .token_counter
            .as_ref()
            .map(TokenCounter::format_compact)
            .filter(|s| !s.is_empty())
            .map(|s| format!(" | {s}"))
            .unwrap_or_default();

        queue!(
            out,
            SavePosition,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(self.theme.spinner_active),
            Print(frame),
            Print(" "),
            ResetColor,
            SetForegroundColor(self.theme.dim),
            Print(&label),
            Print(" "),
            SetForegroundColor(self.theme.dim),
            Print(&elapsed_str),
            Print(&token_info),
            ResetColor,
            RestorePosition
        )?;
        out.flush()
    }

    pub fn finish(&mut self, message: &str, out: &mut impl Write) -> io::Result<()> {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let elapsed_str = format!("{elapsed:.1}s");

        let token_info = self
            .token_counter
            .as_ref()
            .map(TokenCounter::format_compact)
            .filter(|s| !s.is_empty())
            .map(|s| format!(" | {s}"))
            .unwrap_or_default();

        execute!(
            out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(self.theme.spinner_done),
            Print("✔ "),
            ResetColor,
            Print(message),
            Print(" "),
            SetForegroundColor(self.theme.dim),
            Print(&elapsed_str),
            Print(&token_info),
            ResetColor,
            Print("\n")
        )?;
        out.flush()
    }

    pub fn fail(&mut self, message: &str, out: &mut impl Write) -> io::Result<()> {
        execute!(
            out,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(self.theme.spinner_failed),
            Print("✘ "),
            ResetColor,
            Print(message),
            ResetColor,
            Print("\n")
        )?;
        out.flush()
    }
}

/// Tool call box renderer - OpenCode style with borders
pub struct ToolCallRenderer {
    theme: ColorTheme,
}

impl Default for ToolCallRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: ColorTheme::default(),
        }
    }

    /// Render tool call start with nice box formatting
    pub fn render_start(&self, name: &str, detail: &str) -> String {
        let icon = self.tool_icon(name);
        let max_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        let content_width = max_width.saturating_sub(4);

        // Truncate detail if needed
        let detail_display = if detail.len() > content_width {
            format!("{}...", &detail[..content_width.saturating_sub(3)])
        } else {
            detail.to_string()
        };

        let top_border = format!(
            "\x1b[38;5;240m╭─ {}\x1b[1;36m{}\x1b[0;38;5;240m ─╮\x1b[0m",
            icon, name
        );
        let content = format!("\x1b[38;5;240m│\x1b[0m \x1b[2m{}\x1b[0m", detail_display);
        let name_len = name.chars().count() + icon.chars().count() + 4;
        let bottom = "─".repeat(name_len.min(content_width));
        let bottom_border = format!("\x1b[38;5;240m╰{}╯\x1b[0m", bottom);

        format!("{}\n{}\n{}", top_border, content, bottom_border)
    }

    /// Render tool result with success/error styling
    pub fn render_result(&self, name: &str, summary: &str, is_error: bool) -> String {
        let icon = if is_error { "✗" } else { "✓" };
        let color = if is_error { "1;31" } else { "1;32" };

        if summary.is_empty() {
            format!("\x1b[{}m{}\x1b[0m \x1b[2m{}\x1b[0m", color, icon, name)
        } else {
            let lines: Vec<&str> = summary.lines().take(3).collect();
            let display = lines.join("\n   ");
            format!(
                "\x1b[{}m{}\x1b[0m \x1b[2m{}\x1b[0m\n   \x1b[2m{}\x1b[0m",
                color, icon, name, display
            )
        }
    }

    fn tool_icon(&self, name: &str) -> &'static str {
        match name.to_lowercase().as_str() {
            "bash" => "$ ",
            "read" | "read_file" => "📄 ",
            "write" | "write_file" => "✏️ ",
            "edit" | "edit_file" => "📝 ",
            "glob" | "glob_search" => "🔎 ",
            "grep" | "grep_search" => "🔍 ",
            "web_search" | "websearch" => "🌐 ",
            "task" => "📋 ",
            "todowrite" => "☑️ ",
            "question" => "❓ ",
            "webfetch" => "🌐 ",
            "skill" => "🎯 ",
            _ => "⚡ ",
        }
    }
}

/// Inline diff viewer for file edits - shows colored additions/deletions
#[derive(Debug)]
pub struct DiffViewer {
    theme: ColorTheme,
    context_lines: usize,
}

impl Default for DiffViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffViewer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: ColorTheme::default(),
            context_lines: 3,
        }
    }

    /// Set number of context lines around changes
    pub fn set_context_lines(&mut self, lines: usize) {
        self.context_lines = lines;
    }

    /// Render a unified diff with colors
    #[must_use]
    pub fn render_unified_diff(&self, old_text: &str, new_text: &str, file_path: &str) -> String {
        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let mut output = String::new();

        // Header
        output.push_str(&format!("\x1b[1;38;5;243m─── {} ───\x1b[0m\n", file_path));

        // Compute diff using simple LCS-based algorithm
        let diff_ops = compute_diff(&old_lines, &new_lines);

        let mut old_idx = 0;
        let mut new_idx = 0;

        for op in diff_ops {
            match op {
                DiffOp::Equal(count) => {
                    // Show context lines
                    let start = old_idx;
                    let end = (old_idx + count).min(old_lines.len());

                    // Skip middle lines if too many
                    if count > self.context_lines * 2 {
                        // Show first context_lines
                        for i in start..(start + self.context_lines).min(end) {
                            output.push_str(&format!(
                                "\x1b[38;5;243m {:>4} │\x1b[0m {}\n",
                                i + 1,
                                old_lines[i]
                            ));
                        }
                        output.push_str(&format!(
                            "\x1b[38;5;243m  ··· │\x1b[0m \x1b[2m({} unchanged lines)\x1b[0m\n",
                            count - self.context_lines * 2
                        ));
                        // Show last context_lines
                        let skip_to = end.saturating_sub(self.context_lines);
                        for i in skip_to..end {
                            output.push_str(&format!(
                                "\x1b[38;5;243m {:>4} │\x1b[0m {}\n",
                                i + 1,
                                old_lines[i]
                            ));
                        }
                    } else {
                        for i in start..end {
                            output.push_str(&format!(
                                "\x1b[38;5;243m {:>4} │\x1b[0m {}\n",
                                i + 1,
                                old_lines[i]
                            ));
                        }
                    }
                    old_idx += count;
                    new_idx += count;
                }
                DiffOp::Delete(count) => {
                    for i in old_idx..(old_idx + count).min(old_lines.len()) {
                        output.push_str(&format!(
                            "\x1b[31m {:>4} │- {}\x1b[0m\n",
                            i + 1,
                            old_lines[i]
                        ));
                    }
                    old_idx += count;
                }
                DiffOp::Insert(count) => {
                    for i in new_idx..(new_idx + count).min(new_lines.len()) {
                        output.push_str(&format!(
                            "\x1b[32m {:>4} │+ {}\x1b[0m\n",
                            i + 1,
                            new_lines[i]
                        ));
                    }
                    new_idx += count;
                }
            }
        }

        output
    }

    /// Render a compact inline diff showing just the changed portion
    #[must_use]
    pub fn render_inline_diff(&self, old_text: &str, new_text: &str) -> String {
        let mut output = String::new();

        if old_text == new_text {
            return "\x1b[2m(no changes)\x1b[0m".to_string();
        }

        // Word-level diff for single-line changes
        if !old_text.contains('\n') && !new_text.contains('\n') {
            return self.render_word_diff(old_text, new_text);
        }

        // Line-level diff
        let old_lines: Vec<&str> = old_text.lines().collect();
        let new_lines: Vec<&str> = new_text.lines().collect();

        let diff_ops = compute_diff(&old_lines, &new_lines);

        let mut old_idx = 0;
        let mut new_idx = 0;

        for op in diff_ops {
            match op {
                DiffOp::Equal(count) => {
                    old_idx += count;
                    new_idx += count;
                }
                DiffOp::Delete(count) => {
                    for i in old_idx..(old_idx + count).min(old_lines.len()) {
                        output.push_str(&format!("\x1b[31m-{}\x1b[0m\n", old_lines[i]));
                    }
                    old_idx += count;
                }
                DiffOp::Insert(count) => {
                    for i in new_idx..(new_idx + count).min(new_lines.len()) {
                        output.push_str(&format!("\x1b[32m+{}\x1b[0m\n", new_lines[i]));
                    }
                    new_idx += count;
                }
            }
        }

        if output.is_empty() {
            "\x1b[2m(no visible changes)\x1b[0m".to_string()
        } else {
            output.trim_end().to_string()
        }
    }

    /// Render word-level diff for single-line changes
    fn render_word_diff(&self, old_text: &str, new_text: &str) -> String {
        let old_words: Vec<&str> = old_text.split_whitespace().collect();
        let new_words: Vec<&str> = new_text.split_whitespace().collect();

        let diff_ops = compute_diff(&old_words, &new_words);

        let mut output = String::new();
        let mut old_idx = 0;
        let mut new_idx = 0;

        for op in diff_ops {
            match op {
                DiffOp::Equal(count) => {
                    for i in old_idx..(old_idx + count).min(old_words.len()) {
                        if !output.is_empty() {
                            output.push(' ');
                        }
                        output.push_str(old_words[i]);
                    }
                    old_idx += count;
                    new_idx += count;
                }
                DiffOp::Delete(count) => {
                    for i in old_idx..(old_idx + count).min(old_words.len()) {
                        if !output.is_empty() {
                            output.push(' ');
                        }
                        output.push_str(&format!("\x1b[31;9m{}\x1b[0m", old_words[i]));
                    }
                    old_idx += count;
                }
                DiffOp::Insert(count) => {
                    for i in new_idx..(new_idx + count).min(new_words.len()) {
                        if !output.is_empty() {
                            output.push(' ');
                        }
                        output.push_str(&format!("\x1b[32;1m{}\x1b[0m", new_words[i]));
                    }
                    new_idx += count;
                }
            }
        }

        output
    }

    /// Render a summary of changes (for compact display)
    #[must_use]
    pub fn render_summary(&self, old_text: &str, new_text: &str) -> String {
        let old_lines = old_text.lines().count();
        let new_lines = new_text.lines().count();

        let added = new_lines.saturating_sub(old_lines);
        let removed = old_lines.saturating_sub(new_lines);

        if added == 0 && removed == 0 {
            if old_text == new_text {
                "\x1b[2m(no changes)\x1b[0m".to_string()
            } else {
                format!(
                    "\x1b[33m~{} lines modified\x1b[0m",
                    old_lines.max(new_lines)
                )
            }
        } else {
            let mut parts = Vec::new();
            if added > 0 {
                parts.push(format!("\x1b[32m+{}\x1b[0m", added));
            }
            if removed > 0 {
                parts.push(format!("\x1b[31m-{}\x1b[0m", removed));
            }
            parts.join(" ")
        }
    }
}

/// Diff operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffOp {
    Equal(usize),
    Delete(usize),
    Insert(usize),
}

/// Compute diff operations between two sequences using a simple algorithm
fn compute_diff<T: PartialEq>(old: &[T], new: &[T]) -> Vec<DiffOp> {
    let mut ops = Vec::new();

    // Simple Myers-like diff algorithm
    let (mut i, mut j) = (0, 0);

    while i < old.len() || j < new.len() {
        // Find matching run
        let mut equal_count = 0;
        while i + equal_count < old.len()
            && j + equal_count < new.len()
            && old[i + equal_count] == new[j + equal_count]
        {
            equal_count += 1;
        }

        if equal_count > 0 {
            ops.push(DiffOp::Equal(equal_count));
            i += equal_count;
            j += equal_count;
            continue;
        }

        // Look ahead to find next match
        let mut del_count = 0;
        let mut ins_count = 0;

        // Check if we should delete from old
        if i < old.len() {
            // Look for this old item in remaining new items
            let found_in_new = (j..new.len()).any(|k| old[i] == new[k]);
            if !found_in_new || j >= new.len() {
                del_count = 1;
            }
        }

        // Check if we should insert from new
        if j < new.len() && del_count == 0 {
            let found_in_old = (i..old.len()).any(|k| new[j] == old[k]);
            if !found_in_old || i >= old.len() {
                ins_count = 1;
            }
        }

        // If neither, prefer delete then insert
        if del_count == 0 && ins_count == 0 {
            if i < old.len() {
                del_count = 1;
            } else if j < new.len() {
                ins_count = 1;
            }
        }

        if del_count > 0 {
            ops.push(DiffOp::Delete(del_count));
            i += del_count;
        }
        if ins_count > 0 {
            ops.push(DiffOp::Insert(ins_count));
            j += ins_count;
        }
    }

    // Merge consecutive operations of the same type
    merge_diff_ops(ops)
}

/// Merge consecutive diff operations of the same type
fn merge_diff_ops(ops: Vec<DiffOp>) -> Vec<DiffOp> {
    let mut merged = Vec::new();

    for op in ops {
        match (merged.last_mut(), op) {
            (Some(DiffOp::Equal(ref mut n)), DiffOp::Equal(m)) => *n += m,
            (Some(DiffOp::Delete(ref mut n)), DiffOp::Delete(m)) => *n += m,
            (Some(DiffOp::Insert(ref mut n)), DiffOp::Insert(m)) => *n += m,
            _ => merged.push(op),
        }
    }

    merged
}

/// Startup banner renderer - OpenCode style
pub struct BannerRenderer {
    theme: ColorTheme,
}

impl Default for BannerRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl BannerRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: ColorTheme::default(),
        }
    }

    /// Render compact startup banner
    pub fn render_compact(
        &self,
        name: &str,
        version: &str,
        model: &str,
        permissions: &str,
        directory: &str,
        session_id: &str,
    ) -> String {
        let width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        let line = "─".repeat(width.min(60));

        format!(
            "\x1b[1;38;5;99m{name}\x1b[0m \x1b[2mv{version}\x1b[0m\n\
             \x1b[2m{line}\x1b[0m\n\n\
             \x1b[2m  Model\x1b[0m        \x1b[1m{model}\x1b[0m\n\
             \x1b[2m  Permissions\x1b[0m  {permissions}\n\
             \x1b[2m  Directory\x1b[0m    {directory}\n\
             \x1b[2m  Session\x1b[0m      \x1b[2m{session_id}\x1b[0m\n\n\
             \x1b[2m  /help\x1b[0m commands  \x1b[2m·\x1b[0m  \x1b[2mShift+Enter\x1b[0m newline  \x1b[2m·\x1b[0m  \x1b[2mCtrl+C\x1b[0m cancel\n"
        )
    }

    /// Render full ASCII art banner  
    pub fn render_full(
        &self,
        model: &str,
        permissions: &str,
        directory: &str,
        session_id: &str,
    ) -> String {
        // Keep the existing CLAW ASCII art but make it cleaner
        format!(
            "\x1b[38;5;99m\
   ██████╗██╗      █████╗ ██╗    ██╗\n\
  ██╔════╝██║     ██╔══██╗██║    ██║\n\
  ██║     ██║     ███████║██║ █╗ ██║\n\
  ██║     ██║     ██╔══██║██║███╗██║\n\
  ╚██████╗███████╗██║  ██║╚███╔███╔╝\n\
   ╚═════╝╚══════╝╚═╝  ╚═╝ ╚══╝╚══╝\x1b[0m \x1b[38;5;208mCode\x1b[0m\n\n\
   \x1b[2mModel\x1b[0m        \x1b[1m{model}\x1b[0m\n\
   \x1b[2mPermissions\x1b[0m  {permissions}\n\
   \x1b[2mDirectory\x1b[0m    {directory}\n\
   \x1b[2mSession\x1b[0m      \x1b[2m{session_id}\x1b[0m\n\n\
   \x1b[2m/help\x1b[0m commands  \x1b[2m·\x1b[0m  \x1b[2mShift+Enter\x1b[0m newline  \x1b[2m·\x1b[0m  \x1b[2mCtrl+C\x1b[0m cancel\n"
        )
    }
}

/// Input prompt renderer - OpenCode style
pub struct PromptRenderer {
    theme: ColorTheme,
    git_branch: Option<String>,
}

impl Default for PromptRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: ColorTheme::default(),
            git_branch: None,
        }
    }

    /// Set the current git branch to display in the prompt
    pub fn set_git_branch(&mut self, branch: Option<String>) {
        self.git_branch = branch;
    }

    /// Render the main input prompt
    #[must_use]
    pub fn render(&self) -> String {
        match &self.git_branch {
            Some(branch) => format!("\x1b[38;5;243m{}\x1b[0m \x1b[38;5;99m❯\x1b[0m ", branch),
            None => format!("\x1b[38;5;99m❯\x1b[0m "),
        }
    }

    /// Render continuation prompt for multi-line input
    #[must_use]
    pub fn render_continuation(&self) -> String {
        format!("\x1b[2m·\x1b[0m ")
    }

    /// Render prompt with mode indicator
    #[must_use]
    pub fn render_with_mode(&self, mode: Option<&str>) -> String {
        match mode {
            Some(m) => format!("\x1b[2m[{}]\x1b[0m \x1b[38;5;99m❯\x1b[0m ", m),
            None => self.render(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListKind {
    Unordered,
    Ordered { next_index: u64 },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct TableState {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
}

impl TableState {
    fn push_cell(&mut self) {
        let cell = self.current_cell.trim().to_string();
        self.current_row.push(cell);
        self.current_cell.clear();
    }

    fn finish_row(&mut self) {
        if self.current_row.is_empty() {
            return;
        }
        let row = std::mem::take(&mut self.current_row);
        if self.in_head {
            self.headers = row;
        } else {
            self.rows.push(row);
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RenderState {
    emphasis: usize,
    strong: usize,
    heading_level: Option<u8>,
    quote: usize,
    list_stack: Vec<ListKind>,
    link_stack: Vec<LinkState>,
    table: Option<TableState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkState {
    destination: String,
    text: String,
}

impl RenderState {
    fn style_text(&self, text: &str, theme: &ColorTheme) -> String {
        let mut style = text.stylize();

        if matches!(self.heading_level, Some(1 | 2)) || self.strong > 0 {
            style = style.bold();
        }
        if self.emphasis > 0 {
            style = style.italic();
        }

        if let Some(level) = self.heading_level {
            style = match level {
                1 => style.with(theme.heading),
                2 => style.white(),
                3 => style.with(Color::Blue),
                _ => style.with(Color::Grey),
            };
        } else if self.strong > 0 {
            style = style.with(theme.strong);
        } else if self.emphasis > 0 {
            style = style.with(theme.emphasis);
        }

        if self.quote > 0 {
            style = style.with(theme.quote);
        }

        format!("{style}")
    }

    fn append_raw(&mut self, output: &mut String, text: &str) {
        if let Some(link) = self.link_stack.last_mut() {
            link.text.push_str(text);
        } else if let Some(table) = self.table.as_mut() {
            table.current_cell.push_str(text);
        } else {
            output.push_str(text);
        }
    }

    fn append_styled(&mut self, output: &mut String, text: &str, theme: &ColorTheme) {
        let styled = self.style_text(text, theme);
        self.append_raw(output, &styled);
    }
}

#[derive(Debug)]
pub struct TerminalRenderer {
    syntax_set: SyntaxSet,
    syntax_theme: Theme,
    color_theme: ColorTheme,
}

impl Default for TerminalRenderer {
    fn default() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let syntax_theme = ThemeSet::load_defaults()
            .themes
            .remove("base16-ocean.dark")
            .unwrap_or_default();
        Self {
            syntax_set,
            syntax_theme,
            color_theme: ColorTheme::default(),
        }
    }
}

impl TerminalRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn color_theme(&self) -> &ColorTheme {
        &self.color_theme
    }

    #[must_use]
    pub fn render_markdown(&self, markdown: &str) -> String {
        let mut output = String::new();
        let mut state = RenderState::default();
        let mut code_language = String::new();
        let mut code_buffer = String::new();
        let mut in_code_block = false;

        for event in Parser::new_ext(markdown, Options::all()) {
            self.render_event(
                event,
                &mut state,
                &mut output,
                &mut code_buffer,
                &mut code_language,
                &mut in_code_block,
            );
        }

        output.trim_end().to_string()
    }

    #[must_use]
    pub fn markdown_to_ansi(&self, markdown: &str) -> String {
        self.render_markdown(markdown)
    }

    #[allow(clippy::too_many_lines)]
    fn render_event(
        &self,
        event: Event<'_>,
        state: &mut RenderState,
        output: &mut String,
        code_buffer: &mut String,
        code_language: &mut String,
        in_code_block: &mut bool,
    ) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.start_heading(state, level as u8, output);
            }
            Event::End(TagEnd::Paragraph) => output.push_str("\n\n"),
            Event::Start(Tag::BlockQuote(..)) => self.start_quote(state, output),
            Event::End(TagEnd::BlockQuote(..)) => {
                state.quote = state.quote.saturating_sub(1);
                output.push('\n');
            }
            Event::End(TagEnd::Heading(..)) => {
                state.heading_level = None;
                output.push_str("\n\n");
            }
            Event::End(TagEnd::Item) | Event::SoftBreak | Event::HardBreak => {
                state.append_raw(output, "\n");
            }
            Event::Start(Tag::List(first_item)) => {
                let kind = match first_item {
                    Some(index) => ListKind::Ordered { next_index: index },
                    None => ListKind::Unordered,
                };
                state.list_stack.push(kind);
            }
            Event::End(TagEnd::List(..)) => {
                state.list_stack.pop();
                output.push('\n');
            }
            Event::Start(Tag::Item) => Self::start_item(state, output),
            Event::Start(Tag::CodeBlock(kind)) => {
                *in_code_block = true;
                *code_language = match kind {
                    CodeBlockKind::Indented => String::from("text"),
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                };
                code_buffer.clear();
                self.start_code_block(code_language, output);
            }
            Event::End(TagEnd::CodeBlock) => {
                self.finish_code_block(code_buffer, code_language, output);
                *in_code_block = false;
                code_language.clear();
                code_buffer.clear();
            }
            Event::Start(Tag::Emphasis) => state.emphasis += 1,
            Event::End(TagEnd::Emphasis) => state.emphasis = state.emphasis.saturating_sub(1),
            Event::Start(Tag::Strong) => state.strong += 1,
            Event::End(TagEnd::Strong) => state.strong = state.strong.saturating_sub(1),
            Event::Code(code) => {
                let rendered =
                    format!("{}", format!("`{code}`").with(self.color_theme.inline_code));
                state.append_raw(output, &rendered);
            }
            Event::Rule => output.push_str("---\n"),
            Event::Text(text) => {
                self.push_text(text.as_ref(), state, output, code_buffer, *in_code_block);
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                state.append_raw(output, &html);
            }
            Event::FootnoteReference(reference) => {
                state.append_raw(output, &format!("[{reference}]"));
            }
            Event::TaskListMarker(done) => {
                state.append_raw(output, if done { "[x] " } else { "[ ] " });
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => {
                state.append_raw(output, &math);
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                state.link_stack.push(LinkState {
                    destination: dest_url.to_string(),
                    text: String::new(),
                });
            }
            Event::End(TagEnd::Link) => {
                if let Some(link) = state.link_stack.pop() {
                    let label = if link.text.is_empty() {
                        link.destination.clone()
                    } else {
                        link.text
                    };
                    let rendered = format!(
                        "{}",
                        format!("[{label}]({})", link.destination)
                            .underlined()
                            .with(self.color_theme.link)
                    );
                    state.append_raw(output, &rendered);
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let rendered = format!(
                    "{}",
                    format!("[image:{dest_url}]").with(self.color_theme.link)
                );
                state.append_raw(output, &rendered);
            }
            Event::Start(Tag::Table(..)) => state.table = Some(TableState::default()),
            Event::End(TagEnd::Table) => {
                if let Some(table) = state.table.take() {
                    output.push_str(&self.render_table(&table));
                    output.push_str("\n\n");
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(table) = state.table.as_mut() {
                    table.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(table) = state.table.as_mut() {
                    table.finish_row();
                    table.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(table) = state.table.as_mut() {
                    table.current_row.clear();
                    table.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(table) = state.table.as_mut() {
                    table.finish_row();
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(table) = state.table.as_mut() {
                    table.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(table) = state.table.as_mut() {
                    table.push_cell();
                }
            }
            Event::Start(Tag::Paragraph | Tag::MetadataBlock(..) | _)
            | Event::End(TagEnd::Image | TagEnd::MetadataBlock(..) | _) => {}
        }
    }

    #[allow(clippy::unused_self)]
    fn start_heading(&self, state: &mut RenderState, level: u8, output: &mut String) {
        state.heading_level = Some(level);
        if !output.is_empty() {
            output.push('\n');
        }
    }

    fn start_quote(&self, state: &mut RenderState, output: &mut String) {
        state.quote += 1;
        let _ = write!(output, "{}", "│ ".with(self.color_theme.quote));
    }

    fn start_item(state: &mut RenderState, output: &mut String) {
        let depth = state.list_stack.len().saturating_sub(1);
        output.push_str(&"  ".repeat(depth));

        let marker = match state.list_stack.last_mut() {
            Some(ListKind::Ordered { next_index }) => {
                let value = *next_index;
                *next_index += 1;
                format!("{value}. ")
            }
            _ => "• ".to_string(),
        };
        output.push_str(&marker);
    }

    fn start_code_block(&self, code_language: &str, output: &mut String) {
        let label = if code_language.is_empty() {
            "code".to_string()
        } else {
            code_language.to_string()
        };
        let _ = writeln!(
            output,
            "{}",
            format!("╭─ {label}")
                .bold()
                .with(self.color_theme.code_block_border)
        );
    }

    fn finish_code_block(&self, code_buffer: &str, code_language: &str, output: &mut String) {
        output.push_str(&self.highlight_code(code_buffer, code_language));
        let _ = write!(
            output,
            "{}",
            "╰─".bold().with(self.color_theme.code_block_border)
        );
        output.push_str("\n\n");
    }

    fn push_text(
        &self,
        text: &str,
        state: &mut RenderState,
        output: &mut String,
        code_buffer: &mut String,
        in_code_block: bool,
    ) {
        if in_code_block {
            code_buffer.push_str(text);
        } else {
            state.append_styled(output, text, &self.color_theme);
        }
    }

    fn render_table(&self, table: &TableState) -> String {
        let mut rows = Vec::new();
        if !table.headers.is_empty() {
            rows.push(table.headers.clone());
        }
        rows.extend(table.rows.iter().cloned());

        if rows.is_empty() {
            return String::new();
        }

        let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths = (0..column_count)
            .map(|column| {
                rows.iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| visible_width(cell))
                    .max()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();

        let border = format!("{}", "│".with(self.color_theme.table_border));
        let separator = widths
            .iter()
            .map(|width| "─".repeat(*width + 2))
            .collect::<Vec<_>>()
            .join(&format!("{}", "┼".with(self.color_theme.table_border)));
        let separator = format!("{border}{separator}{border}");

        let mut output = String::new();
        if !table.headers.is_empty() {
            output.push_str(&self.render_table_row(&table.headers, &widths, true));
            output.push('\n');
            output.push_str(&separator);
            if !table.rows.is_empty() {
                output.push('\n');
            }
        }

        for (index, row) in table.rows.iter().enumerate() {
            output.push_str(&self.render_table_row(row, &widths, false));
            if index + 1 < table.rows.len() {
                output.push('\n');
            }
        }

        output
    }

    fn render_table_row(&self, row: &[String], widths: &[usize], is_header: bool) -> String {
        let border = format!("{}", "│".with(self.color_theme.table_border));
        let mut line = String::new();
        line.push_str(&border);

        for (index, width) in widths.iter().enumerate() {
            let cell = row.get(index).map_or("", String::as_str);
            line.push(' ');
            if is_header {
                let _ = write!(line, "{}", cell.bold().with(self.color_theme.heading));
            } else {
                line.push_str(cell);
            }
            let padding = width.saturating_sub(visible_width(cell));
            line.push_str(&" ".repeat(padding + 1));
            line.push_str(&border);
        }

        line
    }

    #[must_use]
    pub fn highlight_code(&self, code: &str, language: &str) -> String {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(language)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        let mut syntax_highlighter = HighlightLines::new(syntax, &self.syntax_theme);
        let mut colored_output = String::new();

        for line in LinesWithEndings::from(code) {
            match syntax_highlighter.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                    colored_output.push_str(&apply_code_block_background(&escaped));
                }
                Err(_) => colored_output.push_str(&apply_code_block_background(line)),
            }
        }

        colored_output
    }

    pub fn stream_markdown(&self, markdown: &str, out: &mut impl Write) -> io::Result<()> {
        let rendered_markdown = self.markdown_to_ansi(markdown);
        write!(out, "{rendered_markdown}")?;
        if !rendered_markdown.ends_with('\n') {
            writeln!(out)?;
        }
        out.flush()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarkdownStreamState {
    pending: String,
}

impl MarkdownStreamState {
    #[must_use]
    pub fn push(&mut self, renderer: &TerminalRenderer, delta: &str) -> Option<String> {
        self.pending.push_str(delta);
        let split = find_stream_safe_boundary(&self.pending)?;
        let ready = self.pending[..split].to_string();
        self.pending.drain(..split);
        Some(renderer.markdown_to_ansi(&ready))
    }

    #[must_use]
    pub fn flush(&mut self, renderer: &TerminalRenderer) -> Option<String> {
        if self.pending.trim().is_empty() {
            self.pending.clear();
            None
        } else {
            let pending = std::mem::take(&mut self.pending);
            Some(renderer.markdown_to_ansi(&pending))
        }
    }
}

fn apply_code_block_background(line: &str) -> String {
    let trimmed = line.trim_end_matches('\n');
    let trailing_newline = if trimmed.len() == line.len() {
        ""
    } else {
        "\n"
    };
    let with_background = trimmed.replace("\u{1b}[0m", "\u{1b}[0;48;5;236m");
    format!("\u{1b}[48;5;236m{with_background}\u{1b}[0m{trailing_newline}")
}

fn find_stream_safe_boundary(markdown: &str) -> Option<usize> {
    let mut in_fence = false;
    let mut last_boundary = None;

    for (offset, line) in markdown.split_inclusive('\n').scan(0usize, |cursor, line| {
        let start = *cursor;
        *cursor += line.len();
        Some((start, line))
    }) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            if !in_fence {
                last_boundary = Some(offset + line.len());
            }
            continue;
        }

        if in_fence {
            continue;
        }

        if trimmed.is_empty() {
            last_boundary = Some(offset + line.len());
        }
    }

    last_boundary
}

fn visible_width(input: &str) -> usize {
    strip_ansi(input).chars().count()
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            output.push(ch);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{strip_ansi, MarkdownStreamState, Spinner, TerminalRenderer};

    #[test]
    fn renders_markdown_with_styling_and_lists() {
        let terminal_renderer = TerminalRenderer::new();
        let markdown_output = terminal_renderer
            .render_markdown("# Heading\n\nThis is **bold** and *italic*.\n\n- item\n\n`code`");

        assert!(markdown_output.contains("Heading"));
        assert!(markdown_output.contains("• item"));
        assert!(markdown_output.contains("code"));
        assert!(markdown_output.contains('\u{1b}'));
    }

    #[test]
    fn renders_links_as_colored_markdown_labels() {
        let terminal_renderer = TerminalRenderer::new();
        let markdown_output =
            terminal_renderer.render_markdown("See [Claw](https://example.com/docs) now.");
        let plain_text = strip_ansi(&markdown_output);

        assert!(plain_text.contains("[Claw](https://example.com/docs)"));
        assert!(markdown_output.contains('\u{1b}'));
    }

    #[test]
    fn highlights_fenced_code_blocks() {
        let terminal_renderer = TerminalRenderer::new();
        let markdown_output =
            terminal_renderer.markdown_to_ansi("```rust\nfn hi() { println!(\"hi\"); }\n```");
        let plain_text = strip_ansi(&markdown_output);

        assert!(plain_text.contains("╭─ rust"));
        assert!(plain_text.contains("fn hi"));
        assert!(markdown_output.contains('\u{1b}'));
        assert!(markdown_output.contains("[48;5;236m"));
    }

    #[test]
    fn renders_ordered_and_nested_lists() {
        let terminal_renderer = TerminalRenderer::new();
        let markdown_output =
            terminal_renderer.render_markdown("1. first\n2. second\n   - nested\n   - child");
        let plain_text = strip_ansi(&markdown_output);

        assert!(plain_text.contains("1. first"));
        assert!(plain_text.contains("2. second"));
        assert!(plain_text.contains("  • nested"));
        assert!(plain_text.contains("  • child"));
    }

    #[test]
    fn renders_tables_with_alignment() {
        let terminal_renderer = TerminalRenderer::new();
        let markdown_output = terminal_renderer
            .render_markdown("| Name | Value |\n| ---- | ----- |\n| alpha | 1 |\n| beta | 22 |");
        let plain_text = strip_ansi(&markdown_output);
        let lines = plain_text.lines().collect::<Vec<_>>();

        assert_eq!(lines[0], "│ Name  │ Value │");
        assert_eq!(lines[1], "│───────┼───────│");
        assert_eq!(lines[2], "│ alpha │ 1     │");
        assert_eq!(lines[3], "│ beta  │ 22    │");
        assert!(markdown_output.contains('\u{1b}'));
    }

    #[test]
    fn streaming_state_waits_for_complete_blocks() {
        let renderer = TerminalRenderer::new();
        let mut state = MarkdownStreamState::default();

        assert_eq!(state.push(&renderer, "# Heading"), None);
        let flushed = state
            .push(&renderer, "\n\nParagraph\n\n")
            .expect("completed block");
        let plain_text = strip_ansi(&flushed);
        assert!(plain_text.contains("Heading"));
        assert!(plain_text.contains("Paragraph"));

        assert_eq!(state.push(&renderer, "```rust\nfn main() {}\n"), None);
        let code = state
            .push(&renderer, "```\n")
            .expect("closed code fence flushes");
        assert!(strip_ansi(&code).contains("fn main()"));
    }

    #[test]
    fn spinner_advances_frames() {
        let terminal_renderer = TerminalRenderer::new();
        let mut spinner = Spinner::new();
        let mut out = Vec::new();
        spinner
            .tick("Working", terminal_renderer.color_theme(), &mut out)
            .expect("tick succeeds");
        spinner
            .tick("Working", terminal_renderer.color_theme(), &mut out)
            .expect("tick succeeds");

        let output = String::from_utf8_lossy(&out);
        assert!(output.contains("Working"));
    }

    #[test]
    fn token_counter_formats_compact() {
        use super::TokenCounter;

        let counter = TokenCounter::new();
        assert!(counter.format_compact().is_empty());

        counter.set_input(1500);
        counter.add_output(500);
        let compact = counter.format_compact();
        assert!(compact.contains("1.5k"));
        assert!(compact.contains("500"));
    }

    #[test]
    fn progress_phase_has_icons_and_labels() {
        use super::ProgressPhase;

        assert!(!ProgressPhase::Thinking.icon().is_empty());
        assert!(!ProgressPhase::Thinking.label().is_empty());
        assert!(!ProgressPhase::ToolUse.icon().is_empty());
        assert!(!ProgressPhase::Writing.label().is_empty());
    }

    #[test]
    fn tool_call_renderer_creates_boxes() {
        use super::ToolCallRenderer;

        let renderer = ToolCallRenderer::new();
        let output = renderer.render_start("bash", "echo hello");

        assert!(output.contains("bash"));
        assert!(output.contains("echo hello"));
        assert!(output.contains("╭"));
        assert!(output.contains("╰"));
    }

    #[test]
    fn banner_renderer_includes_model_info() {
        use super::BannerRenderer;

        let renderer = BannerRenderer::new();
        let banner = renderer.render_compact(
            "Claw",
            "0.1.0",
            "claude-sonnet-4",
            "danger-full-access",
            "/home/user",
            "session-123",
        );

        assert!(banner.contains("claude-sonnet-4"));
        assert!(banner.contains("danger-full-access"));
        assert!(banner.contains("/home/user"));
    }

    #[test]
    fn prompt_renderer_creates_styled_prompt() {
        use super::PromptRenderer;

        let renderer = PromptRenderer::new();
        let prompt = renderer.render();

        // Should contain the prompt symbol
        assert!(prompt.contains("❯"));
    }
}
