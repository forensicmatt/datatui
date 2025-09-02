use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType};

use crate::action::Action;
use crate::components::Component;

/// Simple reusable message dialog for transient notifications (info/success/warning).
#[derive(Debug, Clone)]
pub struct MessageDialog {
    title: String,
    message: String,
    pub show_instructions: bool,
}

impl MessageDialog {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            title: "Message".to_string(),
            message: message.into(),
            show_instructions: true,
        }
    }

    pub fn with_title(message: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            show_instructions: true,
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) { self.message = message.into(); }
    pub fn set_title(&mut self, title: impl Into<String>) { self.title = title.into(); }

    fn modal_area(&self, area: Rect) -> Rect {
        let max_width = area.width.clamp(20, 40);
        let wrap_width = max_width.saturating_sub(4) as usize;
        let wrapped = textwrap::wrap(&self.message, wrap_width);
        let content_lines = wrapped.len() as u16;
        let height = content_lines
            .saturating_add(4) // borders + padding
            .clamp(5, area.height.saturating_sub(4));
        let width = max_width;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect { x, y, width, height }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Do not clear entire area; overlay on top of underlying content
        let modal = self.modal_area(area);

        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner = block.inner(modal);
        block.render(modal, buf);

        let wrap_width = inner.width.saturating_sub(2) as usize;
        let wrapped = textwrap::wrap(&self.message, wrap_width);

        for (i, line) in wrapped.iter().enumerate() {
            if i as u16 >= inner.height { break; }
            buf.set_string(inner.x + 1, inner.y + i as u16, line, Style::default().fg(Color::White));
        }

        let hint = "Enter/Esc to close";
        let hint_x = inner.x + inner.width.saturating_sub(hint.len() as u16 + 1);
        let hint_y = inner.y + inner.height.saturating_sub(1);
        buf.set_string(hint_x, hint_y, hint, Style::default().fg(Color::Gray));
    }
}

impl Component for MessageDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        use crossterm::event::KeyModifiers;
        if key.kind == KeyEventKind::Press {
            if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.show_instructions = !self.show_instructions;
                return Ok(None);
            }
            match key.code {
                KeyCode::Enter | KeyCode::Esc => return Ok(Some(Action::DialogClose)),
                _ => {}
            }
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}


