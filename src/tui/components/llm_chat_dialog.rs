//! LLM Chat Dialog
//!
//! An interactive chat dialog for communicating with an LLM agent.
//! Displays user and AI messages, tool call details (inputs/outputs),
//! the current system prompt, and token usage statistics.

use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

// ── Chat message types ──────────────────────────────────────────────────

/// A message in the chat history.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    /// A message from the user.
    User(String),
    /// A response from the AI assistant.
    Assistant(String),
    /// A tool call made by the AI (tool name, input JSON).
    ToolCall { tool_name: String, input: String },
    /// The result of a tool call.
    ToolResult { success: bool, message: String },
    /// A system/status message (e.g. "Sending…", errors).
    System(String),
}

// ── Dialog result ───────────────────────────────────────────────────────

/// Result from the LLM chat dialog.
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// User submitted a message to send to the LLM.
    SendMessage(String),
    /// User closed the dialog.
    Close,
}

// ── View mode ───────────────────────────────────────────────────────────

/// Which view the dialog is currently showing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ViewMode {
    /// Normal chat view.
    Chat,
    /// Showing the system prompt.
    SystemPrompt,
}

// ── LlmChatDialog ───────────────────────────────────────────────────────

/// Interactive LLM chat dialog.
pub struct LlmChatDialog {
    /// Chat messages.
    pub messages: Vec<ChatMessage>,

    /// User input text.
    pub input: String,

    /// Cursor position in input text.
    pub cursor_pos: usize,

    /// Scroll offset for the messages area (line-based).
    pub scroll_offset: usize,

    /// Total rendered lines (calculated during render).
    pub total_lines: usize,

    /// Visible lines in the messages area.
    pub visible_lines: usize,

    /// Current view mode.
    view_mode: ViewMode,

    /// System prompt text.
    pub system_prompt: String,

    /// System prompt scroll offset.
    pub system_prompt_scroll: usize,

    /// Token usage.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,

    /// Whether we are waiting for a response.
    pub waiting: bool,

    /// Show instructions.
    pub show_instructions: bool,

    /// Pending result.
    pub pending_result: Option<DialogResult>,

    /// Whether the dialog is focused.
    pub focused: bool,
}

impl LlmChatDialog {
    /// Create a new LLM chat dialog.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            total_lines: 0,
            visible_lines: 0,
            view_mode: ViewMode::Chat,
            system_prompt: String::new(),
            system_prompt_scroll: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            waiting: false,
            show_instructions: false,
            pending_result: None,
            focused: false,
        }
    }

    /// Take the pending result.
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }

    /// Push a chat message.
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        // Auto-scroll to bottom.
        self.scroll_to_bottom();
    }

    /// Set the system prompt.
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }

    /// Update token usage.
    pub fn set_token_usage(&mut self, input: u64, output: u64, total: u64) {
        self.input_tokens = input;
        self.output_tokens = output;
        self.total_tokens = total;
    }

    /// Mark as waiting for a response.
    pub fn set_waiting(&mut self, waiting: bool) {
        self.waiting = waiting;
    }

    /// Scroll to the bottom of the messages.
    fn scroll_to_bottom(&mut self) {
        if self.total_lines > self.visible_lines {
            self.scroll_offset = self.total_lines.saturating_sub(self.visible_lines);
        }
    }

    // ── Input editing ───────────────────────────────────────────────────

    fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self
                .input
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor_pos = prev;
        }
    }

    fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self
                .input
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor_pos = prev;
        }
    }

    fn cursor_right(&mut self) {
        if self.cursor_pos < self.input.len() {
            let next = self
                .input
                .char_indices()
                .find(|(i, _)| *i > self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.input.len());
            self.cursor_pos = next;
        }
    }

    fn submit_message(&mut self) {
        let text = self.input.trim().to_string();
        if !text.is_empty() && !self.waiting {
            self.pending_result = Some(DialogResult::SendMessage(text));
            self.input.clear();
            self.cursor_pos = 0;
        }
    }

    // ── Rendering helpers ───────────────────────────────────────────────

    /// Render a single chat message into styled `Line`s.
    fn render_message<'a>(msg: &'a ChatMessage, theme: &'a Theme) -> Vec<Line<'a>> {
        match msg {
            ChatMessage::User(text) => {
                let mut lines = vec![Line::from(vec![Span::styled(
                    "You: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )])];
                for line in text.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line, Style::default().fg(Color::Cyan)),
                    ]));
                }
                lines.push(Line::from("")); // spacer
                lines
            }
            ChatMessage::Assistant(text) => {
                let mut lines = vec![Line::from(vec![Span::styled(
                    "AI: ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )])];
                for line in text.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line, Style::default().fg(Color::White)),
                    ]));
                }
                lines.push(Line::from("")); // spacer
                lines
            }
            ChatMessage::ToolCall { tool_name, input } => {
                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        "⚙ Tool Call: ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(tool_name.as_str(), Style::default().fg(Color::Yellow)),
                ])];
                lines.push(Line::from(vec![
                    Span::raw("  Input: "),
                    Span::styled(input.as_str(), Style::default().fg(Color::DarkGray)),
                ]));
                lines
            }
            ChatMessage::ToolResult { success, message } => {
                let (icon, color) = if *success {
                    ("✓", Color::Green)
                } else {
                    ("✗", Color::Red)
                };
                let mut lines = vec![Line::from(vec![Span::styled(
                    format!("  {} Result: ", icon),
                    Style::default().fg(color),
                )])];
                for line in message.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(line, Style::default().fg(color)),
                    ]));
                }
                lines.push(Line::from("")); // spacer
                lines
            }
            ChatMessage::System(text) => {
                vec![
                    Line::from(vec![Span::styled(
                        format!("  {}", text),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )]),
                    Line::from(""),
                ]
            }
        }
    }
}

impl Focusable for LlmChatDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Component for LlmChatDialog {
    fn name(&self) -> &str {
        "LlmChatDialog"
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<KeyEventResult> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // System prompt view: only Ctrl+S toggles back, Esc closes, Up/Down scroll.
        if self.view_mode == ViewMode::SystemPrompt {
            match key.code {
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.view_mode = ViewMode::Chat;
                    return Ok(KeyEventResult::Consumed);
                }
                KeyCode::Esc => {
                    self.view_mode = ViewMode::Chat;
                    return Ok(KeyEventResult::Consumed);
                }
                KeyCode::Up => {
                    self.system_prompt_scroll = self.system_prompt_scroll.saturating_sub(1);
                    return Ok(KeyEventResult::Consumed);
                }
                KeyCode::Down => {
                    self.system_prompt_scroll += 1;
                    return Ok(KeyEventResult::Consumed);
                }
                _ => return Ok(KeyEventResult::Consumed),
            }
        }

        // Chat view key handling.
        match key.code {
            // Ctrl+S: show system prompt
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.view_mode = ViewMode::SystemPrompt;
                self.system_prompt_scroll = 0;
                Ok(KeyEventResult::Consumed)
            }
            // Ctrl+Enter (or Ctrl+J): send message
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit_message();
                Ok(KeyEventResult::Consumed)
            }
            // Enter on its own: insert newline in input
            KeyCode::Enter => {
                self.insert_char('\n');
                Ok(KeyEventResult::Consumed)
            }
            // Text input
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.insert_char(c);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Backspace => {
                self.delete_char();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Left => {
                self.cursor_left();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Right => {
                self.cursor_right();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::End => {
                self.cursor_pos = self.input.len();
                Ok(KeyEventResult::Consumed)
            }
            _ => Ok(KeyEventResult::Ignored),
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Escape | Action::Cancel => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // close dialog
            }
            Action::MoveUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                Ok(true)
            }
            Action::MoveDown => {
                if self.scroll_offset + self.visible_lines < self.total_lines {
                    self.scroll_offset += 1;
                }
                Ok(true)
            }
            Action::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(self.visible_lines.max(1));
                Ok(true)
            }
            Action::PageDown => {
                let max = self.total_lines.saturating_sub(self.visible_lines);
                self.scroll_offset = (self.scroll_offset + self.visible_lines.max(1)).min(max);
                Ok(true)
            }
            Action::GoToTop => {
                self.scroll_offset = 0;
                Ok(true)
            }
            Action::GoToBottom => {
                self.scroll_to_bottom();
                Ok(true)
            }
            Action::ToggleHelp => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            _ => Ok(true), // consume unknown actions while dialog is open
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Clear the area behind the dialog.
        frame.render_widget(Clear, area);

        let outer_block = Block::default()
            .title(" LLM Chat ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            });

        let inner_area = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // System prompt view.
        if self.view_mode == ViewMode::SystemPrompt {
            self.render_system_prompt_view(frame, inner_area, theme);
            return;
        }

        // Layout: messages | status bar | input | instructions
        let instructions_height = if self.show_instructions { 4 } else { 1 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),                      // messages
                Constraint::Length(1),                   // status bar (token usage)
                Constraint::Length(3),                   // input
                Constraint::Length(instructions_height), // instructions
            ])
            .split(inner_area);

        self.render_messages(frame, chunks[0], theme);
        self.render_status_bar(frame, chunks[1], theme);
        self.render_input(frame, chunks[2], theme);
        self.render_instructions(frame, chunks[3], theme);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Escape,
            Action::MoveUp,
            Action::MoveDown,
            Action::PageUp,
            Action::PageDown,
            Action::GoToTop,
            Action::GoToBottom,
            Action::ToggleHelp,
        ]
    }
}

// ── Rendering methods ───────────────────────────────────────────────────

impl LlmChatDialog {
    fn render_messages(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(theme.border_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Build all lines.
        let mut all_lines: Vec<Line> = Vec::new();
        if self.messages.is_empty() {
            all_lines.push(Line::from(Span::styled(
                "  Type a message and press Ctrl+Enter to send.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for msg in &self.messages {
                let lines = Self::render_message(msg, theme);
                all_lines.extend(lines);
            }
        }

        if self.waiting {
            all_lines.push(Line::from(Span::styled(
                "  ⏳ Waiting for response…",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        self.total_lines = all_lines.len();
        self.visible_lines = inner.height as usize;

        // Clamp scroll offset.
        if self.total_lines > self.visible_lines {
            if self.scroll_offset > self.total_lines - self.visible_lines {
                self.scroll_offset = self.total_lines - self.visible_lines;
            }
        } else {
            self.scroll_offset = 0;
        }

        let visible: Vec<Line> = all_lines
            .into_iter()
            .skip(self.scroll_offset)
            .take(self.visible_lines)
            .collect();

        let paragraph = Paragraph::new(visible).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);

        // Scrollbar.
        if self.total_lines > self.visible_lines {
            let mut scrollbar_state = ScrollbarState::new(self.total_lines)
                .position(self.scroll_offset)
                .viewport_content_length(self.visible_lines);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scrollbar_state,
            );
        }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let status = if self.total_tokens > 0 {
            format!(
                " Tokens: {} in / {} out / {} total",
                self.input_tokens, self.output_tokens, self.total_tokens
            )
        } else {
            " Tokens: –".to_string()
        };

        let status_line = Paragraph::new(Line::from(vec![Span::styled(
            status,
            Style::default().fg(Color::DarkGray),
        )]));
        frame.render_widget(status_line, area);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let prompt_label = if self.waiting { " ⏳ " } else { " > " };
        let block = Block::default()
            .title(prompt_label)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if self.waiting {
                Style::default().fg(Color::DarkGray)
            } else {
                theme.focused_border_style()
            });

        let inner = block.inner(area);

        // Render input text with cursor.
        let (before, after) = self.input.split_at(self.cursor_pos.min(self.input.len()));
        let cursor_char = after.chars().next().unwrap_or(' ');
        let rest = if after.len() > cursor_char.len_utf8() {
            &after[cursor_char.len_utf8()..]
        } else {
            ""
        };

        let input_line = Line::from(vec![
            Span::raw(before),
            Span::styled(
                cursor_char.to_string(),
                Style::default().add_modifier(Modifier::REVERSED),
            ),
            Span::raw(rest),
        ]);

        frame.render_widget(block, area);
        let paragraph = Paragraph::new(input_line).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    fn render_instructions(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        if self.show_instructions {
            let instructions = vec![Line::from(vec![
                Span::styled("Ctrl+Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Send  "),
                Span::styled("Ctrl+S", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" System Prompt  "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Close  "),
                Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Scroll  "),
                Span::styled("Ctrl+I", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Toggle Help"),
            ])];
            let para = Paragraph::new(instructions)
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true });
            frame.render_widget(para, area);
        } else {
            let hint =
                Paragraph::new("  Ctrl+I for help").style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint, area);
        }
    }

    fn render_system_prompt_view(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" System Prompt (Ctrl+S or Esc to return) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.focused_border_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines: Vec<Line> = self
            .system_prompt
            .lines()
            .map(|l| Line::from(Span::raw(l)))
            .collect();

        let total = lines.len();

        // Clamp scroll.
        let visible = inner.height as usize;
        if total > visible && self.system_prompt_scroll > total - visible {
            self.system_prompt_scroll = total.saturating_sub(visible);
        }

        let paragraph = Paragraph::new(lines)
            .scroll((self.system_prompt_scroll as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);

        // Scrollbar.
        if total > visible {
            let mut state = ScrollbarState::new(total)
                .position(self.system_prompt_scroll)
                .viewport_content_length(visible);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                inner,
                &mut state,
            );
        }
    }
}
