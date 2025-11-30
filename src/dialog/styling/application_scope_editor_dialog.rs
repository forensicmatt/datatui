//! ApplicationScopeEditorDialog: Dialog for editing ApplicationScope (scope + style attributes)
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{
    ApplicationScope, MatchedStyle, ScopeEnum, DynamicStyle, 
    GradientStyle, GradientScale, CategoricalStyle
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

/// Style type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleTypeOption {
    #[default]
    Static,
    Gradient,
    Categorical,
}

impl StyleTypeOption {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::Gradient => "Gradient",
            Self::Categorical => "Categorical",
        }
    }
    
    pub fn next(&self) -> Self {
        match self {
            Self::Static => Self::Gradient,
            Self::Gradient => Self::Categorical,
            Self::Categorical => Self::Static,
        }
    }
    
    pub fn prev(&self) -> Self {
        match self {
            Self::Static => Self::Categorical,
            Self::Gradient => Self::Static,
            Self::Categorical => Self::Gradient,
        }
    }
}

/// Focus field in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationScopeField {
    Scope,
    StyleType,
    // Static style fields
    Foreground,
    Background,
    Modifiers,
    // Gradient style fields
    GradientSourceColumn,
    GradientMinBg,
    GradientMaxBg,
    GradientScale,
    // Categorical style fields
    CategoricalSourceColumn,
    CategoricalApplyTo,
    // Common
    Buttons,
}

/// Dialog mode
#[derive(Debug)]
pub enum ApplicationScopeEditorMode {
    Editing,
    ForegroundColorPicker(Box<ColorPickerDialog>),
    BackgroundColorPicker(Box<ColorPickerDialog>),
    GradientMinColorPicker(Box<ColorPickerDialog>),
    GradientMaxColorPicker(Box<ColorPickerDialog>),
}

/// ApplicationScopeEditorDialog: UI for editing an ApplicationScope
#[derive(Debug)]
pub struct ApplicationScopeEditorDialog {
    /// The scope being edited
    pub scope: ScopeEnum,
    /// Style type (Static, Gradient, Categorical)
    pub style_type: StyleTypeOption,
    
    // === Static style fields ===
    /// Foreground color
    pub fg: Option<Color>,
    /// Background color
    pub bg: Option<Color>,
    /// Selected modifiers
    pub modifiers: Vec<Modifier>,
    
    // === Gradient style fields ===
    /// Gradient source column
    pub gradient_source_column: String,
    /// Gradient min background color
    pub gradient_min_bg: Option<Color>,
    /// Gradient max background color
    pub gradient_max_bg: Option<Color>,
    /// Gradient scale type
    pub gradient_scale: GradientScale,
    
    // === Categorical style fields ===
    /// Categorical source column
    pub categorical_source_column: String,
    /// Apply categorical colors to foreground (true) or background (false)
    pub categorical_apply_to_fg: bool,
    
    // === UI state ===
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
    /// Text input buffer for source column editing
    pub text_input_buffer: String,
    /// Whether we're in text input mode
    pub text_input_active: bool,
}

impl ApplicationScopeEditorDialog {
    /// Create a new ApplicationScopeEditorDialog
    pub fn new(app_scope: ApplicationScope) -> Self {
        // Determine style type and extract fields from dynamic_style if present
        let (style_type, gradient_source, gradient_min_bg, gradient_max_bg, gradient_scale,
             categorical_source, categorical_apply_fg) = match &app_scope.dynamic_style {
            Some(DynamicStyle::Gradient(g)) => (
                StyleTypeOption::Gradient,
                g.source_column.clone(),
                g.min_style.bg,
                g.max_style.bg,
                g.scale,
                String::new(),
                true,
            ),
            Some(DynamicStyle::Categorical(c)) => (
                StyleTypeOption::Categorical,
                String::new(),
                None,
                None,
                GradientScale::Linear,
                c.source_column.clone(),
                c.apply_to_fg,
            ),
            Some(DynamicStyle::Static(_)) | None => (
                StyleTypeOption::Static,
                String::new(),
                Some(Color::Rgb(50, 100, 200)),  // Default blue
                Some(Color::Rgb(200, 50, 50)),   // Default red
                GradientScale::Linear,
                String::new(),
                true,
            ),
        };
        
        Self {
            scope: app_scope.scope,
            style_type,
            fg: app_scope.style.fg,
            bg: app_scope.style.bg,
            modifiers: app_scope.style.modifiers.unwrap_or_default(),
            gradient_source_column: gradient_source,
            gradient_min_bg,
            gradient_max_bg,
            gradient_scale,
            categorical_source_column: categorical_source,
            categorical_apply_to_fg: categorical_apply_fg,
            focus_field: ApplicationScopeField::Scope,
            previous_focus_field: ApplicationScopeField::Scope,
            previous_modifier_index: 0,
            selected_modifier_index: 0,
            selected_button: 0,
            mode: ApplicationScopeEditorMode::Editing,
            show_instructions: true,
            config: Config::default(),
            text_input_buffer: String::new(),
            text_input_active: false,
        }
    }

    /// Create a new ApplicationScopeEditorDialog with defaults
    pub fn new_default() -> Self {
        Self::new(ApplicationScope {
            scope: ScopeEnum::Row,
            target_columns: None,
            style: MatchedStyle {
                fg: None,
                bg: None,
                modifiers: None,
            },
            dynamic_style: None,
        })
    }

    /// Build the resulting ApplicationScope
    pub fn build_application_scope(&self) -> ApplicationScope {
        let dynamic_style = match self.style_type {
            StyleTypeOption::Static => None,
            StyleTypeOption::Gradient => {
                Some(DynamicStyle::Gradient(GradientStyle {
                    source_column: if self.gradient_source_column.is_empty() {
                        "*".to_string()
                    } else {
                        self.gradient_source_column.clone()
                    },
                    min_style: MatchedStyle {
                        fg: None,
                        bg: self.gradient_min_bg,
                        modifiers: None,
                    },
                    max_style: MatchedStyle {
                        fg: Some(Color::White),
                        bg: self.gradient_max_bg,
                        modifiers: Some(vec![Modifier::BOLD]),
                    },
                    scale: self.gradient_scale,
                    bounds: None, // Auto-detect from data
                }))
            }
            StyleTypeOption::Categorical => {
                Some(DynamicStyle::Categorical(CategoricalStyle {
                    source_column: if self.categorical_source_column.is_empty() {
                        "*".to_string()
                    } else {
                        self.categorical_source_column.clone()
                    },
                    palette: vec![
                        Color::Rgb(66, 133, 244),   // Blue
                        Color::Rgb(234, 67, 53),    // Red
                        Color::Rgb(251, 188, 5),    // Yellow
                        Color::Rgb(52, 168, 83),    // Green
                        Color::Rgb(154, 160, 166),  // Gray
                        Color::Rgb(255, 112, 67),   // Deep Orange
                        Color::Rgb(156, 39, 176),   // Purple
                        Color::Rgb(0, 188, 212),    // Cyan
                    ],
                    apply_to_fg: self.categorical_apply_to_fg,
                }))
            }
        };
        
        ApplicationScope {
            scope: self.scope,
            target_columns: None, // TODO: Add UI for target_columns
            style: MatchedStyle {
                fg: self.fg,
                bg: self.bg,
                modifiers: if self.modifiers.is_empty() {
                    None
                } else {
                    Some(self.modifiers.clone())
                },
            },
            dynamic_style,
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
                let field_hint = if self.text_input_active {
                    "Type to edit  Enter: Confirm  Esc: Cancel"
                } else {
                    match self.focus_field {
                        ApplicationScopeField::Scope => "Space: Toggle Row/Cell/Header  →: Buttons",
                        ApplicationScopeField::StyleType => "Space: Toggle Static/Gradient/Categorical  →: Buttons",
                        ApplicationScopeField::Foreground => "Enter: Color Picker  Del: Clear  →: Buttons",
                        ApplicationScopeField::Background => "Enter: Color Picker  Del: Clear  →: Buttons",
                        ApplicationScopeField::Modifiers => "Space: Toggle Modifier  →: Buttons",
                        ApplicationScopeField::GradientSourceColumn => "Enter: Edit column name  →: Buttons",
                        ApplicationScopeField::GradientMinBg => "Enter: Color Picker  Del: Clear  →: Buttons",
                        ApplicationScopeField::GradientMaxBg => "Enter: Color Picker  Del: Clear  →: Buttons",
                        ApplicationScopeField::GradientScale => "Space: Toggle Linear/Log/Percentile  →: Buttons",
                        ApplicationScopeField::CategoricalSourceColumn => "Enter: Edit column name  →: Buttons",
                        ApplicationScopeField::CategoricalApplyTo => "Space: Toggle Foreground/Background  →: Buttons",
                        ApplicationScopeField::Buttons => "Enter: Activate  ↑/↓: Switch  ←: Back",
                    }
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
            ApplicationScopeEditorMode::BackgroundColorPicker(_) |
            ApplicationScopeEditorMode::GradientMinColorPicker(_) |
            ApplicationScopeEditorMode::GradientMaxColorPicker(_) => {
                "Selecting color...".to_string()
            }
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in color picker mode, render that instead
        match &self.mode {
            ApplicationScopeEditorMode::ForegroundColorPicker(picker) |
            ApplicationScopeEditorMode::BackgroundColorPicker(picker) |
            ApplicationScopeEditorMode::GradientMinColorPicker(picker) |
            ApplicationScopeEditorMode::GradientMaxColorPicker(picker) => {
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
        let scope_label = format!("Scope: {}", 
            match self.scope {
                ScopeEnum::Row => "[Row]  Cell   Header",
                ScopeEnum::Cell => " Row  [Cell]  Header",
                ScopeEnum::Header => " Row   Cell  [Header]",
            }
        );
        buf.set_string(start_x, y, &scope_label, highlight(ApplicationScopeField::Scope));
        y += 2;

        // Style Type field
        let style_type_label = format!("Style Type: {}",
            match self.style_type {
                StyleTypeOption::Static => "[Static]  Gradient   Categorical",
                StyleTypeOption::Gradient => " Static  [Gradient]  Categorical",
                StyleTypeOption::Categorical => " Static   Gradient  [Categorical]",
            }
        );
        buf.set_string(start_x, y, &style_type_label, highlight(ApplicationScopeField::StyleType));
        y += 2;

        // Render style-specific fields based on style type
        match self.style_type {
            StyleTypeOption::Static => {
                // Foreground field
                let fg_value = self.fg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
                let fg_label = format!("Foreground: {}", fg_value);
                buf.set_string(start_x, y, &fg_label, highlight(ApplicationScopeField::Foreground));
                if let Some(color) = self.fg {
                    let swatch_x = start_x + fg_label.len() as u16 + 2;
                    buf.set_string(swatch_x, y, "████", Style::default().fg(color));
                }
                y += 2;

                // Background field
                let bg_value = self.bg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
                let bg_label = format!("Background: {}", bg_value);
                buf.set_string(start_x, y, &bg_label, highlight(ApplicationScopeField::Background));
                if let Some(color) = self.bg {
                    let swatch_x = start_x + bg_label.len() as u16 + 2;
                    buf.set_string(swatch_x, y, "████", Style::default().bg(color));
                }
                y += 2;

                // Modifiers field
                let modifiers_label = "Modifiers:";
                buf.set_string(start_x, y, modifiers_label, highlight(ApplicationScopeField::Modifiers));
                y += 1;

                // Render modifier list (compact: 3 per row)
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
                buf.set_string(start_x, y, "Sample Text Preview", preview_style);
            }
            StyleTypeOption::Gradient => {
                // Source column field
                let source_display = if self.text_input_active && self.focus_field == ApplicationScopeField::GradientSourceColumn {
                    format!("{}▌", self.text_input_buffer)
                } else if self.gradient_source_column.is_empty() {
                    "*  (auto-detect)".to_string()
                } else {
                    self.gradient_source_column.clone()
                };
                let source_label = format!("Source Column: {}", source_display);
                buf.set_string(start_x, y, &source_label, highlight(ApplicationScopeField::GradientSourceColumn));
                y += 2;

                // Min color (low values)
                let min_value = self.gradient_min_bg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
                let min_label = format!("Min Color (low values): {}", min_value);
                buf.set_string(start_x, y, &min_label, highlight(ApplicationScopeField::GradientMinBg));
                if let Some(color) = self.gradient_min_bg {
                    let swatch_x = start_x + min_label.len() as u16 + 2;
                    buf.set_string(swatch_x, y, "████", Style::default().bg(color));
                }
                y += 2;

                // Max color (high values)
                let max_value = self.gradient_max_bg.map(|c| color_to_hex_string(&c)).unwrap_or_else(|| "None".to_string());
                let max_label = format!("Max Color (high values): {}", max_value);
                buf.set_string(start_x, y, &max_label, highlight(ApplicationScopeField::GradientMaxBg));
                if let Some(color) = self.gradient_max_bg {
                    let swatch_x = start_x + max_label.len() as u16 + 2;
                    buf.set_string(swatch_x, y, "████", Style::default().bg(color));
                }
                y += 2;

                // Scale type
                let scale_label = format!("Scale: {}",
                    match self.gradient_scale {
                        GradientScale::Linear => "[Linear]  Logarithmic   Percentile",
                        GradientScale::Logarithmic => " Linear  [Logarithmic]  Percentile",
                        GradientScale::Percentile => " Linear   Logarithmic  [Percentile]",
                    }
                );
                buf.set_string(start_x, y, &scale_label, highlight(ApplicationScopeField::GradientScale));
                y += 2;

                // Gradient preview
                buf.set_string(start_x, y, "Preview (low → high):", Style::default().fg(Color::Gray));
                y += 1;
                
                // Draw gradient bar
                let bar_width = 30.min(inner.width.saturating_sub(4) as usize);
                for i in 0..bar_width {
                    let t = i as f64 / (bar_width - 1).max(1) as f64;
                    let color = self.interpolate_gradient_preview(t);
                    buf.set_string(start_x + i as u16, y, "█", Style::default().fg(color));
                }
            }
            StyleTypeOption::Categorical => {
                // Source column field
                let source_display = if self.text_input_active && self.focus_field == ApplicationScopeField::CategoricalSourceColumn {
                    format!("{}▌", self.text_input_buffer)
                } else if self.categorical_source_column.is_empty() {
                    "*  (auto-detect)".to_string()
                } else {
                    self.categorical_source_column.clone()
                };
                let source_label = format!("Source Column: {}", source_display);
                buf.set_string(start_x, y, &source_label, highlight(ApplicationScopeField::CategoricalSourceColumn));
                y += 2;

                // Apply to foreground/background
                let apply_label = format!("Apply Colors To: {}",
                    if self.categorical_apply_to_fg {
                        "[Foreground]  Background"
                    } else {
                        " Foreground  [Background]"
                    }
                );
                buf.set_string(start_x, y, &apply_label, highlight(ApplicationScopeField::CategoricalApplyTo));
                y += 2;

                // Color palette preview
                buf.set_string(start_x, y, "Color Palette:", Style::default().fg(Color::Gray));
                y += 1;
                
                let palette = [
                    Color::Rgb(66, 133, 244),   // Blue
                    Color::Rgb(234, 67, 53),    // Red
                    Color::Rgb(251, 188, 5),    // Yellow
                    Color::Rgb(52, 168, 83),    // Green
                    Color::Rgb(154, 160, 166),  // Gray
                    Color::Rgb(255, 112, 67),   // Deep Orange
                    Color::Rgb(156, 39, 176),   // Purple
                    Color::Rgb(0, 188, 212),    // Cyan
                ];
                
                let mut x_offset = start_x;
                for (i, color) in palette.iter().enumerate() {
                    let sample = format!("Cat{} ", i + 1);
                    let style = if self.categorical_apply_to_fg {
                        Style::default().fg(*color)
                    } else {
                        Style::default().bg(*color).fg(Color::White)
                    };
                    buf.set_string(x_offset, y, &sample, style);
                    x_offset += sample.len() as u16;
                }
            }
        }

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
    
    /// Helper to interpolate gradient colors for preview
    fn interpolate_gradient_preview(&self, t: f64) -> Color {
        match (self.gradient_min_bg, self.gradient_max_bg) {
            (Some(Color::Rgb(r1, g1, b1)), Some(Color::Rgb(r2, g2, b2))) => {
                let r = ((1.0 - t) * r1 as f64 + t * r2 as f64) as u8;
                let g = ((1.0 - t) * g1 as f64 + t * g2 as f64) as u8;
                let b = ((1.0 - t) * b1 as f64 + t * b2 as f64) as u8;
                Color::Rgb(r, g, b)
            }
            (Some(c), None) | (None, Some(c)) => c,
            _ => Color::Gray,
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
            ApplicationScopeEditorMode::GradientMinColorPicker(picker) => {
                if let Some(action) = picker.handle_key_event_pub(key) {
                    match action {
                        Action::ColorPickerDialogApplied(color) => {
                            self.gradient_min_bg = color;
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
            ApplicationScopeEditorMode::GradientMaxColorPicker(picker) => {
                if let Some(action) = picker.handle_key_event_pub(key) {
                    match action {
                        Action::ColorPickerDialogApplied(color) => {
                            self.gradient_max_bg = color;
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

        // Handle text input mode
        if self.text_input_active {
            match key.code {
                KeyCode::Enter => {
                    // Confirm text input
                    match self.focus_field {
                        ApplicationScopeField::GradientSourceColumn => {
                            self.gradient_source_column = self.text_input_buffer.clone();
                        }
                        ApplicationScopeField::CategoricalSourceColumn => {
                            self.categorical_source_column = self.text_input_buffer.clone();
                        }
                        _ => {}
                    }
                    self.text_input_active = false;
                    self.text_input_buffer.clear();
                    return None;
                }
                KeyCode::Esc => {
                    // Cancel text input
                    self.text_input_active = false;
                    self.text_input_buffer.clear();
                    return None;
                }
                KeyCode::Backspace => {
                    self.text_input_buffer.pop();
                    return None;
                }
                KeyCode::Char(c) => {
                    self.text_input_buffer.push(c);
                    return None;
                }
                _ => return None,
            }
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
                        ApplicationScopeField::GradientMinBg => {
                            let mut picker = ColorPickerDialog::new(self.gradient_min_bg);
                            let _ = picker.register_config_handler(self.config.clone());
                            self.mode = ApplicationScopeEditorMode::GradientMinColorPicker(Box::new(picker));
                            return None;
                        }
                        ApplicationScopeField::GradientMaxBg => {
                            let mut picker = ColorPickerDialog::new(self.gradient_max_bg);
                            let _ = picker.register_config_handler(self.config.clone());
                            self.mode = ApplicationScopeEditorMode::GradientMaxColorPicker(Box::new(picker));
                            return None;
                        }
                        ApplicationScopeField::GradientSourceColumn => {
                            self.text_input_buffer = self.gradient_source_column.clone();
                            self.text_input_active = true;
                            return None;
                        }
                        ApplicationScopeField::CategoricalSourceColumn => {
                            self.text_input_buffer = self.categorical_source_column.clone();
                            self.text_input_active = true;
                            return None;
                        }
                        ApplicationScopeField::Scope | ApplicationScopeField::StyleType |
                        ApplicationScopeField::Modifiers | ApplicationScopeField::GradientScale |
                        ApplicationScopeField::CategoricalApplyTo => {
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
                    self.navigate_up();
                    return None;
                }
                Action::Down => {
                    self.navigate_down();
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

        // Handle space key for toggling
        if key.code == KeyCode::Char(' ') {
            match self.focus_field {
                ApplicationScopeField::Scope => {
                    self.scope = self.scope.next();
                }
                ApplicationScopeField::StyleType => {
                    self.style_type = self.style_type.next();
                }
                ApplicationScopeField::Modifiers => {
                    if let Some((modifier, _)) = AVAILABLE_MODIFIERS.get(self.selected_modifier_index) {
                        self.toggle_modifier(*modifier);
                    }
                }
                ApplicationScopeField::GradientScale => {
                    self.gradient_scale = match self.gradient_scale {
                        GradientScale::Linear => GradientScale::Logarithmic,
                        GradientScale::Logarithmic => GradientScale::Percentile,
                        GradientScale::Percentile => GradientScale::Linear,
                    };
                }
                ApplicationScopeField::CategoricalApplyTo => {
                    self.categorical_apply_to_fg = !self.categorical_apply_to_fg;
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
                ApplicationScopeField::GradientMinBg => {
                    self.gradient_min_bg = None;
                }
                ApplicationScopeField::GradientMaxBg => {
                    self.gradient_max_bg = None;
                }
                _ => {}
            }
            return None;
        }

        None
    }
    
    /// Navigate up through fields based on current style type
    fn navigate_up(&mut self) {
        match self.style_type {
            StyleTypeOption::Static => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                    ApplicationScopeField::StyleType => {
                        self.focus_field = ApplicationScopeField::Scope;
                    }
                    ApplicationScopeField::Foreground => {
                        self.focus_field = ApplicationScopeField::StyleType;
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
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                }
            }
            StyleTypeOption::Gradient => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                    ApplicationScopeField::StyleType => {
                        self.focus_field = ApplicationScopeField::Scope;
                    }
                    ApplicationScopeField::GradientSourceColumn => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                    ApplicationScopeField::GradientMinBg => {
                        self.focus_field = ApplicationScopeField::GradientSourceColumn;
                    }
                    ApplicationScopeField::GradientMaxBg => {
                        self.focus_field = ApplicationScopeField::GradientMinBg;
                    }
                    ApplicationScopeField::GradientScale => {
                        self.focus_field = ApplicationScopeField::GradientMaxBg;
                    }
                    ApplicationScopeField::Buttons => {
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                }
            }
            StyleTypeOption::Categorical => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                    ApplicationScopeField::StyleType => {
                        self.focus_field = ApplicationScopeField::Scope;
                    }
                    ApplicationScopeField::CategoricalSourceColumn => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                    ApplicationScopeField::CategoricalApplyTo => {
                        self.focus_field = ApplicationScopeField::CategoricalSourceColumn;
                    }
                    ApplicationScopeField::Buttons => {
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                }
            }
        }
    }
    
    /// Navigate down through fields based on current style type
    fn navigate_down(&mut self) {
        match self.style_type {
            StyleTypeOption::Static => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                    ApplicationScopeField::StyleType => {
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
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                }
            }
            StyleTypeOption::Gradient => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                    ApplicationScopeField::StyleType => {
                        self.focus_field = ApplicationScopeField::GradientSourceColumn;
                    }
                    ApplicationScopeField::GradientSourceColumn => {
                        self.focus_field = ApplicationScopeField::GradientMinBg;
                    }
                    ApplicationScopeField::GradientMinBg => {
                        self.focus_field = ApplicationScopeField::GradientMaxBg;
                    }
                    ApplicationScopeField::GradientMaxBg => {
                        self.focus_field = ApplicationScopeField::GradientScale;
                    }
                    ApplicationScopeField::GradientScale => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                    ApplicationScopeField::Buttons => {
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                }
            }
            StyleTypeOption::Categorical => {
                match self.focus_field {
                    ApplicationScopeField::Scope => {
                        self.focus_field = ApplicationScopeField::StyleType;
                    }
                    ApplicationScopeField::StyleType => {
                        self.focus_field = ApplicationScopeField::CategoricalSourceColumn;
                    }
                    ApplicationScopeField::CategoricalSourceColumn => {
                        self.focus_field = ApplicationScopeField::CategoricalApplyTo;
                    }
                    ApplicationScopeField::CategoricalApplyTo => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                    ApplicationScopeField::Buttons => {
                        if self.selected_button < 1 {
                            self.selected_button = 1;
                        } else {
                            self.selected_button = 0;
                        }
                    }
                    _ => {
                        self.focus_field = ApplicationScopeField::Buttons;
                        self.selected_button = 0;
                    }
                }
            }
        }
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

