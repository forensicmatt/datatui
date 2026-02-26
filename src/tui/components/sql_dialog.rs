//! SQL Dialog Component
//!
//! Modal dialog for writing and executing SQL queries against datasets.

use crate::tui::sql_suggestions;
use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

/// Type of suggestion
#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionType {
    Field,
    Function,
}

impl SuggestionType {
    pub fn as_str(&self) -> &str {
        match self {
            SuggestionType::Field => "Field",
            SuggestionType::Function => "Function",
        }
    }
}

/// Individual suggestion item with metadata
#[derive(Debug, Clone)]
pub struct SuggestionItem {
    pub name: String,
    pub suggestion_type: SuggestionType,
    pub description: String,
}

impl SuggestionItem {
    pub fn field(name: String) -> Self {
        Self {
            name,
            suggestion_type: SuggestionType::Field,
            description: String::new(),
        }
    }

    pub fn function(name: String, description: String) -> Self {
        Self {
            name,
            suggestion_type: SuggestionType::Function,
            description,
        }
    }
}

/// Dialog result from SQL dialog
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// Execute the SQL query
    ExecuteQuery(String),
    /// Close the dialog without executing
    Close,
}

/// State snapshot for undo/redo
#[derive(Debug, Clone)]
struct UndoState {
    query_text: String,
    cursor_pos: usize,
}

/// SQL Dialog for entering and executing SQL queries
pub struct SqlDialog {
    /// SQL query text (multi-line)
    query_text: String,

    /// Cursor position in the text
    cursor_pos: usize,

    /// Available column names for suggestions
    columns: Vec<String>,

    /// Current suggestions
    suggestions: Vec<SuggestionItem>,

    /// Selected suggestion index
    selected_suggestion: Option<usize>,

    /// Scroll offset for suggestions list
    suggestion_scroll_offset: usize,

    /// Cached viewport height for suggestions (calculated during render)
    cached_suggestion_viewport_height: usize,

    /// Type-ahead buffer for quick suggestion selection
    suggestion_typeahead_buffer: String,

    /// Undo stack for edit history
    undo_stack: Vec<UndoState>,

    /// Redo stack for redo functionality
    redo_stack: Vec<UndoState>,

    /// Error message to display (if any)
    error_message: Option<String>,

    /// Show instructions
    show_instructions: bool,

    /// Pending result
    pending_result: Option<DialogResult>,

    /// Whether the dialog has focus
    focused: bool,
}

impl SqlDialog {
    /// Create a new SQL dialog
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            query_text: String::new(),
            cursor_pos: 0,
            columns,
            suggestions: Vec::new(),
            selected_suggestion: None,
            suggestion_scroll_offset: 0,
            cached_suggestion_viewport_height: 5, // Default, updated during render
            suggestion_typeahead_buffer: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            error_message: None,
            show_instructions: true,
            pending_result: None,
            focused: true,
        }
    }

    /// Set the initial query text (e.g., from current query)
    pub fn set_query_text(&mut self, text: String) {
        self.query_text = text.clone();
        self.cursor_pos = text.len();
        self.update_suggestions();
    }

    /// Get the current query text
    pub fn query_text(&self) -> &str {
        &self.query_text
    }

    /// Set error message
    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    /// Clear error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Check if a suggestion is currently selected
    pub fn has_selected_suggestion(&self) -> bool {
        self.selected_suggestion.is_some()
    }

    /// Take the pending result
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }

    /// Save current state to undo stack (call before making changes)
    fn save_state(&mut self) {
        // Limit undo stack size to prevent unbounded growth
        const MAX_UNDO_STACK: usize = 50;

        let state = UndoState {
            query_text: self.query_text.clone(),
            cursor_pos: self.cursor_pos,
        };

        self.undo_stack.push(state);

        // Trim stack if too large
        if self.undo_stack.len() > MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }

        // Clear redo stack when new edits are made
        self.redo_stack.clear();
    }

    /// Undo the last change
    pub fn undo(&mut self) {
        if let Some(current_state) = self.undo_stack.pop() {
            // Save current state to redo stack
            let redo_state = UndoState {
                query_text: self.query_text.clone(),
                cursor_pos: self.cursor_pos,
            };
            self.redo_stack.push(redo_state);

            // Restore previous state
            self.query_text = current_state.query_text;
            self.cursor_pos = current_state.cursor_pos;

            // Update suggestions for new text
            self.update_suggestions();
        }
    }

    /// Redo the last undone change
    pub fn redo(&mut self) {
        if let Some(redo_state) = self.redo_stack.pop() {
            // Save current state to undo stack
            let undo_state = UndoState {
                query_text: self.query_text.clone(),
                cursor_pos: self.cursor_pos,
            };
            self.undo_stack.push(undo_state);

            // Restore redo state
            self.query_text = redo_state.query_text;
            self.cursor_pos = redo_state.cursor_pos;

            // Update suggestions for new text
            self.update_suggestions();
        }
    }

    /// Execute the current query (e.g., triggered by Ctrl+Enter)
    pub fn execute_query(&mut self) {
        if !self.query_text.trim().is_empty() {
            self.pending_result = Some(DialogResult::ExecuteQuery(self.query_text.clone()));
        }
    }

    /// Insert character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.save_state();
        self.query_text.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
        self.update_suggestions();
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.save_state();
            let prev_pos = self
                .query_text
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.query_text.remove(prev_pos);
            self.cursor_pos = prev_pos;
            self.update_suggestions();
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let prev_pos = self
                .query_text
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor_pos = prev_pos;
            self.update_suggestions();
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor_pos < self.query_text.len() {
            let next_pos = self
                .query_text
                .char_indices()
                .find(|(i, _)| *i > self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.query_text.len());
            self.cursor_pos = next_pos;
            self.update_suggestions();
        }
    }

    /// Move cursor to start of line
    pub fn cursor_home(&mut self) {
        // Find start of current line
        let before_cursor = &self.query_text[..self.cursor_pos];
        if let Some(pos) = before_cursor.rfind('\n') {
            self.cursor_pos = pos + 1;
        } else {
            self.cursor_pos = 0;
        }
        self.update_suggestions();
    }

    /// Move cursor to end of line
    pub fn cursor_end(&mut self) {
        // Find end of current line
        let after_cursor = &self.query_text[self.cursor_pos..];
        if let Some(pos) = after_cursor.find('\n') {
            self.cursor_pos += pos;
        } else {
            self.cursor_pos = self.query_text.len();
        }
        self.update_suggestions();
    }

    /// Move cursor left by one word (Ctrl+Left)
    pub fn cursor_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        // Helper to check if a character is a word boundary
        let is_boundary =
            |c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',' || c == ';';

        // Convert to characters for easier manipulation
        let chars: Vec<char> = self.query_text.chars().collect();
        let mut char_pos = self.query_text[..self.cursor_pos].chars().count();

        if char_pos == 0 {
            return;
        }

        // Move back one character to start
        char_pos -= 1;

        // Skip over any boundaries
        while char_pos > 0 && is_boundary(chars[char_pos]) {
            char_pos -= 1;
        }

        // Skip over the word
        while char_pos > 0 && !is_boundary(chars[char_pos]) {
            char_pos -= 1;
        }

        // If we stopped on a boundary, move forward one to be at the start of the word
        if char_pos > 0 || is_boundary(chars[0]) {
            if is_boundary(chars[char_pos]) {
                char_pos += 1;
            }
        }

        // Convert character position back to byte position
        self.cursor_pos = chars.iter().take(char_pos).map(|c| c.len_utf8()).sum();
        self.update_suggestions();
    }

    /// Move cursor right by one word (Ctrl+Right)
    pub fn cursor_word_right(&mut self) {
        if self.cursor_pos >= self.query_text.len() {
            return;
        }

        // Helper to check if a character is a word boundary
        let is_boundary =
            |c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',' || c == ';';

        // Convert to characters for easier manipulation
        let chars: Vec<char> = self.query_text.chars().collect();
        let mut char_pos = self.query_text[..self.cursor_pos].chars().count();

        if char_pos >= chars.len() {
            return;
        }

        // Skip over the current word
        while char_pos < chars.len() && !is_boundary(chars[char_pos]) {
            char_pos += 1;
        }

        // Skip over any boundaries
        while char_pos < chars.len() && is_boundary(chars[char_pos]) {
            char_pos += 1;
        }

        // Convert character position back to byte position
        self.cursor_pos = chars.iter().take(char_pos).map(|c| c.len_utf8()).sum();
        self.update_suggestions();
    }

    /// Delete word to the left (Ctrl+Backspace)
    pub fn delete_word_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        self.save_state();

        // Helper to check if a character is a word boundary
        let is_boundary =
            |c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',' || c == ';';

        // Convert to characters for easier manipulation
        let chars: Vec<char> = self.query_text.chars().collect();
        let current_char_pos = self.query_text[..self.cursor_pos].chars().count();
        let mut char_pos = current_char_pos;

        if char_pos == 0 {
            return;
        }

        // Move back one character to start
        char_pos -= 1;

        // Skip over any boundaries
        while char_pos > 0 && is_boundary(chars[char_pos]) {
            char_pos -= 1;
        }

        // Skip over the word
        while char_pos > 0 && !is_boundary(chars[char_pos]) {
            char_pos -= 1;
        }

        // If we stopped on a boundary, move forward one to be at the start of the word
        if char_pos > 0 || is_boundary(chars[0]) {
            if is_boundary(chars[char_pos]) {
                char_pos += 1;
            }
        }

        // Calculate byte positions for deletion
        let start_byte: usize = chars.iter().take(char_pos).map(|c| c.len_utf8()).sum();

        // Delete the range from start of word to cursor
        self.query_text
            .replace_range(start_byte..self.cursor_pos, "");
        self.cursor_pos = start_byte;
        self.update_suggestions();
    }

    /// Insert newline
    pub fn insert_newline(&mut self) {
        self.save_state();
        self.query_text.insert(self.cursor_pos, '\n');
        self.cursor_pos += 1;
        self.update_suggestions();
    }

    /// Update suggestions based on current query and cursor position
    fn update_suggestions(&mut self) {
        self.suggestions =
            sql_suggestions::get_sql_suggestions(&self.query_text, self.cursor_pos, &self.columns);

        // Reset selected suggestion if list changed
        if self.suggestions.is_empty() {
            self.selected_suggestion = None;
        } else if let Some(idx) = self.selected_suggestion {
            if idx >= self.suggestions.len() {
                self.selected_suggestion = Some(0);
            }
        }
    }

    /// Select next suggestion
    pub fn next_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_suggestion = Some(match self.selected_suggestion {
                Some(idx) => (idx + 1) % self.suggestions.len(),
                None => 0,
            });
        }
    }

    /// Select previous suggestion
    pub fn prev_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected_suggestion = Some(match self.selected_suggestion {
                Some(idx) => {
                    if idx == 0 {
                        self.suggestions.len() - 1
                    } else {
                        idx - 1
                    }
                }
                None => self.suggestions.len() - 1,
            });
        }
    }

    /// Jump down by one page in suggestions (Ctrl+Down)
    pub fn page_down_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            let page_size = self.cached_suggestion_viewport_height.max(1);
            self.selected_suggestion = Some(match self.selected_suggestion {
                Some(idx) => {
                    let new_idx = idx + page_size;
                    if new_idx >= self.suggestions.len() {
                        self.suggestions.len() - 1
                    } else {
                        new_idx
                    }
                }
                None => page_size.min(self.suggestions.len() - 1),
            });
        }
    }

    /// Jump up by one page in suggestions (Ctrl+Up)
    pub fn page_up_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            let page_size = self.cached_suggestion_viewport_height.max(1);
            self.selected_suggestion = Some(match self.selected_suggestion {
                Some(idx) => {
                    if idx < page_size {
                        0
                    } else {
                        idx - page_size
                    }
                }
                None => 0,
            });
        }
    }

    /// Clear suggestion selection
    pub fn clear_suggestion_selection(&mut self) {
        self.selected_suggestion = None;
        self.suggestion_scroll_offset = 0;
        self.suggestion_typeahead_buffer.clear();
    }

    /// Add a character to the typeahead buffer and find matching suggestion
    pub fn add_typeahead_char(&mut self, c: char) {
        self.suggestion_typeahead_buffer.push(c);
        self.find_next_matching_suggestion();
    }

    /// Clear the typeahead buffer
    pub fn clear_typeahead(&mut self) {
        self.suggestion_typeahead_buffer.clear();
    }

    /// Get the current typeahead buffer (for display)
    pub fn typeahead_buffer(&self) -> &str {
        &self.suggestion_typeahead_buffer
    }

    /// Find and select the next suggestion that matches the typeahead buffer
    fn find_next_matching_suggestion(&mut self) {
        if self.suggestion_typeahead_buffer.is_empty() || self.suggestions.is_empty() {
            return;
        }

        let prefix = self.suggestion_typeahead_buffer.to_lowercase();
        let start_index = self.selected_suggestion.map(|i| i + 1).unwrap_or(0);

        // Search from current position forward
        for i in start_index..self.suggestions.len() {
            if self.suggestions[i].name.to_lowercase().starts_with(&prefix) {
                self.selected_suggestion = Some(i);
                return;
            }
        }

        // Wrap around and search from beginning
        for i in 0..start_index {
            if self.suggestions[i].name.to_lowercase().starts_with(&prefix) {
                self.selected_suggestion = Some(i);
                return;
            }
        }

        // No match found - keep current selection or select first if none
        if self.selected_suggestion.is_none() && !self.suggestions.is_empty() {
            self.selected_suggestion = Some(0);
        }
    }

    /// Update scroll offset to keep selected suggestion visible
    /// viewport_height: number of suggestions visible at once
    fn update_scroll_for_selection(&mut self, viewport_height: usize) {
        if let Some(selected) = self.selected_suggestion {
            // Ensure selected item is visible
            if selected < self.suggestion_scroll_offset {
                // Selected item is above viewport, scroll up
                self.suggestion_scroll_offset = selected;
            } else if selected >= self.suggestion_scroll_offset + viewport_height {
                // Selected item is below viewport, scroll down
                self.suggestion_scroll_offset = selected.saturating_sub(viewport_height - 1);
            }
        }
    }

    /// Accept the selected suggestion
    pub fn accept_suggestion(&mut self) {
        if let Some(idx) = self.selected_suggestion {
            // Clone the suggestion name to avoid borrow conflicts
            let suggestion_name = self.suggestions.get(idx).map(|s| s.name.clone());

            if let Some(name) = suggestion_name {
                self.save_state();

                // Find the word being completed
                let before_cursor = &self.query_text[..self.cursor_pos];
                let word_start = before_cursor
                    .rfind(|c: char| c.is_whitespace() || c == '(' || c == ',')
                    .map(|i| i + 1)
                    .unwrap_or(0);

                // Replace the partial word with the suggestion
                self.query_text
                    .replace_range(word_start..self.cursor_pos, &name);
                self.cursor_pos = word_start + name.len();

                // Add a space after the suggestion (cursor stays before the space)
                self.query_text.insert(self.cursor_pos, ' ');

                self.update_suggestions();

                // Clear the suggestion selection to return focus to query
                self.clear_suggestion_selection();
            }
        }
    }

    /// Clear all text
    pub fn clear_text(&mut self) {
        self.save_state();
        self.query_text.clear();
        self.cursor_pos = 0;
        self.update_suggestions();
    }
    /// Handle a key event
    /// Returns KeyEventResult
    pub fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<KeyEventResult> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // If a suggestion is selected, only allow navigation keys
        if self.has_selected_suggestion() {
            if key.code == KeyCode::Enter {
                self.accept_suggestion();
                return Ok(KeyEventResult::Consumed);
            } else if key.code == KeyCode::Up {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.page_up_suggestion();
                } else {
                    self.prev_suggestion();
                }
                return Ok(KeyEventResult::Consumed);
            } else if key.code == KeyCode::Down {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.page_down_suggestion();
                } else {
                    self.next_suggestion();
                }
                return Ok(KeyEventResult::Consumed);
            } else if key.code == KeyCode::Esc {
                self.clear_suggestion_selection();
                return Ok(KeyEventResult::Consumed);
            }

            // Character input uses type-ahead to navigate suggestions
            if let KeyCode::Char(c) = key.code {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.add_typeahead_char(c);
                    return Ok(KeyEventResult::Consumed);
                }
            } else if key.code == KeyCode::Backspace {
                // Remove last character from typeahead buffer or delete char
                let buffer = self.typeahead_buffer().to_string();
                if !buffer.is_empty() {
                    let mut chars: Vec<char> = buffer.chars().collect();
                    chars.pop();
                    self.clear_typeahead();
                    for ch in chars {
                        self.add_typeahead_char(ch);
                    }
                } else {
                    self.clear_suggestion_selection();
                    self.delete_char();
                }
                return Ok(KeyEventResult::Consumed);
            }
            // Ignore other keys when suggestion is selected (except movement keys not handled here?)
            // Actually we probably want to let other keys fall through?
            // Original logic in App returned Ok(()) essentially consuming the key.
            return Ok(KeyEventResult::Consumed);
        } else {
            // Normal text input mode

            // Handle Ctrl+Enter to execute query
            if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.execute_query();
                return Ok(KeyEventResult::Consumed);
            }

            // Handle Ctrl+Left - move cursor left by word
            if key.code == KeyCode::Left && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.cursor_word_left();
                return Ok(KeyEventResult::Consumed);
            }

            // Handle Ctrl+Right - move cursor right by word
            if key.code == KeyCode::Right && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.cursor_word_right();
                return Ok(KeyEventResult::Consumed);
            }

            if let KeyCode::Char(c) = key.code {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.insert_char(c);
                    return Ok(KeyEventResult::Consumed);
                }
            } else if key.code == KeyCode::Backspace {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.delete_word_left();
                } else {
                    self.delete_char();
                }
                return Ok(KeyEventResult::Consumed);
            } else if key.code == KeyCode::Enter {
                self.insert_newline();
                return Ok(KeyEventResult::Consumed);
            } else if key.code == KeyCode::Tab {
                self.next_suggestion();
                return Ok(KeyEventResult::Consumed);
            }
        }

        Ok(KeyEventResult::Ignored)
    }
}

impl Component for SqlDialog {
    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<KeyEventResult> {
        self.handle_key_event(key)
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Escape | Action::Cancel => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // Close dialog
            }

            Action::RunSqlQuery => {
                if self.query_text.trim().is_empty() {
                    self.set_error("Query cannot be empty".to_string());
                    Ok(true)
                } else {
                    self.pending_result = Some(DialogResult::ExecuteQuery(self.query_text.clone()));
                    Ok(false) // Close dialog after execution
                }
            }

            Action::ClearSqlText => {
                self.clear_text();
                Ok(true)
            }

            Action::SqlNextSuggestion => {
                self.next_suggestion();
                Ok(true)
            }

            Action::SqlPreviousSuggestion => {
                self.prev_suggestion();
                Ok(true)
            }

            Action::SqlAcceptSuggestion => {
                self.accept_suggestion();
                Ok(true)
            }

            Action::ToggleHelp => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }

            Action::MoveLeft => {
                self.cursor_left();
                Ok(true)
            }

            Action::MoveRight => {
                self.cursor_right();
                Ok(true)
            }

            Action::Home => {
                self.cursor_home();
                Ok(true)
            }

            Action::End => {
                self.cursor_end();
                Ok(true)
            }

            Action::SqlUndo => {
                self.undo();
                Ok(true)
            }

            Action::SqlRedo => {
                self.redo();
                Ok(true)
            }

            _ => Ok(true), // Ignore other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Clear background
        frame.render_widget(Clear, area);

        // Outer block with double border
        let outer_block = Block::default()
            .title("SQL Query")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            });
        let outer_inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Calculate layout
        let (content_area, instructions_area) = if self.show_instructions {
            let instruction_height = 5;
            let content_height = outer_inner.height.saturating_sub(instruction_height + 1);

            (
                Rect {
                    y: outer_inner.y,
                    height: content_height,
                    ..outer_inner
                },
                Some(Rect {
                    y: outer_inner.y + content_height,
                    height: instruction_height,
                    ..outer_inner
                }),
            )
        } else {
            (outer_inner, None)
        };

        // Split content area for query and suggestions
        let suggestion_height = if !self.suggestions.is_empty() { 6 } else { 0 };
        let query_height = content_area.height.saturating_sub(suggestion_height);

        let query_area = Rect {
            height: query_height,
            ..content_area
        };

        let suggestion_area = if suggestion_height > 0 {
            Some(Rect {
                y: content_area.y + query_height,
                height: suggestion_height,
                ..content_area
            })
        } else {
            None
        };

        // Render query input block
        let query_block = Block::default()
            .title("Query ({table} will be replaced with dataset name)")
            .borders(Borders::ALL);
        let query_inner = query_block.inner(query_area);
        frame.render_widget(query_block, query_area);

        // Render query text with cursor
        let lines: Vec<&str> = self.query_text.split('\n').collect();
        let mut current_pos = 0;
        let mut cursor_line = 0;
        let mut cursor_col = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_end = current_pos + line.len();
            if self.cursor_pos >= current_pos && self.cursor_pos <= line_end {
                cursor_line = line_idx;
                cursor_col = self.cursor_pos - current_pos;
                break;
            }
            current_pos = line_end + 1; // +1 for newline
        }

        // Render visible lines
        let max_lines = query_inner.height as usize;
        let scroll_offset = cursor_line.saturating_sub(max_lines.saturating_sub(1));

        for (i, line) in lines.iter().enumerate().skip(scroll_offset).take(max_lines) {
            let y = query_inner.y + (i - scroll_offset) as u16;
            if y >= query_inner.bottom() {
                break;
            }

            let line_text = if i == cursor_line {
                // Show cursor
                // cursor_col is in bytes, but we need to work with character indices
                let char_col = line[..cursor_col.min(line.len())].chars().count();

                let before: String = line.chars().take(char_col).collect();
                let cursor_char = line.chars().nth(char_col).unwrap_or(' ');
                let after: String = line.chars().skip(char_col + 1).collect();

                Line::from(vec![
                    Span::raw(before),
                    Span::styled(cursor_char.to_string(), theme.selected_cell_style()),
                    Span::raw(after),
                ])
            } else {
                Line::from(*line)
            };

            frame.render_widget(
                Paragraph::new(line_text),
                Rect {
                    x: query_inner.x,
                    y,
                    width: query_inner.width,
                    height: 1,
                },
            );
        }

        // Render error message if present
        if let Some(ref error) = self.error_message {
            let error_y = query_inner.bottom().saturating_sub(2);
            frame.render_widget(
                Paragraph::new(error.as_str())
                    .style(theme.error_style().add_modifier(Modifier::BOLD)),
                Rect {
                    x: query_inner.x,
                    y: error_y,
                    width: query_inner.width,
                    height: 2,
                },
            );
        }

        // Render suggestions
        if let Some(sug_area) = suggestion_area {
            if !self.suggestions.is_empty() {
                // Build title with typeahead indicator
                let title = if !self.suggestion_typeahead_buffer.is_empty() {
                    format!(
                        "Suggestions - Type to search: '{}'",
                        self.suggestion_typeahead_buffer
                    )
                } else {
                    "Suggestions (↑/↓ to navigate, Enter to accept, Esc to return)".to_string()
                };

                let sug_block = Block::default().title(title).borders(Borders::ALL);
                let sug_inner = sug_block.inner(sug_area);
                frame.render_widget(sug_block, sug_area);

                // Calculate viewport height and cache it for navigation
                let viewport_height = sug_inner.height.max(1) as usize;
                self.cached_suggestion_viewport_height = viewport_height;

                // Update scroll offset before rendering
                self.update_scroll_for_selection(viewport_height);

                // Check if we need a scrollbar
                let has_scrollbar = self.suggestions.len() > viewport_height;

                // Calculate the width for rendering items (leave space for scrollbar if present)
                let item_width = if has_scrollbar {
                    sug_inner.width.saturating_sub(1)
                } else {
                    sug_inner.width
                };

                // Render visible suggestions manually
                let end =
                    (self.suggestion_scroll_offset + viewport_height).min(self.suggestions.len());
                for (vis_idx, i) in (self.suggestion_scroll_offset..end).enumerate() {
                    let suggestion = &self.suggestions[i];
                    let selected = Some(i) == self.selected_suggestion;

                    let style = if selected {
                        theme.selected_style()
                    } else {
                        theme.normal_style()
                    };

                    let y = sug_inner.y + vis_idx as u16;
                    // Format: name | type | description
                    let type_str = suggestion.suggestion_type.as_str();
                    let text = if !suggestion.description.is_empty() {
                        format!(
                            "{:20} | {:8} | {}",
                            suggestion.name, type_str, suggestion.description
                        )
                    } else {
                        format!("{:20} | {:8}", suggestion.name, type_str)
                    };
                    let para = Paragraph::new(text).style(style);
                    frame.render_widget(
                        para,
                        Rect {
                            x: sug_inner.x,
                            y,
                            width: item_width,
                            height: 1,
                        },
                    );
                }

                // Render scrollbar if there are more suggestions than visible rows
                if has_scrollbar {
                    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .style(theme.focused_border_style());
                    let mut scrollbar_state = ScrollbarState::new(self.suggestions.len())
                        .position(self.suggestion_scroll_offset)
                        .viewport_content_length(viewport_height);
                    frame.render_stateful_widget(scrollbar, sug_inner, &mut scrollbar_state);
                }
            }
        }

        // Render instructions
        if self.show_instructions {
            if let Some(inst_area) = instructions_area {
                let instructions_text = vec![
                    "  • Ctrl+Enter: Execute query",
                    "  • Tab: Enter suggestion mode  •  ↑/↓: Navigate  •  Ctrl+↑/↓: Page  •  Type to search",
                    "  • Enter (on suggestion): Accept suggestion  •  Esc: Return to query",
                    "  • Ctrl+←/→: Move by word  •  Ctrl+Backspace: Delete word  •  Ctrl+L: Clear text",
                    "  • Ctrl+Z: Undo  •  Ctrl+Y: Redo  •  Ctrl+I: Toggle help",
                ];

                let instructions = Paragraph::new(instructions_text.join("\n"))
                    .block(Block::default().borders(Borders::TOP).title("Instructions"))
                    .style(theme.warning_style())
                    .wrap(Wrap { trim: true });

                frame.render_widget(instructions, inst_area);
            }
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Escape,
            Action::Cancel,
            Action::RunSqlQuery,
            Action::ClearSqlText,
            Action::SqlNextSuggestion,
            Action::SqlPreviousSuggestion,
            Action::SqlAcceptSuggestion,
            Action::ToggleHelp,
            Action::MoveLeft,
            Action::MoveRight,
            Action::Home,
            Action::End,
        ]
    }

    fn name(&self) -> &str {
        "SqlDialog"
    }
}

impl Focusable for SqlDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
