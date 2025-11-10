use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, BorderType};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use tui_textarea::TextArea;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EmbeddingsPromptDialogMode {
    Input,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SelectedField {
    EmbeddingsRow,
    NewColumnName,
    Prompt,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingsPromptDialog {
    pub styles: crate::style::StyleConfig,
    pub show_instructions: bool,
    pub mode: EmbeddingsPromptDialogMode,
    pub columns: Vec<String>,
    pub selected_column_index: usize,
    pub new_column_name: String,
    #[serde(skip)]
    pub new_column_input: TextArea<'static>,
    #[serde(skip)]
    pub prompt_input: TextArea<'static>,
    pub buttons_mode: bool,
    pub selected_button: usize,
    #[serde(skip)]
    pub config: crate::config::Config,
    #[serde(skip)]
    pub embedding_column_config_mapping: HashMap<String, crate::components::datatable_container::EmbeddingColumnConfig>,
    pub selected_field: SelectedField, // Which field is currently selected
    pub emb_col_button_selected: bool,
}

impl EmbeddingsPromptDialog {
    pub fn new_with_mapping(
        mapping: HashMap<String, crate::components::datatable_container::EmbeddingColumnConfig>,
        initial_selected: Option<String>,
    ) -> Self {
        let mut columns: Vec<String> = mapping.keys().cloned().collect();
        columns.sort();
        let mut selected_column_index = 0usize;
        if let Some(name) = initial_selected {
            if let Some(idx) = columns.iter().position(|n| n == &name) { selected_column_index = idx; }
        }
        let no_existing_columns = columns.is_empty();
        let mut prompt_input = TextArea::default();
        prompt_input.set_block(Block::default());
        let mut new_column_input = TextArea::default();
        new_column_input.set_block(Block::default());
        Self {
            styles: crate::style::StyleConfig::default(),
            show_instructions: true,
            mode: EmbeddingsPromptDialogMode::Input,
            columns,
            selected_column_index,
            new_column_name: String::new(),
            new_column_input,
            prompt_input,
            buttons_mode: false,
            selected_button: 0,
            config: crate::config::Config::default(),
            embedding_column_config_mapping: mapping,
            selected_field: SelectedField::EmbeddingsRow,
            emb_col_button_selected: no_existing_columns, // true when there are no existing embedding columns
        }
    }

    fn apply(&self) -> color_eyre::Result<Action> {
        let source_column = self.columns.get(self.selected_column_index).cloned().unwrap_or_default();
        let maybe_cfg = self.embedding_column_config_mapping.get(&source_column);
        if maybe_cfg.is_none() {
            // Instead of error, initiate generate-embeddings flow from here
            let prompt = self.prompt_input.lines().join("\n");
            return Ok(Action::EmbeddingsPromptDialogRequestGenerateEmbeddings {
                prompt_text: prompt,
                new_similarity_column: self.new_column_name.clone(),
            });
        }
        let cfg = maybe_cfg.unwrap();
        let prompt = self.prompt_input.lines().join("\n");
        let dims_opt = if cfg.num_dimensions > 0 { Some(cfg.num_dimensions) } else { None };
        let vecs = self.config.llm_config.fetch_embeddings_via_provider(
            cfg.provider.clone(),
            &cfg.model_name,
            &vec![prompt],
            dims_opt,
        )?;
        let prompt_embedding = vecs.into_iter().next().unwrap_or_default();
        Ok(Action::EmbeddingsPromptDialogApplied {
            source_column,
            new_column_name: self.new_column_name.clone(),
            prompt_embedding,
        })
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let outer_block = Block::default()
            .title("Prompt Similarity")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);
        let inner_total_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let instructions = self.build_instructions_from_config();
        let layout = crate::components::dialog_layout::split_dialog_area(inner_total_area, self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;

        match &self.mode {
            EmbeddingsPromptDialogMode::Input => {
                let block = Block::default().title("Configure").borders(Borders::ALL);
                let inner = block.inner(content_area);
                block.render(content_area, buf);

                // Row 0: Embeddings Column (enum display with [From New Column] button)
                let y = inner.y;
                let is_selected = !self.buttons_mode && matches!(self.selected_field, SelectedField::EmbeddingsRow);
                let label_text = "Embeddings Column:";
                let label_style = if is_selected && !self.emb_col_button_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                buf.set_string(inner.x + 1, y, label_text, label_style);
                let col_name = self.columns.get(self.selected_column_index).cloned().unwrap_or_default();
                let col_text = format!(" {}", col_name);
                buf.set_string(inner.x + 1 + label_text.len() as u16, y, col_text, Style::default());
                // Draw [From New Column] button at right
                let button_text = "[From New Column]";
                let button_x = inner.x + inner.width.saturating_sub(button_text.len() as u16 + 1);
                let button_style = if is_selected && self.emb_col_button_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                } else {
                    Style::default()
                        .fg(Color::White)
                };
                buf.set_string(button_x, y, button_text, button_style);

                // Row 1: New Column Name (text input)
                let y = inner.y + 1;
                let is_selected = !self.buttons_mode && matches!(self.selected_field, SelectedField::NewColumnName);
                let label = "New Column Name:".to_string();
                let label_style = if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                buf.set_string(inner.x + 1, y, label.clone(), label_style);
                let label_width = label.len() as u16 + 2;
                let input_area = Rect { x: inner.x + 1 + label_width, y, width: inner.width.saturating_sub(label_width + 2), height: 1 };
                let mut ta = self.new_column_input.clone();
                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                ta.render(input_area, buf);

                // Prompt block (multi-line)
                let prm_outer = Rect {
                    x: inner.x + 1,
                    y: inner.y + 3,
                    width: inner.width.saturating_sub(2),
                    height: inner.height.saturating_sub(6),
                };
                let prm_block = Block::default().title("Prompt").borders(Borders::ALL);
                let prm_inner = prm_block.inner(prm_outer);
                prm_block.render(prm_outer, buf);

                let is_selected = !self.buttons_mode && matches!(self.selected_field, SelectedField::Prompt);
                let mut ta = self.prompt_input.clone();
                if !is_selected { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                ta.render(prm_inner, buf);

                // Buttons
                let buttons = ["[Apply]", "[Close]"];
                let total_len: u16 = buttons.iter().map(|b| b.len() as u16 + 1).sum();
                let bx = inner.x + inner.width.saturating_sub(total_len + 1);
                let by = inner.y + inner.height.saturating_sub(1);
                let mut x = bx;
                for (idx, b) in buttons.iter().enumerate() {
                    let style = if self.buttons_mode && self.selected_button == idx {
                        Style::default().fg(Color::Black).bg(Color::White)
                    } else if idx == 0 {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    buf.set_string(x, by, *b, style);
                    x += b.len() as u16 + 1;
                }
            }
            EmbeddingsPromptDialogMode::Error(msg) => {
                let block = Block::default().title("Configure").borders(Borders::ALL);
                let inner = block.inner(content_area);
                block.render(content_area, buf);
                let err_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
                let err_text = format!("Error: {msg}");
                buf.set_string(inner.x + 1, inner.y, err_text, err_style);
            }
        }

        if self.show_instructions {
            if let Some(instructions_area) = layout.instructions_area {
                if !instructions.is_empty() {
                    let paragraph = ratatui::widgets::Paragraph::new(instructions.as_str())
                        .block(Block::default().borders(Borders::ALL).title("Instructions"))
                        .style(Style::default().fg(Color::Yellow))
                        .wrap(ratatui::widgets::Wrap { trim: true });
                    paragraph.render(instructions_area, buf);
                }
            }
        }
    }

    fn build_instructions_from_config(&self) -> String {
        let base = self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, Action::Escape),
            (crate::config::Mode::Global, Action::Backspace),
            (crate::config::Mode::Global, Action::ToggleInstructions),
            (crate::config::Mode::ColumnOperationOptions, Action::ToggleButtons),
        ]);
        if base.is_empty() {
            "  • Embeddings Column: Left/Right to select  • Tab to select [From New Column], Enter to open".to_string()
        } else {
            format!("{base}  • Embeddings Column: Left/Right to select  • Tab to select [From New Column], Enter to open")
        }
    }

    fn feed_text_input_key(&mut self, field: SelectedField, code: KeyCode) {
        use crossterm::event::{KeyEvent, KeyModifiers};
        let kev = KeyEvent::new(code, KeyModifiers::empty());
        let inp = tui_textarea::Input::from(kev);
        match field {
            SelectedField::NewColumnName => {
                self.new_column_input.input(inp.clone());
                self.new_column_name = self.new_column_input.lines().join("\n");
            }
            SelectedField::Prompt => {
                self.prompt_input.input(inp);
            }
            SelectedField::EmbeddingsRow => {}
        }
    }

    pub fn handle_key_event_inner(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press { return None; }
        // When Prompt is selected, handle keys as literals first, with special escape rules for Up/Down/Tab
        if !self.buttons_mode && matches!(self.selected_field, SelectedField::Prompt) {
            match key.code {
                KeyCode::Up => {
                    let (row, _col) = self.prompt_input.cursor();
                    if row > 0 {
                        self.feed_text_input_key(SelectedField::Prompt, KeyCode::Up);
                    } else {
                        self.selected_field = SelectedField::NewColumnName;
                    }
                    return None;
                }
                KeyCode::Down => {
                    let (row, _col) = self.prompt_input.cursor();
                    let last_idx = self.prompt_input.lines().len().saturating_sub(1);
                    if row < last_idx {
                        self.feed_text_input_key(SelectedField::Prompt, KeyCode::Down);
                    } else {
                        self.buttons_mode = true;
                        self.selected_button = 0;
                    }
                    return None;
                }
                KeyCode::Tab => {
                    // Treat Tab as literal unless at the last line, then escape to [Apply]
                    let (row, _col) = self.prompt_input.cursor();
                    let last_idx = self.prompt_input.lines().len().saturating_sub(1);
                    if row < last_idx {
                        self.feed_text_input_key(SelectedField::Prompt, KeyCode::Tab);
                    } else {
                        self.buttons_mode = true;
                        self.selected_button = 0;
                    }
                    return None;
                }
                KeyCode::Enter
                | KeyCode::Backspace
                | KeyCode::Delete
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown => {
                    self.feed_text_input_key(SelectedField::Prompt, key.code);
                    return None;
                }
                KeyCode::Char(ch) => {
                    self.feed_text_input_key(SelectedField::Prompt, KeyCode::Char(ch));
                    return None;
                }
                _ => {
                    // Fall through for things like Escape
                }
            }
        }
        // Global actions first
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => return Some(Action::DialogClose),
                Action::Enter => {
                    if self.buttons_mode {
                        if self.selected_button == 0 {
                            if let Ok(a) = self.apply() { return Some(a); }
                            return None;
                        } else {
                            return Some(Action::DialogClose);
                        }
                    }
                    // If on Embeddings row and button is selected, open Generate Embeddings options
                    if matches!(self.selected_field, SelectedField::EmbeddingsRow) && self.emb_col_button_selected {
                        let prompt = self.prompt_input.lines().join("\n");
                        return Some(Action::EmbeddingsPromptDialogRequestGenerateEmbeddings {
                            prompt_text: prompt,
                            new_similarity_column: self.new_column_name.clone(),
                        });
                    }
                    // Otherwise insert newline in prompt if on prompt
                    if matches!(self.selected_field, SelectedField::Prompt) { self.feed_text_input_key(SelectedField::Prompt, KeyCode::Enter); }
                    return None;
                }
                Action::Backspace => {
                    // Route backspace to active text input if on a text field
                    if matches!(self.selected_field, SelectedField::NewColumnName) {
                        self.feed_text_input_key(SelectedField::NewColumnName, KeyCode::Backspace);
                        return None;
                    }
                }
                Action::Up => {
                    if self.buttons_mode {
                        // If [Close] is selected, up should select [From New Column]
                        if self.selected_button == 1 {
                            self.buttons_mode = false;
                            self.selected_field = SelectedField::EmbeddingsRow;
                            self.emb_col_button_selected = true;
                        } else {
                            // From [Apply] up goes to Prompt
                            self.buttons_mode = false;
                            self.selected_field = SelectedField::Prompt;
                        }
                    } else {
                        // Up rotation along: Apply -> Prompt -> New Column Name -> Embeddings Column
                        match self.selected_field {
                            SelectedField::Prompt => {
                                // If prompt has multiple lines and cursor not on top line, move the cursor up
                                let (row, _col) = self.prompt_input.cursor();
                                if row > 0 {
                                    self.feed_text_input_key(SelectedField::Prompt, KeyCode::Up);
                                } else {
                                    self.selected_field = SelectedField::NewColumnName;
                                }
                            }
                            SelectedField::NewColumnName => { self.selected_field = SelectedField::EmbeddingsRow; self.emb_col_button_selected = false; }
                            SelectedField::EmbeddingsRow => { /* stay */ }
                        }
                    }
                    return None;
                }
                Action::Down => {
                    if self.buttons_mode {
                        // No down movement within buttons per spec
                    } else {
                        // Down rotation: Embeddings Column -> New Column Name -> Prompt -> Apply
                        match self.selected_field {
                            SelectedField::EmbeddingsRow => {
                                if self.emb_col_button_selected {
                                    // [From New Column] -> [Apply]
                                    self.buttons_mode = true;
                                    self.selected_button = 0; // Apply
                                } else {
                                    self.selected_field = SelectedField::NewColumnName;
                                }
                            }
                            SelectedField::NewColumnName => { self.selected_field = SelectedField::Prompt; }
                            SelectedField::Prompt => {
                                // If cursor not on last line, move cursor down; else move to [Apply]
                                let (row, _col) = self.prompt_input.cursor();
                                let last_idx = self.prompt_input.lines().len().saturating_sub(1);
                                if row < last_idx {
                                    self.feed_text_input_key(SelectedField::Prompt, KeyCode::Down);
                                } else {
                                    self.buttons_mode = true;
                                    self.selected_button = 0;
                                }
                            }
                        }
                    }
                    return None;
                }
                Action::Left => {
                    if self.buttons_mode {
                        // If [Apply] is selected, Left selects Prompt; if [Close], move to [Apply]
                        if self.selected_button == 0 {
                            self.buttons_mode = false;
                            self.selected_field = SelectedField::Prompt;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    else if matches!(self.selected_field, SelectedField::EmbeddingsRow) {
                        if self.emb_col_button_selected {
                            // [From New Column] -> Embedding Column
                            self.emb_col_button_selected = false;
                        } else {
                            if !self.columns.is_empty() {
                                if self.selected_column_index == 0 { self.selected_column_index = self.columns.len().saturating_sub(1); } else { self.selected_column_index -= 1; }
                            }
                        }
                    } else {
                        self.feed_text_input_key(self.selected_field, KeyCode::Left);
                    }
                    return None;
                }
                Action::Right => {
                    if self.buttons_mode {
                        // [Apply] -> [Close]
                        if self.selected_button == 0 { self.selected_button = 1; } else { /* stay on Close */ }
                    }
                    else {
                        match self.selected_field {
                            SelectedField::EmbeddingsRow => {
                                // Move focus to the "[From New Column]" button
                                self.emb_col_button_selected = true;
                            }
                            SelectedField::NewColumnName => {
                                // If cursor is at end of the last line, jump to the button; otherwise move cursor
                                let (row, col) = self.new_column_input.cursor();
                                let lines = self.new_column_input.lines();
                                let at_last_line = row >= lines.len().saturating_sub(1);
                                let current_line_len = lines.get(row).map(|s| s.len()).unwrap_or(0);
                                let at_end_of_line = col >= current_line_len;
                                if at_last_line && at_end_of_line {
                                    self.selected_field = SelectedField::EmbeddingsRow;
                                    self.emb_col_button_selected = true;
                                } else {
                                    self.feed_text_input_key(SelectedField::NewColumnName, KeyCode::Right);
                                }
                            }
                            SelectedField::Prompt => {
                                self.feed_text_input_key(SelectedField::Prompt, KeyCode::Right);
                            }
                        }
                    }
                    return None;
                }
                Action::Tab => {
                    // No special Tab behavior in the new navigation spec
                    return None;
                }
                _ => {}
            }
        }
        // Fallback for character input or unhandled keys
        match key.code {
            KeyCode::Enter => {
                if matches!(self.selected_field, SelectedField::EmbeddingsRow) && self.emb_col_button_selected {
                    let prompt = self.prompt_input.lines().join("\n");
                    return Some(Action::EmbeddingsPromptDialogRequestGenerateEmbeddings {
                        prompt_text: prompt,
                        new_similarity_column: self.new_column_name.clone(),
                    });
                }
            }
            KeyCode::Char(' ') => {
                if matches!(self.selected_field, SelectedField::NewColumnName | SelectedField::Prompt) {
                    self.feed_text_input_key(self.selected_field, KeyCode::Char(' '));
                }
            }
            KeyCode::Char(ch) => {
                if matches!(self.selected_field, SelectedField::NewColumnName | SelectedField::Prompt) {
                    // ch is embedded within KeyCode::Char here
                    self.feed_text_input_key(self.selected_field, KeyCode::Char(ch));
                }
            }
            KeyCode::Backspace => {
                if matches!(self.selected_field, SelectedField::NewColumnName | SelectedField::Prompt) {
                    self.feed_text_input_key(self.selected_field, KeyCode::Backspace);
                }
            }
            _ => {}
        }
        None
    }
}

impl Component for EmbeddingsPromptDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { self.config = _config; Ok(()) }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> { Ok(self.handle_key_event_inner(key)) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}
