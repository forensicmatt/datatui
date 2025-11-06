use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, BorderType, Paragraph, Wrap};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use serde::{Deserialize, Serialize};
use arboard::Clipboard;
use tui_textarea::TextArea;

use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
// use crate::dialog::error_dialog::{ErrorDialog, render_error_dialog};
use crate::style::StyleConfig;
use crate::action::Action;
use super::column_operations_dialog::ColumnOperationKind;
use crate::dialog::LlmProvider;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterAlgorithm {
    Kmeans,
    Dbscan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct KmeansOptions {
    pub number_of_clusters: usize,
    pub runs: usize,
    pub tolerance: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DbscanOptions {
    pub minimum_points: usize,
    pub tolerance: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbeddingParams {
    pub model_name: String,
    pub num_dimensions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationOptions {
    GenerateEmbeddings { model_name: String, num_dimensions: usize },
    Pca { target_embedding_size: usize },
    Cluster { algorithm: ClusterAlgorithm, kmeans: Option<KmeansOptions>, dbscan: Option<DbscanOptions> },
    SortByPromptSimilarity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnOperationConfig {
    pub operation: ColumnOperationKind,
    pub new_column_name: String,
    pub source_column: String,
    pub options: OperationOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ColumnOperationOptionsMode {
    Input,
    Error(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnOperationOptionsDialog {
    pub styles: StyleConfig,
    pub show_instructions: bool,
    pub mode: ColumnOperationOptionsMode,
    pub operation: ColumnOperationKind,
    pub new_column_name: String,
    #[serde(skip)]
    pub new_column_input: TextArea<'static>,
    pub selected_field_index: usize,
    pub selected_provider: LlmProvider,
    pub model_name: String,
    #[serde(skip)]
    pub model_name_input: TextArea<'static>,
    pub num_dimensions: usize,
    #[serde(skip)]
    pub num_dimensions_input: TextArea<'static>,
    pub target_embedding_size: usize,
    #[serde(skip)]
    pub target_embedding_size_input: TextArea<'static>,
    pub cluster_algorithm: ClusterAlgorithm,
    pub kmeans: KmeansOptions,
    pub dbscan: DbscanOptions,
    #[serde(skip)]
    pub kmeans_number_of_clusters_input: TextArea<'static>,
    #[serde(skip)]
    pub kmeans_runs_input: TextArea<'static>,
    #[serde(skip)]
    pub kmeans_tolerance_input: TextArea<'static>,
    #[serde(skip)]
    pub dbscan_minimum_points_input: TextArea<'static>,
    #[serde(skip)]
    pub dbscan_tolerance_input: TextArea<'static>,
    pub buttons_mode: bool,
    pub selected_button: usize,
    pub columns: Vec<String>,
    pub selected_column_index: usize,
    #[serde(skip)]
    pub config: crate::config::Config,
}

impl ColumnOperationOptionsDialog {
    pub fn new(operation: ColumnOperationKind) -> Self {
        Self {
            styles: StyleConfig::default(),
            show_instructions: true,
            mode: ColumnOperationOptionsMode::Input,
            operation,
            new_column_name: String::new(),
            new_column_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t
            },
            selected_field_index: 0,
            selected_provider: LlmProvider::OpenAI,
            model_name: String::from("text-embedding-3-small"),
            model_name_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("text-embedding-3-small");
                t
            },
            num_dimensions: 1536,
            num_dimensions_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("1536");
                t
            },
            target_embedding_size: 0,
            target_embedding_size_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("0");
                t
            },
            cluster_algorithm: ClusterAlgorithm::Kmeans,
            kmeans: KmeansOptions { number_of_clusters: 8, runs: 1, tolerance: 1 },
            dbscan: DbscanOptions { minimum_points: 5, tolerance: 1 },
            kmeans_number_of_clusters_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("8");
                t
            },
            kmeans_runs_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("1");
                t
            },
            kmeans_tolerance_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("1");
                t
            },
            dbscan_minimum_points_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("5");
                t
            },
            dbscan_tolerance_input: {
                let mut t = TextArea::default();
                t.set_block(Block::default());
                t.insert_str("1");
                t
            },
            buttons_mode: false,
            selected_button: 0,
            columns: Vec::new(),
            selected_column_index: 0,
            config: crate::config::Config::default(),
        }
    }

    pub fn new_with_columns(operation: ColumnOperationKind, columns: Vec<String>, selected_column_index: usize) -> Self {
        let mut s = Self::new(operation);
        s.columns = columns;
        s.selected_column_index = selected_column_index.min(s.columns.len().saturating_sub(1));
        s
    }

    fn fields_for_operation(&self) -> Vec<String> {
        let mut fields = vec![
            "New Column Name:".to_string(),
            format!("Source Column: {}", self.columns.get(self.selected_column_index).cloned().unwrap_or_default()),
        ];
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                fields.push(format!("Provider: {}", self.selected_provider.display_name()));
                fields.push("Model Name:".to_string());
                fields.push(format!("Number of Dimensions: {}", self.num_dimensions));
            }
            ColumnOperationKind::Pca => {
                fields.push(format!("Target Embedding Size: {}", self.target_embedding_size));
            }
            ColumnOperationKind::Cluster => {
                fields.push(format!("Algorithm: {}", match self.cluster_algorithm { ClusterAlgorithm::Kmeans => "Kmeans", ClusterAlgorithm::Dbscan => "Dbscan" }));
                match self.cluster_algorithm {
                    ClusterAlgorithm::Kmeans => {
                        fields.push(format!("Number of Clusters: {}", self.kmeans.number_of_clusters));
                        fields.push(format!("Runs: {}", self.kmeans.runs));
                        fields.push(format!("Tolerance: {}", self.kmeans.tolerance));
                    }
                    ClusterAlgorithm::Dbscan => {
                        fields.push(format!("Minimum Points: {}", self.dbscan.minimum_points));
                        fields.push(format!("Tolerance: {}", self.dbscan.tolerance));
                    }
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => {
                // No extra fields; handled by dedicated dialog
            }
        }
        fields
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let outer_block = Block::default()
            .title("Operation Options")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);
        let inner_total_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let instructions = match self.build_instructions_from_config() {
            s if s.trim().is_empty() => None,
            s => Some(s),
        };
        let layout = split_dialog_area(
            inner_total_area,
            self.show_instructions, 
            instructions.as_deref()
        );
        let content_area = layout.content_area;

        match &self.mode {
            ColumnOperationOptionsMode::Input => {
                let block = Block::default()
                    .title(match self.operation { ColumnOperationKind::GenerateEmbeddings => "Generate Embeddings", ColumnOperationKind::Pca => "PCA", ColumnOperationKind::Cluster => "Cluster", ColumnOperationKind::SortByPromptSimilarity => "Prompt Similarity" })
                    .borders(Borders::ALL);
                let inner = block.inner(content_area);
                block.render(content_area, buf);

                if matches!(self.operation, ColumnOperationKind::GenerateEmbeddings) {
                    // Row 0: New Column Name (text input)
                    let i = 0usize;
                    let y = inner.y;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "New Column Name:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(inner.x + 1, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                    let mut ta = self.new_column_input.clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);

                    // Row 1: Source Column (enum display)
                    let i = 1usize;
                    let y = inner.y + 1;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let line = format!("Source Column: {}", self.columns.get(self.selected_column_index).cloned().unwrap_or_default());
                    let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(inner.x + 1, y, line, style);

                    // Embedding Options block for Provider, Model, and Dimensions
                    let emb_outer = Rect {
                        x: inner.x + 1,
                        y: inner.y + 3,
                        width: inner.width.saturating_sub(2),
                        height: 5, // three lines of content
                    };
                    let emb_block = Block::default().title("Embedding Options").borders(Borders::ALL);
                    let emb_inner = emb_block.inner(emb_outer);
                    emb_block.render(emb_outer, buf);

                    // Row 2 inside block: Provider (enum)
                    let i = 2usize;
                    let y = emb_inner.y;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let line = format!("Provider: {}", self.selected_provider.display_name());
                    let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, line, style);

                    // Row 3 inside block: Model Name (text input)
                    let i = 3usize;
                    let y = emb_inner.y + 1;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "Model Name:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: emb_inner.x + label_width, y, width: emb_inner.width.saturating_sub(label_width), height: 1 };
                    let mut ta = self.model_name_input.clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);

                    // Row 4 inside block: Number of Dimensions (number input)
                    let i = 4usize;
                    let y = emb_inner.y + 2;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "Number of Dimensions:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: emb_inner.x + label_width, y, width: emb_inner.width.saturating_sub(label_width), height: 1 };
                    let mut ta = self.get_number_input_by_index(i).clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);
                } else {
                    let lines = self.fields_for_operation();
                    for (i, line) in lines.iter().enumerate() {
                        let y = inner.y + i as u16;
                        if y >= inner.y + inner.height { break; }
                        let is_selected = !self.buttons_mode && i == self.selected_field_index;
                        if (i == 0) || (self.operation == ColumnOperationKind::GenerateEmbeddings && i == 2) {
                            let label = line.trim_end_matches(':').to_string() + ":";
                            let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, label.clone(), label_style);
                            let label_width = label.len() as u16 + 2;
                            let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                            if i == 0 {
                                let mut ta = self.new_column_input.clone();
                                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                                ta.render(input_area, buf);
                            } else {
                                let mut ta = self.model_name_input.clone();
                                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                                ta.render(input_area, buf);
                            }
                        } else if self.is_index_number_field(i) {
                            let label = line.split(':').next().unwrap_or("").to_string() + ":";
                            let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, label.clone(), label_style);
                            let label_width = label.len() as u16 + 2;
                            let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                            let mut ta = self.get_number_input_by_index(i).clone();
                            if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                            ta.render(input_area, buf);
                        } else {
                            let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, line, style);
                        }
                    }
                }

                let buttons = ["[Apply]", "[Close]"];
                let total_len: u16 = buttons.iter()
                    .map(|b| b.len() as u16 + 1)
                    .sum();
                let bx = inner.x + inner.width.saturating_sub(total_len + 1);
                let by = inner.y + inner.height.saturating_sub(1);
                let mut x = bx;
                for (idx, b) in buttons.iter().enumerate() {
                    let style = if self.buttons_mode && self.selected_button == idx {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::White)
                        } else if idx == 0 {
                            Style::default()
                            .fg(Color::Yellow)
                        } else {
                            Style::default()
                                .fg(Color::White)
                        };
                    buf.set_string(x, by, *b, style);
                    x += b.len() as u16 + 1;
                }
            }
            ColumnOperationOptionsMode::Error(msg) => {
                let block = Block::default()
                    .title(match self.operation { ColumnOperationKind::GenerateEmbeddings => "Generate Embeddings", ColumnOperationKind::Pca => "PCA", ColumnOperationKind::Cluster => "Cluster", ColumnOperationKind::SortByPromptSimilarity => "Prompt Similarity" })
                    .borders(Borders::ALL);
                let inner = block.inner(content_area);
                block.render(content_area, buf);

                // Render inline error message at the top of the inner area
                let err_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
                let err_text = format!("Error: {msg}");
                buf.set_string(inner.x + 1, inner.y, err_text, err_style);

                // Render fields starting one line below the error
                if matches!(self.operation, ColumnOperationKind::GenerateEmbeddings) {
                    let base_y = inner.y + 1;
                    // Row 0: New Column Name (text input)
                    let i = 0usize;
                    let y = base_y;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "New Column Name:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(inner.x + 1, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                    let mut ta = self.new_column_input.clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);

                    // Row 1: Source Column (enum display)
                    let i = 1usize;
                    let y = base_y + 1;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let line = format!("Source Column: {}", self.columns.get(self.selected_column_index).cloned().unwrap_or_default());
                    let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(inner.x + 1, y, line, style);

                    // Embedding Options block for Provider, Model, and Dimensions
                    let emb_outer = Rect {
                        x: inner.x + 1,
                        y: base_y + 2,
                        width: inner.width.saturating_sub(2),
                        height: 5,
                    };
                    let emb_block = Block::default().title("Embedding Options").borders(Borders::ALL);
                    let emb_inner = emb_block.inner(emb_outer);
                    emb_block.render(emb_outer, buf);

                    // Row 2 inside block: Provider
                    let i = 2usize;
                    let y = emb_inner.y;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let line = format!("Provider: {}", self.selected_provider.display_name());
                    let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, line, style);

                    // Row 3 inside block: Model Name
                    let i = 3usize;
                    let y = emb_inner.y + 1;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "Model Name:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: emb_inner.x + label_width, y, width: emb_inner.width.saturating_sub(label_width), height: 1 };
                    let mut ta = self.model_name_input.clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);

                    // Row 4 inside block: Number of Dimensions
                    let i = 4usize;
                    let y = emb_inner.y + 2;
                    let is_selected = !self.buttons_mode && i == self.selected_field_index;
                    let label = "Number of Dimensions:".to_string();
                    let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                    buf.set_string(emb_inner.x, y, label.clone(), label_style);
                    let label_width = label.len() as u16 + 2;
                    let input_area = Rect { x: emb_inner.x + label_width, y, width: emb_inner.width.saturating_sub(label_width), height: 1 };
                    let mut ta = self.get_number_input_by_index(i).clone();
                    if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                    ta.render(input_area, buf);
                } else {
                    let lines = self.fields_for_operation();
                    for (i, line) in lines.iter().enumerate() {
                        let y = inner.y + 1 + i as u16;
                        if y >= inner.y + inner.height { break; }
                        let is_selected = !self.buttons_mode && i == self.selected_field_index;
                        if (i == 0) || (self.operation == ColumnOperationKind::GenerateEmbeddings && i == 2) {
                            let label = line.trim_end_matches(':').to_string() + ":";
                            let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, label.clone(), label_style);
                            let label_width = label.len() as u16 + 2;
                            let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                            if i == 0 {
                                let mut ta = self.new_column_input.clone();
                                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                                ta.render(input_area, buf);
                            } else {
                                let mut ta = self.model_name_input.clone();
                                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                                ta.render(input_area, buf);
                            }
                        } else if self.is_index_number_field(i) {
                            let label = line.split(':').next().unwrap_or("").to_string() + ":";
                            let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, label.clone(), label_style);
                            let label_width = label.len() as u16 + 2;
                            let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                            let mut ta = self.get_number_input_by_index(i).clone();
                            if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                            ta.render(input_area, buf);
                        } else {
                            let style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                            buf.set_string(inner.x + 1, y, line, style);
                        }
                    }
                }

                // Render buttons
                let buttons = ["[Apply]", "[Close]"];
                let total_len: u16 = buttons.iter().map(|b| b.len() as u16 + 1).sum();
                let bx = inner.x + inner.width.saturating_sub(total_len + 1);
                let by = inner.y + inner.height.saturating_sub(1);
                let mut x = bx;
                for (idx, b) in buttons.iter().enumerate() {
                    let style = if self.buttons_mode && self.selected_button == idx { Style::default().fg(Color::Black).bg(Color::White) } else if idx == 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) };
                    buf.set_string(x, by, *b, style);
                    x += b.len() as u16 + 1;
                }
            }
        }

        // Render instructions area at the bottom if enabled
        if self.show_instructions {
            if let Some(instructions_area) = layout.instructions_area {
                if let Some(instructions_text) = &instructions {
                    let instructions_paragraph = Paragraph::new(instructions_text.as_str())
                        .block(Block::default().borders(Borders::ALL).title("Instructions"))
                        .style(Style::default().fg(Color::Yellow))
                        .wrap(Wrap { trim: true });
                    instructions_paragraph.render(instructions_area, buf);
                }
            }
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match self.mode {
            ColumnOperationOptionsMode::Input => {
                let base_instructions = self.config.actions_to_instructions(&[
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                    (crate::config::Mode::Global, crate::action::Action::Backspace),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Paste),
                    (crate::config::Mode::ColumnOperationOptions, crate::action::Action::ToggleField),
                    (crate::config::Mode::ColumnOperationOptions, crate::action::Action::ToggleButtons),
                ]);

                // Add operation-specific instructions
                let operation_instructions = match self.operation {
                    ColumnOperationKind::GenerateEmbeddings => {
                        "  • Provider: Left/Right toggle  • Model Name: Tab cycles models or text input  • Dimensions: Numeric input"
                    }
                    ColumnOperationKind::Pca => {
                        "  • Target Size: Numeric input"
                    }
                    ColumnOperationKind::Cluster => {
                        match self.cluster_algorithm {
                            ClusterAlgorithm::Kmeans => {
                                "  • Algorithm: Space to toggle  • Clusters/Runs/Tolerance: Numeric input"
                            }
                            ClusterAlgorithm::Dbscan => {
                                "  • Algorithm: Space to toggle  • Min Points/Tolerance: Numeric input"
                            }
                        }
                    }
                    ColumnOperationKind::SortByPromptSimilarity => {
                        "  • Source Column: Left/Right to select"
                    }
                };

                if base_instructions.is_empty() {
                    operation_instructions.to_string()
                } else {
                    format!("{base_instructions}{operation_instructions}")
                }
            }
            ColumnOperationOptionsMode::Error(_) => String::new(),
        }
    }

    fn embedding_models_for_provider(&self, provider: &LlmProvider) -> Vec<(&'static str, usize)> {
        match provider {
            LlmProvider::OpenAI => vec![
                ("text-embedding-3-small", 1536),
                ("text-embedding-3-large", 3072),
            ],
            LlmProvider::Azure => vec![
                ("text-embedding-3-small", 1536),
                ("text-embedding-3-large", 3072),
            ],
            LlmProvider::Ollama => vec![
                ("nomic-embed-text", 768),
                ("mxbai-embed-large", 1024),
            ],
        }
    }

    fn set_model_and_dimensions(&mut self, model: &str, dims: usize) {
        // Update model string and textarea
        self.model_name = model.to_string();
        self.model_name_input = {
            let mut t = TextArea::default();
            t.set_block(Block::default());
            t.insert_str(model);
            t
        };
        // Update dimensions value and textarea
        self.num_dimensions = dims;
        self.num_dimensions_input = {
            let mut t = TextArea::default();
            t.set_block(Block::default());
            t.insert_str(&dims.to_string());
            t
        };
    }

    fn apply(&self) -> Action {
        let options = match self.operation {
            ColumnOperationKind::GenerateEmbeddings => OperationOptions::GenerateEmbeddings {
                model_name: self.model_name.clone(),
                num_dimensions: self.num_dimensions
            },
            ColumnOperationKind::Pca => OperationOptions::Pca {
                target_embedding_size: self.target_embedding_size
            },
            ColumnOperationKind::Cluster => OperationOptions::Cluster {
                algorithm: self.cluster_algorithm.clone(),
                kmeans: if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                    Some(self.kmeans.clone())
                } else { None },
                dbscan: if matches!(self.cluster_algorithm, ClusterAlgorithm::Dbscan) {
                    Some(self.dbscan.clone())
                } else { None }
            },
            ColumnOperationKind::SortByPromptSimilarity => OperationOptions::SortByPromptSimilarity,
        };
        let source_column = self.columns.get(self.selected_column_index)
            .cloned()
            .unwrap_or_default();
        let cfg = ColumnOperationConfig {
            operation: self.operation.clone(),
            new_column_name: self.new_column_name.clone(),
            source_column,
            options
        };
        Action::ColumnOperationOptionsApplied(cfg)
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind != KeyEventKind::Press { return Ok(None); }
        // Intercept Tab on Model Name to cycle models before config actions can toggle buttons
        if matches!(self.operation, ColumnOperationKind::GenerateEmbeddings)
            && self.selected_field_index == 3
        {
            if let KeyCode::Tab = key.code {
                let models = self.embedding_models_for_provider(&self.selected_provider);
                if !models.is_empty() {
                    let mut idx = models
                        .iter()
                        .position(|(name, _)| *name == self.model_name.as_str())
                        .unwrap_or(0);
                    idx = (idx + 1) % models.len();
                    let (next_model, next_dims) = models[idx];
                    self.set_model_and_dimensions(next_model, next_dims);
                }
                return Ok(None);
            }
        }
        
        // First, honor config-driven Global actions
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => return Ok(Some(Action::DialogClose)),
                Action::Enter => {
                    if self.buttons_mode {
                        if self.selected_button == 0 {
                            return Ok(Some(self.apply()));
                        } else {
                            return Ok(Some(Action::DialogClose));
                        }
                    }
                    return Ok(None);
                }
                Action::Up => {
                    if self.buttons_mode {
                        // Move from buttons back to last field
                        self.buttons_mode = false;
                    } else if self.selected_field_index > 0 {
                        self.selected_field_index -= 1;
                    }
                    return Ok(None);
                }
                Action::Down => {
                    if self.buttons_mode {
                        // Stay in buttons
                    } else {
                        let max_fields = self.fields_for_operation().len();
                        if self.selected_field_index + 1 >= max_fields {
                            // Jump to buttons
                            self.buttons_mode = true;
                            self.selected_button = 0;
                        } else {
                            self.selected_field_index = self.selected_field_index.saturating_add(1);
                        }
                    }
                    return Ok(None);
                }
                Action::Left => {
                    if self.buttons_mode {
                        self.selected_button = self.selected_button.saturating_sub(1) % 2;
                    } else {
                        // Special case: source column (index 1) cycles columns
                        if self.selected_field_index == 1 {
                            if !self.columns.is_empty() {
                                if self.selected_column_index == 0 { self.selected_column_index = self.columns.len().saturating_sub(1); } else { self.selected_column_index -= 1; }
                            }
                        } else if self.is_current_field_text() {
                            // Move cursor left in TextArea
                            self.feed_text_input_key(KeyCode::Left, false, false, false);
                        } else {
                            self.modify_current_field(false);
                        }
                    }
                    return Ok(None);
                }
                Action::Right => {
                    if self.buttons_mode {
                        self.selected_button = (self.selected_button + 1) % 2;
                    } else if self.selected_field_index == 1 {
                        if !self.columns.is_empty() { self.selected_column_index = (self.selected_column_index + 1) % self.columns.len(); }
                    } else if self.is_current_field_text() {
                        self.feed_text_input_key(KeyCode::Right, false, false, false);
                    } else {
                        self.modify_current_field(true);
                    }
                    return Ok(None);
                }
                Action::Backspace => {
                    if self.is_current_field_text() { self.backspace_in_text_field(); }
                    else if self.current_field_kind() == "number" {
                        // Route to TextArea for numeric fields
                        let idx = self.selected_field_index;
                        let inp = {
                            use crossterm::event::{KeyEvent, KeyModifiers};
                            let kev = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
                            tui_textarea::Input::from(kev)
                        };
                        let area = self.get_number_input_by_index_mut(idx);
                        area.input(inp);
                        // Sync numeric values from their text areas
                        self.sync_numbers_from_inputs();
                    }
                    return Ok(None);
                }
                Action::ToggleInstructions => {
                    self.show_instructions = !self.show_instructions;
                    return Ok(None);
                }
                Action::Paste => {
                    self.paste_from_clipboard_into_text_field();
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Next, check for dialog-specific actions
        if let Some(dialog_action) = self.config.action_for_key(crate::config::Mode::ColumnOperationOptions, key) {
            match dialog_action {
                Action::ToggleField => {
                    self.toggle_current_field();
                    return Ok(None);
                }
                Action::ToggleButtons => {
                    if self.buttons_mode {
                        self.buttons_mode = false;
                    } else {
                        self.buttons_mode = true;
                        self.selected_button = 0;
                    }
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Fallback for character input or other unhandled keys
        use crossterm::event::KeyModifiers;
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_current_text_to_clipboard();
            }
            KeyCode::Char(ch) => {
                if self.is_current_field_text() { self.insert_char_into_text_field(ch); }
                else if self.current_field_kind() == "number" && ch.is_ascii_digit() {
                    let idx = self.selected_field_index;
                    let inp = {
                        use crossterm::event::{KeyEvent, KeyModifiers};
                        let kev = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
                        tui_textarea::Input::from(kev)
                    };
                    let area = self.get_number_input_by_index_mut(idx);
                    area.input(inp);
                    self.sync_numbers_from_inputs();
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn modify_current_field(&mut self, increment: bool) {
        // Index 0 is always new column name; other fields depend on operation
        if self.selected_field_index == 0 {
            // Text field: ignore numeric adjust; cursor movement is handled by Char/Backspace
            return;
        }
        let idx = self.selected_field_index - 1;
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                match idx {
                    1 => {
                        // Provider selection rotate
                        let order = [LlmProvider::Azure, LlmProvider::OpenAI, LlmProvider::Ollama];
                        let mut pos = order.iter().position(|p| p == &self.selected_provider).unwrap_or(1);
                        if increment { pos = (pos + 1) % order.len(); } else { pos = (pos + order.len() - 1) % order.len(); }
                        self.selected_provider = order[pos].clone();
                        // Update model and dimensions to provider default
                        let models = self.embedding_models_for_provider(&self.selected_provider);
                        if let Some((model, dims)) = models.first() {
                            self.set_model_and_dimensions(model, *dims);
                        }
                    }
                    3 => {
                        // Number of dimensions adjust
                        if increment { self.num_dimensions = self.num_dimensions.saturating_add(1); } else { self.num_dimensions = self.num_dimensions.saturating_sub(1); }
                    }
                    _ => {}
                }
            }
            ColumnOperationKind::Pca => {
                if idx == 0 { if increment { self.target_embedding_size = self.target_embedding_size.saturating_add(1); } else { self.target_embedding_size = self.target_embedding_size.saturating_sub(1); } }
            }
            ColumnOperationKind::Cluster => {
                match idx {
                    0 => {
                        // Toggle algorithm left/right also
                        self.cluster_algorithm = match (self.cluster_algorithm.clone(), increment) {
                            (ClusterAlgorithm::Kmeans, true) | (ClusterAlgorithm::Dbscan, false) => ClusterAlgorithm::Dbscan,
                            _ => ClusterAlgorithm::Kmeans,
                        };
                    }
                    1 => {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) { if increment { self.kmeans.number_of_clusters = self.kmeans.number_of_clusters.saturating_add(1); } else { self.kmeans.number_of_clusters = self.kmeans.number_of_clusters.saturating_sub(1); } }
                    }
                    2 => {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) { if increment { self.kmeans.runs = self.kmeans.runs.saturating_add(1); } else { self.kmeans.runs = self.kmeans.runs.saturating_sub(1); } }
                    }
                    3 => {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) { if increment { self.kmeans.tolerance = self.kmeans.tolerance.saturating_add(1); } else { self.kmeans.tolerance = self.kmeans.tolerance.saturating_sub(1); } }
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Dbscan) { if increment { self.dbscan.minimum_points = self.dbscan.minimum_points.saturating_add(1); } else { self.dbscan.minimum_points = self.dbscan.minimum_points.saturating_sub(1); } }
                    }
                    4 => {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Dbscan) { if increment { self.dbscan.tolerance = self.dbscan.tolerance.saturating_add(1); } else { self.dbscan.tolerance = self.dbscan.tolerance.saturating_sub(1); } }
                    }
                    _ => {}
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => {
                // No adjustable fields in this dialog for this operation
            }
        }
    }

    fn toggle_current_field(&mut self) {
        // Space toggles algorithm when on that field
        if self.operation == ColumnOperationKind::Cluster {
            // Algorithm line is index 1 (0 = new column)
            if self.selected_field_index == 1 {
                self.cluster_algorithm = match self.cluster_algorithm {
                    ClusterAlgorithm::Kmeans => ClusterAlgorithm::Dbscan,
                    ClusterAlgorithm::Dbscan => ClusterAlgorithm::Kmeans,
                };
            }
        }
    }

    fn current_field_kind(&self) -> &'static str {
        // Return "text" | "number" | "enum"
        if self.selected_field_index == 0 { return "text"; }
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => {
                match self.selected_field_index {
                    1 => "enum", // source column selector
                    2 => "enum", // provider selector
                    3 => "text", // model name
                    4 => "number", // num dims
                    _ => "number",
                }
            }
            ColumnOperationKind::Pca => {
                match self.selected_field_index {
                    1 => "enum", // source column selector
                    2 => "number", // target size
                    _ => "number",
                }
            }
            ColumnOperationKind::Cluster => {
                match self.selected_field_index {
                    1 => "enum", // source column selector
                    _ => {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                            // 2,3,4 are numbers for KMeans
                            "number"
                        } else {
                            // 2,3 are numbers for DBSCAN
                            "number"
                        }
                    }
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => {
                match self.selected_field_index {
                    1 => "enum", // source column selector
                    _ => "text",
                }
            }
        }
    }

    fn is_current_field_text(&self) -> bool { self.current_field_kind() == "text" }

    fn is_index_number_field(&self, index: usize) -> bool {
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => index == 4,
            ColumnOperationKind::Pca => index == 2,
            ColumnOperationKind::Cluster => {
                if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                    index == 3 || index == 4 || index == 5
                } else { // Dbscan
                    index == 3 || index == 4
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => false,
        }
    }

    fn get_number_input_by_index(&self, index: usize) -> &TextArea<'static> {
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => &self.num_dimensions_input,
            ColumnOperationKind::Pca => &self.target_embedding_size_input,
            ColumnOperationKind::Cluster => {
                if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                    match index {
                        3 => &self.kmeans_number_of_clusters_input,
                        4 => &self.kmeans_runs_input,
                        5 => &self.kmeans_tolerance_input,
                        _ => &self.kmeans_number_of_clusters_input,
                    }
                } else {
                    match index {
                        3 => &self.dbscan_minimum_points_input,
                        4 => &self.dbscan_tolerance_input,
                        _ => &self.dbscan_minimum_points_input,
                    }
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => &self.num_dimensions_input,
        }
    }

    fn get_number_input_by_index_mut(&mut self, index: usize) -> &mut TextArea<'static> {
        match self.operation {
            ColumnOperationKind::GenerateEmbeddings => &mut self.num_dimensions_input,
            ColumnOperationKind::Pca => &mut self.target_embedding_size_input,
            ColumnOperationKind::Cluster => {
                if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                    match index {
                        3 => &mut self.kmeans_number_of_clusters_input,
                        4 => &mut self.kmeans_runs_input,
                        5 => &mut self.kmeans_tolerance_input,
                        _ => &mut self.kmeans_number_of_clusters_input,
                    }
                } else {
                    match index {
                        3 => &mut self.dbscan_minimum_points_input,
                        4 => &mut self.dbscan_tolerance_input,
                        _ => &mut self.dbscan_minimum_points_input,
                    }
                }
            }
            ColumnOperationKind::SortByPromptSimilarity => &mut self.num_dimensions_input,
        }
    }

    fn insert_char_into_text_field(&mut self, ch: char) {
        if self.buttons_mode { return; }
        if !self.is_current_field_text() { return; }
        self.feed_text_input_key(KeyCode::Char(ch), false, false, false);
    }

    fn backspace_in_text_field(&mut self) {
        if self.buttons_mode { return; }
        if !self.is_current_field_text() { return; }
        self.feed_text_input_key(KeyCode::Backspace, false, false, false);
    }

    fn copy_current_text_to_clipboard(&mut self) {
        if self.current_field_kind() != "text" { return; }
        let text = if self.selected_field_index == 0 { self.new_column_name.clone() } else { self.model_name.clone() };
        if let Ok(mut clipboard) = Clipboard::new() { let _ = clipboard.set_text(text); }
    }

    fn paste_from_clipboard_into_text_field(&mut self) {
        if self.buttons_mode { return; }
        if !self.is_current_field_text() { return; }
        if let Ok(mut clipboard) = Clipboard::new()
            && let Ok(text) = clipboard.get_text() {
            let first_line = text.lines().next().unwrap_or("").to_string();
            if self.selected_field_index == 0 { self.new_column_input.insert_str(&first_line); self.new_column_name = self.new_column_input.lines().join("\n"); }
            if self.operation == ColumnOperationKind::GenerateEmbeddings && self.selected_field_index == 2 { self.model_name_input.insert_str(&first_line); self.model_name = self.model_name_input.lines().join("\n"); }
        }
    }

    #[allow(dead_code)]
    fn insert_char_or_digit(&mut self, ch: char) {
        if self.buttons_mode { return; }
        match self.current_field_kind() {
            "text" => self.insert_char_into_text_field(ch),
            "number" => {
                if ch.is_ascii_digit() {
                    if self.operation == ColumnOperationKind::GenerateEmbeddings && self.selected_field_index == 3 {
                        let next = format!("{}{}", self.num_dimensions, ch);
                        if let Ok(v) = next.parse::<usize>() { self.num_dimensions = v; }
                    } else if self.operation == ColumnOperationKind::Pca && self.selected_field_index == 2 {
                        let next = format!("{}{}", self.target_embedding_size, ch);
                        if let Ok(v) = next.parse::<usize>() { self.target_embedding_size = v; }
                    } else if self.operation == ColumnOperationKind::Cluster {
                        if matches!(self.cluster_algorithm, ClusterAlgorithm::Kmeans) {
                            match self.selected_field_index {
                                3 => { let next = format!("{}{}", self.kmeans.number_of_clusters, ch); if let Ok(v) = next.parse::<usize>() { self.kmeans.number_of_clusters = v; } }
                                4 => { let next = format!("{}{}", self.kmeans.runs, ch); if let Ok(v) = next.parse::<usize>() { self.kmeans.runs = v; } }
                                5 => { let next = format!("{}{}", self.kmeans.tolerance, ch); if let Ok(v) = next.parse::<usize>() { self.kmeans.tolerance = v; } }
                                _ => {}
                            }
                        } else {
                            match self.selected_field_index {
                                3 => { let next = format!("{}{}", self.dbscan.minimum_points, ch); if let Ok(v) = next.parse::<usize>() { self.dbscan.minimum_points = v; } }
                                4 => { let next = format!("{}{}", self.dbscan.tolerance, ch); if let Ok(v) = next.parse::<usize>() { self.dbscan.tolerance = v; } }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn feed_text_input_key(&mut self, code: KeyCode, ctrl: bool, alt: bool, shift: bool) {
        use crossterm::event::{KeyEvent, KeyModifiers};
        let mut mods = KeyModifiers::empty();
        if ctrl { mods |= KeyModifiers::CONTROL; }
        if alt { mods |= KeyModifiers::ALT; }
        if shift { mods |= KeyModifiers::SHIFT; }
        let kev = KeyEvent::new(code, mods);
        let inp = tui_textarea::Input::from(kev);
        if self.selected_field_index == 0 { self.new_column_input.input(inp.clone()); self.new_column_name = self.new_column_input.lines().join("\n"); }
        if self.operation == ColumnOperationKind::GenerateEmbeddings && self.selected_field_index == 2 { self.model_name_input.input(inp); self.model_name = self.model_name_input.lines().join("\n"); }
    }

    fn sync_numbers_from_inputs(&mut self) {
        // GenerateEmbeddings
        if let Ok(v) = self.num_dimensions_input.lines().join("").parse::<usize>() { self.num_dimensions = v; }
        // PCA
        if let Ok(v) = self.target_embedding_size_input.lines().join("").parse::<usize>() { self.target_embedding_size = v; }
        // KMeans
        if let Ok(v) = self.kmeans_number_of_clusters_input.lines().join("").parse::<usize>() { self.kmeans.number_of_clusters = v; }
        if let Ok(v) = self.kmeans_runs_input.lines().join("").parse::<usize>() { self.kmeans.runs = v; }
        if let Ok(v) = self.kmeans_tolerance_input.lines().join("").parse::<usize>() { self.kmeans.tolerance = v; }
        // DBSCAN
        if let Ok(v) = self.dbscan_minimum_points_input.lines().join("").parse::<usize>() { self.dbscan.minimum_points = v; }
        if let Ok(v) = self.dbscan_tolerance_input.lines().join("").parse::<usize>() { self.dbscan.tolerance = v; }
    }
}

impl Component for ColumnOperationOptionsDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { 
        self.config = _config; 
        Ok(()) 
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> { self.handle_key_event(key) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}


