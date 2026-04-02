//! UI rendering for the TUI using ratatui

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

use super::app::{App, AppMode, InputMode, Message, MessageRole, ToolStatus};

/// Main render function - draws the entire UI
pub fn render(frame: &mut Frame, app: &App) {
    // Main layout: messages area + input area + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Messages area (flexible)
            Constraint::Length(5), // Input area
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Render main components
    render_messages(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);

    // Render overlays based on mode
    match app.mode {
        AppMode::CommandPalette => render_command_palette(frame, app),
        AppMode::ModelSelect => render_model_select(frame, app),
        AppMode::SessionSelect => render_session_select(frame, app),
        AppMode::Help => render_help(frame, app),
        AppMode::Confirm => render_confirm_dialog(frame, app),
        AppMode::Normal => {}
    }
}

/// Render the message list area
fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" {} ", app.model),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.is_empty() {
        // Empty state
        let empty_text = Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Welcome to ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "CLAW",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Code", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Type a message to start chatting",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Press Ctrl+K for commands, Ctrl+O to change model",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(empty_text, inner);
        return;
    }

    // Build message items
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .enumerate()
        .flat_map(|(i, msg)| message_to_lines(msg, i == app.messages.len() - 1 && msg.is_streaming))
        .map(ListItem::new)
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);

    // Scrollbar
    if app.messages.len() > inner.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        let mut scrollbar_state =
            ScrollbarState::new(app.messages.len()).position(app.message_scroll);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

/// Convert a message to display lines
fn message_to_lines(msg: &Message, is_streaming: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Role indicator
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

    lines.push(Line::from(vec![
        Span::styled(format!("{} ", role_text), role_style),
        if is_streaming {
            Span::styled("●", Style::default().fg(Color::Green))
        } else {
            Span::raw("")
        },
    ]));

    // Tool use indicator
    if let Some(tool) = &msg.tool_use {
        let status_icon = match tool.status {
            ToolStatus::Pending => "◐",
            ToolStatus::Running => "◐",
            ToolStatus::Success => "✓",
            ToolStatus::Error => "✗",
        };
        let status_color = match tool.status {
            ToolStatus::Pending => Color::Yellow,
            ToolStatus::Running => Color::Cyan,
            ToolStatus::Success => Color::Green,
            ToolStatus::Error => Color::Red,
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", status_icon),
                Style::default().fg(status_color),
            ),
            Span::styled(tool.name.clone(), Style::default().fg(Color::Cyan)),
            if let Some(detail) = &tool.detail {
                Span::styled(format!(" {}", detail), Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]));
    }

    // Message content
    for line in msg.content.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {}", line),
            Style::default().fg(Color::White),
        )));
    }

    // Empty line after message
    lines.push(Line::from(""));

    lines
}

/// Render the input area
fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let mode_indicator = match app.input_mode {
        InputMode::Normal => ("NORMAL", Color::Blue),
        InputMode::Insert => ("INSERT", Color::Green),
        InputMode::Visual => ("VISUAL", Color::Magenta),
    };

    let title = if let Some(branch) = &app.git_branch {
        format!(" {} │ {} ", branch, mode_indicator.0)
    } else {
        format!(" {} ", mode_indicator.0)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.input_mode == InputMode::Insert {
            Color::Green
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(title, Style::default().fg(mode_indicator.1)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Input text with cursor
    let input_text = if app.input.is_empty() && app.input_mode == InputMode::Insert {
        Paragraph::new(Span::styled(
            "Type a message... (Shift+Enter for newline)",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        // Show cursor position
        let before_cursor = &app.input[..app.input_cursor];
        let cursor_char = app.input.chars().nth(app.input_cursor).unwrap_or(' ');
        let after_cursor = if app.input_cursor < app.input.len() {
            &app.input[app.input_cursor + cursor_char.len_utf8()..]
        } else {
            ""
        };

        Paragraph::new(Line::from(vec![
            Span::raw(before_cursor.to_string()),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(Color::White).fg(Color::Black),
            ),
            Span::raw(after_cursor.to_string()),
        ]))
    };

    frame.render_widget(input_text.wrap(Wrap { trim: false }), inner);

    // Set cursor position for terminal cursor
    if app.input_mode == InputMode::Insert && app.mode == AppMode::Normal {
        // Calculate visual cursor position
        let cursor_x = inner.x + (app.input_cursor as u16 % inner.width);
        let cursor_y = inner.y + (app.input_cursor as u16 / inner.width);
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Render the status bar
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" ", Style::default()),
        Span::styled(&app.model, Style::default().fg(Color::Cyan)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.permission_mode.as_str(),
            Style::default().fg(Color::Yellow),
        ),
    ];

    // Token usage
    if app.cumulative_usage.input_tokens > 0 || app.cumulative_usage.output_tokens > 0 {
        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!(
                "{}↓ {}↑",
                format_tokens(u64::from(app.cumulative_usage.input_tokens)),
                format_tokens(u64::from(app.cumulative_usage.output_tokens))
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Context usage
    if app.estimated_context_tokens > 0 {
        let ctx_pct = (app.estimated_context_tokens as f64 / 200_000.0) * 100.0;
        let ctx_color = if ctx_pct > 90.0 {
            Color::Red
        } else if ctx_pct > 75.0 {
            Color::Yellow
        } else {
            Color::Green
        };
        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("ctx:{:.0}%", ctx_pct),
            Style::default().fg(ctx_color),
        ));
    }

    // Loading indicator
    if app.is_loading {
        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("◐ {}", app.loading_phase),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Status message
    if let Some((msg, _)) = &app.status_message {
        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(msg, Style::default().fg(Color::Gray)));
    }

    // Working directory (right aligned)
    let cwd_str = app.cwd.file_name().and_then(|n| n.to_str()).unwrap_or("~");

    let left_line = Line::from(spans);
    let _right_text = format!("{} ", cwd_str);

    let status_bar = Paragraph::new(left_line).style(Style::default().bg(Color::Rgb(30, 30, 30)));

    frame.render_widget(status_bar, area);
}

/// Render the command palette overlay
fn render_command_palette(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());

    // Clear the area
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Commands (Ctrl+K) ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area for input and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Input field
    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.command_input),
        Span::styled("█", Style::default().fg(Color::White)),
    ]));
    frame.render_widget(input, chunks[0]);

    // Command list
    let items: Vec<ListItem> = app
        .command_results
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == app.command_selected {
                Style::default().bg(Color::Rgb(50, 50, 80)).fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(Span::styled(format!("  /{}", cmd), style)))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, chunks[1]);
}

/// Render the model selection overlay
fn render_model_select(frame: &mut Frame, app: &App) {
    let area = centered_rect(40, 30, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(Span::styled(
            " Select Model (Ctrl+O) ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = app
        .available_models
        .iter()
        .enumerate()
        .map(|(i, model)| {
            let is_current = model == &app.model;
            let is_selected = i == app.model_selected;

            let prefix = if is_current { "● " } else { "  " };
            let style = if is_selected {
                Style::default().bg(Color::Rgb(50, 50, 80)).fg(Color::White)
            } else if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(Line::from(Span::styled(
                format!("{}{}", prefix, model),
                style,
            )))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render the session selection overlay
fn render_session_select(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Sessions ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.sessions.is_empty() {
        let text = Paragraph::new("No saved sessions").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(text, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let is_current = session.id == app.session_id;
            let is_selected = i == app.session_selected;

            let prefix = if is_current { "● " } else { "  " };
            let style = if is_selected {
                Style::default().bg(Color::Rgb(50, 50, 80)).fg(Color::White)
            } else if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(Line::from(Span::styled(
                format!(
                    "{}{} ({} messages)",
                    prefix, session.id, session.message_count
                ),
                style,
            )))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render the help overlay
fn render_help(frame: &mut Frame, _app: &App) {
    let area = centered_rect(70, 80, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ctrl+C       ", Style::default().fg(Color::Yellow)),
            Span::raw("Cancel / Quit"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+K       ", Style::default().fg(Color::Yellow)),
            Span::raw("Command palette"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+O       ", Style::default().fg(Color::Yellow)),
            Span::raw("Select model"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+R       ", Style::default().fg(Color::Yellow)),
            Span::raw("Session picker"),
        ]),
        Line::from(vec![
            Span::styled("  Enter        ", Style::default().fg(Color::Yellow)),
            Span::raw("Submit message"),
        ]),
        Line::from(vec![
            Span::styled("  Shift+Enter  ", Style::default().fg(Color::Yellow)),
            Span::raw("New line"),
        ]),
        Line::from(vec![
            Span::styled("  Esc          ", Style::default().fg(Color::Yellow)),
            Span::raw("Normal mode / Close overlay"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Vim-style Navigation (Normal mode)",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  i            ", Style::default().fg(Color::Yellow)),
            Span::raw("Enter insert mode"),
        ]),
        Line::from(vec![
            Span::styled("  h/l          ", Style::default().fg(Color::Yellow)),
            Span::raw("Move cursor left/right"),
        ]),
        Line::from(vec![
            Span::styled("  j/k          ", Style::default().fg(Color::Yellow)),
            Span::raw("Scroll messages up/down"),
        ]),
        Line::from(vec![
            Span::styled("  g/G          ", Style::default().fg(Color::Yellow)),
            Span::raw("Scroll to top/bottom"),
        ]),
        Line::from(vec![
            Span::styled("  q            ", Style::default().fg(Color::Yellow)),
            Span::raw("Quit"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Slash Commands",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /help        ", Style::default().fg(Color::Yellow)),
            Span::raw("Show this help"),
        ]),
        Line::from(vec![
            Span::styled("  /model       ", Style::default().fg(Color::Yellow)),
            Span::raw("Change model"),
        ]),
        Line::from(vec![
            Span::styled("  /clear       ", Style::default().fg(Color::Yellow)),
            Span::raw("Clear conversation"),
        ]),
        Line::from(vec![
            Span::styled("  /status      ", Style::default().fg(Color::Yellow)),
            Span::raw("Show status info"),
        ]),
        Line::from(vec![
            Span::styled("  /quit        ", Style::default().fg(Color::Yellow)),
            Span::raw("Exit"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Permission Prompt",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  y            ", Style::default().fg(Color::Yellow)),
            Span::raw("Allow once"),
        ]),
        Line::from(vec![
            Span::styled("  a            ", Style::default().fg(Color::Yellow)),
            Span::raw("Allow always (session)"),
        ]),
        Line::from(vec![
            Span::styled("  n            ", Style::default().fg(Color::Yellow)),
            Span::raw("Deny"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Esc or Enter to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Render a confirmation dialog
fn render_confirm_dialog(frame: &mut Frame, app: &App) {
    let Some(dialog) = &app.confirm_dialog else {
        return;
    };

    let area = centered_rect(50, 20, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            format!(" {} ", dialog.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);

    // Message
    let message = Paragraph::new(dialog.message.clone()).wrap(Wrap { trim: true });
    frame.render_widget(message, chunks[0]);

    // Buttons
    let button_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let confirm_style = if dialog.selected {
        Style::default().bg(Color::Green).fg(Color::Black)
    } else {
        Style::default().fg(Color::Green)
    };

    let cancel_style = if !dialog.selected {
        Style::default().bg(Color::Red).fg(Color::Black)
    } else {
        Style::default().fg(Color::Red)
    };

    let confirm_btn = Paragraph::new(format!(" {} ", dialog.confirm_label))
        .style(confirm_style)
        .alignment(ratatui::layout::Alignment::Center);

    let cancel_btn = Paragraph::new(format!(" {} ", dialog.cancel_label))
        .style(cancel_style)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(confirm_btn, button_area[0]);
    frame.render_widget(cancel_btn, button_area[1]);
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Format token count compactly
fn format_tokens(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}
