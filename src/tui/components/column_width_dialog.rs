use crate::core::ColumnWidthConfig;
use crate::tui::{Action, Component};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::collections::HashMap;

/// Result from column width dialog
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// Apply the configuration changes
    ApplyConfig(ColumnWidthConfig),
    /// Reorder columns
    ReorderColumns(Vec<String>),
    /// Close without applying
    Close,
}

/// Input mode for width editing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,       // Not editing
    EditingWidth, // Editing a width value
}

/// Column width dialog for configuring column widths and visibility
pub struct ColumnWidthDialog {
    columns: Vec<String>,
    config: ColumnWidthConfig,
    active_index: usize,
    scroll_offset: usize,
    pub input_mode: InputMode,
    pub input_buffer: String,
    editing_column: Option<usize>,
    pending_result: Option<DialogResult>,
    current_calculated_widths: HashMap<String, u16>,
    show_instructions: bool,
}

impl ColumnWidthDialog {
    /// Create a new column width dialog
    pub fn new(columns: Vec<String>) -> Self {
        let config = ColumnWidthConfig::from_columns(columns.clone());

        Self {
            columns,
            config,
            active_index: 0,
            scroll_offset: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            editing_column: None,
            pending_result: None,
            current_calculated_widths: HashMap::new(),
            show_instructions: true,
        }
    }

    /// Set the column configuration
    pub fn set_config(&mut self, config: ColumnWidthConfig) {
        self.config = config;
    }

    /// Set the current calculated widths (for display purposes)
    pub fn set_calculated_widths(&mut self, widths: HashMap<String, u16>) {
        self.current_calculated_widths = widths;
    }

    /// Take the pending result (if any)
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }

    /// Get column at index
    fn get_column(&self, index: usize) -> Option<&String> {
        if index == 0 {
            None // Index 0 is auto-expand toggle
        } else {
            self.columns.get(index - 1)
        }
    }

    /// Start editing a column width
    fn start_editing(&mut self, col_index: usize) {
        if let Some(col_name) = self.columns.get(col_index) {
            self.editing_column = Some(col_index);
            self.input_mode = InputMode::EditingWidth;

            // Initialize buffer with current width if set
            if let Some(width) = self.config.get_effective_width(col_name) {
                self.input_buffer = width.to_string();
            } else {
                // Use calculated width or default
                self.input_buffer = self
                    .current_calculated_widths
                    .get(col_name)
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| "10".to_string());
            }
        }
    }

    /// Finish editing and apply width
    fn finish_editing(&mut self) {
        if let Some(col_idx) = self.editing_column {
            if let Some(col_name) = self.columns.get(col_idx) {
                if let Ok(width) = self.input_buffer.parse::<u16>() {
                    if (4..=255).contains(&width) {
                        self.config.manual_widths.insert(col_name.clone(), width);
                    }
                }
            }
        }

        self.editing_column = None;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
    }

    /// Cancel editing
    fn cancel_editing(&mut self) {
        self.editing_column = None;
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
    }

    /// Toggle auto-expand
    fn toggle_auto_expand(&mut self) {
        let was_auto = self.config.auto_expand;
        self.config.auto_expand = !was_auto;

        // When turning off auto-expand, lock all columns to their current widths
        if was_auto && !self.config.auto_expand {
            for col in &self.columns {
                if !self.config.manual_widths.contains_key(col) {
                    if let Some(&width) = self.current_calculated_widths.get(col) {
                        self.config.manual_widths.insert(col.clone(), width);
                    }
                }
            }
        }
        // When turning auto-expand back on, clear all manual widths so they show "auto"
        else if !was_auto && self.config.auto_expand {
            self.config.manual_widths.clear();
        }
    }

    /// Toggle column visibility
    fn toggle_visibility(&mut self, col_index: usize) -> Result<()> {
        if let Some(col_name) = self.columns.get(col_index) {
            let is_visible = self.config.is_column_visible(col_name);

            // Check if hiding would hide all columns
            if is_visible {
                let would_be_visible = self
                    .columns
                    .iter()
                    .filter(|c| {
                        if *c == col_name {
                            false
                        } else {
                            self.config.is_column_visible(c)
                        }
                    })
                    .count();

                if would_be_visible == 0 {
                    return Err(color_eyre::eyre::eyre!(
                        "Cannot hide all columns. At least one must be visible."
                    ));
                }
            }

            self.config
                .hidden_columns
                .insert(col_name.clone(), is_visible);
        }

        Ok(())
    }

    /// Move column up in the list
    fn move_column_up(&mut self, col_index: usize) -> bool {
        if col_index > 0 && col_index < self.columns.len() {
            self.columns.swap(col_index, col_index - 1);
            self.config.column_order = self.columns.clone();

            // Move active index with the column
            if self.active_index == col_index + 1 {
                self.active_index -= 1;
            }

            true
        } else {
            false
        }
    }

    /// Move column down in the list
    fn move_column_down(&mut self, col_index: usize) -> bool {
        if col_index < self.columns.len() - 1 {
            self.columns.swap(col_index, col_index + 1);
            self.config.column_order = self.columns.clone();

            // Move active index with the column
            if self.active_index == col_index + 1 {
                self.active_index += 1;
            }

            true
        } else {
            false
        }
    }
}

impl Component for ColumnWidthDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Escape => {
                if self.input_mode == InputMode::EditingWidth {
                    // Cancel editing
                    self.cancel_editing();
                    Ok(true)
                } else {
                    // Close dialog WITHOUT applying changes
                    self.pending_result = Some(DialogResult::Close);
                    Ok(false) // Close dialog
                }
            }

            Action::Confirm => {
                if self.input_mode == InputMode::EditingWidth {
                    // Finish editing
                    self.finish_editing();
                    Ok(true)
                } else {
                    // Apply config and close dialog
                    self.pending_result = Some(DialogResult::ApplyConfig(self.config.clone()));
                    Ok(false) // Close dialog
                }
            }

            // Space key: Start editing width OR toggle auto-expand
            Action::EditWidth => {
                if self.input_mode == InputMode::Normal {
                    if self.active_index == 0 {
                        // Toggle auto-expand
                        self.toggle_auto_expand();
                    } else {
                        // Start editing width for selected column
                        if let Some(col_idx) = self.active_index.checked_sub(1) {
                            self.start_editing(col_idx);
                        }
                    }
                }
                Ok(true)
            }

            Action::MoveUp => {
                if self.input_mode == InputMode::Normal && self.active_index > 0 {
                    self.active_index -= 1;

                    // Adjust scroll if needed
                    if self.active_index < self.scroll_offset {
                        self.scroll_offset = self.active_index;
                    }
                }
                Ok(true)
            }

            Action::MoveDown => {
                if self.input_mode == InputMode::Normal {
                    let max_index = self.columns.len(); // +1 for auto-expand, -1 for 0-based
                    if self.active_index < max_index {
                        self.active_index += 1;
                    }
                }
                Ok(true)
            }

            Action::ToggleVisibility => {
                if self.input_mode == InputMode::Normal {
                    if let Some(col_idx) = self.active_index.checked_sub(1) {
                        let _ = self.toggle_visibility(col_idx); // Ignore error for now
                    }
                }
                Ok(true)
            }

            Action::MoveColumnUp => {
                if self.input_mode == InputMode::Normal {
                    if let Some(col_idx) = self.active_index.checked_sub(1) {
                        if self.move_column_up(col_idx) {
                            self.pending_result =
                                Some(DialogResult::ReorderColumns(self.columns.clone()));
                        }
                    }
                }
                Ok(true)
            }

            Action::MoveColumnDown => {
                if self.input_mode == InputMode::Normal {
                    if let Some(col_idx) = self.active_index.checked_sub(1) {
                        if self.move_column_down(col_idx) {
                            self.pending_result =
                                Some(DialogResult::ReorderColumns(self.columns.clone()));
                        }
                    }
                }
                Ok(true)
            }

            Action::ToggleHelp => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }

            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Clear background
        frame.render_widget(Clear, area);

        // Outer block
        let outer_block = Block::default()
            .title("Column Width Configuration")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Calculate visible rows (leave space for help text at bottom)
        let max_rows = (inner_area.height.saturating_sub(4)) as usize;

        // Render auto-expand toggle
        let auto_text = format!(
            "{} Auto-expand columns: {}",
            if self.active_index == 0 { ">" } else { " " },
            if self.config.auto_expand {
                "✓"
            } else {
                "✗"
            }
        );
        let auto_style = if self.active_index == 0 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        frame.render_widget(
            Paragraph::new(Line::from(auto_text)).style(auto_style),
            Rect {
                x: inner_area.x,
                y: inner_area.y,
                width: inner_area.width,
                height: 1,
            },
        );

        // Render column list
        let list_start_y = inner_area.y + 2;
        let end = (self.scroll_offset + max_rows).min(self.columns.len());

        for (vis_idx, col_idx) in (self.scroll_offset..end).enumerate() {
            let y = list_start_y + vis_idx as u16;
            let col_name = &self.columns[col_idx];
            let selected = col_idx + 1 == self.active_index;
            let is_editing = self.editing_column == Some(col_idx);
            let is_visible = self.config.is_column_visible(col_name);

            let width_display = if is_editing {
                format!("[{}]", self.input_buffer)
            } else if let Some(w) = self.config.get_effective_width(col_name) {
                format!("{}", w)
            } else {
                "auto".to_string()
            };

            let visibility_indicator = if is_visible { "☑" } else { "☐" };

            let text = format!(
                "{} {} {}: {}",
                if selected { ">" } else { " " },
                visibility_indicator,
                col_name,
                width_display
            );

            let mut style = Style::default();
            if selected {
                style = style
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
            } else if col_idx % 2 == 1 {
                style = style.bg(Color::Rgb(30, 30, 30));
            }

            if !is_visible {
                style = style.fg(Color::DarkGray);
            }

            frame.render_widget(
                Paragraph::new(Line::from(text)).style(style),
                Rect {
                    x: inner_area.x,
                    y,
                    width: inner_area.width,
                    height: 1,
                },
            );
        }

        // Render instructions block if visible
        if self.show_instructions {
            let instructions_height = 5; // Allocate 5 lines for instructions
            let instructions_area = Rect {
                x: inner_area.x,
                y: inner_area.bottom().saturating_sub(instructions_height),
                width: inner_area.width,
                height: instructions_height,
            };

            let instructions_text = if self.input_mode == InputMode::EditingWidth {
                vec![
                    Line::from("Editing Width:"),
                    Line::from("  • Type numbers (4-255) to set width"),
                    Line::from("  • Enter: Apply width"),
                    Line::from("  • Esc: Cancel editing"),
                ]
            } else {
                vec![
                    Line::from("Column Configuration:"),
                    Line::from("  • ↑/↓: Select column  • Enter: Apply & Close  • Esc: Cancel"),
                    Line::from("  • Space: Edit width  • t: Toggle visibility"),
                    Line::from("  • Ctrl+↑/↓: Reorder columns  • Ctrl+i: Toggle this help"),
                ]
            };

            let instructions_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray))
                .title("Instructions (Ctrl+i to hide)");

            let instructions_para = Paragraph::new(instructions_text)
                .block(instructions_block)
                .style(Style::default().fg(Color::Gray))
                .wrap(ratatui::widgets::Wrap { trim: true });

            frame.render_widget(instructions_para, instructions_area);
        } else {
            // Show minimal help hint
            let hint_area = Rect {
                x: inner_area.x,
                y: inner_area.bottom().saturating_sub(1),
                width: inner_area.width,
                height: 1,
            };

            frame.render_widget(
                Paragraph::new("Press Ctrl+i for help").style(Style::default().fg(Color::DarkGray)),
                hint_area,
            );
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[]
    }

    fn name(&self) -> &str {
        "ColumnWidthDialog"
    }
}
