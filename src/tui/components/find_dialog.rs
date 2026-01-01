//! Find Dialog Component
//!
//! A TUI dialog for searching text in datasets with configurable options.
//! Inspired by Notepad++'s find dialog.

use crate::services::search_service::{FindOptions, SearchMode};
use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use serde::{Deserialize, Serialize};

/// Find dialog mode
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindDialogMode {
    Main,
    Error(String),
    Count(String),
}

/// Active field in the find dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindDialogField {
    Pattern,
    Backward,
    WholeWord,
    MatchCase,
    WrapAround,
    SearchMode,
    ActionsRow,
}

/// Selected action in the actions row
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindActionSelected {
    FindNext,
    Count,
    FindAll,
}

/// Find dialog component
pub struct FindDialog {
    pub search_pattern: String,
    pub search_pattern_cursor: usize,
    pub options: FindOptions,
    pub search_mode: SearchMode,
    pub active_field: FindDialogField,
    pub mode: FindDialogMode,
    pub action_selected: FindActionSelected,
}

impl Default for FindDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl FindDialog {
    pub fn new() -> Self {
        Self {
            search_pattern: String::new(),
            search_pattern_cursor: 0,
            options: FindOptions::default(),
            search_mode: SearchMode::Normal,
            active_field: FindDialogField::Pattern,
            mode: FindDialogMode::Main,
            action_selected: FindActionSelected::FindNext,
        }
    }

    /// Get the next field in tab order
    fn next_field(&self) -> FindDialogField {
        use FindDialogField::*;
        match self.active_field {
            Pattern => Backward,
            Backward => WholeWord,
            WholeWord => MatchCase,
            MatchCase => WrapAround,
            WrapAround => SearchMode,
            SearchMode => ActionsRow,
            ActionsRow => Pattern,
        }
    }

    /// Get the previous field in tab order
    fn prev_field(&self) -> FindDialogField {
        use FindDialogField::*;
        match self.active_field {
            Pattern => ActionsRow,
            Backward => Pattern,
            WholeWord => Backward,
            MatchCase => WholeWord,
            WrapAround => MatchCase,
            SearchMode => WrapAround,
            ActionsRow => SearchMode,
        }
    }

    /// Get the search parameters for executing a search
    pub fn get_search_params(&self) -> (String, FindOptions, SearchMode) {
        (
            self.search_pattern.clone(),
            self.options.clone(),
            self.search_mode.clone(),
        )
    }

    /// Set error mode with message
    pub fn set_error(&mut self, message: String) {
        self.mode = FindDialogMode::Error(message);
    }

    /// Set count mode with message
    pub fn set_count(&mut self, count: usize) {
        self.mode = FindDialogMode::Count(format!(
            "Found {} match{}",
            count,
            if count == 1 { "" } else { "es" }
        ));
    }

    /// Clear overlay (error or count)
    pub fn clear_overlay(&mut self) {
        self.mode = FindDialogMode::Main;
    }

    /// Insert character at cursor position
    fn insert_char(&mut self, c: char) {
        let cursor = self.search_pattern_cursor.min(self.search_pattern.len());
        self.search_pattern.insert(cursor, c);
        self.search_pattern_cursor = cursor + 1;
    }

    /// Delete character before cursor
    fn backspace(&mut self) {
        if self.search_pattern_cursor > 0 && !self.search_pattern.is_empty() {
            let cursor = self.search_pattern_cursor;
            let mut chars: Vec<char> = self.search_pattern.chars().collect();
            chars.remove(cursor - 1);
            self.search_pattern = chars.into_iter().collect();
            self.search_pattern_cursor -= 1;
        }
    }

    /// Delete character at cursor
    fn delete(&mut self) {
        let cursor = self.search_pattern_cursor;
        if cursor < self.search_pattern.len() && !self.search_pattern.is_empty() {
            let mut chars: Vec<char> = self.search_pattern.chars().collect();
            chars.remove(cursor);
            self.search_pattern = chars.into_iter().collect();
        }
    }

    /// Move cursor left
    fn cursor_left(&mut self) {
        if self.search_pattern_cursor > 0 {
            self.search_pattern_cursor -= 1;
        }
    }

    /// Move cursor right
    fn cursor_right(&mut self) {
        if self.search_pattern_cursor < self.search_pattern.len() {
            self.search_pattern_cursor += 1;
        }
    }

    /// Toggle a checkbox option
    fn toggle_checkbox(&mut self) {
        use FindDialogField::*;
        match self.active_field {
            Backward => self.options.backward = !self.options.backward,
            WholeWord => self.options.whole_word = !self.options.whole_word,
            MatchCase => self.options.match_case = !self.options.match_case,
            WrapAround => self.options.wrap_around = !self.options.wrap_around,
            SearchMode => {
                use crate::services::search_service::SearchMode as SM;
                self.search_mode = match self.search_mode {
                    SM::Normal => SM::Regex,
                    SM::Regex => SM::Normal,
                }
            }
            _ => {}
        }
    }

    /// Cycle action selection
    fn cycle_action_right(&mut self) {
        use FindActionSelected::*;
        self.action_selected = match self.action_selected {
            FindNext => Count,
            Count => FindAll,
            FindAll => FindNext,
        };
    }

    /// Cycle action selection backward
    fn cycle_action_left(&mut self) {
        use FindActionSelected::*;
        self.action_selected = match self.action_selected {
            FindNext => FindAll,
            Count => FindNext,
            FindAll => Count,
        };
    }
}

impl Component for FindDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Handle overlay dismissal first
        match &self.mode {
            FindDialogMode::Error(_) | FindDialogMode::Count(_) => {
                match action {
                    Action::Confirm | Action::Cancel => {
                        self.clear_overlay();
                        return Ok(true);
                    }
                    _ => return Ok(true), // Consume all actions while overlay is shown
                }
            }
            FindDialogMode::Main => {}
        }

        match action {
            Action::Cancel => Ok(false), // Close dialog

            Action::Confirm => {
                // Execute selected action
                if self.search_pattern.is_empty() {
                    self.set_error("Search pattern cannot be empty".to_string());
                    return Ok(true);
                }

                // Emit ExecuteFind action based on selected action
                // This will be handled by App
                Ok(true)
            }

            // Navigation
            Action::MoveDown => {
                self.active_field = self.next_field();
                Ok(true)
            }
            Action::MoveUp => {
                self.active_field = self.prev_field();
                Ok(true)
            }
            Action::MoveLeft => {
                if self.active_field == FindDialogField::Pattern {
                    self.cursor_left();
                } else if self.active_field == FindDialogField::ActionsRow {
                    self.cycle_action_left();
                } else {
                    self.toggle_checkbox();
                }
                Ok(true)
            }
            Action::MoveRight => {
                if self.active_field == FindDialogField::Pattern {
                    self.cursor_right();
                } else if self.active_field == FindDialogField::ActionsRow {
                    self.cycle_action_right();
                } else {
                    self.toggle_checkbox();
                }
                Ok(true)
            }

            // Text editing (only in Pattern field)
            _ => {
                if self.active_field == FindDialogField::Pattern {
                    // Handle character input - this will need to be done differently
                    // For now, return false to indicate action not handled
                }
                Ok(false)
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = Theme::default();

        // Clear area
        frame.render_widget(Clear, area);

        // Outer block with double border
        let outer_block = Block::default()
            .title("Find")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.focused_border_style());

        let inner_area = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Calculate positions for each element
        let mut y = inner_area.y + 1;
        let x = inner_area.x + 2;

        // Render pattern input
        self.render_pattern_field(frame.buffer_mut(), x, y, inner_area.width - 4, &theme);
        y += 2;

        // Render checkboxes
        self.render_checkboxes(frame.buffer_mut(), x, y, &theme);
        y += 5;

        // Render search mode radio
        self.render_search_mode(frame.buffer_mut(), x, y, &theme);
        y += 2;

        // Render action buttons
        self.render_actions(frame.buffer_mut(), x, y, &theme);

        // Render overlays if needed
        match &self.mode {
            FindDialogMode::Error(msg) => {
                self.render_error_overlay(frame, area, msg, &theme);
            }
            FindDialogMode::Count(msg) => {
                self.render_count_overlay(frame, area, msg, &theme);
            }
            FindDialogMode::Main => {}
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Confirm,
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
        ]
    }

    fn name(&self) -> &str {
        "FindDialog"
    }
}

impl FindDialog {
    fn render_pattern_field(
        &self,
        buf: &mut ratatui::buffer::Buffer,
        x: u16,
        y: u16,
        width: u16,
        theme: &Theme,
    ) {
        let label = "Search Pattern:";
        let is_active = self.active_field == FindDialogField::Pattern;

        buf.set_string(x, y, label, Style::default().add_modifier(Modifier::BOLD));

        let input_x = x + 18;
        let input_width = width.saturating_sub(18);

        if is_active {
            // Render with cursor
            buf.set_string(input_x, y, "> ", theme.focused_border_style());

            let mut cursor_x = input_x + 2;
            for (i, c) in self.search_pattern.chars().enumerate() {
                let style = if i == self.search_pattern_cursor {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    theme.selected_style()
                };
                buf.set_string(cursor_x, y, c.to_string(), style);
                cursor_x += 1;
            }

            // Show cursor at end if needed
            if self.search_pattern_cursor == self.search_pattern.len() {
                buf.set_string(
                    cursor_x,
                    y,
                    " ",
                    Style::default().fg(Color::Black).bg(Color::Yellow),
                );
            }
        } else {
            buf.set_string(input_x, y, &self.search_pattern, Style::default());
        }
    }

    fn render_checkboxes(&self, buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, theme: &Theme) {
        let checkboxes = [
            (
                "Backward direction",
                FindDialogField::Backward,
                self.options.backward,
            ),
            (
                "Match whole word only",
                FindDialogField::WholeWord,
                self.options.whole_word,
            ),
            (
                "Match case",
                FindDialogField::MatchCase,
                self.options.match_case,
            ),
            (
                "Wrap around",
                FindDialogField::WrapAround,
                self.options.wrap_around,
            ),
        ];

        for (i, (label, field, checked)) in checkboxes.iter().enumerate() {
            let check = if *checked { "[✓]" } else { "[ ]" };
            let style = if self.active_field == *field {
                theme.selected_style()
            } else {
                Style::default()
            };

            buf.set_string(x, y + i as u16, check, style);
            buf.set_string(x + 4, y + i as u16, label, style);
        }
    }

    fn render_search_mode(&self, buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, theme: &Theme) {
        buf.set_string(
            x,
            y,
            "Search Mode:",
            Style::default().add_modifier(Modifier::BOLD),
        );

        let is_active = self.active_field == FindDialogField::SearchMode;
        let normal_selected = matches!(self.search_mode, SearchMode::Normal);
        let regex_selected = matches!(self.search_mode, SearchMode::Regex);

        let normal_style = if is_active && normal_selected {
            theme.selected_style()
        } else if normal_selected {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let regex_style = if is_active && regex_selected {
            theme.selected_style()
        } else if regex_selected {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let normal_radio = if normal_selected {
            "(●) Normal"
        } else {
            "( ) Normal"
        };
        let regex_radio = if regex_selected {
            "(●) Regular Expression"
        } else {
            "( ) Regular Expression"
        };

        buf.set_string(x + 14, y, normal_radio, normal_style);
        buf.set_string(x + 28, y, regex_radio, regex_style);
    }

    fn render_actions(&self, buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, theme: &Theme) {
        let actions = [
            ("Find Next", FindActionSelected::FindNext),
            ("Count", FindActionSelected::Count),
            ("Find All", FindActionSelected::FindAll),
        ];

        let is_active_row = self.active_field == FindDialogField::ActionsRow;
        let mut btn_x = x;

        for (label, action) in actions.iter() {
            let is_selected = self.action_selected == *action;
            let style = if is_active_row && is_selected {
                theme.selected_style().add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };

            buf.set_string(btn_x, y, *label, style);
            btn_x += label.len() as u16 + 4;
        }
    }

    fn render_error_overlay(&self, frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
        let width = area.width.saturating_sub(10).clamp(20, 50);
        let height = 5;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        let overlay_area = Rect {
            x,
            y,
            width,
            height,
        };

        // Clear background
        frame.render_widget(Clear, overlay_area);

        // Use theme's normal background with error color for border
        let block = Block::default()
            .title("Error")
            .borders(Borders::ALL)
            .border_style(theme.error_style())
            .style(theme.normal_style());

        let inner = block.inner(overlay_area);
        frame.render_widget(block, overlay_area);

        let text = Paragraph::new(message)
            .style(theme.error_style())
            .wrap(Wrap { trim: true });
        frame.render_widget(text, inner);
    }

    fn render_count_overlay(&self, frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
        let width = area.width.saturating_sub(10).clamp(20, 40);
        let height = 5;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        let overlay_area = Rect {
            x,
            y,
            width,
            height,
        };

        // Clear background
        frame.render_widget(Clear, overlay_area);

        // Use theme's normal background with success color for border
        let block = Block::default()
            .title("Count")
            .borders(Borders::ALL)
            .border_style(theme.success_style())
            .style(theme.normal_style());

        let inner = block.inner(overlay_area);
        frame.render_widget(block, overlay_area);

        let text = Paragraph::new(message)
            .style(theme.normal_style())
            .wrap(Wrap { trim: true });
        frame.render_widget(text, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_dialog_creation() {
        let dialog = FindDialog::new();
        assert_eq!(dialog.search_pattern, "");
        assert_eq!(dialog.active_field, FindDialogField::Pattern);
    }

    #[test]
    fn test_field_navigation() {
        let dialog = FindDialog::new();
        let next = dialog.next_field();
        assert_eq!(next, FindDialogField::Backward);
    }

    #[test]
    fn test_pattern_editing() {
        let mut dialog = FindDialog::new();
        dialog.insert_char('h');
        dialog.insert_char('i');
        assert_eq!(dialog.search_pattern, "hi");
        assert_eq!(dialog.search_pattern_cursor, 2);
    }

    #[test]
    fn test_checkbox_toggle() {
        let mut dialog = FindDialog::new();
        dialog.active_field = FindDialogField::MatchCase;
        assert!(!dialog.options.match_case);

        dialog.toggle_checkbox();
        assert!(dialog.options.match_case);
    }

    #[test]
    fn test_action_cycling() {
        let mut dialog = FindDialog::new();
        dialog.active_field = FindDialogField::ActionsRow;

        assert_eq!(dialog.action_selected, FindActionSelected::FindNext);
        dialog.cycle_action_right();
        assert_eq!(dialog.action_selected, FindActionSelected::Count);
    }
}
