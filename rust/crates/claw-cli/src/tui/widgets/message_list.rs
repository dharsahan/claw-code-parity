//! Message list widget for displaying conversation history

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};

use crate::tui::app::{Message, MessageRole, ToolStatus};

/// State for the message list widget
#[derive(Debug, Default)]
pub struct MessageListState {
    /// Current scroll offset
    pub offset: usize,
    /// Selected message index (if any)
    pub selected: Option<usize>,
}

/// A widget for displaying a list of messages
pub struct MessageList<'a> {
    messages: &'a [Message],
    block: Option<Block<'a>>,
    highlight_style: Style,
}

impl<'a> MessageList<'a> {
    pub fn new(messages: &'a [Message]) -> Self {
        Self {
            messages,
            block: None,
            highlight_style: Style::default().bg(Color::Rgb(40, 40, 60)),
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    #[allow(dead_code)]
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Convert a message to styled lines
    fn message_to_lines(msg: &Message, is_streaming: bool) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Role header
        let (role_text, role_style) = match msg.role {
            MessageRole::User => (
                "You",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            MessageRole::Assistant => (
                "Assistant",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            MessageRole::System => ("System", Style::default().fg(Color::Yellow)),
            MessageRole::Tool => ("Tool", Style::default().fg(Color::Magenta)),
        };

        let mut header_spans = vec![Span::styled(role_text.to_string(), role_style)];

        if is_streaming {
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled("●", Style::default().fg(Color::Green)));
        }

        lines.push(Line::from(header_spans));

        // Tool use info
        if let Some(tool) = &msg.tool_use {
            let (icon, color) = match tool.status {
                ToolStatus::Pending => ("◐", Color::Yellow),
                ToolStatus::Running => ("◐", Color::Cyan),
                ToolStatus::Success => ("✓", Color::Green),
                ToolStatus::Error => ("✗", Color::Red),
            };

            let mut tool_spans = vec![
                Span::styled(format!("  {} ", icon), Style::default().fg(color)),
                Span::styled(tool.name.clone(), Style::default().fg(Color::Cyan)),
            ];

            if let Some(detail) = &tool.detail {
                tool_spans.push(Span::styled(
                    format!(" {}", detail),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            lines.push(Line::from(tool_spans));
        }

        // Message content with indentation
        for line in msg.content.lines() {
            lines.push(Line::from(Span::raw(format!("  {}", line))));
        }

        // Blank line separator
        lines.push(Line::from(""));

        lines
    }
}

impl StatefulWidget for MessageList<'_> {
    type State = MessageListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Render block if present
        let inner_area = match &self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.clone().render(area, buf);
                inner
            }
            None => area,
        };

        if self.messages.is_empty() {
            return;
        }

        // Calculate all lines
        let all_lines: Vec<(usize, Vec<Line<'static>>)> = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let is_streaming = i == self.messages.len() - 1 && msg.is_streaming;
                (i, Self::message_to_lines(msg, is_streaming))
            })
            .collect();

        // Calculate total height
        let total_lines: usize = all_lines.iter().map(|(_, lines)| lines.len()).sum();

        // Adjust offset to keep content in view
        let visible_height = inner_area.height as usize;
        if total_lines > visible_height {
            // Auto-scroll to bottom if we're near the bottom
            let max_offset = total_lines.saturating_sub(visible_height);
            if state.offset > max_offset {
                state.offset = max_offset;
            }
        } else {
            state.offset = 0;
        }

        // Render visible lines
        let mut y = inner_area.y;
        let mut current_line = 0;

        for (msg_idx, lines) in all_lines {
            for line in lines {
                if current_line >= state.offset && y < inner_area.bottom() {
                    // Check if this message is selected
                    let style = if Some(msg_idx) == state.selected {
                        self.highlight_style
                    } else {
                        Style::default()
                    };

                    // Render line
                    let x = inner_area.x;
                    let width = inner_area.width;

                    // Clear the line with background style if selected
                    if Some(msg_idx) == state.selected {
                        for i in 0..width {
                            buf[(x + i, y)].set_style(style);
                        }
                    }

                    // Render spans
                    let mut x_offset = 0;
                    for span in line.spans {
                        let span_style = span.style.patch(style);
                        let content = span.content;
                        for c in content.chars() {
                            if x + x_offset < inner_area.right() {
                                buf[(x + x_offset, y)].set_char(c).set_style(span_style);
                                x_offset += 1;
                            }
                        }
                    }

                    y += 1;
                }
                current_line += 1;
            }
        }
    }
}

impl Widget for MessageList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = MessageListState::default();
        StatefulWidget::render(self, area, buf, &mut state);
    }
}
