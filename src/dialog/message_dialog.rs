use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType};
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::components::Component;
use crate::config::Config;

/// Simple reusable message dialog for transient notifications (info/success/warning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDialog {
    title: String,
    pub message: String,
    pub show_instructions: bool,
    #[serde(skip)]
    pub config: Config,
}

impl MessageDialog {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            title: "Message".to_string(),
            message: message.into(),
            show_instructions: true,
            config: Config::default(),
        }
    }

    pub fn with_title(message: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            show_instructions: true,
            config: Config::default(),
        }
    }

    pub fn set_message(&mut self, message: impl Into<String>) { self.message = message.into(); }
    pub fn set_title(&mut self, title: impl Into<String>) { self.title = title.into(); }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Escape),
        ])
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let modal = area;

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

        let instructions = self.build_instructions_from_config();
        let hint = if instructions.is_empty() { 
            "Enter/Esc to close".to_string() 
        } else { 
            instructions 
        };
        let hint_x = inner.x + inner.width.saturating_sub(hint.len() as u16 + 1);
        let hint_y = inner.y + inner.height.saturating_sub(1);
        buf.set_string(hint_x, hint_y, hint, Style::default().fg(Color::Gray));
    }
}

impl Component for MessageDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind == KeyEventKind::Press {
            // First, honor config-driven Global actions
            if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                match global_action {
                    Action::Escape => return Ok(Some(Action::DialogClose)),
                    Action::Enter => return Ok(Some(Action::DialogClose)),
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // Next, check for dialog-specific actions
            if let Some(dialog_action) = self.config.action_for_key(crate::config::Mode::MessageDialog, key) {
                match dialog_action {
                    Action::Escape => return Ok(Some(Action::DialogClose)),
                    Action::Enter => return Ok(Some(Action::DialogClose)),
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}


