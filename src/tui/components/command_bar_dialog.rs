//! Command Bar Dialog Component
//!
//! A vim-style command bar for executing commands without navigating through UI components.

use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// Result returned by CommandBarDialog after user action
#[derive(Debug)]
pub enum DialogResult {
    /// Execute a command
    ExecuteCommand(String),
    /// Dialog was cancelled/closed
    Close,
}

/// Command bar dialog component
pub struct CommandBarDialog {
    /// Current command input
    pub command: String,
    /// Cursor position in command input
    pub cursor: usize,
    /// Command history (for up/down arrow navigation)
    history: Vec<String>,
    /// Current position in history (None = not navigating history)
    history_index: Option<usize>,
    /// Error message if command is invalid
    error: Option<String>,
    /// Future: Autocomplete suggestions
    #[allow(dead_code)]
    suggestions: Vec<String>,
    /// Future: Selected suggestion index
    #[allow(dead_code)]
    selected_suggestion: Option<usize>,
    /// Pending result to be retrieved by App
    pending_result: Option<DialogResult>,
}

impl Default for CommandBarDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBarDialog {
    pub fn new() -> Self {
        Self {
            command: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            error: None,
            suggestions: Vec::new(),
            selected_suggestion: None,
            pending_result: None,
        }
    }

    /// Take the pending dialog result if any
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }

    /// Set error message
    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
    }

    /// Clear error message
    fn clear_error(&mut self) {
        self.error = None;
    }

    /// Move cursor left
    fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    fn cursor_right(&mut self) {
        if self.cursor < self.command.len() {
            self.cursor += 1;
        }
    }

    /// Navigate to previous command in history
    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Start at the most recent command
                self.history_index = Some(self.history.len() - 1);
                self.command = self.history[self.history.len() - 1].clone();
                self.cursor = self.command.len();
            }
            Some(idx) => {
                if idx > 0 {
                    self.history_index = Some(idx - 1);
                    self.command = self.history[idx - 1].clone();
                    self.cursor = self.command.len();
                }
            }
        }
    }

    /// Navigate to next command in history
    fn history_down(&mut self) {
        if let Some(idx) = self.history_index {
            if idx < self.history.len() - 1 {
                self.history_index = Some(idx + 1);
                self.command = self.history[idx + 1].clone();
                self.cursor = self.command.len();
            } else {
                // Back to empty command
                self.history_index = None;
                self.command.clear();
                self.cursor = 0;
            }
        }
    }

    /// Execute the command
    fn execute_command(&mut self) {
        let cmd = self.command.trim().to_string();

        if cmd.is_empty() {
            // Close without executing
            self.pending_result = Some(DialogResult::Close);
            return;
        }

        // Add to history if not a duplicate of the last command
        if self.history.last().map(|s| s.as_str()) != Some(&cmd) {
            self.history.push(cmd.clone());
        }

        self.pending_result = Some(DialogResult::ExecuteCommand(cmd));
    }

    /// Helper to format content with visual cursor
    fn format_content_with_cursor(&self, error: Option<&str>) -> String {
        let prompt = ":";
        let mut content = format!("{} {}", prompt, self.command);

        // Add cursor indicator
        // prompt (1) + space (1) + cursor index
        let cursor_pos = prompt.len() + 1 + self.cursor;
        content.insert(cursor_pos, '|');

        // Add error message if present
        if let Some(msg) = error {
            format!("{}\n{}", content, msg)
        } else {
            content
        }
    }
}

impl Component for CommandBarDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Clear error on any action
        if self.error.is_some() && action != Action::Cancel {
            self.clear_error();
        }

        match action {
            Action::Cancel => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // Close dialog
            }

            Action::Confirm => {
                self.execute_command();
                Ok(false) // Close dialog after execution
            }

            Action::MoveLeft => {
                self.cursor_left();
                Ok(true)
            }

            Action::MoveRight => {
                self.cursor_right();
                Ok(true)
            }

            Action::MoveUp => {
                self.history_up();
                Ok(true)
            }

            Action::MoveDown => {
                self.history_down();
                Ok(true)
            }

            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = Theme::default();

        // Clear area
        frame.render_widget(Clear, area);

        // Create block with simple border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.focused_border_style())
            .style(theme.normal_style());

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Content with cursor
        let content = self.format_content_with_cursor(self.error.as_deref());

        let paragraph = Paragraph::new(content)
            .style(theme.normal_style())
            .block(Block::default());

        frame.render_widget(paragraph, inner_area);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Confirm,
            Action::MoveLeft,
            Action::MoveRight,
            Action::MoveUp,
            Action::MoveDown,
        ]
    }

    fn name(&self) -> &str {
        "CommandBarDialog"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_bar_creation() {
        let dialog = CommandBarDialog::new();
        assert_eq!(dialog.command, "");
        assert_eq!(dialog.cursor, 0);
        assert!(dialog.history.is_empty());
    }

    #[test]
    fn test_cursor_movement() {
        let mut dialog = CommandBarDialog::new();
        dialog.command = "test".to_string();
        dialog.cursor = 2;

        dialog.cursor_left();
        assert_eq!(dialog.cursor, 1);

        dialog.cursor_right();
        assert_eq!(dialog.cursor, 2);
    }

    #[test]
    fn test_command_execution() {
        let mut dialog = CommandBarDialog::new();
        dialog.command = "quit".to_string();
        dialog.execute_command();

        assert!(matches!(
            dialog.take_result(),
            Some(DialogResult::ExecuteCommand(cmd)) if cmd == "quit"
        ));
        assert_eq!(dialog.history.len(), 1);
    }

    #[test]
    fn test_history_navigation() {
        let mut dialog = CommandBarDialog::new();
        dialog.history = vec!["cmd1".to_string(), "cmd2".to_string()];

        dialog.history_up();
        assert_eq!(dialog.command, "cmd2");

        dialog.history_up();
        assert_eq!(dialog.command, "cmd1");

        dialog.history_down();
        assert_eq!(dialog.command, "cmd2");
    }

    #[test]
    fn test_render_cursor_position() {
        let mut bar = CommandBarDialog::new();

        // Empty state
        // Command: ""
        // Cursor: 0
        // Expected: ": |"
        assert_eq!(bar.format_content_with_cursor(None), ": |");

        // Type 'a'
        bar.command.push('a');
        bar.cursor += 1;
        // Command: "a"
        // Cursor: 1 (after 'a')
        // Expected: ": a|"
        assert_eq!(bar.format_content_with_cursor(None), ": a|");

        // Type 'b'
        bar.command.push('b');
        bar.cursor += 1;
        // Command: "ab"
        // Cursor: 2 (after 'b')
        // Expected: ": ab|"
        assert_eq!(bar.format_content_with_cursor(None), ": ab|");

        // Move left
        bar.cursor_left();
        // Command: "ab"
        // Cursor: 1 (between 'a' and 'b')
        // Expected: ": a|b"
        assert_eq!(bar.format_content_with_cursor(None), ": a|b");

        // Move left again
        bar.cursor_left();
        // Command: "ab"
        // Cursor: 0 (before 'a')
        // Expected: ": |ab"
        assert_eq!(bar.format_content_with_cursor(None), ": |ab");
    }
}
