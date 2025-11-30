//! ApplicationScopeEditorDialog: Dialog for editing ApplicationScope (scope + style attributes)
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{ApplicationScope, MatchedStyle, ScopeEnum};
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
    Foreground,
    Background,
    Modifiers,
    Buttons,
}

/// Dialog mode
#[derive(Debug)]
pub enum ApplicationScopeEditorMode {
    Editing,
    ForegroundColorPicker(Box<ColorPickerDialog>),
    BackgroundColorPicker(Box<ColorPickerDialog>),
}

/// ApplicationScopeEditorDialog: UI for editing an ApplicationScope
#[derive(Debug)]
pub struct ApplicationScopeEditorDialog {
    /// The scope being edited
    pub scope: ScopeEnum,
    /// Foreground color
    pub fg: Option<Color>,
    /// Background color
    pub bg: Option<Color>,
    /// Selected modifiers
    pub modifiers: Vec<Modifier>,
    /// Current focus field
    pub focus_field: ApplicationScopeField,
    /// Previous focus field (for returning from buttons with Left)
    pub previous_focus_field: ApplicationScopeField,
    /// Previous modifier index (for returning from buttons with Left)
    pub previous_modifier_index: usize,
    /// Selected modifier index (when in Modifiers field)
    pub selected_modifier_index: usize,
    /// Selected button index (0 = Apply, 1 = Cancel)
    pub selected_button: usize,
    /// Dialog mode
    pub mode: ApplicationScopeEditorMode,
    /// Show instructions
    pub show_instructions: bool,
    /// Config
    pub config: Config,
}

impl ApplicationScopeEditorDialog {
    /// Create a new ApplicationScopeEditorDialog
    pub fn new(app_scope: ApplicationScope) -> Self {
        Self {
            scope: app_scope.scope,
            fg: app_scope.style.fg,
            bg: app_scope.style.bg,
            modifiers: app_scope.style.modifiers.unwrap_or_default(),
            focus_field: ApplicationScopeField::Scope,
            previous_focus_field: ApplicationScopeField::Scope,
            previous_modifier_index: 0,
            selected_modifier_index: 0,
            selected_button: 0,
            mode: ApplicationScopeEditorMode::Editing,
            show_instructions: true,
            config: Config::default(),
        }
    }

    /// Create a new ApplicationScopeEditorDialog with defaults
    pub fn new_default() -> Self {
        Self::new(ApplicationScope {
            scope: ScopeEnum::Row,
            style: MatchedStyle {
                fg: None,
                bg: None,
                modifiers: None,
            },
        })
    }

    /// Build the resulting ApplicationScope
    pub fn build_application_scope(&self) -> ApplicationScope {
        ApplicationScope {
            scope: self.scope,
            style: MatchedStyle {
                fg: self.fg,
                bg: self.bg,
                modifiers: if self.modifiers.is_empty() {
                    None
                } else {
                    Some(self.modifiers.clone())
                },
            },
        }
    }

    /// Check if a modifier is currently selected
    fn is_modifier_selected(&self, modifier: Modifier) -> bool {
        self.modifiers.contains(&modifier)
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
        match &self.mode {
            ApplicationScopeEditorMode::Editing => {
                let field_hint = match self.focus_field {
                    ApplicationScopeField::Scope => "Space: Toggle Row/Cell  →: Buttons",
                    ApplicationScopeField::Foreground => "Enter: Color Picker  Del: Clear  →: Buttons",
                    ApplicationScopeField::Background => "Enter: Color Picker  Del: Clear  →: Buttons",
                    ApplicationScopeField::Modifiers => "Space: Toggle Modifier  →: Buttons",
                    ApplicationScopeField::Buttons => "Enter: Activate  ↑/↓: Switch  ←: Back",
                };
                format!(
                    "{}  {}",
                    field_hint,
                    self.config.actions_to_instructions(&[
                        (Mode::Global, Action::Up),
                        (Mode::Global, Action::Down),
                        (Mode::Global, Action::Escape),
                    ])
                )
            }
            ApplicationScopeEditorMode::ForegroundColorPicker(_) |
            ApplicationScopeEditorMode::BackgroundColorPicker(_) => {
                "Selecting color...".to_string()
            }
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in color picker mode, render that instead
        match &self.mode {
            ApplicationScopeEditorMode::ForegroundColorPicker(picker) |
            ApplicationScopeEditorMode::BackgroundColorPicker(picker) => {
                picker.render(area, buf);
                return;
            }
            ApplicationScopeEditorMode::Editing => {}
        }

        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Application Scope Editor")
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
            .title("Edit Scope & Style")
            .borders(Borders::ALL);
        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let start_x = inner.x;
        let mut y = inner.y;

        let highlight = |field: ApplicationScopeField| -> Style {
            if self.focus_field == field {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        // Scope field
        let scope_label = format!("Scope: {} (Row applies to entire row, Cell applies only to matching cell)", 
            match self.scope {
                ScopeEnum::Row => "[Row]  Cell",
                ScopeEnum::Cell => " Row  [Cell]",
            }
        );
        buf.set_string(start_x, y, &scope_label, highlight(ApplicationScopeField::Scope));
        y += 2;

        // Foreground field
        let fg_value = self.fg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
        let fg_label = format!("Foreground: {}", fg_value);
        buf.set_string(start_x, y, &fg_label, highlight(ApplicationScopeField::Foreground));
        // Show color preview swatch
        if let Some(color) = self.fg {
            let swatch_x = start_x + fg_label.len() as u16 + 2;
            buf.set_string(swatch_x, y, "████", Style::default().fg(color));
        }
        y += 2;

        // Background field
        let bg_value = self.bg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
        let bg_label = format!("Background: {}", bg_value);
        buf.set_string(start_x, y, &bg_label, highlight(ApplicationScopeField::Background));
        // Show color preview swatch
        if let Some(color) = self.bg {
            let swatch_x = start_x + bg_label.len() as u16 + 2;
            buf.set_string(swatch_x, y, "████", Style::default().bg(color));
        }
        y += 2;

        // Modifiers field
        let modifiers_label = "Modifiers:";
        buf.set_string(start_x, y, modifiers_label, highlight(ApplicationScopeField::Modifiers));
        y += 1;

        // Render modifier list
        let is_modifiers_focused = self.focus_field == ApplicationScopeField::Modifiers;
        for (i, (modifier, name)) in AVAILABLE_MODIFIERS.iter().enumerate() {
            let is_selected = self.is_modifier_selected(*modifier);
            let checkbox = if is_selected { "[✓]" } else { "[ ]" };
            
            let style = if is_modifiers_focused && i == self.selected_modifier_index {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            let modifier_text = format!("{} {}", checkbox, name);
            buf.set_string(start_x + 2, y, &modifier_text, style);
            y += 1;
        }

        // Style preview section
        y += 1;
        buf.set_string(start_x, y, "Preview:", Style::default().fg(Color::Gray));
        y += 1;

        // Build preview style
        let mut preview_style = Style::default();
        if let Some(fg) = self.fg {
            preview_style = preview_style.fg(fg);
        }
        if let Some(bg) = self.bg {
            preview_style = preview_style.bg(bg);
        }
        for m in &self.modifiers {
            preview_style = preview_style.add_modifier(*m);
        }

        let preview_text = "Sample Text Preview";
        buf.set_string(start_x, y, preview_text, preview_style);

        // Render Apply and Cancel buttons
        let buttons = ["[Apply]", "[Cancel]"];
        let total_len: u16 = buttons.iter().map(|b| b.len() as u16 + 1).sum();
        let bx = inner.x + inner.width.saturating_sub(total_len + 1);
        let by = inner.y + inner.height.saturating_sub(1);
        let mut x = bx;
        let is_buttons_focused = self.focus_field == ApplicationScopeField::Buttons;
        for (idx, b) in buttons.iter().enumerate() {
            let style = if is_buttons_focused && self.selected_button == idx {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(x, by, *b, style);
            x += b.len() as u16 + 1;
        }

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

        // Handle color picker modes first
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

        // Check Global actions first
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
                            return None;
                        }
                        ApplicationScopeField::Background => {
                            let mut picker = ColorPickerDialog::new(self.bg);
                            let _ = picker.register_config_handler(self.config.clone());
                            self.mode = ApplicationScopeEditorMode::BackgroundColorPicker(Box::new(picker));
                            return None;
                        }
                        ApplicationScopeField::Scope | ApplicationScopeField::Modifiers => {
                            // Enter does NOT toggle - use Space instead
                            return None;
                        }
                        ApplicationScopeField::Buttons => {
                            // Apply or Cancel based on selected button
                            if self.selected_button == 0 {
                                let app_scope = self.build_application_scope();
                                return Some(Action::ApplicationScopeEditorDialogApplied(app_scope));
                            } else {
                                return Some(Action::CloseApplicationScopeEditorDialog);
                            }
                        }
                    }
                }
                Action::Up => {
                    match self.focus_field {
                        ApplicationScopeField::Scope => {
                            self.focus_field = ApplicationScopeField::Buttons;
                            self.selected_button = 0;
                        }
                        ApplicationScopeField::Foreground => {
                            self.focus_field = ApplicationScopeField::Scope;
                        }
                        ApplicationScopeField::Background => {
                            self.focus_field = ApplicationScopeField::Foreground;
                        }
                        ApplicationScopeField::Modifiers => {
                            if self.selected_modifier_index > 0 {
                                self.selected_modifier_index -= 1;
                            } else {
                                self.focus_field = ApplicationScopeField::Background;
                            }
                        }
                        ApplicationScopeField::Buttons => {
                            // Switch between Apply and Cancel
                            if self.selected_button < 1 {
                                self.selected_button = 1;
                            } else {
                                self.selected_button = 0;
                            }
                        }
                    }
                    return None;
                }
                Action::Down => {
                    match self.focus_field {
                        ApplicationScopeField::Scope => {
                            self.focus_field = ApplicationScopeField::Foreground;
                        }
                        ApplicationScopeField::Foreground => {
                            self.focus_field = ApplicationScopeField::Background;
                        }
                        ApplicationScopeField::Background => {
                            self.focus_field = ApplicationScopeField::Modifiers;
                            self.selected_modifier_index = 0;
                        }
                        ApplicationScopeField::Modifiers => {
                            if self.selected_modifier_index < AVAILABLE_MODIFIERS.len() - 1 {
                                self.selected_modifier_index += 1;
                            } else {
                                self.focus_field = ApplicationScopeField::Buttons;
                                self.selected_button = 0;
                            }
                        }
                        ApplicationScopeField::Buttons => {
                            // Switch between Apply and Cancel
                            if self.selected_button < 1 {
                                self.selected_button = 1;
                            } else {
                                self.selected_button = 0;
                            }
                        }
                    }
                    return None;
                }
                Action::Left => {
                    if self.focus_field == ApplicationScopeField::Buttons {
                        // Move back to the previously selected option
                        self.focus_field = self.previous_focus_field;
                        self.selected_modifier_index = self.previous_modifier_index;
                    }
                    return None;
                }
                Action::Right => {
                    if self.focus_field != ApplicationScopeField::Buttons {
                        // Save current position before moving to buttons
                        self.previous_focus_field = self.focus_field;
                        self.previous_modifier_index = self.selected_modifier_index;
                        // Move to [Apply] button
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
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

        // Check ApplicationScopeEditorDialog specific actions
        if let Some(dialog_action) = self.config.action_for_key(Mode::ApplicationScopeEditorDialog, key) {
            match dialog_action {
                Action::ToggleScope => {
                    self.scope = match self.scope {
                        ScopeEnum::Row => ScopeEnum::Cell,
                        ScopeEnum::Cell => ScopeEnum::Row,
                    };
                    return None;
                }
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
                Action::ToggleModifier => {
                    if self.focus_field == ApplicationScopeField::Modifiers {
                        if let Some((modifier, _)) = AVAILABLE_MODIFIERS.get(self.selected_modifier_index) {
                            self.toggle_modifier(*modifier);
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Handle space key for toggling
        if key.code == KeyCode::Char(' ') {
            match self.focus_field {
                ApplicationScopeField::Scope => {
                    self.scope = match self.scope {
                        ScopeEnum::Row => ScopeEnum::Cell,
                        ScopeEnum::Cell => ScopeEnum::Row,
                    };
                }
                ApplicationScopeField::Modifiers => {
                    if let Some((modifier, _)) = AVAILABLE_MODIFIERS.get(self.selected_modifier_index) {
                        self.toggle_modifier(*modifier);
                    }
                }
                _ => {}
            }
            return None;
        }

        // Handle delete key to clear colors
        if key.code == KeyCode::Delete {
            match self.focus_field {
                ApplicationScopeField::Foreground => {
                    self.fg = None;
                }
                ApplicationScopeField::Background => {
                    self.bg = None;
                }
                _ => {}
            }
            return None;
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

