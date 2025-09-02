use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;

/// Simple reusable dialog for displaying error messages.
#[derive(Debug, Clone)]
pub struct ErrorDialog {
    message: String,
    title: String,
}

impl ErrorDialog {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            title: "Error".to_string(),
        }
    }

    pub fn with_title(message: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            title: title.into(),
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    fn modal_area(&self, area: Rect) -> Rect {
        let max_width = area.width.saturating_sub(10).clamp(20, 80);
        let wrap_width = max_width.saturating_sub(4) as usize;
        let wrapped = textwrap::wrap(&self.message, wrap_width);
        let content_lines = wrapped.len() as u16;
        let height = content_lines
            .saturating_add(4) // top/bottom padding + hint line
            .clamp(5, area.height.saturating_sub(4));

        let width = max_width;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect { x, y, width, height }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let modal = self.modal_area(area);

        // Opaque background fill inside modal region
        for y in modal.y..modal.y + modal.height {
            let line = " ".repeat(modal.width as usize);
            buf.set_string(modal.x, y, &line, Style::default().bg(Color::Black));
        }

        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .style(Style::default().bg(Color::Black));
        let inner = block.inner(modal);
        block.render(modal, buf);

        let hint = "Press Enter or Esc to close";
        let wrap = Paragraph::new(self.message.as_str())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Red).bg(Color::Black));
        wrap.render(inner, buf);

        if inner.height >= 2 {
            let hint_y = inner.y + inner.height - 1;
            let hint_x = inner.x + 1;
            buf.set_string(hint_x, hint_y, hint, Style::default().fg(Color::Gray).bg(Color::Black));
        }
    }
}

/// Helper to render an `ErrorDialog` directly into a buffer within a given area.
pub fn render_error_dialog(dialog: &ErrorDialog, area: Rect, buf: &mut Buffer) {
    dialog.render(area, buf);
}

impl Component for ErrorDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind == KeyEventKind::Press {
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


