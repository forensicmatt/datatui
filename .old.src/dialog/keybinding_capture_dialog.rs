use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Wrap};

use crate::action::Action;
use crate::components::Component;
use crate::config::{self, Config};
use crate::components::dialog_layout::split_dialog_area;

#[derive(Debug, Default)]
pub struct KeybindingCaptureDialog {
    pub show_instructions: bool,
    pub pressed_keys: Vec<KeyEvent>,
    pub pressed_display: String,
    pub config: Config,
}

impl KeybindingCaptureDialog {
    pub fn new() -> Self {
        Self {
            show_instructions: true,
            pressed_keys: Vec::new(),
            pressed_display: String::new(),
            config: Config::default(),
        }
    }

    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, Action::Enter),
            (crate::config::Mode::Global, Action::Escape),
        ])
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Compute a centered, smaller modal window inside the provided area
        let modal_width = area.width.saturating_sub(4).min(64).max(40);
        let modal_height = area.height.saturating_sub(4).min(10).max(7);
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal = Rect { x: modal_x, y: modal_y, width: modal_width, height: modal_height };

        Clear.render(modal, buf);

        let outer_block = Block::default()
            .title("Capture Keybinding")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let outer_inner = outer_block.inner(modal);
        outer_block.render(modal, buf);

        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(outer_inner, self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;

        let message = if self.pressed_display.is_empty() {
            "Press keys, Enter to apply, Esc to cancel".to_string()
        } else {
            self.pressed_display.clone()
        };

        let inner_block = Block::default().borders(Borders::ALL).title("New Binding");
        let inner = inner_block.inner(content_area);
        inner_block.render(content_area, buf);

        let p = Paragraph::new(message).wrap(Wrap { trim: true });
        p.render(inner, buf);

        // Render instructions panel if enabled
        if self.show_instructions {
            if let Some(instr_area) = layout.instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, buf);
            }
        }
    }
}

impl Component for KeybindingCaptureDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind != KeyEventKind::Press { return Ok(None); }

        // Respect global actions for confirm/cancel
        if let Some(a) = self.config.action_for_key(config::Mode::Global, key) {
            match a {
                Action::Enter => return Ok(Some(Action::ConfirmRebinding)),
                Action::Escape => return Ok(Some(Action::CancelRebinding)),
                _ => {}
            }
        }

        // Only allow a single key combination; replace any previous value
        self.pressed_keys.clear();
        self.pressed_keys.push(key);
        let key_strs: Vec<String> = self.pressed_keys.iter().map(config::key_event_to_string).collect();
        self.pressed_display = key_strs.join(" ");
        Ok(None)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}


