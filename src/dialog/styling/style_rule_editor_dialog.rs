//! StyleRuleEditorDialog: Dialog for editing individual style rules
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{
    StyleRule, StyleLogic, Condition, ConditionalStyle,
    StyleApplication, MergeMode, GradientStyle, CategoricalStyle,
};
use crate::dialog::styling::application_scope_editor_dialog::ApplicationScopeEditorDialog;
use crate::dialog::styling::color_picker_dialog::color_to_hex_string;
use crate::dialog::filter_dialog::{FilterDialog, FilterExpr};
use ratatui::style::Color;

/// Focus field in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleRuleField {
    Name,
    LogicType,       // Conditional, Gradient, Categorical
    // For Conditional:
    ConditionType,   // Filter or Regex
    ConditionColumns,
    FilterExpr,      // For Filter condition
    RegexPattern,    // For Regex condition
    Applications,    // List of StyleApplication
    // Common fields:
    Priority,
    MergeMode,
}

/// Type of style logic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicTypeSelection {
    Conditional,
    Gradient,
    Categorical,
}

impl LogicTypeSelection {
    pub fn next(&self) -> Self {
        match self {
            Self::Conditional => Self::Gradient,
            Self::Gradient => Self::Categorical,
            Self::Categorical => Self::Conditional,
        }
    }
    
    pub fn prev(&self) -> Self {
        match self {
            Self::Conditional => Self::Categorical,
            Self::Gradient => Self::Conditional,
            Self::Categorical => Self::Gradient,
        }
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Conditional => "Conditional",
            Self::Gradient => "Gradient",
            Self::Categorical => "Categorical",
        }
    }
}

/// Type of condition (for conditional rules)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionTypeSelection {
    Filter,
    Regex,
}

impl ConditionTypeSelection {
    pub fn next(&self) -> Self {
        match self {
            Self::Filter => Self::Regex,
            Self::Regex => Self::Filter,
        }
    }
    
    pub fn prev(&self) -> Self {
        // Same as next for 2 options
        self.next()
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Filter => "Filter Expression",
            Self::Regex => "Regex Pattern",
        }
    }
}

/// Dialog mode
#[derive(Debug)]
pub enum StyleRuleEditorMode {
    Editing,
    FilterEditor(Box<FilterDialog>),
    ApplicationEditor(Box<ApplicationScopeEditorDialog>),
}

/// StyleRuleEditorDialog: UI for editing a single StyleRule
#[derive(Debug)]
pub struct StyleRuleEditorDialog {
    // Rule metadata
    pub name: String,
    pub priority: i32,
    pub merge_mode: MergeMode,
    
    // Logic type selection
    pub logic_type: LogicTypeSelection,
    
    // For Conditional logic:
    pub condition_type: ConditionTypeSelection,
    pub condition_columns: String,  // Comma-separated glob patterns
    pub filter_expr: FilterExpr,
    pub regex_pattern: String,
    pub applications: Vec<StyleApplication>,
    pub selected_application_index: usize,
    
    // For Gradient logic:
    pub gradient_style: GradientStyle,
    
    // For Categorical logic:
    pub categorical_style: CategoricalStyle,
    
    // UI state
    pub focus_field: StyleRuleField,
    pub cursor_position: usize,
    pub selection_start: Option<usize>,
    pub selection_end: Option<usize>,
    pub columns: Vec<String>,
    pub mode: StyleRuleEditorMode,
    pub show_instructions: bool,
    pub config: Config,
    filter_max_rows: usize,
}

impl StyleRuleEditorDialog {
    /// Create a new StyleRuleEditorDialog from an existing rule
    pub fn new(rule: StyleRule, columns: Vec<String>) -> Self {
        let name = rule.name.unwrap_or_default();
        let priority = rule.priority;
        let merge_mode = rule.merge_mode;
        
        // Extract data based on logic type
        let (logic_type, condition_type, condition_columns, filter_expr, regex_pattern, applications, gradient_style, categorical_style) = 
            match rule.logic {
                StyleLogic::Conditional(cond) => {
                    let (cond_type, cols, expr, pattern) = match cond.condition {
                        Condition::Filter { expr, columns } => {
                            (ConditionTypeSelection::Filter, columns.unwrap_or_default().join(", "), expr, String::new())
                        }
                        Condition::Regex { pattern, columns } => {
                            (ConditionTypeSelection::Regex, columns.unwrap_or_default().join(", "), FilterExpr::And(vec![]), pattern)
                        }
                    };
                    (LogicTypeSelection::Conditional, cond_type, cols, expr, pattern, cond.applications, GradientStyle::default(), CategoricalStyle::default())
                }
                StyleLogic::Gradient(g) => {
                    (LogicTypeSelection::Gradient, ConditionTypeSelection::Filter, String::new(), FilterExpr::And(vec![]), String::new(), vec![], g, CategoricalStyle::default())
                }
                StyleLogic::Categorical(c) => {
                    (LogicTypeSelection::Categorical, ConditionTypeSelection::Filter, String::new(), FilterExpr::And(vec![]), String::new(), vec![], GradientStyle::default(), c)
                }
            };
        
        let applications = if applications.is_empty() {
            vec![StyleApplication::default()]
        } else {
            applications
        };
        
        Self {
            name,
            priority,
            merge_mode,
            logic_type,
            condition_type,
            condition_columns,
            filter_expr,
            regex_pattern,
            applications,
            selected_application_index: 0,
            gradient_style,
            categorical_style,
            focus_field: StyleRuleField::Name,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
            columns,
            mode: StyleRuleEditorMode::Editing,
            show_instructions: true,
            config: Config::default(),
            filter_max_rows: 10,
        }
    }

    /// Create a new StyleRuleEditorDialog with empty rule
    pub fn new_empty(columns: Vec<String>) -> Self {
        Self::new(StyleRule::default(), columns)
    }

    /// Build the resulting StyleRule
    pub fn build_style_rule(&self) -> StyleRule {
        let logic = match self.logic_type {
            LogicTypeSelection::Conditional => {
                let columns = if self.condition_columns.trim().is_empty() {
                    None
                } else {
                    Some(self.condition_columns.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                };
                
                let condition = match self.condition_type {
                    ConditionTypeSelection::Filter => Condition::Filter {
                        expr: self.filter_expr.clone(),
                        columns,
                    },
                    ConditionTypeSelection::Regex => Condition::Regex {
                        pattern: self.regex_pattern.clone(),
                        columns,
                    },
                };
                
                StyleLogic::Conditional(ConditionalStyle {
                    condition,
                    applications: self.applications.clone(),
                })
            }
            LogicTypeSelection::Gradient => {
                StyleLogic::Gradient(self.gradient_style.clone())
            }
            LogicTypeSelection::Categorical => {
                StyleLogic::Categorical(self.categorical_style.clone())
            }
        };
        
        StyleRule {
            name: if self.name.is_empty() { None } else { Some(self.name.clone()) },
            logic,
            priority: self.priority,
            merge_mode: self.merge_mode,
        }
    }

    #[allow(dead_code)]
    fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    fn get_current_text(&self) -> &str {
        match self.focus_field {
            StyleRuleField::Name => &self.name,
            StyleRuleField::ConditionColumns => &self.condition_columns,
            StyleRuleField::RegexPattern => &self.regex_pattern,
            _ => "",
        }
    }
    
    fn get_current_text_mut(&mut self) -> &mut String {
        match self.focus_field {
            StyleRuleField::Name => &mut self.name,
            StyleRuleField::ConditionColumns => &mut self.condition_columns,
            StyleRuleField::RegexPattern => &mut self.regex_pattern,
            _ => &mut self.name, // Fallback
        }
    }

    fn build_instructions_from_config(&self) -> String {
        match &self.mode {
            StyleRuleEditorMode::Editing => {
                let field_hint = match self.focus_field {
                    StyleRuleField::Name => "Type rule name",
                    StyleRuleField::LogicType => "Space: Toggle logic type",
                    StyleRuleField::ConditionType => "Space: Toggle condition type",
                    StyleRuleField::ConditionColumns => "Type glob patterns (e.g., col_*, *_id)",
                    StyleRuleField::FilterExpr => "Enter: Edit Filter Expression",
                    StyleRuleField::RegexPattern => "Type regex pattern",
                    StyleRuleField::Applications => "Enter: Edit, +: Add, -: Remove, ←/→: Select",
                    StyleRuleField::Priority => "←/→: Adjust priority",
                    StyleRuleField::MergeMode => "Space: Toggle merge mode",
                };
                format!(
                    "{}  {}",
                    field_hint,
                    self.config.actions_to_instructions(&[
                        (Mode::Global, Action::Up),
                        (Mode::Global, Action::Down),
                        (Mode::Global, Action::Escape),
                        (Mode::StyleRuleEditorDialog, Action::SaveStyleSet),
                    ])
                )
            }
            StyleRuleEditorMode::FilterEditor(_) => "Editing filter expression...".to_string(),
            StyleRuleEditorMode::ApplicationEditor(_) => "Editing style application...".to_string(),
        }
    }

    fn get_filter_expr_summary(&self) -> String {
        match &self.filter_expr {
            FilterExpr::And(children) if children.is_empty() => "No conditions (matches all)".to_string(),
            FilterExpr::And(children) => format!("AND group with {} condition(s)", children.len()),
            FilterExpr::Or(children) if children.is_empty() => "OR group (empty)".to_string(),
            FilterExpr::Or(children) => format!("OR group with {} condition(s)", children.len()),
            FilterExpr::Condition(cf) => cf.summary(),
        }
    }

    fn get_application_summary(&self, app: &StyleApplication) -> String {
        let scope_str = app.scope.display_name();
        let mut parts = vec![format!("Scope: {}", scope_str)];
        
        if let Some(fg) = app.style.fg {
            parts.push(format!("FG: {}", color_to_hex_string(&fg)));
        }
        if let Some(bg) = app.style.bg {
            parts.push(format!("BG: {}", color_to_hex_string(&bg)));
        }
        if let Some(ref mods) = app.style.modifiers {
            if !mods.is_empty() {
                parts.push(format!("Mods: {}", mods.len()));
            }
        }
        
        parts.join("  ")
    }

    /// Helper to render a text field with block cursor
    fn render_text_field(&self, buf: &mut Buffer, x: u16, y: u16, text: &str, placeholder: &str, is_focused: bool, cursor_pos: usize) {
        if is_focused {
            // When focused: show text (no placeholder) with cursor
            let text_style = Style::default().fg(Color::White);
            buf.set_string(x, y, text, text_style);
            
            // Draw block cursor
            let cursor_x = x + cursor_pos as u16;
            let char_at_cursor = text.chars().nth(cursor_pos).unwrap_or(' ');
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(cursor_x, y, char_at_cursor.to_string(), cursor_style);
        } else {
            // When not focused: show placeholder in gray if empty
            if text.is_empty() {
                buf.set_string(x, y, placeholder, Style::default().fg(Color::DarkGray));
            } else {
                buf.set_string(x, y, text, Style::default().fg(Color::Cyan));
            }
        }
    }
    
    /// Helper to render a label with highlight when focused
    fn render_label(&self, buf: &mut Buffer, x: u16, y: u16, label: &str, field: StyleRuleField) -> u16 {
        let style = if self.focus_field == field {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(x, y, label, style);
        label.len() as u16
    }
    
    /// Helper to render a value with appropriate style
    fn render_value(&self, buf: &mut Buffer, x: u16, y: u16, value: &str, field: StyleRuleField) {
        let style = if self.focus_field == field {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };
        buf.set_string(x, y, value, style);
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in sub-editor mode, render that instead
        match &self.mode {
            StyleRuleEditorMode::FilterEditor(dialog) => {
                dialog.render(area, buf);
                return;
            }
            StyleRuleEditorMode::ApplicationEditor(dialog) => {
                dialog.render(area, buf);
                return;
            }
            StyleRuleEditorMode::Editing => {}
        }

        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Rule Editor")
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
            .title("Edit Rule")
            .borders(Borders::ALL);
        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let start_x = inner.x;
        let label_width: u16 = 16; // Consistent label column width
        let value_x = start_x + label_width;
        let mut y = inner.y;

        // Name field (text input with cursor)
        self.render_label(buf, start_x, y, "Name:", StyleRuleField::Name);
        self.render_text_field(
            buf, value_x, y, 
            &self.name, "(unnamed)", 
            self.focus_field == StyleRuleField::Name,
            self.cursor_position
        );
        y += 2;

        // Logic Type field (toggle)
        self.render_label(buf, start_x, y, "Logic Type:", StyleRuleField::LogicType);
        let logic_indicator = if self.focus_field == StyleRuleField::LogicType { "◀ " } else { "  " };
        let logic_display = format!("{}{}", logic_indicator, self.logic_type.display_name());
        self.render_value(buf, value_x, y, &logic_display, StyleRuleField::LogicType);
        if self.focus_field == StyleRuleField::LogicType {
            buf.set_string(value_x + logic_display.len() as u16, y, " ▶", Style::default().fg(Color::Yellow));
        }
        y += 2;

        // Conditional-specific fields
        if self.logic_type == LogicTypeSelection::Conditional {
            // Condition Type (toggle)
            self.render_label(buf, start_x, y, "Condition:", StyleRuleField::ConditionType);
            let cond_indicator = if self.focus_field == StyleRuleField::ConditionType { "◀ " } else { "  " };
            let cond_display = format!("{}{}", cond_indicator, self.condition_type.display_name());
            self.render_value(buf, value_x, y, &cond_display, StyleRuleField::ConditionType);
            if self.focus_field == StyleRuleField::ConditionType {
                buf.set_string(value_x + cond_display.len() as u16, y, " ▶", Style::default().fg(Color::Yellow));
            }
            y += 1;

            // Condition Columns (text input with cursor)
            self.render_label(buf, start_x, y, "Columns:", StyleRuleField::ConditionColumns);
            self.render_text_field(
                buf, value_x, y,
                &self.condition_columns, "(all columns)",
                self.focus_field == StyleRuleField::ConditionColumns,
                self.cursor_position
            );
            y += 1;

            // Filter or Regex
            match self.condition_type {
                ConditionTypeSelection::Filter => {
                    self.render_label(buf, start_x, y, "Filter:", StyleRuleField::FilterExpr);
                    let filter_summary = self.get_filter_expr_summary();
                    let filter_style = if self.focus_field == StyleRuleField::FilterExpr {
                        Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::UNDERLINED)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };
                    buf.set_string(value_x, y, &filter_summary, filter_style);
                    if self.focus_field == StyleRuleField::FilterExpr {
                        buf.set_string(value_x + filter_summary.len() as u16 + 1, y, "[Enter to edit]", Style::default().fg(Color::DarkGray));
                    }
                }
                ConditionTypeSelection::Regex => {
                    self.render_label(buf, start_x, y, "Pattern:", StyleRuleField::RegexPattern);
                    self.render_text_field(
                        buf, value_x, y,
                        &self.regex_pattern, "(no pattern)",
                        self.focus_field == StyleRuleField::RegexPattern,
                        self.cursor_position
                    );
                }
            }
            y += 2;

            // Applications section
            self.render_label(buf, start_x, y, "Applications:", StyleRuleField::Applications);
            let app_count = self.applications.len();
            let app_hint = if self.focus_field == StyleRuleField::Applications {
                format!("({}) [+: Add, -: Remove, Enter: Edit]", app_count)
            } else {
                format!("({})", app_count)
            };
            buf.set_string(value_x, y, &app_hint, Style::default().fg(Color::DarkGray));
            y += 1;
            
            for (i, app) in self.applications.iter().enumerate() {
                let is_selected = i == self.selected_application_index && self.focus_field == StyleRuleField::Applications;
                let marker = if is_selected { "▶ " } else { "  " };
                let marker_style = if is_selected {
                    Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let summary_style = if is_selected {
                    Style::default().fg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                
                // Show index number
                let index_str = format!("{}.", i + 1);
                buf.set_string(start_x + 2, y, marker, marker_style);
                buf.set_string(start_x + 4, y, &index_str, Style::default().fg(Color::DarkGray));
                buf.set_string(start_x + 7, y, &self.get_application_summary(app), summary_style);
                y += 1;
            }
            y += 1;
        }

        // Gradient-specific fields
        if self.logic_type == LogicTypeSelection::Gradient {
            buf.set_string(start_x, y, "Source Column:", Style::default().fg(Color::Gray));
            buf.set_string(value_x, y, &self.gradient_style.source_column, Style::default().fg(Color::Cyan));
            y += 1;
            buf.set_string(start_x, y, "Scale:", Style::default().fg(Color::Gray));
            buf.set_string(value_x, y, self.gradient_style.scale.display_name(), Style::default().fg(Color::Cyan));
            y += 2;
        }

        // Categorical-specific fields
        if self.logic_type == LogicTypeSelection::Categorical {
            buf.set_string(start_x, y, "Source Column:", Style::default().fg(Color::Gray));
            buf.set_string(value_x, y, &self.categorical_style.source_column, Style::default().fg(Color::Cyan));
            y += 1;
            buf.set_string(start_x, y, "Palette:", Style::default().fg(Color::Gray));
            buf.set_string(value_x, y, &format!("{} colors", self.categorical_style.palette.len()), Style::default().fg(Color::Cyan));
            y += 2;
        }

        // Priority (number with arrows)
        self.render_label(buf, start_x, y, "Priority:", StyleRuleField::Priority);
        let priority_display = if self.focus_field == StyleRuleField::Priority {
            format!("◀ {} ▶", self.priority)
        } else {
            self.priority.to_string()
        };
        self.render_value(buf, value_x, y, &priority_display, StyleRuleField::Priority);
        y += 1;

        // Merge Mode (toggle)
        self.render_label(buf, start_x, y, "Merge Mode:", StyleRuleField::MergeMode);
        let merge_str = match self.merge_mode {
            MergeMode::Override => "Override",
            MergeMode::Merge => "Merge",
            MergeMode::Additive => "Additive",
        };
        let merge_indicator = if self.focus_field == StyleRuleField::MergeMode { "◀ " } else { "  " };
        let merge_display = format!("{}{}", merge_indicator, merge_str);
        self.render_value(buf, value_x, y, &merge_display, StyleRuleField::MergeMode);
        if self.focus_field == StyleRuleField::MergeMode {
            buf.set_string(value_x + merge_display.len() as u16, y, " ▶", Style::default().fg(Color::Yellow));
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

        // Handle sub-editor modes first
        match &mut self.mode {
            StyleRuleEditorMode::FilterEditor(dialog) => {
                if let Some(action) = dialog.handle_key_event(key, self.filter_max_rows) {
                    match action {
                        Action::FilterDialogApplied(expr) => {
                            self.filter_expr = expr;
                            self.mode = StyleRuleEditorMode::Editing;
                        }
                        Action::DialogClose => {
                            self.mode = StyleRuleEditorMode::Editing;
                        }
                        _ => {}
                    }
                }
                return None;
            }
            StyleRuleEditorMode::ApplicationEditor(dialog) => {
                if let Some(action) = dialog.handle_key_event_pub(key) {
                    match action {
                        Action::ApplicationScopeEditorDialogApplied(app) => {
                            if self.selected_application_index < self.applications.len() {
                                self.applications[self.selected_application_index] = app;
                            }
                            self.mode = StyleRuleEditorMode::Editing;
                        }
                        Action::CloseApplicationScopeEditorDialog => {
                            self.mode = StyleRuleEditorMode::Editing;
                        }
                        _ => {}
                    }
                }
                return None;
            }
            StyleRuleEditorMode::Editing => {}
        }

        // Check Global actions
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseStyleRuleEditorDialog);
                }
                Action::Enter => {
                    match self.focus_field {
                        StyleRuleField::FilterExpr => {
                            let mut filter_dialog = FilterDialog::new(self.columns.clone());
                            filter_dialog.set_root_expr(self.filter_expr.clone());
                            filter_dialog.set_free_column(true);
                            let _ = filter_dialog.register_config_handler(self.config.clone());
                            self.mode = StyleRuleEditorMode::FilterEditor(Box::new(filter_dialog));
                        }
                        StyleRuleField::Applications => {
                            if let Some(app) = self.applications.get(self.selected_application_index) {
                                let mut app_dialog = ApplicationScopeEditorDialog::new(app.clone());
                                let _ = app_dialog.register_config_handler(self.config.clone());
                                self.mode = StyleRuleEditorMode::ApplicationEditor(Box::new(app_dialog));
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Up => {
                    self.focus_field = self.prev_field();
                    self.cursor_position = 0;
                    return None;
                }
                Action::Down => {
                    self.focus_field = self.next_field();
                    self.cursor_position = 0;
                    return None;
                }
                Action::Left => {
                    match self.focus_field {
                        StyleRuleField::Priority => {
                            self.priority = (self.priority - 1).max(-100);
                        }
                        StyleRuleField::LogicType => {
                            self.logic_type = self.logic_type.prev();
                        }
                        StyleRuleField::ConditionType => {
                            self.condition_type = self.condition_type.prev();
                        }
                        StyleRuleField::MergeMode => {
                            self.merge_mode = match self.merge_mode {
                                MergeMode::Override => MergeMode::Additive,
                                MergeMode::Merge => MergeMode::Override,
                                MergeMode::Additive => MergeMode::Merge,
                            };
                        }
                        StyleRuleField::Applications => {
                            // Navigate up through applications list
                            if self.selected_application_index > 0 {
                                self.selected_application_index -= 1;
                            }
                        }
                        StyleRuleField::Name | StyleRuleField::ConditionColumns | StyleRuleField::RegexPattern => {
                            if self.cursor_position > 0 {
                                self.cursor_position -= 1;
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Right => {
                    match self.focus_field {
                        StyleRuleField::Priority => {
                            self.priority = (self.priority + 1).min(100);
                        }
                        StyleRuleField::LogicType => {
                            self.logic_type = self.logic_type.next();
                        }
                        StyleRuleField::ConditionType => {
                            self.condition_type = self.condition_type.next();
                        }
                        StyleRuleField::MergeMode => {
                            self.merge_mode = match self.merge_mode {
                                MergeMode::Override => MergeMode::Merge,
                                MergeMode::Merge => MergeMode::Additive,
                                MergeMode::Additive => MergeMode::Override,
                            };
                        }
                        StyleRuleField::Applications => {
                            // Navigate down through applications list
                            if self.selected_application_index < self.applications.len().saturating_sub(1) {
                                self.selected_application_index += 1;
                            }
                        }
                        StyleRuleField::Name | StyleRuleField::ConditionColumns | StyleRuleField::RegexPattern => {
                            let len = self.get_current_text().chars().count();
                            if self.cursor_position < len {
                                self.cursor_position += 1;
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Backspace => {
                    if matches!(self.focus_field, StyleRuleField::Name | StyleRuleField::ConditionColumns | StyleRuleField::RegexPattern) {
                        if self.cursor_position > 0 {
                            let pos = self.cursor_position;
                            let text = self.get_current_text_mut();
                            let chars: Vec<char> = text.chars().collect();
                            *text = chars[..pos - 1].iter().chain(chars[pos..].iter()).collect();
                            self.cursor_position -= 1;
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

        // Space toggles for certain fields
        if key.code == KeyCode::Char(' ') {
            match self.focus_field {
                StyleRuleField::LogicType => {
                    self.logic_type = self.logic_type.next();
                    return None;
                }
                StyleRuleField::ConditionType => {
                    self.condition_type = self.condition_type.next();
                    return None;
                }
                StyleRuleField::MergeMode => {
                    self.merge_mode = match self.merge_mode {
                        MergeMode::Override => MergeMode::Merge,
                        MergeMode::Merge => MergeMode::Additive,
                        MergeMode::Additive => MergeMode::Override,
                    };
                    return None;
                }
                _ => {}
            }
        }
        
        // Insert key or '+' to add new StyleApplication
        if self.focus_field == StyleRuleField::Applications {
            if key.code == KeyCode::Insert || key.code == KeyCode::Char('+') {
                // Add a new default StyleApplication
                self.applications.push(StyleApplication::default());
                self.selected_application_index = self.applications.len() - 1;
                return None;
            }
            // Delete key or '-' to remove selected StyleApplication
            if (key.code == KeyCode::Delete || key.code == KeyCode::Char('-')) && self.applications.len() > 1 {
                // Keep at least one application
                self.applications.remove(self.selected_application_index);
                if self.selected_application_index >= self.applications.len() {
                    self.selected_application_index = self.applications.len().saturating_sub(1);
                }
                return None;
            }
        }

        // Check StyleRuleEditorDialog specific actions
        if let Some(dialog_action) = self.config.action_for_key(Mode::StyleRuleEditorDialog, key) {
            if dialog_action == Action::SaveStyleSet {
                let rule = self.build_style_rule();
                return Some(Action::StyleRuleEditorDialogApplied(rule));
            }
        }

        // Handle character input for text fields
        if matches!(self.focus_field, StyleRuleField::Name | StyleRuleField::ConditionColumns | StyleRuleField::RegexPattern) {
            if let KeyCode::Char(c) = key.code {
                let pos = self.cursor_position;
                let text = self.get_current_text_mut();
                let chars: Vec<char> = text.chars().collect();
                let before: String = chars[..pos].iter().collect();
                let after: String = chars[pos..].iter().collect();
                *text = format!("{}{}{}", before, c, after);
                self.cursor_position += 1;
                return None;
            }
            if key.code == KeyCode::Delete {
                let pos = self.cursor_position;
                let text = self.get_current_text_mut();
                let chars: Vec<char> = text.chars().collect();
                if pos < chars.len() {
                    *text = chars[..pos].iter().chain(chars[pos + 1..].iter()).collect();
                }
                return None;
            }
        }

        // Ctrl+S to save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            let rule = self.build_style_rule();
            return Some(Action::StyleRuleEditorDialogApplied(rule));
        }

        None
    }
    
    fn next_field(&self) -> StyleRuleField {
        match self.logic_type {
            LogicTypeSelection::Conditional => {
                match self.focus_field {
                    StyleRuleField::Name => StyleRuleField::LogicType,
                    StyleRuleField::LogicType => StyleRuleField::ConditionType,
                    StyleRuleField::ConditionType => StyleRuleField::ConditionColumns,
                    StyleRuleField::ConditionColumns => {
                        match self.condition_type {
                            ConditionTypeSelection::Filter => StyleRuleField::FilterExpr,
                            ConditionTypeSelection::Regex => StyleRuleField::RegexPattern,
                        }
                    }
                    StyleRuleField::FilterExpr | StyleRuleField::RegexPattern => StyleRuleField::Applications,
                    StyleRuleField::Applications => StyleRuleField::Priority,
                    StyleRuleField::Priority => StyleRuleField::MergeMode,
                    StyleRuleField::MergeMode => StyleRuleField::Name,
                }
            }
            LogicTypeSelection::Gradient | LogicTypeSelection::Categorical => {
                match self.focus_field {
                    StyleRuleField::Name => StyleRuleField::LogicType,
                    StyleRuleField::LogicType => StyleRuleField::Priority,
                    StyleRuleField::Priority => StyleRuleField::MergeMode,
                    StyleRuleField::MergeMode => StyleRuleField::Name,
                    _ => StyleRuleField::Name,
                }
            }
        }
    }
    
    fn prev_field(&self) -> StyleRuleField {
        match self.logic_type {
            LogicTypeSelection::Conditional => {
                match self.focus_field {
                    StyleRuleField::Name => StyleRuleField::MergeMode,
                    StyleRuleField::LogicType => StyleRuleField::Name,
                    StyleRuleField::ConditionType => StyleRuleField::LogicType,
                    StyleRuleField::ConditionColumns => StyleRuleField::ConditionType,
                    StyleRuleField::FilterExpr | StyleRuleField::RegexPattern => StyleRuleField::ConditionColumns,
                    StyleRuleField::Applications => {
                        match self.condition_type {
                            ConditionTypeSelection::Filter => StyleRuleField::FilterExpr,
                            ConditionTypeSelection::Regex => StyleRuleField::RegexPattern,
                        }
                    }
                    StyleRuleField::Priority => StyleRuleField::Applications,
                    StyleRuleField::MergeMode => StyleRuleField::Priority,
                }
            }
            LogicTypeSelection::Gradient | LogicTypeSelection::Categorical => {
                match self.focus_field {
                    StyleRuleField::Name => StyleRuleField::MergeMode,
                    StyleRuleField::LogicType => StyleRuleField::Name,
                    StyleRuleField::Priority => StyleRuleField::LogicType,
                    StyleRuleField::MergeMode => StyleRuleField::Priority,
                    _ => StyleRuleField::Name,
                }
            }
        }
    }
}

impl Component for StyleRuleEditorDialog {
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
