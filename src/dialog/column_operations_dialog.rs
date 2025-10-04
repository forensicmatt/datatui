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
                // Inner content block
                let block = Block::default()
                    .title("Select an operation")
                    .borders(Borders::ALL);
                let inner = block.inner(content_area);
                block.render(content_area, buf);

                // Render options list
                for (i, op) in self.operations.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y >= inner.y + inner.height { break; }
                    let label = Self::operation_label(op);
                    let is_selected = i == self.selected_index;
                    let marker = if is_selected { ">" } else { " " };
                    let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                    let line = format!("{marker} {label}");
                    buf.set_string(inner.x + 1, y, line, style);
                }

                // Bottom-right buttons: [Apply] [Close]
                let buttons = ["[Apply]", "[Close]"];
                let total_len: u16 = buttons.iter().map(|b| b.len() as u16 + 1).sum();
                let bx = inner.x + inner.width.saturating_sub(total_len + 1);
                let by = inner.y + inner.height.saturating_sub(1);
                let mut x = bx;
                for (idx, b) in buttons.iter().enumerate() {
                    let style = if idx == 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) };
                    buf.set_string(x, by, *b, style);
                    x += b.len() as u16 + 1;
                }
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


