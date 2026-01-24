//! Command Bar Dialog Component
//!
//! A vim-style command bar for executing commands without navigating through UI components.

use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
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
    pub selected_suggestion: Option<usize>,
    /// Pending result to be retrieved by App
    pending_result: Option<DialogResult>,
    /// State for the suggestions list
    suggestions_state: ListState,
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
            suggestions_state: ListState::default(),
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

    pub fn pending_result(&self) -> &Option<DialogResult> {
        &self.pending_result
    }

    /// Set suggestions and reset selection
    pub fn set_suggestions(&mut self, suggestions: Vec<String>) {
        self.suggestions = suggestions;
        if self.suggestions.is_empty() {
            self.selected_suggestion = None;
            self.suggestions_state.select(None);
        } else {
            // Auto-select first suggestion if configured?
            // For now, let's keep it None until user cycles
            self.selected_suggestion = None;
            self.suggestions_state.select(None);
        }
    }

    /// Cycle to the next suggestion
    pub fn next_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.selected_suggestion = match self.selected_suggestion {
            None => Some(0),
            Some(i) => {
                if i + 1 < self.suggestions.len() {
                    Some(i + 1)
                } else {
                    Some(0) // Wrap around
                }
            }
        };
        self.suggestions_state.select(self.selected_suggestion);
    }

    /// Cycle to the previous suggestion
    pub fn previous_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.selected_suggestion = match self.selected_suggestion {
            None => Some(self.suggestions.len() - 1),
            Some(i) => {
                if i > 0 {
                    Some(i - 1)
                } else {
                    Some(self.suggestions.len() - 1) // Wrap around
                }
            }
        };
        self.suggestions_state.select(self.selected_suggestion);
    }

    /// Clear the current suggestion selection
    pub fn clear_selection(&mut self) {
        self.selected_suggestion = None;
        self.suggestions_state.select(None);
    }

    /// Accept the currently selected suggestion
    pub fn accept_suggestion(&mut self) {
        if let Some(idx) = self.selected_suggestion {
            if let Some(suggestion) = self.suggestions.get(idx) {
                // Determine what part of the command we are replacing
                // Simple strategy: replace the last word or the whole command depending on context
                // For now, let's assume we are appending/replacing current token

                let parts: Vec<&str> = self.command.split_whitespace().collect();
                if parts.is_empty() {
                    self.command = suggestion.clone();
                } else {
                    // Check if last part is being typed
                    if self.command.ends_with(' ') {
                        self.command.push_str(suggestion);
                    } else {
                        // Replace last part
                        // Find the start of the last token
                        if let Some(last_space) = self.command.rfind(' ') {
                            self.command.truncate(last_space + 1);
                            self.command.push_str(suggestion);
                        } else {
                            // Only one word
                            self.command = suggestion.clone();
                        }
                    }
                }
                // Add a space for convenience
                self.command.push(' ');
                self.cursor = self.command.len();

                // Clear suggestions after acceptance
                self.suggestions.clear();
                self.selected_suggestion = None;
                self.suggestions_state.select(None);
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
                if !self.suggestions.is_empty() {
                    self.suggestions.clear();
                    self.clear_selection();
                    Ok(true) // Keep dialog open, just hide suggestions
                } else {
                    self.pending_result = Some(DialogResult::Close);
                    Ok(false) // Close dialog
                }
            }

            Action::Confirm => {
                if self.selected_suggestion.is_some() {
                    self.accept_suggestion();
                    Ok(true) // Keep dialog open
                } else {
                    self.execute_command();
                    Ok(false) // Close dialog after execution
                }
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
                if self.selected_suggestion.is_some() {
                    self.previous_suggestion();
                    Ok(true)
                } else {
                    self.history_up();
                    Ok(true)
                }
            }

            Action::MoveDown => {
                if self.suggestions.is_empty() {
                    self.history_down();
                } else {
                    // If we have suggestions, Down should prioritize them?
                    // Or only if already selected?
                    // User request: "allow up/down to be used to navigate the items" implies when active/highlighted.
                    // But if I press down and list is visible but not selected?
                    // The plan says: "If suggestions active: Navigate suggestion list."
                    // "suggestions active" usually means visible.
                    // But strict reading of "suggestion box becomes active" from user prompt might mean "highlighted".
                    // Let's stick to plan: "If suggestions active (meaning selected/highlighted?)"
                    // Actually, if I am typing and suggestions appear, usually Down key jumps into them in most IDEs.
                    // Let's check `selected_suggestion.is_some()`.
                    // BUT, if I just typed, `selected_suggestion` is None.
                    // If is None, do we want Down to go to history or suggestions?
                    // Usually History is Up.
                    // If I press Down in empty input, nothing happens (unless history navigation).
                    // If suggestions are present, Down should probably select the first one.

                    if !self.suggestions.is_empty() {
                        self.next_suggestion();
                    } else {
                        self.history_down();
                    }
                }
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

        // Render suggestions popup if active
        if !self.suggestions.is_empty() {
            let suggestions_height = self.suggestions.len().min(5) as u16 + 2; // +2 for borders
            let popup_area = Rect {
                x: area.x,
                y: area.y.saturating_sub(suggestions_height),
                width: 40, // Fixed width for now, or dynamic based on content
                height: suggestions_height,
            };

            frame.render_widget(Clear, popup_area);

            let items: Vec<ListItem> = self
                .suggestions
                .iter()
                .enumerate()
                .map(|(_, s)| ListItem::new(s.clone()))
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Suggestions"))
                .highlight_style(theme.focused_border_style())
                .style(theme.normal_style());

            frame.render_stateful_widget(list, popup_area, &mut self.suggestions_state);

            let mut scrollbar_state = ScrollbarState::default()
                .content_length(self.suggestions.len())
                .position(self.suggestions_state.offset());

            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            frame.render_stateful_widget(
                scrollbar,
                popup_area.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }

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

    #[test]
    fn test_accept_suggestion_on_confirm() {
        let mut dialog = CommandBarDialog::new();
        dialog.command = "so".to_string();
        dialog.cursor = 2; // End of "so"

        dialog.set_suggestions(vec!["sort".to_string()]);

        // Select the suggestion
        dialog.next_suggestion(); // selects "sort" (index 0)
        assert_eq!(dialog.selected_suggestion, Some(0));

        // Press Enter (Confirm)
        let result = dialog.handle_action(Action::Confirm).unwrap();

        // Should return true (keep dialog open)
        assert!(result);

        // Command should be updated
        assert_eq!(dialog.command, "sort ");

        // Suggestion should be cleared
        assert_eq!(dialog.selected_suggestion, None);
        // Suggestions list is also cleared in accept_suggestion
        assert!(dialog.suggestions.is_empty());

        // Now press Enter again (Confirm)
        let result = dialog.handle_action(Action::Confirm).unwrap();

        // Should return false (close dialog)
        assert!(!result);

        // Should have result
        assert!(matches!(
            dialog.take_result(),
            Some(DialogResult::ExecuteCommand(cmd)) if cmd == "sort"
        ));
    }

    #[test]
    fn test_suggestion_selection_updates_state() {
        let mut dialog = CommandBarDialog::new();
        dialog.set_suggestions(vec!["a".to_string(), "b".to_string(), "c".to_string()]);

        // Initial state
        assert_eq!(dialog.selected_suggestion, None);
        assert_eq!(dialog.suggestions_state.selected(), None);

        // Next suggestion
        dialog.next_suggestion();
        assert_eq!(dialog.selected_suggestion, Some(0));
        assert_eq!(dialog.suggestions_state.selected(), Some(0));

        // Next suggestion (1)
        dialog.next_suggestion();
        assert_eq!(dialog.selected_suggestion, Some(1));
        assert_eq!(dialog.suggestions_state.selected(), Some(1));

        // Set suggestions resets state
        dialog.set_suggestions(vec!["d".to_string()]);
        assert_eq!(dialog.selected_suggestion, None);
        assert_eq!(dialog.suggestions_state.selected(), None);
    }

    #[test]
    fn test_navigation_refinement() {
        let mut dialog = CommandBarDialog::new();
        dialog.set_suggestions(vec!["a".to_string(), "b".to_string()]);

        // Initial: None
        assert_eq!(dialog.selected_suggestion, None);

        // Down -> First (0)
        let _ = dialog.handle_action(Action::MoveDown);
        assert_eq!(dialog.selected_suggestion, Some(0));

        // Down -> Next (1)
        let _ = dialog.handle_action(Action::MoveDown);
        assert_eq!(dialog.selected_suggestion, Some(1));

        // Up -> Prev (0)
        let _ = dialog.handle_action(Action::MoveUp);
        assert_eq!(dialog.selected_suggestion, Some(0));

        // Up -> Wrap (1)
        let _ = dialog.handle_action(Action::MoveUp);
        assert_eq!(dialog.selected_suggestion, Some(1));

        // Cancel -> Clear selection (None), returns true (consumed)
        let consumed = dialog.handle_action(Action::Cancel).unwrap();
        assert!(consumed);
        assert_eq!(dialog.selected_suggestion, None);
        assert!(dialog.pending_result.is_none());

        // Cancel again -> Close dialog, returns false (not consumed/close)
        let consumed = dialog.handle_action(Action::Cancel).unwrap();
        assert!(!consumed);
        assert!(matches!(dialog.pending_result, Some(DialogResult::Close)));
    }

    #[test]
    fn test_esc_hides_visible_suggestions() {
        let mut dialog = CommandBarDialog::new();
        // Setup suggestions but NO selection
        dialog.set_suggestions(vec!["a".to_string()]);
        assert!(!dialog.suggestions.is_empty());
        assert_eq!(dialog.selected_suggestion, None);

        // Cancel -> Should clear suggestions, stay open
        let consumed = dialog.handle_action(Action::Cancel).unwrap();
        assert!(consumed);
        assert!(dialog.suggestions.is_empty()); // Suggestions hidden/cleared
        assert!(dialog.pending_result.is_none());

        // Cancel again -> Should close dialog
        let consumed = dialog.handle_action(Action::Cancel).unwrap();
        assert!(!consumed);
        assert!(matches!(dialog.pending_result, Some(DialogResult::Close)));
    }
}
