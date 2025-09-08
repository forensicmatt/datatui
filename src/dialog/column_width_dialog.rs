//! ColumnWidthDialog: Popup dialog for configuring column widths in a DataFrame table

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;

/// Represents column width configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnWidthConfig {
    /// Whether columns should auto-expand based on content
    pub auto_expand: bool,
    /// Manual column widths (column name -> width in characters)
    pub manual_widths: HashMap<String, u16>,
    /// Hidden columns (column name -> hidden status)
    pub hidden_columns: HashMap<String, bool>,
}

impl Default for ColumnWidthConfig {
    fn default() -> Self {
        Self {
            auto_expand: true,
            manual_widths: HashMap::new(),
            hidden_columns: HashMap::new(),
        }
    }
}

impl Display for ColumnWidthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Auto-expand: {}", if self.auto_expand { "Yes" } else { "No" })?;
        if !self.manual_widths.is_empty() {
            write!(f, ", Manual widths: {}", self.manual_widths.len())?;
        }
        Ok(())
    }
}

/// Dialog mode: main view
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnWidthDialogMode {
    Main,
}

/// Input mode for column width editing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMode {
    Number,
    Auto,
}

/// ColumnWidthDialog: UI for configuring column widths
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnWidthDialog {
    pub columns: Vec<String>,
    pub config: ColumnWidthConfig,
    pub active_index: usize,
    pub mode: ColumnWidthDialogMode,
    /// Scrolling state for the column list
    pub scroll_offset: usize,
    /// Current input buffer for editing column width
    pub input_buffer: String,
    /// Input mode for the current column
    pub input_mode: InputMode,
    /// Currently editing column index
    pub editing_column: Option<usize>,
    pub show_instructions: bool, // new: show instructions area (default true)
}

impl ColumnWidthDialog {
    /// Create a new ColumnWidthDialog
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            config: ColumnWidthConfig::default(),
            active_index: 0,
            mode: ColumnWidthDialogMode::Main,
            scroll_offset: 0,
            input_buffer: String::new(),
            input_mode: InputMode::Number,
            editing_column: None,
            show_instructions: true,
        }
    }

    /// Set the columns for the dialog
    pub fn set_columns(&mut self, columns: Vec<String>) {
        self.columns = columns;
        // Reset active index if it's out of bounds
        if self.active_index > self.columns.len() { // +1 for auto-expand toggle
            self.active_index = 0;
        }
        // Ensure scroll offset is within bounds
        if self.scroll_offset >= self.columns.len() {
            self.scroll_offset = 0;
        }
        // Initialize hidden columns for any new columns that don't have a setting
        for col in &self.columns {
            if !self.config.hidden_columns.contains_key(col) {
                self.config.hidden_columns.insert(col.clone(), false);
            }
        }
    }

    /// Set the current column width configuration
    pub fn set_config(&mut self, config: ColumnWidthConfig) {
        self.config = config;
    }

    /// Get the current width for a column
    fn get_column_width(&self, column: &str) -> Option<u16> {
        // Always show manual widths if they exist, regardless of auto_expand setting
        self.config.manual_widths.get(column).copied()
    }

    /// Set the width for a column
    fn set_column_width(&mut self, column: &str, width: Option<u16>) {
        if let Some(w) = width {
            self.config.manual_widths.insert(column.to_string(), w);
        } else {
            self.config.manual_widths.remove(column);
        }
    }

    /// Get the hidden status for a column
    fn get_column_hidden(&self, column: &str) -> bool {
        self.config.hidden_columns.get(column)
            .copied()
            .unwrap_or(false)
    }

    /// Toggle the hidden status for a column
    fn toggle_column_hidden(&mut self, column: &str) {
        let current = self.get_column_hidden(column);
        self.config.hidden_columns.insert(column.to_string(), !current);
    }

    /// Move a column up or down in the order
    fn move_column(&mut self, col_idx: usize, direction: i32) -> bool {
        if col_idx >= self.columns.len() {
            return false;
        }
        
        let new_idx = col_idx as i32 + direction;
        if new_idx < 0 || new_idx >= self.columns.len() as i32 {
            return false;
        }
        
        let new_idx = new_idx as usize;
        
        // Move the column in the columns vector
        let column = self.columns.remove(col_idx);
        self.columns.insert(new_idx, column);
        
        // Update active_index to follow the moved column
        if self.active_index > 0 { // Not the auto-expand toggle
            let actual_col_idx = self.active_index - 1; // Convert to 0-based column index
            if actual_col_idx == col_idx {
                // We moved the selected column, update the active_index
                self.active_index = new_idx + 1; // Convert back to 1-based with auto-expand
            } else if actual_col_idx > col_idx && actual_col_idx <= new_idx {
                // A column was moved up, shifting our selection down
                self.active_index -= 1;
            } else if actual_col_idx < col_idx && actual_col_idx >= new_idx {
                // A column was moved down, shifting our selection up
                self.active_index += 1;
            }
        }
        
        // Adjust scroll offset if needed
        if self.active_index > 0 {
            let actual_col_idx = self.active_index - 1;
            if actual_col_idx < self.scroll_offset {
                self.scroll_offset = actual_col_idx;
            } else if actual_col_idx >= self.scroll_offset + 10 { // Assuming 10 is max visible rows
                self.scroll_offset = actual_col_idx.saturating_sub(9);
            }
        }
        
        true
    }

    /// Render the dialog (UI skeleton)
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        // Clear the background for the popup
        Clear.render(area, buf);
        let instructions = "Space: Auto/Number  h: Toggle Hide  Ctrl+↑/↓: Reorder  Esc: Apply/Close";
        // Outer container with double border and title "Column Widths"
        let outer_block = Block::default()
            .title("Column Widths")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, Some(instructions));
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        // Draw dialog frame
        let block = Block::default()
            .title("Configuration")
            .borders(Borders::ALL);
        block.render(content_area, buf);

        // Calculate the maximum number of rows that can be displayed
        let start_y = content_area.y + 1;
        let first_column_y = start_y + 1;
        let start_x = content_area.x + 1;
        let max_rows = (content_area.bottom().saturating_sub(start_y + 2 + 2)).min(content_area.height.saturating_sub(2 + 2)) as usize;
        let max_rows = std::cmp::max(1, max_rows + 2);
        match self.mode {
            ColumnWidthDialogMode::Main => {
                // Auto-expand toggle
                let auto_text = format!("Auto-expand columns: {}", if self.config.auto_expand { "✓" } else { "✗" });
                let auto_style = if self.active_index == 0 {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                buf.set_string(start_x + 1, start_y, auto_text, auto_style);
                // Column list with scroll bar
                let list_start_y = first_column_y;
                let end = (self.scroll_offset + max_rows).min(self.columns.len());
                // Draw scroll bar on the left side
                if self.columns.len() > max_rows {
                    let scroll_bar_x = start_x;
                    // Calculate scroll bar position
                    let scroll_bar_height = max_rows;
                    let scroll_bar_y_start = list_start_y;
                    // Calculate thumb position and size
                    let total_items = self.columns.len();
                    let visible_items = max_rows;
                    let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
                    let thumb_position = if total_items > visible_items {
                        (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
                    } else {
                        0
                    };
                    // Draw scroll bar track
                    for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                        buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
                    }
                    // Draw scroll bar thumb
                    let thumb_start = scroll_bar_y_start + thumb_position as u16;
                    let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
                    for y in thumb_start..thumb_end {
                        buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
                    }
                }
                for (vis_idx, i) in (self.scroll_offset..end).enumerate() {
                    let y = list_start_y + vis_idx as u16;
                    let col = &self.columns[i];
                    let selected = i + 1 == self.active_index; // +1 because index 0 is auto-expand toggle
                    let zebra = i % 2 == 0;
                    let is_editing = self.editing_column == Some(i);
                    let is_hidden = self.get_column_hidden(col);
                    
                    let width_display = match self.get_column_width(col) {
                        Some(w) => format!("{w}"),
                        None => "auto".to_string(),
                    };
                    
                    // Create toggle box for hidden status
                    let toggle_box = if is_hidden { "[✓]" } else { "[ ]" };
                    
                    let text = if selected {
                        if is_editing {
                            match self.input_mode {
                                InputMode::Number => format!("> {toggle_box} {col}: [{}]", self.input_buffer),
                                InputMode::Auto => format!("> {toggle_box} {col}: [auto]"),
                            }
                        } else {
                            format!("> {toggle_box} {col}: {width_display}")
                        }
                    } else {
                        format!("  {toggle_box} {col}: {width_display}")
                    };
                    
                    let mut style = Style::default();
                    if selected {
                        style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
                    } else if zebra {
                        style = style.bg(Color::Rgb(30,30,30));
                    }
                    if is_hidden {
                        style = style.fg(Color::DarkGray);
                    }
                    
                    // Use the appropriate x position based on whether scroll bar is shown
                    let x_pos = content_area.x + 2; // +2 for scroll bar (1 char) + space;
                    buf.set_string(x_pos, y, text, style);
                }
            }
        }
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
            }
        max_rows
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        if key.kind == KeyEventKind::Press {
            // Handle Ctrl+I to toggle instructions
            if key.code == KeyCode::Char('i') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                self.show_instructions = !self.show_instructions;
                return None;
            }
            
            match self.mode {
                ColumnWidthDialogMode::Main => {
                    match key.code {
                        KeyCode::Esc => return Some(Action::ColumnWidthDialogApplied(self.config.clone())),
                        KeyCode::Up => {
                            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                // Ctrl+Up: Move column up
                                if self.active_index > 0 {
                                    let col_idx = self.active_index - 1; // -1 because index 0 is auto-expand
                                    if col_idx < self.columns.len()
                                        && self.move_column(col_idx, -1) {
                                            // Return the reorder action
                                            return Some(Action::ColumnWidthDialogReordered(self.columns.clone()));
                                        }
                                }
                            } else {
                                // Normal Up: Move selection up
                                if self.active_index > 0 {
                                    self.active_index -= 1;
                                    // Adjust scroll if needed
                                    if self.active_index < self.scroll_offset {
                                        self.scroll_offset = self.active_index;
                                    }
                                }
                            }
                        }
                        KeyCode::Down => {
                            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                                // Ctrl+Down: Move column down
                                if self.active_index > 0 {
                                    let col_idx = self.active_index - 1; // -1 because index 0 is auto-expand
                                    if col_idx < self.columns.len()
                                        && self.move_column(col_idx, 1) {
                                            // Return the reorder action
                                            return Some(Action::ColumnWidthDialogReordered(self.columns.clone()));
                                        }
                                }
                            } else {
                                // Normal Down: Move selection down
                                let max_index = self.columns.len() + 1; // +1 for auto-expand toggle
                                if self.active_index + 1 < max_index {
                                    self.active_index += 1;
                                    // Adjust scroll if needed
                                    let visible_end = self.scroll_offset + max_rows;
                                    if self.active_index > visible_end {
                                        self.scroll_offset = self.active_index.saturating_sub(max_rows);
                                    }
                                }
                            }
                        }
                        KeyCode::Char(' ') => {
                            if self.active_index == 0 {
                                // Toggle auto-expand
                                self.config.auto_expand = !self.config.auto_expand;
                            } else {
                                // Toggle between number and auto for the selected column
                                let col_idx = self.active_index - 1; // -1 because index 0 is auto-expand
                                if col_idx < self.columns.len() {
                                    if self.editing_column == Some(col_idx) {
                                        // Toggle input mode
                                        self.input_mode = match self.input_mode {
                                            InputMode::Number => InputMode::Auto,
                                            InputMode::Auto => InputMode::Number,
                                        };
                                        if let InputMode::Number = self.input_mode {
                                            // Initialize with current width if available
                                            if let Some(width) = self.get_column_width(&self.columns[col_idx]) {
                                                self.input_buffer = width.to_string();
                                            } else {
                                                self.input_buffer.clear();
                                            }
                                        }
                                    } else {
                                        // Start editing this column
                                        self.editing_column = Some(col_idx);
                                        self.input_mode = InputMode::Number;
                                        if let Some(width) = self.get_column_width(&self.columns[col_idx]) {
                                            self.input_buffer = width.to_string();
                                        } else {
                                            self.input_buffer.clear();
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Enter => {
                            // Apply the current editing state immediately
                            if let Some(col_idx) = self.editing_column
                                && col_idx < self.columns.len() {
                                    let col_name = self.columns[col_idx].clone();
                                    match self.input_mode {
                                        InputMode::Number => {
                                            if let Ok(width) = self.input_buffer.parse::<u16>()
                                                && (4..=255).contains(&width) {
                                                    self.set_column_width(&col_name, Some(width));
                                                }
                                        }
                                        InputMode::Auto => {
                                            self.set_column_width(&col_name, None);
                                        }
                                    }
                                }
                            // Stop editing
                            self.editing_column = None;
                            self.input_buffer.clear();
                        }
                        KeyCode::Char('h') => {
                            // Toggle hidden status for the selected column
                            if self.active_index > 0 {
                                let col_idx = self.active_index - 1; // -1 because index 0 is auto-expand
                                if col_idx < self.columns.len() {
                                    let col_name = self.columns[col_idx].clone();
                                    self.toggle_column_hidden(&col_name);
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            // Handle direct number input when editing
                            if let Some(editing_col) = self.editing_column
                                && editing_col == self.active_index - 1 && matches!(self.input_mode, InputMode::Number)
                                    && c.is_ascii_digit() {
                                        self.input_buffer.push(c);
                                    }
                        }
                        KeyCode::Backspace => {
                            // Handle backspace when editing
                            if let Some(editing_col) = self.editing_column
                                && editing_col == self.active_index - 1 && matches!(self.input_mode, InputMode::Number) {
                                    self.input_buffer.pop();
                                }
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }
}

impl Component for ColumnWidthDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
        Ok(())
    }

    fn init(&mut self, _area: Size) -> Result<()> {
        Ok(())
    }

    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        match event {
            Some(Event::Key(key_event)) => {
                let max_rows = 10; // Default fallback
                Ok(self.handle_key_event(key_event, max_rows))
            }
            _ => Ok(None),
        }
    }

    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        Ok(None) // Handled in handle_events
    }

    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let _max_rows = self.render(area, frame.buffer_mut());
        Ok(())
    }
} 