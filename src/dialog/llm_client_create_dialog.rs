//! LlmClientCreateDialog: Create an Embeddings or Completion client selection
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType, List, ListItem, ListState};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;
use serde::{Deserialize, Serialize};
use strum::Display;
use crate::dialog::llm_client_dialog::{LlmProvider, LlmConfig};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum LlmClientCreateMode {
    Embeddings,
    Completion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmClientSelection {
    pub provider: LlmProvider,
    pub mode: LlmClientCreateMode,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiMode {
    ProviderSelection,
    Options,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Model,
    Temperature,
    MaxTokens,
}

impl Default for Field {
    fn default() -> Self { Self::Model }
}

#[derive(Debug)]
pub struct LlmClientCreateDialog {
    pub app_config: Config,
    pub show_instructions: bool,
    pub ui_mode: UiMode,
    pub create_mode: LlmClientCreateMode,
    pub provider_list_state: ListState,
    pub selected_provider: LlmProvider,
    pub current_field: Field,
    pub cursor_position: usize,
    pub model: String,
    pub temperature: String,
    pub max_tokens: String,
    pub llm_config: LlmConfig,
}

impl Default for LlmClientCreateDialog {
    fn default() -> Self {
        Self::new(Config::default(), LlmConfig::default(), LlmClientCreateMode::Embeddings)
    }
}

impl LlmClientCreateDialog {
    pub fn new(app_config: Config, llm_config: LlmConfig, create_mode: LlmClientCreateMode) -> Self {
        let mut provider_list_state = ListState::default();
        provider_list_state.select(Some(0));
        let selected_provider = LlmProvider::OpenAI;
        let model = match create_mode {
            LlmClientCreateMode::Embeddings => llm_config.default_embedding_model_for(&selected_provider).to_string(),
            LlmClientCreateMode::Completion => llm_config.default_completion_model_for(&selected_provider).to_string(),
        };
        Self {
            app_config,
            show_instructions: true,
            ui_mode: UiMode::ProviderSelection,
            create_mode,
            provider_list_state,
            selected_provider,
            current_field: Field::Model,
            cursor_position: model.len(),
            model,
            temperature: "1.0".to_string(),
            max_tokens: "512".to_string(),
            llm_config,
        }
    }

    fn build_instructions_from_config(&self) -> String {
        self.app_config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Up),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Down),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Tab),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Backspace),
        ])
    }

    fn get_provider_list_items() -> Vec<ListItem<'static>> {
        vec![
            ListItem::new("Azure OpenAI"),
            ListItem::new("OpenAI"),
            ListItem::new("Ollama"),
        ]
    }

    fn update_selected_provider(&mut self) {
        if let Some(selected) = self.provider_list_state.selected() {
            self.selected_provider = match selected {
                0 => LlmProvider::Azure,
                1 => LlmProvider::OpenAI,
                2 => LlmProvider::Ollama,
                _ => LlmProvider::OpenAI,
            };
        }
        // Update default model on provider change if user has not edited model (heuristic)
        if self.model.is_empty() {
            self.model = match self.create_mode {
                LlmClientCreateMode::Embeddings => self.llm_config.default_embedding_model_for(&self.selected_provider).to_string(),
                LlmClientCreateMode::Completion => self.llm_config.default_completion_model_for(&self.selected_provider).to_string(),
            };
            self.cursor_position = self.model.len();
        }
    }

    fn move_to_next_field(&mut self) {
        self.current_field = match (self.create_mode.clone(), self.current_field.clone()) {
            (LlmClientCreateMode::Embeddings, Field::Model) => Field::Model,
            (LlmClientCreateMode::Embeddings, Field::Temperature) => Field::Model,
            (LlmClientCreateMode::Embeddings, Field::MaxTokens) => Field::Model,
            (LlmClientCreateMode::Completion, Field::Model) => Field::Temperature,
            (LlmClientCreateMode::Completion, Field::Temperature) => Field::MaxTokens,
            (LlmClientCreateMode::Completion, Field::MaxTokens) => Field::Model,
        };
        self.cursor_position = self.get_current_field_value().len();
    }

    fn move_to_previous_field(&mut self) {
        self.current_field = match (self.create_mode.clone(), self.current_field.clone()) {
            (LlmClientCreateMode::Embeddings, Field::Model) => Field::Model,
            (LlmClientCreateMode::Embeddings, Field::Temperature) => Field::Model,
            (LlmClientCreateMode::Embeddings, Field::MaxTokens) => Field::Model,
            (LlmClientCreateMode::Completion, Field::Model) => Field::MaxTokens,
            (LlmClientCreateMode::Completion, Field::Temperature) => Field::Model,
            (LlmClientCreateMode::Completion, Field::MaxTokens) => Field::Temperature,
        };
        self.cursor_position = self.get_current_field_value().len();
    }

    fn get_current_field_value(&self) -> &str {
        match self.current_field {
            Field::Model => &self.model,
            Field::Temperature => &self.temperature,
            Field::MaxTokens => &self.max_tokens,
        }
    }

    fn set_current_field_value(&mut self, value: String) {
        match self.current_field {
            Field::Model => self.model = value,
            Field::Temperature => self.temperature = value,
            Field::MaxTokens => self.max_tokens = value,
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        let current_value = self.get_current_field_value();
        if self.cursor_position < current_value.len() {
            self.cursor_position += 1;
        }
    }

    fn move_cursor_to_end(&mut self) {
        self.cursor_position = self.get_current_field_value().len();
    }

    fn move_cursor_to_start(&mut self) {
        self.cursor_position = 0;
    }

    fn parse_numeric_fields(&self) -> std::result::Result<(Option<f32>, Option<u32>), String> {
        match self.create_mode {
            LlmClientCreateMode::Embeddings => Ok((None, None)),
            LlmClientCreateMode::Completion => {
                let temp = if self.temperature.trim().is_empty() { 1.0 } else { self.temperature.trim().parse::<f32>().map_err(|_| "Invalid temperature".to_string())? };
                let max = if self.max_tokens.trim().is_empty() { 512 } else { self.max_tokens.trim().parse::<u32>().map_err(|_| "Invalid max tokens".to_string())? };
                Ok((Some(temp), Some(max)))
            }
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();

        let title = match self.create_mode {
            LlmClientCreateMode::Embeddings => "Create Embeddings Client",
            LlmClientCreateMode::Completion => "Create Completion Client",
        };

        let outer_block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let wrap_width = content_area.width.saturating_sub(2) as usize;

        match &self.ui_mode {
            UiMode::ProviderSelection => {
                let block = Block::default()
                    .title("Select Provider")
                    .borders(Borders::ALL);
                let list_area = block.inner(content_area);
                block.render(content_area, buf);

                let items = Self::get_provider_list_items();
                let list = List::new(items)
                    .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                    .highlight_symbol("> ");

                ratatui::prelude::StatefulWidget::render(
                    list, list_area, buf, 
                    &mut self.provider_list_state
                );
            }
            UiMode::Options => {
                let block = Block::default()
                    .title(format!("{} options for {}", self.create_mode, self.selected_provider.display_name()))
                    .borders(Borders::ALL);
                let form_area = block.inner(content_area);
                block.render(content_area, buf);

                let mut y = form_area.y;
                let x = form_area.x;

                let mut render_field = |label: &str, value: &str, is_current: bool| {
                    let style = if is_current { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Yellow) };
                    buf.set_string(x, y, label, style);
                    let value_style = if is_current { Style::default().fg(Color::White).add_modifier(Modifier::UNDERLINED) } else { Style::default().fg(Color::White) };
                    if is_current {
                        let cursor_pos = self.cursor_position.min(value.len());
                        buf.set_string(x, y + 1, value, value_style);
                        let cursor_x = x + value.chars().take(cursor_pos).map(|c| c.len_utf8()).sum::<usize>() as u16;
                        if cursor_pos < value.len() {
                            let char_at_cursor = value.chars().nth(cursor_pos).unwrap_or(' ');
                            buf.set_string(cursor_x, y + 1, char_at_cursor.to_string(), self.app_config.style_config.cursor.block());
                        } else {
                            buf.set_string(cursor_x, y + 1, " ", self.app_config.style_config.cursor.block());
                        }
                    } else {
                        buf.set_string(x, y + 1, value, value_style);
                    }
                    y += 3;
                };

                render_field("Model:", &self.model, self.current_field == Field::Model);
                if self.create_mode == LlmClientCreateMode::Completion {
                    render_field("Temperature:", &self.temperature, self.current_field == Field::Temperature);
                    render_field("Max tokens:", &self.max_tokens, self.current_field == Field::MaxTokens);
                }
            }
            UiMode::Error(msg) => {
                let y = content_area.y;
                buf.set_string(content_area.x, y, "Error:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                let error_lines = textwrap::wrap(msg, wrap_width);
                for (i, line) in error_lines.iter().enumerate() {
                    buf.set_string(content_area.x, y + 1 + i as u16, line, Style::default().fg(Color::Red));
                }
                buf.set_string(content_area.x, y + 1 + error_lines.len() as u16, "Press Esc or Enter to close error", Style::default().fg(Color::Yellow));
            }
        }

        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    pub fn handle_key_event_impl(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;

        if key.kind != KeyEventKind::Press {
            return None;
        }

        let optional_global_action = self.app_config.action_for_key(crate::config::Mode::Global, key);
        let llm_dialog_action = self.app_config.action_for_key(crate::config::Mode::LlmClientDialog, key);

        if let Some(global_action) = &optional_global_action
            && global_action == &Action::ToggleInstructions {
                self.show_instructions = !self.show_instructions;
                return None;
            }

        match &mut self.ui_mode {
            UiMode::ProviderSelection => {
                if let Some(global_action) = &optional_global_action {
                    match global_action {
                        Action::Escape => { return Some(Action::DialogClose); }
                        _ => {}
                    }
                }

                if let Some(dialog_action) = &llm_dialog_action {
                    match dialog_action {
                        Action::Enter => {
                            self.update_selected_provider();
                            self.ui_mode = UiMode::Options;
                            self.current_field = Field::Model;
                            self.cursor_position = self.model.len();
                            return None;
                        }
                        Action::Up => {
                            if let Some(selected) = self.provider_list_state.selected() {
                                let new_selection = if selected == 0 { 2 } else { selected - 1 };
                                self.provider_list_state.select(Some(new_selection));
                                self.update_selected_provider();
                            }
                            return None;
                        }
                        Action::Down => {
                            if let Some(selected) = self.provider_list_state.selected() {
                                let new_selection = if selected == 2 { 0 } else { selected + 1 };
                                self.provider_list_state.select(Some(new_selection));
                                self.update_selected_provider();
                            }
                            return None;
                        }
                        _ => {}
                    }
                }

                match key.code {
                    KeyCode::Esc => { return Some(Action::DialogClose); }
                    KeyCode::Enter => {
                        self.update_selected_provider();
                        self.ui_mode = UiMode::Options;
                        self.current_field = Field::Model;
                        self.cursor_position = self.model.len();
                        return None;
                    }
                    KeyCode::Up => {
                        if let Some(selected) = self.provider_list_state.selected() {
                            let new_selection = if selected == 0 { 2 } else { selected - 1 };
                            self.provider_list_state.select(Some(new_selection));
                            self.update_selected_provider();
                        }
                        return None;
                    }
                    KeyCode::Down => {
                        if let Some(selected) = self.provider_list_state.selected() {
                            let new_selection = if selected == 2 { 0 } else { selected + 1 };
                            self.provider_list_state.select(Some(new_selection));
                            self.update_selected_provider();
                        }
                        return None;
                    }
                    _ => {}
                }
            }
            UiMode::Options => {
                if let Some(global_action) = &optional_global_action {
                    match global_action {
                        Action::Escape => { return Some(Action::DialogClose); }
                        _ => {}
                    }
                }

                if let Some(dialog_action) = &llm_dialog_action {
                    match dialog_action {
                        Action::Enter => {
                            match self.parse_numeric_fields() {
                                Ok((temperature, max_tokens)) => {
                                    let selection = LlmClientSelection {
                                        provider: self.selected_provider.clone(),
                                        mode: self.create_mode.clone(),
                                        model: self.model.clone(),
                                        temperature,
                                        max_tokens,
                                    };
                                    return Some(Action::LlmClientCreateDialogApplied(selection));
                                }
                                Err(e) => {
                                    self.ui_mode = UiMode::Error(e);
                                    return None;
                                }
                            }
                        }
                        Action::Up => { self.move_to_previous_field(); return None; }
                        Action::Down => { self.move_to_next_field(); return None; }
                        Action::Tab => { self.move_to_next_field(); return None; }
                        Action::Backspace => {
                            let mut current_value = self.get_current_field_value().to_string();
                            if self.cursor_position > 0 && self.cursor_position <= current_value.len() {
                                current_value.remove(self.cursor_position - 1);
                                self.cursor_position -= 1;
                                self.set_current_field_value(current_value);
                            }
                            return None;
                        }
                        _ => {}
                    }
                }

                match key.code {
                    KeyCode::Esc => { return Some(Action::DialogClose); }
                    KeyCode::Enter => {
                        match self.parse_numeric_fields() {
                            Ok((temperature, max_tokens)) => {
                                let selection = LlmClientSelection {
                                    provider: self.selected_provider.clone(),
                                    mode: self.create_mode.clone(),
                                    model: self.model.clone(),
                                    temperature,
                                    max_tokens,
                                };
                                return Some(Action::LlmClientCreateDialogApplied(selection));
                            }
                            Err(e) => { self.ui_mode = UiMode::Error(e); return None; }
                        }
                    }
                    KeyCode::Up => { self.move_to_previous_field(); return None; }
                    KeyCode::Down => { self.move_to_next_field(); return None; }
                    KeyCode::Tab => { self.move_to_next_field(); return None; }
                    KeyCode::Left => { self.move_cursor_left(); return None; }
                    KeyCode::Right => { self.move_cursor_right(); return None; }
                    KeyCode::Home => { self.move_cursor_to_start(); return None; }
                    KeyCode::End => { self.move_cursor_to_end(); return None; }
                    KeyCode::Backspace => {
                        let mut current_value = self.get_current_field_value().to_string();
                        if self.cursor_position > 0 && self.cursor_position <= current_value.len() {
                            current_value.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                            self.set_current_field_value(current_value);
                        }
                        return None;
                    }
                    KeyCode::Delete => {
                        let mut current_value = self.get_current_field_value().to_string();
                        if self.cursor_position < current_value.len() {
                            current_value.remove(self.cursor_position);
                            self.set_current_field_value(current_value);
                        }
                        return None;
                    }
                    KeyCode::Char(c) => {
                        let mut current_value = self.get_current_field_value().to_string();
                        let cursor_pos = self.cursor_position.min(current_value.len());
                        current_value.insert(cursor_pos, c);
                        self.cursor_position += 1;
                        self.set_current_field_value(current_value);
                        return None;
                    }
                    _ => {}
                }
            }
            UiMode::Error(_) => {
                if let Some(Action::Escape | Action::Enter) = &optional_global_action {
                    self.ui_mode = UiMode::Options;
                    return None;
                }
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => { self.ui_mode = UiMode::Options; }
                    _ => {}
                }
            }
        }
        None
    }
}

impl Component for LlmClientCreateDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { self.app_config = _config; Ok(()) }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(self.handle_key_event_impl(_key)) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}


