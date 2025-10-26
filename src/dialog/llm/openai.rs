//! OpenAI Configuration Dialog
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;
use crate::dialog::llm_client_dialog::LlmProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY")
            .unwrap_or_else(|_| String::new());

        let base_url = if !api_key.is_empty() {
            "https://api.openai.com/v1".to_string()
        } else {
            String::new()
        };

        Self {
            api_key,
            base_url,
        }
    }
}

#[derive(Debug)]
pub struct OpenAiConfigDialog {
    pub config: OpenAIConfig,
    pub error_active: bool,
    pub show_instructions: bool,
    pub app_config: Config,
    pub current_field: Field,
    pub cursor_position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    ApiKey,
    BaseUrl,
}

impl Default for Field {
    fn default() -> Self {
        Self::ApiKey
    }
}

impl Default for OpenAiConfigDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiConfigDialog {
    pub fn new() -> Self {
        Self {
            config: OpenAIConfig::default(),
            error_active: false,
            show_instructions: true,
            app_config: Config::default(),
            current_field: Field::ApiKey,
            cursor_position: 0,
        }
    }

    pub fn new_with_config(config: Config, openai_config: OpenAIConfig) -> Self {
        Self {
            config: openai_config,
            error_active: false,
            show_instructions: true,
            app_config: config,
            current_field: Field::ApiKey,
            cursor_position: 0,
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.app_config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Tab),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Backspace),
        ])
    }

    fn get_current_field_value(&self) -> &str {
        match self.current_field {
            Field::ApiKey => &self.config.api_key,
            Field::BaseUrl => &self.config.base_url,
        }
    }

    fn set_current_field_value(&mut self, value: String) {
        match self.current_field {
            Field::ApiKey => self.config.api_key = value,
            Field::BaseUrl => self.config.base_url = value,
        }
    }

    fn move_to_next_field(&mut self) {
        self.current_field = match self.current_field {
            Field::ApiKey => Field::BaseUrl,
            Field::BaseUrl => Field::ApiKey,
        };
        self.cursor_position = self.get_current_field_value().len();
    }

    fn move_to_previous_field(&mut self) {
        self.current_field = match self.current_field {
            Field::ApiKey => Field::BaseUrl,
            Field::BaseUrl => Field::ApiKey,
        };
        self.cursor_position = self.get_current_field_value().len();
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

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        
        // Outer container with double border
        let outer_block = Block::default()
            .title("OpenAI Configuration")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let _wrap_width = content_area.width.saturating_sub(2) as usize;

        let block = Block::default()
            .title("Configure OpenAI")
            .borders(Borders::ALL);
        let form_area = block.inner(content_area);
        block.render(content_area, buf);

        let mut y = form_area.y;
        let x = form_area.x;

        // Field labels and values
        let fields = [
            (Field::ApiKey, "API Key:", &self.config.api_key),
            (Field::BaseUrl, "Base URL:", &self.config.base_url),
        ];

        for (field, label, value) in fields.iter() {
            let is_current = *field == self.current_field;
            let style = if is_current {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };
            
            buf.set_string(x, y, *label, style);
            
            let value_style = if is_current {
                Style::default().fg(Color::White).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::White)
            };
            
            if is_current {
                // Display text with overlay block cursor
                let cursor_pos = self.cursor_position.min(value.len());
                
                // Draw the full text first
                buf.set_string(x, y + 1, value, value_style);
                
                // Overlay the block cursor at the cursor position
                let cursor_x = x + value.chars().take(cursor_pos).map(|c| c.len_utf8()).sum::<usize>() as u16;
                if cursor_pos < value.len() {
                    // Cursor is on a character - overlay it with block cursor
                    let char_at_cursor = value.chars().nth(cursor_pos).unwrap_or(' ');
                    buf.set_string(cursor_x, y + 1, char_at_cursor.to_string(), self.app_config.style_config.cursor.block());
                } else {
                    // Cursor is at the end - overlay a space with block cursor
                    buf.set_string(cursor_x, y + 1, " ", self.app_config.style_config.cursor.block());
                }
            } else {
                buf.set_string(x, y + 1, value, value_style);
            }
            y += 3;
        }

        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
        1
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Get all configured actions once at the start
        let optional_global_action = self.app_config.action_for_key(crate::config::Mode::Global, key);
        let llm_dialog_action = self.app_config.action_for_key(crate::config::Mode::LlmClientDialog, key);

        // Handle global actions that work in all modes
        if let Some(global_action) = &optional_global_action
            && global_action == &Action::ToggleInstructions {
                self.show_instructions = !self.show_instructions;
                return None;
            }
        
        // First, check Global actions
        if let Some(global_action) = &optional_global_action {
            match global_action {
                Action::Escape => {
                    return Some(Action::DialogClose);
                }
                _ => {}
            }
        }

        // Next, check LlmClientDialog-specific actions
        if let Some(dialog_action) = &llm_dialog_action {
            match dialog_action {
                Action::Enter => {
                    return Some(Action::LlmClientDialogApplied(
                        crate::dialog::llm_client_dialog::LlmConfig {
                            azure: None,
                            openai: Some(self.config.clone()),
                            ollama: None,
                            selected_provider: LlmProvider::OpenAI,
                        }
                    ));
                }
                Action::Up => {
                    self.move_to_previous_field();
                    return None;
                }
                Action::Down => {
                    self.move_to_next_field();
                    return None;
                }
                Action::Tab => {
                    self.move_to_next_field();
                    return None;
                }
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

        // Fallback for hardcoded keys
        match key.code {
            KeyCode::Esc => {
                return Some(Action::DialogClose);
            }
            KeyCode::Enter => {
                return Some(Action::LlmClientDialogApplied(
                    crate::dialog::llm_client_dialog::LlmConfig {
                        azure: None,
                        openai: Some(self.config.clone()),
                        ollama: None,
                        selected_provider: LlmProvider::OpenAI,
                    }
                ));
            }
            KeyCode::Up => {
                self.move_to_previous_field();
                return None;
            }
            KeyCode::Down => {
                self.move_to_next_field();
                return None;
            }
            KeyCode::Tab => {
                self.move_to_next_field();
                return None;
            }
            KeyCode::Left => {
                self.move_cursor_left();
                return None;
            }
            KeyCode::Right => {
                self.move_cursor_right();
                return None;
            }
            KeyCode::Home => {
                self.move_cursor_to_start();
                return None;
            }
            KeyCode::End => {
                self.move_cursor_to_end();
                return None;
            }
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
        None
    }

    /// Set error message and switch to error mode
    pub fn set_error(&mut self, _msg: String) {
        self.error_active = true;
        // Could implement error display here if needed
    }
}

impl Component for OpenAiConfigDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.app_config = _config;
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        if let Some(action) = self.handle_key_event(_key) {
            return Ok(Some(action));
        }
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}
