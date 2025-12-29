//! ApplicationScopeEditorDialog: Dialog for editing StyleApplication (scope + style)
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{
    ApplicationScope, StyleApplication, MatchedStyle, GrepCapture,
};
use crate::dialog::styling::color_picker_dialog::{ColorPickerDialog, color_to_hex_string};
use ratatui::style::{Color, Modifier};

/// Available modifiers for styling
pub const AVAILABLE_MODIFIERS: &[(Modifier, &str)] = &[
    (Modifier::BOLD, "Bold"),
    (Modifier::DIM, "Dim"),
    (Modifier::ITALIC, "Italic"),
    (Modifier::UNDERLINED, "Underlined"),
    (Modifier::SLOW_BLINK, "Slow Blink"),
    (Modifier::RAPID_BLINK, "Rapid Blink"),
    (Modifier::REVERSED, "Reversed"),
    (Modifier::HIDDEN, "Hidden"),
    (Modifier::CROSSED_OUT, "Crossed Out"),
];

/// Focus field in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationScopeField {
    Scope,
    CaptureGroupType,
    CaptureGroupValue,
    Foreground,
    Background,
    Modifiers,
    TargetColumns,
    Buttons,
}

/// Type of capture group specification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureGroupType {
    /// Use a numbered group (0 = entire match)
    Number,
    /// Use a named group
    Name,
}

/// Dialog mode
#[derive(Debug)]
pub enum ApplicationScopeEditorMode {
    Editing,
    ForegroundColorPicker(Box<ColorPickerDialog>),
    BackgroundColorPicker(Box<ColorPickerDialog>),
}

/// ApplicationScopeEditorDialog: UI for editing a StyleApplication
#[derive(Debug)]
pub struct ApplicationScopeEditorDialog {
    /// The scope being edited
    pub scope: ApplicationScope,
    /// Foreground color
    pub fg: Option<Color>,
    /// Background color
    pub bg: Option<Color>,
    /// Selected modifiers
    pub modifiers: Vec<Modifier>,
    /// Target columns (comma-separated)
    pub target_columns: String,
    
    // Capture group settings (for RegexGroup scope)
    /// Type of capture group (number or name)
    pub capture_group_type: CaptureGroupType,
    /// Capture group value (number as string or name)
    pub capture_group_value: String,
    /// Cursor position within capture_group_value field
    pub capture_group_cursor: usize,
    
    // UI state
    pub focus_field: ApplicationScopeField,
    pub selected_modifier_index: usize,
    pub selected_button: usize,
    pub mode: ApplicationScopeEditorMode,
    pub show_instructions: bool,
    pub config: Config,
    pub cursor_position: usize,
}

impl ApplicationScopeEditorDialog {
    /// Create a new ApplicationScopeEditorDialog
    pub fn new(app: StyleApplication) -> Self {
        let target_columns = app.target_columns
            .map(|v| v.join(", "))
            .unwrap_or_default();
        
        // Extract capture group settings from RegexGroup scope
        let (capture_group_type, capture_group_value) = match &app.scope {
            ApplicationScope::RegexGroup(capture) => match capture {
                GrepCapture::Group(n) => (CaptureGroupType::Number, n.to_string()),
                GrepCapture::Name(name) => (CaptureGroupType::Name, name.clone()),
            },
            _ => (CaptureGroupType::Number, "0".to_string()),
        };
        
        Self {
            scope: app.scope,
            fg: app.style.fg,
            bg: app.style.bg,
            modifiers: app.style.modifiers.unwrap_or_default(),
            target_columns,
            capture_group_type,
            capture_group_value,
            capture_group_cursor: 0,
            focus_field: ApplicationScopeField::Scope,
            selected_modifier_index: 0,
            selected_button: 0,
            mode: ApplicationScopeEditorMode::Editing,
            show_instructions: true,
            config: Config::default(),
            cursor_position: 0,
        }
    }

    /// Create a new ApplicationScopeEditorDialog with defaults
    pub fn new_default() -> Self {
        Self::new(StyleApplication::default())
    }

    /// Build the resulting StyleApplication
    pub fn build_application_scope(&self) -> StyleApplication {
        let target_columns = if self.target_columns.trim().is_empty() {
            None
        } else {
            Some(self.target_columns.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        };
        
        // Build the scope with capture group settings for RegexGroup
        let scope = match &self.scope {
            ApplicationScope::RegexGroup(_) => {
                let capture = match self.capture_group_type {
                    CaptureGroupType::Number => {
                        let group_num = self.capture_group_value.parse::<usize>().unwrap_or(0);
                        GrepCapture::Group(group_num)
                    }
                    CaptureGroupType::Name => {
                        GrepCapture::Name(self.capture_group_value.clone())
                    }
                };
                ApplicationScope::RegexGroup(capture)
            }
            other => other.clone(),
        };
        
        StyleApplication {
            scope,
            style: MatchedStyle {
                fg: self.fg,
                bg: self.bg,
                modifiers: if self.modifiers.is_empty() { None } else { Some(self.modifiers.clone()) },
            },
            target_columns,
        }
    }

    /// Toggle a modifier
    fn toggle_modifier(&mut self, modifier: Modifier) {
        if let Some(pos) = self.modifiers.iter().position(|m| *m == modifier) {
            self.modifiers.remove(pos);
        } else {
            self.modifiers.push(modifier);
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        let field_hint = match self.focus_field {
            ApplicationScopeField::Scope => "Space: Toggle scope (Row/Cell/RegexGroup)",
            ApplicationScopeField::CaptureGroupType => "Space: Toggle (Number/Name)",
            ApplicationScopeField::CaptureGroupValue => {
                match self.capture_group_type {
                    CaptureGroupType::Number => "Type group number (0 = entire match)",
                    CaptureGroupType::Name => "Type capture group name",
                }
            }
            ApplicationScopeField::Foreground => "Enter/F: Pick color, Del: Clear",
            ApplicationScopeField::Background => "Enter/B: Pick color, Del: Clear",
            ApplicationScopeField::Modifiers => "Space: Toggle modifier, ←/→: Select",
            ApplicationScopeField::TargetColumns => "Type column patterns",
            ApplicationScopeField::Buttons => "Enter: Confirm selection",
        };
        
        format!("{}", field_hint)
    }

    /// Helper to render a text field with block cursor
    fn render_text_field(&self, buf: &mut Buffer, x: u16, y: u16, text: &str, placeholder: &str, is_focused: bool) {
        let display_text = if text.is_empty() { placeholder } else { text };
        let text_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };
        
        // Draw the text
        buf.set_string(x, y, display_text, text_style);
        
        // If focused and not showing placeholder, draw block cursor
        if is_focused && !text.is_empty() {
            let cursor_x = x + self.cursor_position as u16;
            let char_at_cursor = text.chars().nth(self.cursor_position).unwrap_or(' ');
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(cursor_x, y, char_at_cursor.to_string(), cursor_style);
        } else if is_focused && text.is_empty() {
            // Show cursor at start for empty field
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(x, y, " ", cursor_style);
        }
    }
    
    /// Helper to render the capture group value field with its own cursor
    fn render_capture_group_field(&self, buf: &mut Buffer, x: u16, y: u16, text: &str, placeholder: &str, is_focused: bool) {
        let display_text = if text.is_empty() { placeholder } else { text };
        let text_style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };
        
        // Draw the text
        buf.set_string(x, y, display_text, text_style);
        
        // If focused and not showing placeholder, draw block cursor
        if is_focused && !text.is_empty() {
            let cursor_x = x + self.capture_group_cursor as u16;
            let char_at_cursor = text.chars().nth(self.capture_group_cursor).unwrap_or(' ');
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(cursor_x, y, char_at_cursor.to_string(), cursor_style);
        } else if is_focused && text.is_empty() {
            // Show cursor at start for empty field
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(x, y, " ", cursor_style);
        }
    }
    
    /// Helper to render a label
    fn render_label(&self, buf: &mut Buffer, x: u16, y: u16, label: &str, field: ApplicationScopeField) {
        let style = if self.focus_field == field {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(x, y, label, style);
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in color picker mode, render that instead
        match &self.mode {
            ApplicationScopeEditorMode::ForegroundColorPicker(picker) => {
                picker.render(area, buf);
                return;
            }
            ApplicationScopeEditorMode::BackgroundColorPicker(picker) => {
                picker.render(area, buf);
                return;
            }
            ApplicationScopeEditorMode::Editing => {}
        }

        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Application Editor")
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

        let block = Block::default()
            .title("Edit Style")
            .borders(Borders::ALL);
        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let start_x = inner.x;
        let label_width: u16 = 16;
        let value_x = start_x + label_width;
        let mut y = inner.y;

        // Scope field (toggle)
        self.render_label(buf, start_x, y, "Scope:", ApplicationScopeField::Scope);
        let scope_indicator = if self.focus_field == ApplicationScopeField::Scope { "◀ " } else { "  " };
        let scope_display = format!("{}{}", scope_indicator, self.scope.display_name());
        let scope_style = if self.focus_field == ApplicationScopeField::Scope {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };
        buf.set_string(value_x, y, &scope_display, scope_style);
        if self.focus_field == ApplicationScopeField::Scope {
            buf.set_string(value_x + scope_display.len() as u16, y, " ▶", Style::default().fg(Color::Yellow));
        }
        y += 1;

        // Capture Group fields (only visible when scope is RegexGroup)
        if matches!(self.scope, ApplicationScope::RegexGroup(_)) {
            // Capture Group Type (toggle)
            self.render_label(buf, start_x + 2, y, "Group Type:", ApplicationScopeField::CaptureGroupType);
            let type_indicator = if self.focus_field == ApplicationScopeField::CaptureGroupType { "◀ " } else { "  " };
            let type_display = match self.capture_group_type {
                CaptureGroupType::Number => format!("{}Number", type_indicator),
                CaptureGroupType::Name => format!("{}Name", type_indicator),
            };
            let type_style = if self.focus_field == ApplicationScopeField::CaptureGroupType {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::Cyan)
            };
            buf.set_string(value_x, y, &type_display, type_style);
            if self.focus_field == ApplicationScopeField::CaptureGroupType {
                buf.set_string(value_x + type_display.len() as u16, y, " ▶", Style::default().fg(Color::Yellow));
            }
            y += 1;

            // Capture Group Value (text input)
            let value_label = match self.capture_group_type {
                CaptureGroupType::Number => "Group #:",
                CaptureGroupType::Name => "Group Name:",
            };
            self.render_label(buf, start_x + 2, y, value_label, ApplicationScopeField::CaptureGroupValue);
            let placeholder = match self.capture_group_type {
                CaptureGroupType::Number => "0",
                CaptureGroupType::Name => "(group name)",
            };
            self.render_capture_group_field(
                buf, value_x, y,
                &self.capture_group_value, placeholder,
                self.focus_field == ApplicationScopeField::CaptureGroupValue
            );
            y += 1;
        }
        y += 1;

        // Foreground color
        self.render_label(buf, start_x, y, "Foreground:", ApplicationScopeField::Foreground);
        let fg_text = self.fg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
        if let Some(c) = self.fg {
            // Show color sample
            buf.set_string(value_x, y, "██", Style::default().fg(c));
            buf.set_string(value_x + 3, y, &fg_text, Style::default().fg(Color::Cyan));
        } else {
            buf.set_string(value_x, y, &fg_text, Style::default().fg(Color::DarkGray));
        }
        if self.focus_field == ApplicationScopeField::Foreground {
            buf.set_string(value_x + fg_text.len() as u16 + 4, y, "[F: pick, Del: clear]", Style::default().fg(Color::DarkGray));
        }
        y += 1;

        // Background color
        self.render_label(buf, start_x, y, "Background:", ApplicationScopeField::Background);
        let bg_text = self.bg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
        if let Some(c) = self.bg {
            // Show color sample
            buf.set_string(value_x, y, "██", Style::default().bg(c));
            buf.set_string(value_x + 3, y, &bg_text, Style::default().fg(Color::Cyan));
        } else {
            buf.set_string(value_x, y, &bg_text, Style::default().fg(Color::DarkGray));
        }
        if self.focus_field == ApplicationScopeField::Background {
            buf.set_string(value_x + bg_text.len() as u16 + 4, y, "[B: pick, Del: clear]", Style::default().fg(Color::DarkGray));
        }
        y += 2;

        // Modifiers
        self.render_label(buf, start_x, y, "Modifiers:", ApplicationScopeField::Modifiers);
        y += 1;
        
        let mods_per_row = 3;
        for (i, (modifier, name)) in AVAILABLE_MODIFIERS.iter().enumerate() {
            let is_active = self.modifiers.contains(modifier);
            let is_selected = self.focus_field == ApplicationScopeField::Modifiers && i == self.selected_modifier_index;
            
            let col = i % mods_per_row;
            let x = start_x + 2 + (col as u16 * 16);
            
            if col == 0 && i > 0 {
                y += 1;
            }
            
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else if is_active {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            
            let marker = if is_active { "[✓] " } else { "[ ] " };
            buf.set_string(x, y, format!("{}{}", marker, name), style);
        }
        y += 2;

        // Target Columns (text input with cursor)
        self.render_label(buf, start_x, y, "Target Columns:", ApplicationScopeField::TargetColumns);
        self.render_text_field(
            buf, value_x, y,
            &self.target_columns, "(all matched)",
            self.focus_field == ApplicationScopeField::TargetColumns
        );
        y += 2;

        // Style Preview
        buf.set_string(start_x, y, "Preview:", Style::default().fg(Color::Gray));
        let preview_style = self.build_application_scope().style.to_ratatui_style();
        buf.set_string(value_x, y, "Sample Styled Text", preview_style);
        y += 2;

        // Buttons
        let apply_style = if self.focus_field == ApplicationScopeField::Buttons && self.selected_button == 0 {
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let cancel_style = if self.focus_field == ApplicationScopeField::Buttons && self.selected_button == 1 {
            Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        
        buf.set_string(start_x, y, " Apply ", apply_style);
        buf.set_string(start_x + 10, y, " Cancel ", cancel_style);

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

    /// Handle a key event
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Handle color picker modes
        match &mut self.mode {
            ApplicationScopeEditorMode::ForegroundColorPicker(picker) => {
                if let Some(action) = picker.handle_key_event_pub(key) {
                    match action {
                        Action::ColorPickerDialogApplied(color) => {
                            self.fg = color;
                            self.mode = ApplicationScopeEditorMode::Editing;
                        }
                        Action::CloseColorPickerDialog => {
                            self.mode = ApplicationScopeEditorMode::Editing;
                        }
                        _ => {}
                    }
                }
                return None;
            }
            ApplicationScopeEditorMode::BackgroundColorPicker(picker) => {
                if let Some(action) = picker.handle_key_event_pub(key) {
                    match action {
                        Action::ColorPickerDialogApplied(color) => {
                            self.bg = color;
                            self.mode = ApplicationScopeEditorMode::Editing;
                        }
                        Action::CloseColorPickerDialog => {
                            self.mode = ApplicationScopeEditorMode::Editing;
                        }
                        _ => {}
                    }
                }
                return None;
            }
            ApplicationScopeEditorMode::Editing => {}
        }

        // Check ApplicationScopeEditorDialog specific actions
        if let Some(dialog_action) = self.config.action_for_key(Mode::ApplicationScopeEditorDialog, key) {
            match dialog_action {
                Action::OpenForegroundColorPicker => {
                    let mut picker = ColorPickerDialog::new(self.fg);
                    let _ = picker.register_config_handler(self.config.clone());
                    self.mode = ApplicationScopeEditorMode::ForegroundColorPicker(Box::new(picker));
                    return None;
                }
                Action::OpenBackgroundColorPicker => {
                    let mut picker = ColorPickerDialog::new(self.bg);
                    let _ = picker.register_config_handler(self.config.clone());
                    self.mode = ApplicationScopeEditorMode::BackgroundColorPicker(Box::new(picker));
                    return None;
                }
                Action::ClearForeground => {
                    self.fg = None;
                    return None;
                }
                Action::ClearBackground => {
                    self.bg = None;
                    return None;
                }
                _ => {}
            }
        }

        // Space toggles
        if key.code == KeyCode::Char(' ') {
            match self.focus_field {
                ApplicationScopeField::Scope => {
                    self.scope = self.scope.next();
                    return None;
                }
                ApplicationScopeField::CaptureGroupType => {
                    // Toggle between Number and Name
                    self.capture_group_type = match self.capture_group_type {
                        CaptureGroupType::Number => CaptureGroupType::Name,
                        CaptureGroupType::Name => CaptureGroupType::Number,
                    };
                    // Reset value when switching types
                    self.capture_group_value = match self.capture_group_type {
                        CaptureGroupType::Number => "0".to_string(),
                        CaptureGroupType::Name => String::new(),
                    };
                    self.capture_group_cursor = self.capture_group_value.chars().count();
                    return None;
                }
                ApplicationScopeField::Modifiers => {
                    if let Some((modifier, _)) = AVAILABLE_MODIFIERS.get(self.selected_modifier_index) {
                        self.toggle_modifier(*modifier);
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Check Global actions
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseApplicationScopeEditorDialog);
                }
                Action::Enter => {
                    match self.focus_field {
                        ApplicationScopeField::Foreground => {
                            let mut picker = ColorPickerDialog::new(self.fg);
                            let _ = picker.register_config_handler(self.config.clone());
                            self.mode = ApplicationScopeEditorMode::ForegroundColorPicker(Box::new(picker));
                        }
                        ApplicationScopeField::Background => {
                            let mut picker = ColorPickerDialog::new(self.bg);
                            let _ = picker.register_config_handler(self.config.clone());
                            self.mode = ApplicationScopeEditorMode::BackgroundColorPicker(Box::new(picker));
                        }
                        ApplicationScopeField::Buttons => {
                            if self.selected_button == 0 {
                                let app = self.build_application_scope();
                                return Some(Action::ApplicationScopeEditorDialogApplied(app));
                            } else {
                                return Some(Action::CloseApplicationScopeEditorDialog);
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Up => {
                    let is_regex_group = matches!(self.scope, ApplicationScope::RegexGroup(_));
                    self.focus_field = match self.focus_field {
                        ApplicationScopeField::Scope => ApplicationScopeField::Buttons,
                        ApplicationScopeField::CaptureGroupType => ApplicationScopeField::Scope,
                        ApplicationScopeField::CaptureGroupValue => ApplicationScopeField::CaptureGroupType,
                        ApplicationScopeField::Foreground => {
                            if is_regex_group {
                                ApplicationScopeField::CaptureGroupValue
                            } else {
                                ApplicationScopeField::Scope
                            }
                        }
                        ApplicationScopeField::Background => ApplicationScopeField::Foreground,
                        ApplicationScopeField::Modifiers => ApplicationScopeField::Background,
                        ApplicationScopeField::TargetColumns => ApplicationScopeField::Modifiers,
                        ApplicationScopeField::Buttons => ApplicationScopeField::TargetColumns,
                    };
                    return None;
                }
                Action::Down => {
                    let is_regex_group = matches!(self.scope, ApplicationScope::RegexGroup(_));
                    self.focus_field = match self.focus_field {
                        ApplicationScopeField::Scope => {
                            if is_regex_group {
                                ApplicationScopeField::CaptureGroupType
                            } else {
                                ApplicationScopeField::Foreground
                            }
                        }
                        ApplicationScopeField::CaptureGroupType => ApplicationScopeField::CaptureGroupValue,
                        ApplicationScopeField::CaptureGroupValue => ApplicationScopeField::Foreground,
                        ApplicationScopeField::Foreground => ApplicationScopeField::Background,
                        ApplicationScopeField::Background => ApplicationScopeField::Modifiers,
                        ApplicationScopeField::Modifiers => ApplicationScopeField::TargetColumns,
                        ApplicationScopeField::TargetColumns => ApplicationScopeField::Buttons,
                        ApplicationScopeField::Buttons => ApplicationScopeField::Scope,
                    };
                    return None;
                }
                Action::Left => {
                    match self.focus_field {
                        ApplicationScopeField::Modifiers => {
                            if self.selected_modifier_index > 0 {
                                self.selected_modifier_index -= 1;
                            }
                        }
                        ApplicationScopeField::Buttons => {
                            self.selected_button = 0;
                        }
                        ApplicationScopeField::TargetColumns => {
                            if self.cursor_position > 0 {
                                self.cursor_position -= 1;
                            }
                        }
                        ApplicationScopeField::CaptureGroupValue => {
                            if self.capture_group_cursor > 0 {
                                self.capture_group_cursor -= 1;
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Right => {
                    match self.focus_field {
                        ApplicationScopeField::Modifiers => {
                            if self.selected_modifier_index < AVAILABLE_MODIFIERS.len() - 1 {
                                self.selected_modifier_index += 1;
                            }
                        }
                        ApplicationScopeField::Buttons => {
                            self.selected_button = 1;
                        }
                        ApplicationScopeField::TargetColumns => {
                            if self.cursor_position < self.target_columns.chars().count() {
                                self.cursor_position += 1;
                            }
                        }
                        ApplicationScopeField::CaptureGroupValue => {
                            if self.capture_group_cursor < self.capture_group_value.chars().count() {
                                self.capture_group_cursor += 1;
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Backspace => {
                    if self.focus_field == ApplicationScopeField::TargetColumns && self.cursor_position > 0 {
                        let chars: Vec<char> = self.target_columns.chars().collect();
                        self.target_columns = chars[..self.cursor_position - 1].iter()
                            .chain(chars[self.cursor_position..].iter())
                            .collect();
                        self.cursor_position -= 1;
                    } else if self.focus_field == ApplicationScopeField::CaptureGroupValue && self.capture_group_cursor > 0 {
                        let chars: Vec<char> = self.capture_group_value.chars().collect();
                        self.capture_group_value = chars[..self.capture_group_cursor - 1].iter()
                            .chain(chars[self.capture_group_cursor..].iter())
                            .collect();
                        self.capture_group_cursor -= 1;
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

        // Delete key handling
        if key.code == KeyCode::Delete {
            match self.focus_field {
                ApplicationScopeField::Foreground => {
                    self.fg = None;
                    return None;
                }
                ApplicationScopeField::Background => {
                    self.bg = None;
                    return None;
                }
                ApplicationScopeField::TargetColumns => {
                    let chars: Vec<char> = self.target_columns.chars().collect();
                    if self.cursor_position < chars.len() {
                        self.target_columns = chars[..self.cursor_position].iter()
                            .chain(chars[self.cursor_position + 1..].iter())
                            .collect();
                    }
                    return None;
                }
                ApplicationScopeField::CaptureGroupValue => {
                    let chars: Vec<char> = self.capture_group_value.chars().collect();
                    if self.capture_group_cursor < chars.len() {
                        self.capture_group_value = chars[..self.capture_group_cursor].iter()
                            .chain(chars[self.capture_group_cursor + 1..].iter())
                            .collect();
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Text input for target columns
        if self.focus_field == ApplicationScopeField::TargetColumns {
            if let KeyCode::Char(c) = key.code {
                let chars: Vec<char> = self.target_columns.chars().collect();
                let before: String = chars[..self.cursor_position].iter().collect();
                let after: String = chars[self.cursor_position..].iter().collect();
                self.target_columns = format!("{}{}{}", before, c, after);
                self.cursor_position += 1;
                return None;
            }
        }
        
        // Text input for capture group value
        if self.focus_field == ApplicationScopeField::CaptureGroupValue {
            if let KeyCode::Char(c) = key.code {
                // For number type, only allow digits
                if self.capture_group_type == CaptureGroupType::Number && !c.is_ascii_digit() {
                    return None;
                }
                let chars: Vec<char> = self.capture_group_value.chars().collect();
                let before: String = chars[..self.capture_group_cursor].iter().collect();
                let after: String = chars[self.capture_group_cursor..].iter().collect();
                self.capture_group_value = format!("{}{}{}", before, c, after);
                self.capture_group_cursor += 1;
                return None;
            }
        }

        // Ctrl+S to save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            let app = self.build_application_scope();
            return Some(Action::ApplicationScopeEditorDialogApplied(app));
        }

        None
    }
}

impl Component for ApplicationScopeEditorDialog {
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
