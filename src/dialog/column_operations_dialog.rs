use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, BorderType, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::error_dialog::{ErrorDialog, render_error_dialog};
use crate::style::StyleConfig;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnOperationKind {
    GenerateEmbeddings,
    Pca,
    Cluster,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ColumnOperationsMode {
    SelectOperation,
    Error(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnOperationsDialog {
    pub mode: ColumnOperationsMode,
    pub styles: StyleConfig,
    pub show_instructions: bool,
    pub selected_index: usize,
    pub operations: Vec<ColumnOperationKind>,
    #[serde(skip)]
    pub config: crate::config::Config,
}

impl ColumnOperationsDialog {
    pub fn new() -> Self {
        Self {
            mode: ColumnOperationsMode::SelectOperation,
            styles: StyleConfig::default(),
            show_instructions: true,
            selected_index: 0,
            operations: vec![
                ColumnOperationKind::GenerateEmbeddings,
                ColumnOperationKind::Pca,
                ColumnOperationKind::Cluster,
            ],
            config: crate::config::Config::default(),
        }
    }

    fn operation_label(op: &ColumnOperationKind) -> &'static str {
        match op {
            ColumnOperationKind::GenerateEmbeddings => "Generate Embeddings",
            ColumnOperationKind::Pca => "PCA (Principal Component Analysis)",
            ColumnOperationKind::Cluster => "Cluster",
        }
    }

    fn operation_description(op: &ColumnOperationKind) -> &'static str {
        match op {
            ColumnOperationKind::GenerateEmbeddings => "Convert text data into numerical vectors for machine learning",
            ColumnOperationKind::Pca => "Reduce dimensionality while preserving most of the data variance",
            ColumnOperationKind::Cluster => "Group similar data points together using clustering algorithms",
        }
    }

    fn operation_requirements(op: &ColumnOperationKind) -> &'static str {
        match op {
            ColumnOperationKind::GenerateEmbeddings => "Requires: Text columns, OpenAI API key",
            ColumnOperationKind::Pca => "Requires: Numerical columns only",
            ColumnOperationKind::Cluster => "Requires: Numerical columns, specify number of clusters",
        }
    }

    fn render_operation_selection(&self, area: Rect, buf: &mut Buffer) {
        // Main content block
        let block = Block::default()
            .title("Select an operation")
            .borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        // Render operations list with enhanced formatting
        self.render_operations_list(inner, buf);
        
        // Render operation details for selected item
        if let Some(selected_op) = self.operations.get(self.selected_index) {
            self.render_operation_details(selected_op, inner, buf);
        } else {
            // Show helpful message when no operation is selected
            self.render_no_selection_message(inner, buf);
        }
    }

    fn render_operations_list(&self, area: Rect, buf: &mut Buffer) {
        let list_height = (area.height.saturating_sub(8)).min(self.operations.len() as u16);
        
        for (i, op) in self.operations
                .iter()
                .enumerate()
                .take(list_height as usize)
        {
            let y = area.y + i as u16;
            if y >= area.y + list_height { break; }
            
            let is_selected = i == self.selected_index;
            self.render_operation_item(op, area.x + 1, y, area.width - 2, is_selected, buf);
        }
    }

    fn render_operation_item(&self, op: &ColumnOperationKind, x: u16, y: u16, _width: u16, is_selected: bool, buf: &mut Buffer) {
        let marker = if is_selected { "▶" } else { " " };
        let label = Self::operation_label(op);
        
        let style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::White)
        };

        // Render marker and label
        let line = format!("{marker} {label}");
        buf.set_string(x, y, line, style);
    }

    fn render_operation_details(&self, op: &ColumnOperationKind, area: Rect, buf: &mut Buffer) {
        // Calculate required content
        let requirements = Self::operation_requirements(op);
        let description = Self::operation_description(op);

        // Calculate text width (accounting for borders and padding)
        let text_width = (area.width - 4).max(20) as usize; // 2 for borders + 2 for padding
        
        // Calculate required height for text content
        let desc_header = 1; // "Description:" line
        let desc_lines = self.calculate_text_lines(&format!("  {description}"), text_width);
        let req_header = 1; // "Requirements:" line
        let req_lines = self.calculate_text_lines(&format!("  {requirements}"), text_width);
        let spacing = 1; // Space between sections
        
        let total_content_height = desc_header + desc_lines + spacing + req_header + req_lines;
        let details_height = (total_content_height + 2).min(10) as u16; // +2 for borders, allow a bit taller
        
        // Position the details box
        let details_y = area.y + area.height.saturating_sub(details_height);
        
        if details_y < area.y || details_height < 3 {
            return; // Not enough space
        }

        // Create a details box
        let details_area = Rect::new(
            area.x + 1,
            details_y,
            area.width - 2,
            details_height
        );

        // Calculate inner area before creating the block
        let inner_details = Rect::new(
            details_area.x + 1,
            details_area.y + 1,
            details_area.width - 1,
            details_area.height - 1
        );

        // Draw a subtle border around details
        let details_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .style(Style::default().fg(Color::DarkGray));
        details_block.render(details_area, buf);
        
        // Render description section
        let desc_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        buf.set_string(inner_details.x, inner_details.y, "Description:", desc_style);
        self.render_wrapped_text(
            &format!("  {description}"),
            inner_details.x,
            inner_details.y + 1,
            inner_details.width,
            Style::default().fg(Color::Gray),
            buf
        );

        // Render requirements section below description
        let req_y = inner_details.y + 1 + desc_lines as u16 + spacing as u16;
        let req_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        buf.set_string(inner_details.x, req_y, "Requirements:", req_style);
        self.render_wrapped_text(
            &format!("  {requirements}"),
            inner_details.x,
            req_y + 1,
            inner_details.width,
            Style::default().fg(Color::White),
            buf
        );
    }

    fn calculate_text_lines(&self, text: &str, width: usize) -> usize {
        if text.is_empty() {
            return 0;
        }
        
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return 1;
        }
        
        let mut lines = 1;
        let mut current_line_length = 0;
        
        for word in words {
            let word_length = word.len() + 1; // +1 for space
            if current_line_length + word_length > width {
                lines += 1;
                current_line_length = word.len();
            } else {
                current_line_length += word_length;
            }
        }
        
        lines
    }

    fn render_wrapped_text(&self, text: &str, x: u16, y: u16, width: u16, style: Style, buf: &mut Buffer) {
        if text.is_empty() {
            return;
        }
        
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return;
        }
        
        let mut current_y = y;
        let mut current_line = String::new();
        let mut current_line_length = 0;
        let max_width = width as usize;
        
        for word in words {
            let word_length = word.len() + if current_line.is_empty() { 0 } else { 1 };
            
            if current_line_length + word_length > max_width && !current_line.is_empty() {
                // Render current line and start new one
                buf.set_string(x, current_y, &current_line, style);
                current_y += 1;
                current_line.clear();
                current_line_length = 0;
            }
            
            if !current_line.is_empty() {
                current_line.push(' ');
                current_line_length += 1;
            }
            current_line.push_str(word);
            current_line_length += word.len();
        }
        
        // Render the last line
        if !current_line.is_empty() {
            buf.set_string(x, current_y, &current_line, style);
        }
    }

    fn render_no_selection_message(&self, area: Rect, buf: &mut Buffer) {
        let message_y = area.y + area.height.saturating_sub(4);
        let message = "Use ↑/↓ to select an operation, then press Enter to apply";
        let style = Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::ITALIC);
        
        // Center the message horizontally
        let message_x = area.x + (area.width.saturating_sub(message.len() as u16)) / 2;
        buf.set_string(message_x, message_y, message, style);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let outer_block = Block::default()
            .title("Column Operations")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);
        let inner_total_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(inner_total_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;

        match &self.mode {
            ColumnOperationsMode::SelectOperation => {
                self.render_operation_selection(content_area, buf);
            }
            ColumnOperationsMode::Error(msg) => {
                let err = ErrorDialog::with_title(msg.clone(), "Error");
                render_error_dialog(&err, inner_total_area, buf);
            }
        }

        // Render instructions area if enabled
        if self.show_instructions && let Some(instructions_area) = layout.instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    fn apply_selected(&self) -> Option<Action> {
        if let Some(op) = self.operations.get(self.selected_index) {
            let op_name = match op {
                ColumnOperationKind::GenerateEmbeddings => "GenerateEmbeddings".to_string(),
                ColumnOperationKind::Pca => "Pca".to_string(),
                ColumnOperationKind::Cluster => "Cluster".to_string(),
            };
            return Some(Action::ColumnOperationRequested(op_name));
        }
        None
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match self.mode {
            ColumnOperationsMode::SelectOperation => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::Global, crate::action::Action::Up),
                    (crate::config::Mode::Global, crate::action::Action::Down),
                    (crate::config::Mode::Global, crate::action::Action::Enter),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                ])
            }
            ColumnOperationsMode::Error(_) => String::new(),
        }
    }

    pub fn handle_key_event_internal(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind == KeyEventKind::Press {
            // First, honor config-driven Global actions
            if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                match global_action {
                    Action::Escape => return Ok(Some(Action::DialogClose)),
                    Action::Enter => {
                        if let Some(a) = self.apply_selected() { return Ok(Some(a)); }
                        return Ok(None);
                    }
                    Action::Up => {
                        if !self.operations.is_empty() {
                            if self.selected_index == 0 { self.selected_index = self.operations.len() - 1; } else { self.selected_index -= 1; }
                        }
                        return Ok(None);
                    }
                    Action::Down => {
                        if !self.operations.is_empty() { self.selected_index = (self.selected_index + 1) % self.operations.len(); }
                        return Ok(None);
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // No dialog-specific actions for this simple dialog
            // All functionality is handled by Global actions
        }
        Ok(None)
    }
}

impl Default for ColumnOperationsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for ColumnOperationsDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { 
        self.config = _config; 
        Ok(()) 
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> { self.handle_key_event_internal(key) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}
