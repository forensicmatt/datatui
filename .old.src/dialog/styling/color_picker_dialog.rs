//! ColorPickerDialog: Dialog for selecting colors from a list or entering hex codes
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use ratatui::style::Color;
use arboard::Clipboard;

/// Named colors available in the picker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedColor {
    pub name: &'static str,
    pub color: Color,
}

/// All available named colors
pub const NAMED_COLORS: &[NamedColor] = &[
    NamedColor { name: "Black", color: Color::Black },
    NamedColor { name: "Red", color: Color::Red },
    NamedColor { name: "Green", color: Color::Green },
    NamedColor { name: "Yellow", color: Color::Yellow },
    NamedColor { name: "Blue", color: Color::Blue },
    NamedColor { name: "Magenta", color: Color::Magenta },
    NamedColor { name: "Cyan", color: Color::Cyan },
    NamedColor { name: "White", color: Color::White },
    NamedColor { name: "Gray", color: Color::Gray },
    NamedColor { name: "DarkGray", color: Color::DarkGray },
    NamedColor { name: "LightRed", color: Color::LightRed },
    NamedColor { name: "LightGreen", color: Color::LightGreen },
    NamedColor { name: "LightYellow", color: Color::LightYellow },
    NamedColor { name: "LightBlue", color: Color::LightBlue },
    NamedColor { name: "LightMagenta", color: Color::LightMagenta },
    NamedColor { name: "LightCyan", color: Color::LightCyan },
    NamedColor { name: "Reset", color: Color::Reset },
];

/// Focus area in the color picker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPickerFocus {
    ColorList,
    HexInput,
}

/// ColorPickerDialog: UI for selecting colors
#[derive(Debug)]
pub struct ColorPickerDialog {
    /// Currently selected color index in the list
    pub selected_color_index: usize,
    /// Hex input value (e.g., "#FF5500" or "rgb(255,85,0)")
    pub hex_input: String,
    /// Current focus area
    pub focus: ColorPickerFocus,
    /// The resulting selected color
    pub selected_color: Option<Color>,
    /// Cursor position in hex input
    pub cursor_position: usize,
    /// Selection start for text input
    pub selection_start: Option<usize>,
    /// Selection end for text input
    pub selection_end: Option<usize>,
    /// Show instructions
    pub show_instructions: bool,
    /// Config
    pub config: Config,
    /// Scroll offset for color list
    pub scroll_offset: usize,
}

impl Default for ColorPickerDialog {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ColorPickerDialog {
    /// Create a new ColorPickerDialog with optional initial color
    pub fn new(initial_color: Option<Color>) -> Self {
        let (selected_index, hex_input) = if let Some(color) = initial_color {
            // Try to find matching named color
            let idx = NAMED_COLORS.iter().position(|nc| nc.color == color).unwrap_or(0);
            let hex = color_to_hex_string(&color);
            (idx, hex)
        } else {
            (0, String::new())
        };

        Self {
            selected_color_index: selected_index,
            hex_input,
            focus: ColorPickerFocus::ColorList,
            selected_color: initial_color,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
            show_instructions: true,
            config: Config::default(),
            scroll_offset: 0,
        }
    }

    /// Get the currently selected/entered color
    pub fn get_selected_color(&self) -> Option<Color> {
        // If hex input is not empty and valid, use that; otherwise use list selection
        if !self.hex_input.is_empty() {
            parse_color_string(&self.hex_input)
        } else {
            NAMED_COLORS.get(self.selected_color_index).map(|nc| nc.color)
        }
    }

    /// Clear the current selection
    fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    /// Get the selection range as (start, end) if a selection exists
    fn get_selection_range(&self) -> Option<(usize, usize)> {
        match (self.selection_start, self.selection_end) {
            (Some(start), Some(end)) if start != end => {
                let (min, max) = if start < end { (start, end) } else { (end, start) };
                Some((min, max))
            }
            _ => None,
        }
    }

    /// Select all text in the hex input
    fn select_all(&mut self) {
        let len = self.hex_input.chars().count();
        self.selection_start = Some(0);
        self.selection_end = Some(len);
        self.cursor_position = len;
    }

    /// Delete the selected text if a selection exists
    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            let chars: Vec<char> = self.hex_input.chars().collect();
            self.hex_input = chars[..start].iter().chain(chars[end..].iter()).collect();
            self.cursor_position = start;
            self.clear_selection();
            true
        } else {
            false
        }
    }

    /// Copy text to clipboard
    fn copy_to_clipboard(&self) {
        let text_to_copy = if let Some((start, end)) = self.get_selection_range() {
            let chars: Vec<char> = self.hex_input.chars().collect();
            chars[start..end].iter().collect::<String>()
        } else {
            self.hex_input.clone()
        };

        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text_to_copy);
        }
    }

    /// Delete word backward
    fn delete_word_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.hex_input.is_empty() || self.cursor_position == 0 {
            return;
        }

        let chars: Vec<char> = self.hex_input.chars().collect();
        let mut pos = self.cursor_position.min(chars.len());

        if pos == 0 {
            return;
        }

        // Skip whitespace before cursor
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        // Find the start of the word
        let word_start = if pos > 0 {
            let mut start = pos;
            if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' || chars[pos - 1] == '#' {
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '#') {
                    start -= 1;
                }
            } else {
                start = pos - 1;
            }
            start
        } else {
            0
        };

        self.hex_input = chars[..word_start].iter().chain(chars[self.cursor_position..].iter()).collect();
        self.cursor_position = word_start;
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        let focus_hint = match self.focus {
            ColorPickerFocus::ColorList => "Tab: Switch to Hex Input",
            ColorPickerFocus::HexInput => "Tab: Switch to Color List",
        };
        format!(
            "{}  {}",
            focus_hint,
            self.config.actions_to_instructions(&[
                (Mode::Global, Action::Up),
                (Mode::Global, Action::Down),
                (Mode::Global, Action::Enter),
                (Mode::Global, Action::Escape),
            ])
        )
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Color Picker")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(
            inner_area,
            self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) },
        );
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        // Split content area: left for color list, right for hex input and preview
        let left_width = content_area.width.saturating_sub(2) / 2;
        let right_width = content_area.width.saturating_sub(left_width).saturating_sub(1);

        let left_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: left_width,
            height: content_area.height,
        };

        let right_area = Rect {
            x: content_area.x + left_width + 1,
            y: content_area.y,
            width: right_width,
            height: content_area.height,
        };

        // Render color list on the left
        self.render_color_list(left_area, buf);

        // Render hex input and preview on the right
        self.render_hex_input_and_preview(right_area, buf);

        // Render instructions
        if self.show_instructions {
            if let Some(instr_area) = instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, buf);
            }
        }
    }

    /// Render the color list
    fn render_color_list(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.focus == ColorPickerFocus::ColorList;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title("Colors")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let max_visible = inner.height as usize;
        let total_colors = NAMED_COLORS.len();

        // Calculate scroll offset to keep selected item visible
        let scroll_offset = if self.selected_color_index < self.scroll_offset {
            self.selected_color_index
        } else if self.selected_color_index >= self.scroll_offset + max_visible {
            self.selected_color_index.saturating_sub(max_visible - 1)
        } else {
            self.scroll_offset
        };

        let end = (scroll_offset + max_visible).min(total_colors);

        for (vis_idx, i) in (scroll_offset..end).enumerate() {
            let y = inner.y + vis_idx as u16;
            let nc = &NAMED_COLORS[i];

            let is_selected = i == self.selected_color_index;
            let text_style = if is_selected && is_focused {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Black).bg(Color::Gray)
            } else {
                Style::default()
            };

            // Render color swatch (2 chars) + name
            let swatch = "██";
            let swatch_style = Style::default().fg(nc.color);
            
            buf.set_string(inner.x, y, swatch, swatch_style);
            buf.set_string(inner.x + 3, y, nc.name, text_style);
        }
    }

    /// Render hex input and color preview
    fn render_hex_input_and_preview(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.focus == ColorPickerFocus::HexInput;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title("Hex/RGB Input")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let start_x = inner.x;
        let start_y = inner.y;

        // Label
        buf.set_string(start_x, start_y, "Enter hex (#FF5500) or rgb(r,g,b):", Style::default().fg(Color::Gray));

        // Input field
        let input_y = start_y + 1;
        let input_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Render input with selection highlighting
        if let Some((sel_start, sel_end)) = self.get_selection_range() {
            let chars: Vec<char> = self.hex_input.chars().collect();
            let mut x_pos = start_x;

            // Before selection
            if sel_start > 0 {
                let before: String = chars[..sel_start].iter().collect();
                buf.set_string(x_pos, input_y, &before, input_style);
                x_pos += before.len() as u16;
            }

            // Selected text
            let selected: String = chars[sel_start..sel_end].iter().collect();
            let selection_style = Style::default().fg(Color::Black).bg(Color::White);
            buf.set_string(x_pos, input_y, &selected, selection_style);
            x_pos += selected.len() as u16;

            // After selection
            if sel_end < chars.len() {
                let after: String = chars[sel_end..].iter().collect();
                buf.set_string(x_pos, input_y, &after, input_style);
            }
        } else {
            buf.set_string(start_x, input_y, &self.hex_input, input_style);
        }

        // Render cursor if focused
        if is_focused && self.get_selection_range().is_none() {
            let cursor_x = start_x + self.cursor_position as u16;
            let cursor_char = self.hex_input.chars().nth(self.cursor_position).unwrap_or(' ');
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(cursor_x, input_y, cursor_char.to_string(), cursor_style);
        }

        // Color preview section
        let preview_y = start_y + 3;
        buf.set_string(start_x, preview_y, "Preview:", Style::default().fg(Color::Gray));

        let preview_color = self.get_selected_color();
        let preview_y = preview_y + 1;

        if let Some(color) = preview_color {
            // Large color swatch
            let swatch = "████████████████";
            let swatch_style = Style::default().fg(color);
            buf.set_string(start_x, preview_y, swatch, swatch_style);
            buf.set_string(start_x, preview_y + 1, swatch, swatch_style);

            // Show color value
            let color_str = color_to_hex_string(&color);
            buf.set_string(start_x, preview_y + 3, &color_str, Style::default().fg(Color::White));
        } else {
            buf.set_string(start_x, preview_y, "No valid color", Style::default().fg(Color::DarkGray));
        }
    }

    /// Handle a key event
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Check Global actions first
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseColorPickerDialog);
                }
                Action::Enter => {
                    let color = self.get_selected_color();
                    return Some(Action::ColorPickerDialogApplied(color));
                }
                Action::Up => {
                    match self.focus {
                        ColorPickerFocus::ColorList => {
                            if self.selected_color_index > 0 {
                                self.selected_color_index -= 1;
                            } else {
                                self.selected_color_index = NAMED_COLORS.len() - 1;
                            }
                            // Update hex input to match selected color
                            self.hex_input = color_to_hex_string(&NAMED_COLORS[self.selected_color_index].color);
                            self.cursor_position = self.hex_input.chars().count();
                        }
                        ColorPickerFocus::HexInput => {
                            // Do nothing in hex input
                        }
                    }
                    return None;
                }
                Action::Down => {
                    match self.focus {
                        ColorPickerFocus::ColorList => {
                            if self.selected_color_index < NAMED_COLORS.len() - 1 {
                                self.selected_color_index += 1;
                            } else {
                                self.selected_color_index = 0;
                            }
                            // Update hex input to match selected color
                            self.hex_input = color_to_hex_string(&NAMED_COLORS[self.selected_color_index].color);
                            self.cursor_position = self.hex_input.chars().count();
                        }
                        ColorPickerFocus::HexInput => {
                            // Do nothing in hex input
                        }
                    }
                    return None;
                }
                Action::Left => {
                    if self.focus == ColorPickerFocus::HexInput && self.cursor_position > 0 {
                        self.cursor_position -= 1;
                        self.clear_selection();
                    }
                    return None;
                }
                Action::Right => {
                    if self.focus == ColorPickerFocus::HexInput {
                        let len = self.hex_input.chars().count();
                        if self.cursor_position < len {
                            self.cursor_position += 1;
                            self.clear_selection();
                        }
                    }
                    return None;
                }
                Action::Tab => {
                    self.focus = match self.focus {
                        ColorPickerFocus::ColorList => ColorPickerFocus::HexInput,
                        ColorPickerFocus::HexInput => ColorPickerFocus::ColorList,
                    };
                    return None;
                }
                Action::Backspace => {
                    if self.focus == ColorPickerFocus::HexInput {
                        if !self.delete_selection() && self.cursor_position > 0 {
                            let chars: Vec<char> = self.hex_input.chars().collect();
                            self.hex_input = chars[..self.cursor_position - 1]
                                .iter()
                                .chain(chars[self.cursor_position..].iter())
                                .collect();
                            self.cursor_position -= 1;
                        }
                    }
                    return None;
                }
                Action::SelectAllText => {
                    if self.focus == ColorPickerFocus::HexInput {
                        self.select_all();
                    }
                    return None;
                }
                Action::CopyText => {
                    if self.focus == ColorPickerFocus::HexInput {
                        self.copy_to_clipboard();
                    }
                    return None;
                }
                Action::DeleteWord => {
                    if self.focus == ColorPickerFocus::HexInput {
                        self.delete_word_backward();
                    }
                    return None;
                }
                Action::Paste => {
                    if self.focus == ColorPickerFocus::HexInput {
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                self.delete_selection();
                                let chars: Vec<char> = self.hex_input.chars().collect();
                                let before: String = chars[..self.cursor_position].iter().collect();
                                let after: String = chars[self.cursor_position..].iter().collect();
                                self.hex_input = format!("{}{}{}", before, text, after);
                                self.cursor_position += text.chars().count();
                                self.clear_selection();
                            }
                        }
                    }
                    return None;
                }
                Action::ToggleInstructions => {
                    self.show_instructions = !self.show_instructions;
                    return None;
                }
                _ => {}
            }
        }

        // Handle character input for hex field
        if self.focus == ColorPickerFocus::HexInput {
            if let KeyCode::Char(c) = key.code {
                self.delete_selection();
                let chars: Vec<char> = self.hex_input.chars().collect();
                let before: String = chars[..self.cursor_position].iter().collect();
                let after: String = chars[self.cursor_position..].iter().collect();
                self.hex_input = format!("{}{}{}", before, c, after);
                self.cursor_position += 1;
                self.clear_selection();
                return None;
            }
            if key.code == KeyCode::Delete {
                if !self.delete_selection() {
                    let chars: Vec<char> = self.hex_input.chars().collect();
                    if self.cursor_position < chars.len() {
                        self.hex_input = chars[..self.cursor_position]
                            .iter()
                            .chain(chars[self.cursor_position + 1..].iter())
                            .collect();
                    }
                }
                return None;
            }
        }

        None
    }
}

impl Component for ColorPickerDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        Ok(self.handle_key_event_pub(key))
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}

/// Parse a color string (hex or rgb format) into a Color
pub fn parse_color_string(s: &str) -> Option<Color> {
    let s = s.trim();
    
    // Handle hex format: #RRGGBB or #RGB
    if s.starts_with('#') {
        let hex = &s[1..];
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some(Color::Rgb(r, g, b));
        }
    }
    
    // Handle rgb format: rgb(r,g,b)
    if s.starts_with("rgb(") && s.ends_with(')') {
        let inner = &s[4..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    // Try to match named colors (case-insensitive)
    let lower = s.to_lowercase();
    for nc in NAMED_COLORS {
        if nc.name.to_lowercase() == lower {
            return Some(nc.color);
        }
    }

    None
}

/// Convert a Color to a hex string representation
pub fn color_to_hex_string(color: &Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        Color::Black => "Black".to_string(),
        Color::Red => "Red".to_string(),
        Color::Green => "Green".to_string(),
        Color::Yellow => "Yellow".to_string(),
        Color::Blue => "Blue".to_string(),
        Color::Magenta => "Magenta".to_string(),
        Color::Cyan => "Cyan".to_string(),
        Color::White => "White".to_string(),
        Color::Gray => "Gray".to_string(),
        Color::DarkGray => "DarkGray".to_string(),
        Color::LightRed => "LightRed".to_string(),
        Color::LightGreen => "LightGreen".to_string(),
        Color::LightYellow => "LightYellow".to_string(),
        Color::LightBlue => "LightBlue".to_string(),
        Color::LightMagenta => "LightMagenta".to_string(),
        Color::LightCyan => "LightCyan".to_string(),
        Color::Reset => "Reset".to_string(),
        Color::Indexed(i) => format!("indexed({})", i),
    }
}

