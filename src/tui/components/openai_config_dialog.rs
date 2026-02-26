//! OpenAI Configuration Dialog
//!
//! A TUI dialog for configuring OpenAI API settings.

use crate::core::llm_config::OpenAiConfig;
use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, BorderType, Borders, Clear},
    Frame,
};

/// Active field in the OpenAI config dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiField {
    ApiKey,
    BaseUrl,
}

/// OpenAI configuration dialog component
pub struct OpenAiConfigDialog {
    pub config: OpenAiConfig,
    pub active_field: OpenAiField,
    pub cursor_pos: usize,
    pub focused: bool,
    pub show_instructions: bool,
    /// Result to be retrieved by the parent dialog
    pub result: Option<OpenAiConfig>,
    pub closed: bool,
}

impl OpenAiConfigDialog {
    pub fn new(config: Option<OpenAiConfig>) -> Self {
        let config = config.unwrap_or_default();
        let cursor_pos = config.api_key.len();
        Self {
            config,
            active_field: OpenAiField::ApiKey,
            cursor_pos,
            focused: true,
            show_instructions: true,
            result: None,
            closed: false,
        }
    }

    fn next_field(&mut self) {
        self.active_field = match self.active_field {
            OpenAiField::ApiKey => OpenAiField::BaseUrl,
            OpenAiField::BaseUrl => OpenAiField::ApiKey,
        };
        self.update_cursor_pos();
    }

    fn prev_field(&mut self) {
        self.active_field = match self.active_field {
            OpenAiField::ApiKey => OpenAiField::BaseUrl,
            OpenAiField::BaseUrl => OpenAiField::ApiKey,
        };
        self.update_cursor_pos();
    }

    fn update_cursor_pos(&mut self) {
        let len = match self.active_field {
            OpenAiField::ApiKey => self.config.api_key.len(),
            OpenAiField::BaseUrl => self.config.base_url.len(),
        };
        self.cursor_pos = len;
    }

    fn get_current_value_mut(&mut self) -> &mut String {
        match self.active_field {
            OpenAiField::ApiKey => &mut self.config.api_key,
            OpenAiField::BaseUrl => &mut self.config.base_url,
        }
    }

    fn insert_char(&mut self, c: char) {
        let cursor_pos = self.cursor_pos;
        let val = self.get_current_value_mut();
        if cursor_pos <= val.len() {
            val.insert(cursor_pos, c);
            self.cursor_pos += 1;
        }
    }

    fn backspace(&mut self) {
        let cursor_pos = self.cursor_pos;
        let val = self.get_current_value_mut();
        if cursor_pos > 0 && !val.is_empty() {
            val.remove(cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    fn delete(&mut self) {
        let cursor_pos = self.cursor_pos;
        let val = self.get_current_value_mut();
        if cursor_pos < val.len() {
            val.remove(cursor_pos);
        }
    }

    fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    fn cursor_right(&mut self) {
        let val = match self.active_field {
            OpenAiField::ApiKey => &self.config.api_key,
            OpenAiField::BaseUrl => &self.config.base_url,
        };
        if self.cursor_pos < val.len() {
            self.cursor_pos += 1;
        }
    }
}

impl Component for OpenAiConfigDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        if !self.focused {
            return Ok(KeyEventResult::Ignored);
        }

        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert_char(c);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Backspace => {
                self.backspace();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Delete => {
                self.delete();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Left => {
                self.cursor_left();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Right => {
                self.cursor_right();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Tab => {
                self.next_field();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::BackTab => {
                self.prev_field();
                Ok(KeyEventResult::Consumed)
            }
            _ => Ok(KeyEventResult::Ignored),
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Cancel | Action::Escape => {
                self.closed = true;
                Ok(false)
            }
            Action::Confirm => {
                self.result = Some(self.config.clone());
                self.closed = true;
                Ok(false)
            }
            Action::MoveDown => {
                self.next_field();
                Ok(true)
            }
            Action::MoveUp => {
                self.prev_field();
                Ok(true)
            }
            Action::ToggleInstructions => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);

        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let block = Block::default()
            .title(" OpenAI Configuration ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style);

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let (content_area, instr_area) = if self.show_instructions {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(6)])
                .split(inner_area);
            (chunks[0], Some(chunks[1]))
        } else {
            (inner_area, None)
        };

        let field_layout = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(2)
            .vertical_margin(1)
            .constraints([
                Constraint::Length(3), // API Key
                Constraint::Length(3), // Base URL
            ])
            .split(content_area);

        let api_key = self.config.api_key.clone();
        let base_url = self.config.base_url.clone();
        let active_field = self.active_field;
        let cursor_pos = self.cursor_pos;

        let buf = frame.buffer_mut();

        // Render API Key Field
        let api_key_cursor = if active_field == OpenAiField::ApiKey {
            Some(cursor_pos)
        } else {
            None
        };
        render_field(
            buf,
            field_layout[0],
            "API Key",
            &api_key,
            api_key_cursor,
            theme,
        );

        // Render Base URL Field
        let base_url_cursor = if active_field == OpenAiField::BaseUrl {
            Some(cursor_pos)
        } else {
            None
        };
        render_field(
            buf,
            field_layout[1],
            "Base URL",
            &base_url,
            base_url_cursor,
            theme,
        );

        if let Some(instr_area) = instr_area {
            render_instructions(buf, instr_area, theme);
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Escape,
            Action::Confirm,
            Action::MoveDown,
            Action::MoveUp,
            Action::ToggleInstructions,
        ]
    }

    fn name(&self) -> &str {
        "OpenAiConfigDialog"
    }
}

fn render_field(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    label: &str,
    value: &str,
    cursor_pos: Option<usize>,
    theme: &Theme,
) {
    let is_active = cursor_pos.is_some();
    let border_style = if is_active {
        theme.focused_border_style()
    } else {
        theme.border_style()
    };

    let block = Block::default()
        .title(format!(" {} ", label))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    // Using Widget::render directly on Buffer
    use ratatui::widgets::Widget;
    block.render(area, buf);

    if let Some(pos) = cursor_pos {
        let cursor_pos = pos.min(value.len());
        let (before, after) = value.split_at(cursor_pos);

        let mut x_offset = inner_area.x;
        buf.set_string(x_offset, inner_area.y, before, theme.normal_style());
        x_offset += before.len() as u16;

        let char_at_cursor = if after.is_empty() { " " } else { &after[0..1] };

        buf.set_string(
            x_offset,
            inner_area.y,
            char_at_cursor,
            theme.selected_cell_style(),
        );

        if after.len() > 1 {
            buf.set_string(
                x_offset + 1,
                inner_area.y,
                &after[1..],
                theme.normal_style(),
            );
        }
    } else {
        buf.set_string(inner_area.x, inner_area.y, value, theme.normal_style());
    }
}

fn render_instructions(buf: &mut ratatui::buffer::Buffer, area: Rect, theme: &Theme) {
    let block = Block::default()
        .title(" Instructions (Ctrl+i to hide) ")
        .borders(Borders::TOP)
        .border_style(theme.border_style());

    let inner_area = block.inner(area);
    // Using Widget::render directly on Buffer
    use ratatui::widgets::Widget;
    block.render(area, buf);

    let instructions = vec![
        "• Tab/Shift+Tab, Up/Down: Navigate fields",
        "• Enter: Save and Close",
        "• Esc: Cancel",
        "• Typable: All visible characters",
        "• Backspace/Delete: Remove characters",
    ];

    let mut y = inner_area.y;
    for line in instructions {
        if y < inner_area.bottom() {
            buf.set_string(inner_area.x, y, line, theme.warning_style());
            y += 1;
        }
    }
}

impl Focusable for OpenAiConfigDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
