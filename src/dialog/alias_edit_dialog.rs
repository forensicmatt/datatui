//! AliasEditDialog: Simple dialog for editing a dataset alias

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use ratatui::text::{Span, Line};
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;

/// AliasEditDialog: Simple dialog for editing a dataset alias
#[derive(Debug)]
pub struct AliasEditDialog {
    pub source_id: usize,
    pub dataset_id: String,
    pub dataset_name: String,
    pub current_alias: String,
    pub input_buffer: String,
    pub show_instructions: bool,
    pub cursor_index: usize,
    pub cursor_visible: bool,
}

impl AliasEditDialog {
    /// Create a new AliasEditDialog
    pub fn new(source_id: usize, dataset_id: String, dataset_name: String, current_alias: Option<String>) -> Self {
        let current_alias = current_alias.unwrap_or_default();
        let initial_len = current_alias.len();
        Self {
            source_id,
            dataset_id,
            dataset_name,
            input_buffer: current_alias.clone(),
            current_alias,
            show_instructions: true,
            cursor_index: initial_len,
            cursor_visible: true,
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the background for the popup
        Clear.render(area, buf);
        
        // Outer container with double border and title "Alias"
        let outer_block = Block::default()
            .title("Alias")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let outer_inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let instructions = "Enter: Save  Esc: Cancel  Ctrl+d: Clear";
        let layout = split_dialog_area(outer_inner_area, self.show_instructions, Some(instructions));
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        
        let content_block = Block::default()
            .title(format!("Edit Alias for: {}", self.dataset_name))
            .borders(Borders::ALL);
        
        let content_inner_area = content_block.inner(content_area);
        content_block.render(content_area, buf);

        // Vertical layout: show current alias line, then new alias line directly below
        // Build lines with styled cursor character (black) so underlying letter is visible
        let current_line = format!(
            "Current alias: {}",
            if self.current_alias.is_empty() { "<None>" } else { &self.current_alias }
        );

        let mut new_alias_spans: Vec<Span> = Vec::new();
        new_alias_spans.push(Span::raw("New alias: "));
        let input_len = self.input_buffer.len();
        let idx = self.cursor_index.min(input_len);
        if self.cursor_visible {
            if idx < input_len {
                // Split at byte index and style the char under the cursor
                let ch = self.input_buffer[idx..].chars().next().unwrap();
                let ch_len = ch.len_utf8();
                let prefix = &self.input_buffer[..idx];
                let suffix = &self.input_buffer[idx + ch_len..];
                if !prefix.is_empty() {
                    new_alias_spans.push(Span::raw(prefix));
                }
                new_alias_spans.push(Span::styled(ch.to_string(), Style::default().fg(Color::Black).bg(Color::White)));
                if !suffix.is_empty() {
                    new_alias_spans.push(Span::raw(suffix));
                }
            } else {
                // Cursor at end: show a black space to indicate position
                if !self.input_buffer.is_empty() {
                    new_alias_spans.push(Span::raw(self.input_buffer.as_str()));
                }
                new_alias_spans.push(Span::styled(" ", Style::default().fg(Color::Black).bg(Color::White)));
            }
        } else {
            // Cursor hidden: render normally
            if !self.input_buffer.is_empty() {
                new_alias_spans.push(Span::raw(self.input_buffer.as_str()));
            }
        }

        let paragraph = Paragraph::new(vec![
                Line::from(current_line),
                Line::from(new_alias_spans),
            ])
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true });
        paragraph.render(content_inner_area, buf);

        self.render_instructions(instructions, instructions_area, buf);
    }

    fn render_instructions(&self, instructions: &str, instructions_area: Option<Rect>, buf: &mut Buffer) {
        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    /// Add a character to the input buffer
    pub fn add_char(&mut self, c: char) {
        let idx = self.cursor_index.min(self.input_buffer.len());
        self.input_buffer.insert(idx, c);
        self.cursor_index = (idx + c.len_utf8()).min(self.input_buffer.len());
    }

    /// Remove the last character from the input buffer
    pub fn backspace(&mut self) {
        if self.cursor_index > 0 {
            // Find previous char boundary
            let mut remove_at = self.cursor_index - 1;
            while !self.input_buffer.is_char_boundary(remove_at) {
                remove_at -= 1;
            }
            self.input_buffer.remove(remove_at);
            self.cursor_index = remove_at;
        }
    }

    /// Clear the input buffer
    pub fn clear(&mut self) {
        self.input_buffer.clear();
        self.cursor_index = 0;
    }

    
    /// Get the current input as an alias (None if empty)
    pub fn get_alias(&self) -> Option<String> {
        if self.input_buffer.trim().is_empty() {
            None
        } else {
            Some(self.input_buffer.trim().to_string())
        }
    }
}

impl Component for AliasEditDialog {
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
            Some(Event::Key(key)) => self.handle_key_event(key),
            Some(Event::Tick) => {
                // toggle cursor visibility on ticks for blink
                self.cursor_visible = !self.cursor_visible;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind == crossterm::event::KeyEventKind::Press {
            match key.code {
                KeyCode::Enter => {
                    // Save the alias
                    let alias = self.get_alias();
                    Ok(Some(Action::EditDatasetAlias {
                        source_id: self.source_id,
                        dataset_id: self.dataset_id.clone(),
                        alias,
                    }))
                }
                KeyCode::Esc => {
                    // Cancel the edit
                    Ok(Some(Action::DialogClose))
                }
                KeyCode::Backspace => {
                    self.backspace();
                    Ok(None)
                }
                KeyCode::Left => {
                    if self.cursor_index > 0 {
                        let mut new_idx = self.cursor_index - 1;
                        while !self.input_buffer.is_char_boundary(new_idx) && new_idx > 0 {
                            new_idx -= 1;
                        }
                        self.cursor_index = new_idx;
                    }
                    Ok(None)
                }
                KeyCode::Right => {
                    if self.cursor_index < self.input_buffer.len() {
                        let mut new_idx = self.cursor_index + 1;
                        while new_idx < self.input_buffer.len() && !self.input_buffer.is_char_boundary(new_idx) {
                            new_idx += 1;
                        }
                        self.cursor_index = new_idx.min(self.input_buffer.len());
                    }
                    Ok(None)
                }
                KeyCode::Home => {
                    self.cursor_index = 0;
                    Ok(None)
                }
                KeyCode::End => {
                    self.cursor_index = self.input_buffer.len();
                    Ok(None)
                }
                KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    self.clear();
                    Ok(None)
                }
                KeyCode::Char(c) => {
                    self.add_char(c);
                    Ok(None)
                }
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alias_edit_dialog_creation() {
        let dialog = AliasEditDialog::new(
            1,
            "dataset_id".to_string(),
            "Test Dataset".to_string(),
            Some("Current Alias".to_string())
        );
        
        assert_eq!(dialog.source_id, 1);
        assert_eq!(dialog.dataset_id, "dataset_id");
        assert_eq!(dialog.dataset_name, "Test Dataset");
        assert_eq!(dialog.current_alias, "Current Alias");
        assert_eq!(dialog.input_buffer, "Current Alias");
    }

    #[test]
    fn test_alias_edit_dialog_no_alias() {
        let dialog = AliasEditDialog::new(
            1,
            "dataset_id".to_string(),
            "Test Dataset".to_string(),
            None
        );
        
        assert_eq!(dialog.current_alias, "");
        assert_eq!(dialog.input_buffer, "");
    }

    #[test]
    fn test_alias_edit_operations() {
        let mut dialog = AliasEditDialog::new(
            1,
            "dataset_id".to_string(),
            "Test Dataset".to_string(),
            Some("Original".to_string())
        );
        
        // Test adding characters
        dialog.add_char('X');
        assert_eq!(dialog.input_buffer, "OriginalX");
        
        // Test backspace
        dialog.backspace();
        assert_eq!(dialog.input_buffer, "Original");
        
        // Test clear
        dialog.clear();
        assert_eq!(dialog.input_buffer, "");
        
    }

    #[test]
    fn test_get_alias() {
        let mut dialog = AliasEditDialog::new(
            1,
            "dataset_id".to_string(),
            "Test Dataset".to_string(),
            None
        );
        
        // Empty input should return None
        assert_eq!(dialog.get_alias(), None);
        
        // Whitespace-only input should return None
        dialog.input_buffer = "   ".to_string();
        assert_eq!(dialog.get_alias(), None);
        
        // Valid input should return Some with trimmed value
        dialog.input_buffer = "  Valid Alias  ".to_string();
        assert_eq!(dialog.get_alias(), Some("Valid Alias".to_string()));
    }
}
