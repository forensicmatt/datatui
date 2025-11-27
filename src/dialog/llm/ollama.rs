//! Ollama Configuration Dialog
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;
use serde::{Deserialize, Serialize};
use crate::dialog::llm::LlmConfig;
use arboard::Clipboard;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub host: String,
}
impl LlmConfig for OllamaConfig {
    fn is_configured(&self) -> bool {
        !self.host.is_empty()
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        let host = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
            
        Self {
            host,
        }
    }
}

#[derive(Debug)]
pub struct OllamaConfigDialog {
    pub config: OllamaConfig,
    pub error_active: bool,
    pub show_instructions: bool,
    pub app_config: Config,
    pub current_field: Field,
    pub cursor_position: usize,
    pub selection_start: Option<usize>,
    pub selection_end: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Host,
}

impl Default for Field {
    fn default() -> Self {
        Self::Host
    }
}

impl Default for OllamaConfigDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaConfigDialog {
    pub fn new() -> Self {
        Self {
            config: OllamaConfig::default(),
            error_active: false,
            show_instructions: true,
            app_config: Config::default(),
            current_field: Field::Host,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
        }
    }

    pub fn new_with_config(config: Config, ollama_config: OllamaConfig) -> Self {
        Self {
            config: ollama_config,
            error_active: false,
            show_instructions: true,
            app_config: config,
            current_field: Field::Host,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.app_config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter)
        ])
    }

    fn get_current_field_value(&self) -> &str {
        match self.current_field {
            Field::Host => &self.config.host,
        }
    }

    fn set_current_field_value(&mut self, value: String) {
        match self.current_field {
            Field::Host => self.config.host = value,
        }
    }

    fn move_to_next_field(&mut self) {
        // Only one field, so stay on Host
        self.cursor_position = self.get_current_field_value().len();
        self.clear_selection();
    }

    fn move_to_previous_field(&mut self) {
        // Only one field, so stay on Host
        self.cursor_position = self.get_current_field_value().len();
        self.clear_selection();
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.clear_selection();
        }
    }

    fn move_cursor_right(&mut self) {
        let current_value = self.get_current_field_value();
        if self.cursor_position < current_value.len() {
            self.cursor_position += 1;
            self.clear_selection();
        }
    }

    fn move_cursor_to_end(&mut self) {
        self.cursor_position = self.get_current_field_value().len();
        self.clear_selection();
    }

    fn move_cursor_to_start(&mut self) {
        self.cursor_position = 0;
        self.clear_selection();
    }

    fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    fn get_selection_range(&self) -> Option<(usize, usize)> {
        match (self.selection_start, self.selection_end) {
            (Some(start), Some(end)) if start != end => {
                let (min, max) = if start < end { (start, end) } else { (end, start) };
                Some((min, max))
            }
            _ => None,
        }
    }

    fn select_all(&mut self) {
        let len = self.get_current_field_value().len();
        self.selection_start = Some(0);
        self.selection_end = Some(len);
        self.cursor_position = len;
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            let mut current_value = self.get_current_field_value().to_string();
            current_value.replace_range(start..end, "");
            self.set_current_field_value(current_value);
            self.cursor_position = start;
            self.clear_selection();
            true
        } else {
            false
        }
    }

    fn copy_to_clipboard(&mut self) {
        let text_to_copy = if let Some((start, end)) = self.get_selection_range() {
            // Copy selected text
            let current_value = self.get_current_field_value();
            let chars: Vec<char> = current_value.chars().collect();
            chars[start..end].iter().collect::<String>()
        } else {
            // Copy all text if no selection
            self.get_current_field_value().to_string()
        };
        
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text_to_copy);
        }
    }

    /// Delete the word before the cursor
    /// A word is defined as a sequence of alphanumeric characters and underscores
    fn delete_word_backward(&mut self) {
        // If there's a selection, delete it first
        if self.delete_selection() {
            return;
        }

        let current_value = self.get_current_field_value();
        if current_value.is_empty() || self.cursor_position == 0 {
            return;
        }

        let chars: Vec<char> = current_value.chars().collect();
        let mut pos = self.cursor_position.min(chars.len());
        
        if pos == 0 {
            return;
        }

        // Skip whitespace before cursor
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        // Find the start of the word (alphanumeric + underscore)
        let word_start = if pos > 0 {
            let mut start = pos;
            // Check if we're in a word (alphanumeric or underscore)
            if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' {
                // Move back through word characters
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                    start -= 1;
                }
            } else {
                // We're at a non-word character, delete it
                start = pos - 1;
            }
            start
        } else {
            0
        };

        // Delete from word_start to cursor_position
        let mut new_value = current_value.to_string();
        new_value.replace_range(word_start..self.cursor_position, "");
        self.set_current_field_value(new_value);
        self.cursor_position = word_start;
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        
        // Outer container with double border
        let outer_block = Block::default()
            .title("Ollama Configuration")
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
            .title("Configure Ollama")
            .borders(Borders::ALL);
        let form_area = block.inner(content_area);
        block.render(content_area, buf);

        let mut y = form_area.y;
        let x = form_area.x;

        // Field labels and values
        let fields = [
            (Field::Host, "Host:", &self.config.host),
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
                // Display text with selection and cursor
                let cursor_pos = self.cursor_position.min(value.len());
                
                // Check if there's a selection
                if let Some((sel_start, sel_end)) = self.get_selection_range() {
                    // Render text with selection highlighting
                    let chars: Vec<char> = value.chars().collect();
                    let mut x_pos = x;
                    
                    // Render text before selection
                    if sel_start > 0 {
                        let before_text: String = chars[..sel_start].iter().collect();
                        buf.set_string(x_pos, y + 1, &before_text, value_style);
                        x_pos += before_text.chars().map(|c| c.len_utf8()).sum::<usize>() as u16;
                    }
                    
                    // Render selected text with highlight
                    if sel_end > sel_start {
                        let selected_text: String = chars[sel_start..sel_end].iter().collect();
                        let selection_style = Style::default()
                            .fg(Color::Black)
                            .bg(Color::White);
                        buf.set_string(x_pos, y + 1, &selected_text, selection_style);
                        x_pos += selected_text.chars().map(|c| c.len_utf8()).sum::<usize>() as u16;
                    }
                    
                    // Render text after selection
                    if sel_end < chars.len() {
                        let after_text: String = chars[sel_end..].iter().collect();
                        buf.set_string(x_pos, y + 1, &after_text, value_style);
                    }
                    
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
                    // No selection - render normally
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

        // First, check if there's a selection and handle literal input
        // When text is selected, literal input (characters, backspace, delete) should replace the selection
        let has_selection = self.get_selection_range().is_some();
        if has_selection {
            match key.code {
                KeyCode::Char(c) => {
                    // Replace selection with character
                    self.delete_selection();
                    let mut current_value = self.get_current_field_value().to_string();
                    let cursor_char_pos = self.cursor_position.min(current_value.chars().count());
                    let cursor_byte_pos = current_value.chars().take(cursor_char_pos).map(|c| c.len_utf8()).sum::<usize>();
                    current_value.insert(cursor_byte_pos, c);
                    self.cursor_position = cursor_char_pos + 1;
                    self.set_current_field_value(current_value);
                    self.clear_selection();
                    return None;
                }
                KeyCode::Backspace | KeyCode::Delete => {
                    // Delete the selection
                    self.delete_selection();
                    return None;
                }
                _ => {
                    // For other keys with selection, continue to action handling
                }
            }
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
        
        // Check Global actions
        if let Some(global_action) = &optional_global_action {
            match global_action {
                Action::Escape => {
                    return Some(Action::DialogClose);
                }
                Action::SelectAllText => {
                    self.select_all();
                    return None;
                }
                Action::CopyText => {
                    self.copy_to_clipboard();
                    return None;
                }
                Action::DeleteWord => {
                    self.delete_word_backward();
                    return None;
                }
                Action::Paste => {
                    if let Ok(mut clipboard) = Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            // If there's a selection, replace it; otherwise insert at cursor
                            if self.delete_selection() {
                                // Selection was deleted, cursor is at start position
                            }
                            let mut current_value = self.get_current_field_value().to_string();
                            let cursor_char_pos = self.cursor_position.min(current_value.chars().count());
                            let cursor_byte_pos = current_value.chars().take(cursor_char_pos).map(|c| c.len_utf8()).sum::<usize>();
                            current_value.insert_str(cursor_byte_pos, &text);
                            self.cursor_position = cursor_char_pos + text.chars().count();
                            self.set_current_field_value(current_value);
                            self.clear_selection();
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Next, check LlmClientDialog-specific actions
        if let Some(dialog_action) = &llm_dialog_action {
            match dialog_action {
                Action::Enter => {
                    return Some(Action::LlmClientDialogApplied(
                        {
                            let mut lc = crate::dialog::llm_client_dialog::LlmConfig::default();
                            lc.ollama = Some(self.config.clone());
                            lc
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
                    // If there's a selection, delete it; otherwise delete character before cursor
                    if !self.delete_selection() {
                        let mut current_value = self.get_current_field_value().to_string();
                        if self.cursor_position > 0 && self.cursor_position <= current_value.len() {
                            current_value.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                            self.set_current_field_value(current_value);
                        }
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
                    {
                        let mut lc = crate::dialog::llm_client_dialog::LlmConfig::default();
                        lc.ollama = Some(self.config.clone());
                        lc
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
                // If there's a selection, delete it; otherwise delete character before cursor
                if !self.delete_selection() {
                    let mut current_value = self.get_current_field_value().to_string();
                    if self.cursor_position > 0 {
                        let cursor_char_pos = self.cursor_position.min(current_value.chars().count());
                        if cursor_char_pos > 0 {
                            // Convert character position to byte position for removal
                            let chars: Vec<char> = current_value.chars().collect();
                            let byte_pos = chars[..cursor_char_pos - 1].iter().map(|c| c.len_utf8()).sum::<usize>();
                            let char_to_remove_byte_len = chars[cursor_char_pos - 1].len_utf8();
                            current_value.replace_range(byte_pos..byte_pos + char_to_remove_byte_len, "");
                            self.cursor_position = cursor_char_pos - 1;
                            self.set_current_field_value(current_value);
                        }
                    }
                }
                return None;
            }
            KeyCode::Delete => {
                // If there's a selection, delete it; otherwise delete character at cursor
                if !self.delete_selection() {
                    let mut current_value = self.get_current_field_value().to_string();
                    let cursor_char_pos = self.cursor_position.min(current_value.chars().count());
                    if cursor_char_pos < current_value.chars().count() {
                        // Convert character position to byte position for removal
                        let chars: Vec<char> = current_value.chars().collect();
                        let byte_pos = chars[..cursor_char_pos].iter().map(|c| c.len_utf8()).sum::<usize>();
                        let char_to_remove_byte_len = chars[cursor_char_pos].len_utf8();
                        current_value.replace_range(byte_pos..byte_pos + char_to_remove_byte_len, "");
                        self.set_current_field_value(current_value);
                    }
                }
                return None;
            }
            KeyCode::Char(c) => {
                // Insert character at cursor (selection already handled above)
                let mut current_value = self.get_current_field_value().to_string();
                // Convert character position to byte position for String::insert()
                let cursor_char_pos = self.cursor_position.min(current_value.chars().count());
                let cursor_byte_pos = current_value.chars().take(cursor_char_pos).map(|c| c.len_utf8()).sum::<usize>();
                current_value.insert(cursor_byte_pos, c);
                self.cursor_position = cursor_char_pos + 1;
                self.set_current_field_value(current_value);
                self.clear_selection();
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

impl Component for OllamaConfigDialog {
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
