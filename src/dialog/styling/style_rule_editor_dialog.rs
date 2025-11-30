//! StyleRuleEditorDialog: Dialog for editing individual style rules
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{StyleRule, ApplicationScope, MatchedStyle, ScopeEnum, MergeMode};
use crate::dialog::styling::application_scope_editor_dialog::ApplicationScopeEditorDialog;
use crate::dialog::styling::color_picker_dialog::color_to_hex_string;
use crate::dialog::filter_dialog::{FilterDialog, FilterExpr};
use ratatui::style::Color;
use arboard::Clipboard;

/// Focus field in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleRuleField {
    ConditionColumns,  // Renamed from ColumnScope
    MatchExpr,
    ApplicationScope,
    Priority,          // New field
    MergeMode,         // New field
}

/// Dialog mode
#[derive(Debug)]
pub enum StyleRuleEditorMode {
    Editing,
    FilterEditor(Box<FilterDialog>),
    ApplicationScopeEditor(Box<ApplicationScopeEditorDialog>),
}

/// StyleRuleEditorDialog: UI for editing a single StyleRule
#[derive(Debug)]
pub struct StyleRuleEditorDialog {
    /// Condition columns patterns (comma-separated glob patterns)
    pub condition_columns_input: String,
    /// Match expression (FilterExpr)
    pub match_expr: FilterExpr,
    /// Application scope (scope + style)
    pub app_scope: ApplicationScope,
    /// Rule priority (higher = processed later)
    pub priority: i32,
    /// Merge mode
    pub merge_mode: MergeMode,
    /// Current focus field
    pub focus_field: StyleRuleField,
    /// Cursor position in current text input
    pub cursor_position: usize,
    /// Selection start for text input
    pub selection_start: Option<usize>,
    /// Selection end for text input
    pub selection_end: Option<usize>,
    /// Available columns for filter dialog
    pub columns: Vec<String>,
    /// Dialog mode
    pub mode: StyleRuleEditorMode,
    /// Show instructions
    pub show_instructions: bool,
    /// Config
    pub config: Config,
    /// Max rows for filter dialog rendering
    filter_max_rows: usize,
}

impl StyleRuleEditorDialog {
    /// Create a new StyleRuleEditorDialog
    pub fn new(rule: StyleRule, columns: Vec<String>) -> Self {
        let condition_columns_input = rule.condition_columns
            .map(|v| v.join(", "))
            .unwrap_or_default();
        
        Self {
            condition_columns_input,
            match_expr: rule.match_expr,
            app_scope: rule.style,
            priority: rule.priority,
            merge_mode: rule.merge_mode,
            focus_field: StyleRuleField::ConditionColumns,
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
        Self::new(
            StyleRule {
                condition_columns: None,
                match_expr: FilterExpr::And(vec![]),
                style: ApplicationScope {
                    scope: ScopeEnum::Row,
                    target_columns: None,
                    style: MatchedStyle {
                        fg: None,
                        bg: None,
                        modifiers: None,
                    },
                    dynamic_style: None,
                },
                priority: 0,
                merge_mode: MergeMode::Override,
            },
            columns,
        )
    }

    /// Build the resulting StyleRule
    pub fn build_style_rule(&self) -> StyleRule {
        let condition_columns = if self.condition_columns_input.trim().is_empty() {
            None
        } else {
            Some(
                self.condition_columns_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            )
        };

        StyleRule {
            condition_columns,
            match_expr: self.match_expr.clone(),
            style: self.app_scope.clone(),
            priority: self.priority,
            merge_mode: self.merge_mode,
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

    /// Select all text
    fn select_all(&mut self) {
        let len = self.condition_columns_input.chars().count();
        self.selection_start = Some(0);
        self.selection_end = Some(len);
        self.cursor_position = len;
    }

    /// Delete the selected text if a selection exists
    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            let chars: Vec<char> = self.condition_columns_input.chars().collect();
            self.condition_columns_input = chars[..start].iter().chain(chars[end..].iter()).collect();
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
            let chars: Vec<char> = self.condition_columns_input.chars().collect();
            chars[start..end].iter().collect::<String>()
        } else {
            self.condition_columns_input.clone()
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

        if self.condition_columns_input.is_empty() || self.cursor_position == 0 {
            return;
        }

        let chars: Vec<char> = self.condition_columns_input.chars().collect();
        let mut pos = self.cursor_position.min(chars.len());

        if pos == 0 {
            return;
        }

        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        let word_start = if pos > 0 {
            let mut start = pos;
            if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' || chars[pos - 1] == '*' {
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '*') {
                    start -= 1;
                }
            } else {
                start = pos - 1;
            }
            start
        } else {
            0
        };

        self.condition_columns_input = chars[..word_start].iter().chain(chars[self.cursor_position..].iter()).collect();
        self.cursor_position = word_start;
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match &self.mode {
            StyleRuleEditorMode::Editing => {
                let field_hint = match self.focus_field {
                    StyleRuleField::ConditionColumns => "Type glob patterns (e.g., col_*, *_id)",
                    StyleRuleField::MatchExpr => "Enter: Edit Filter Expression",
                    StyleRuleField::ApplicationScope => "Enter: Edit Scope & Style",
                    StyleRuleField::Priority => "←/→: Adjust priority (-100 to 100)",
                    StyleRuleField::MergeMode => "←/→: Cycle merge mode",
                };
                format!(
                    "{}  {}",
                    field_hint,
                    self.config.actions_to_instructions(&[
                        (Mode::Global, Action::Up),
                        (Mode::Global, Action::Down),
                        (Mode::Global, Action::Enter),
                        (Mode::Global, Action::Escape),
                        (Mode::StyleRuleEditorDialog, Action::SaveStyleSet),
                    ])
                )
            }
            StyleRuleEditorMode::FilterEditor(_) => {
                "Editing filter expression...".to_string()
            }
            StyleRuleEditorMode::ApplicationScopeEditor(_) => {
                "Editing application scope...".to_string()
            }
        }
    }

    /// Get a summary of the match expression
    fn get_match_expr_summary(&self) -> String {
        match &self.match_expr {
            FilterExpr::And(children) if children.is_empty() => "No conditions (matches all)".to_string(),
            FilterExpr::And(children) => format!("AND group with {} condition(s)", children.len()),
            FilterExpr::Or(children) if children.is_empty() => "OR group (empty)".to_string(),
            FilterExpr::Or(children) => format!("OR group with {} condition(s)", children.len()),
            FilterExpr::Condition(cf) => cf.summary(),
        }
    }

    /// Get a summary of the application scope
    fn get_app_scope_summary(&self) -> String {
        let scope_str = match self.app_scope.scope {
            ScopeEnum::Row => "Row",
            ScopeEnum::Cell => "Cell",
            ScopeEnum::Header => "Header",
        };
        
        let mut parts = vec![format!("Scope: {}", scope_str)];
        
        if let Some(fg) = self.app_scope.style.fg {
            parts.push(format!("FG: {}", color_to_hex_string(&fg)));
        }
        if let Some(bg) = self.app_scope.style.bg {
            parts.push(format!("BG: {}", color_to_hex_string(&bg)));
        }
        if let Some(ref mods) = self.app_scope.style.modifiers {
            if !mods.is_empty() {
                let mod_names: Vec<&str> = mods.iter().map(|m| {
                    match *m {
                        ratatui::style::Modifier::BOLD => "Bold",
                        ratatui::style::Modifier::DIM => "Dim",
                        ratatui::style::Modifier::ITALIC => "Italic",
                        ratatui::style::Modifier::UNDERLINED => "Underlined",
                        ratatui::style::Modifier::REVERSED => "Reversed",
                        _ => "Other",
                    }
                }).collect();
                parts.push(format!("Mods: {}", mod_names.join(", ")));
            }
        }
        
        parts.join("  ")
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in sub-editor mode, render that instead
        match &self.mode {
            StyleRuleEditorMode::FilterEditor(dialog) => {
                dialog.render(area, buf);
                return;
            }
            StyleRuleEditorMode::ApplicationScopeEditor(dialog) => {
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
        let mut y = inner.y;

        let highlight = |field: StyleRuleField| -> Style {
            if self.focus_field == field {
                Style::default().fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        // Column Scope field
        buf.set_string(start_x, y, "Column Scope (glob patterns, comma-separated):", Style::default().fg(Color::Gray));
        y += 1;

        let scope_style = if self.focus_field == StyleRuleField::ConditionColumns {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };

        // Render column scope input with selection
        if self.focus_field == StyleRuleField::ConditionColumns {
            if let Some((sel_start, sel_end)) = self.get_selection_range() {
                let chars: Vec<char> = self.condition_columns_input.chars().collect();
                let mut x_pos = start_x;

                if sel_start > 0 {
                    let before: String = chars[..sel_start].iter().collect();
                    buf.set_string(x_pos, y, &before, scope_style);
                    x_pos += before.len() as u16;
                }

                let selected: String = chars[sel_start..sel_end].iter().collect();
                let selection_style = Style::default().fg(Color::Black).bg(Color::White);
                buf.set_string(x_pos, y, &selected, selection_style);
                x_pos += selected.len() as u16;

                if sel_end < chars.len() {
                    let after: String = chars[sel_end..].iter().collect();
                    buf.set_string(x_pos, y, &after, scope_style);
                }
            } else {
                buf.set_string(start_x, y, &self.condition_columns_input, scope_style);
                // Render cursor
                if self.get_selection_range().is_none() {
                    let cursor_x = start_x + self.cursor_position as u16;
                    let cursor_char = self.condition_columns_input.chars().nth(self.cursor_position).unwrap_or(' ');
                    let cursor_style = self.config.style_config.cursor.block();
                    buf.set_string(cursor_x, y, cursor_char.to_string(), cursor_style);
                }
            }
        } else {
            let display_text = if self.condition_columns_input.is_empty() {
                "(all columns)"
            } else {
                &self.condition_columns_input
            };
            buf.set_string(start_x, y, display_text, scope_style);
        }
        y += 2;

        // Match Expression field
        buf.set_string(start_x, y, "Match Expression:", highlight(StyleRuleField::MatchExpr));
        y += 1;
        let expr_summary = self.get_match_expr_summary();
        buf.set_string(start_x + 2, y, &expr_summary, Style::default().fg(Color::Gray));
        y += 2;

        // Application Scope field
        buf.set_string(start_x, y, "Application Scope & Style:", highlight(StyleRuleField::ApplicationScope));
        y += 1;
        let scope_summary = self.get_app_scope_summary();
        buf.set_string(start_x + 2, y, &scope_summary, Style::default().fg(Color::Gray));
        y += 2;

        // Style Preview
        buf.set_string(start_x, y, "Style Preview:", Style::default().fg(Color::Gray));
        y += 1;
        
        let preview_style = self.app_scope.style.to_ratatui_style();
        buf.set_string(start_x + 2, y, "Sample Text With Applied Style", preview_style);

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
                            self.match_expr = expr;
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
            StyleRuleEditorMode::ApplicationScopeEditor(dialog) => {
                if let Some(action) = dialog.handle_key_event_pub(key) {
                    match action {
                        Action::ApplicationScopeEditorDialogApplied(scope) => {
                            self.app_scope = scope;
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

        // Check Global actions first
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseStyleRuleEditorDialog);
                }
                Action::Enter => {
                    match self.focus_field {
                        StyleRuleField::ConditionColumns => {
                            // Move to next field
                            self.focus_field = StyleRuleField::MatchExpr;
                        }
                        StyleRuleField::MatchExpr => {
                            // Open filter editor with free column mode enabled for JMESPath queries
                            let mut filter_dialog = FilterDialog::new(self.columns.clone());
                            filter_dialog.set_root_expr(self.match_expr.clone());
                            filter_dialog.set_free_column(true);
                            let _ = filter_dialog.register_config_handler(self.config.clone());
                            self.mode = StyleRuleEditorMode::FilterEditor(Box::new(filter_dialog));
                        }
                        StyleRuleField::ApplicationScope => {
                            // Open application scope editor
                            let mut scope_dialog = ApplicationScopeEditorDialog::new(self.app_scope.clone());
                            let _ = scope_dialog.register_config_handler(self.config.clone());
                            self.mode = StyleRuleEditorMode::ApplicationScopeEditor(Box::new(scope_dialog));
                        }
                        StyleRuleField::Priority | StyleRuleField::MergeMode => {
                            // Move to next field
                            self.focus_field = StyleRuleField::ConditionColumns;
                        }
                    }
                    return None;
                }
                Action::Up => {
                    self.focus_field = match self.focus_field {
                        StyleRuleField::ConditionColumns => StyleRuleField::MergeMode,
                        StyleRuleField::MatchExpr => StyleRuleField::ConditionColumns,
                        StyleRuleField::ApplicationScope => StyleRuleField::MatchExpr,
                        StyleRuleField::Priority => StyleRuleField::ApplicationScope,
                        StyleRuleField::MergeMode => StyleRuleField::Priority,
                    };
                    return None;
                }
                Action::Down => {
                    self.focus_field = match self.focus_field {
                        StyleRuleField::ConditionColumns => StyleRuleField::MatchExpr,
                        StyleRuleField::MatchExpr => StyleRuleField::ApplicationScope,
                        StyleRuleField::ApplicationScope => StyleRuleField::Priority,
                        StyleRuleField::Priority => StyleRuleField::MergeMode,
                        StyleRuleField::MergeMode => StyleRuleField::ConditionColumns,
                    };
                    return None;
                }
                Action::Left => {
                    if self.focus_field == StyleRuleField::ConditionColumns && self.cursor_position > 0 {
                        self.cursor_position -= 1;
                        self.clear_selection();
                    }
                    return None;
                }
                Action::Right => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        let len = self.condition_columns_input.chars().count();
                        if self.cursor_position < len {
                            self.cursor_position += 1;
                            self.clear_selection();
                        }
                    }
                    return None;
                }
                Action::Backspace => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        if !self.delete_selection() && self.cursor_position > 0 {
                            let chars: Vec<char> = self.condition_columns_input.chars().collect();
                            self.condition_columns_input = chars[..self.cursor_position - 1]
                                .iter()
                                .chain(chars[self.cursor_position..].iter())
                                .collect();
                            self.cursor_position -= 1;
                        }
                    }
                    return None;
                }
                Action::SelectAllText => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        self.select_all();
                    }
                    return None;
                }
                Action::CopyText => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        self.copy_to_clipboard();
                    }
                    return None;
                }
                Action::DeleteWord => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        self.delete_word_backward();
                    }
                    return None;
                }
                Action::Paste => {
                    if self.focus_field == StyleRuleField::ConditionColumns {
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                self.delete_selection();
                                let chars: Vec<char> = self.condition_columns_input.chars().collect();
                                let before: String = chars[..self.cursor_position].iter().collect();
                                let after: String = chars[self.cursor_position..].iter().collect();
                                self.condition_columns_input = format!("{}{}{}", before, text, after);
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

        // Check StyleRuleEditorDialog specific actions
        if let Some(dialog_action) = self.config.action_for_key(Mode::StyleRuleEditorDialog, key) {
            match dialog_action {
                Action::SaveStyleSet => {
                    let rule = self.build_style_rule();
                    return Some(Action::StyleRuleEditorDialogApplied(rule));
                }
                _ => {}
            }
        }

        // Handle character input for column scope field
        if self.focus_field == StyleRuleField::ConditionColumns {
            if let KeyCode::Char(c) = key.code {
                self.delete_selection();
                let chars: Vec<char> = self.condition_columns_input.chars().collect();
                let before: String = chars[..self.cursor_position].iter().collect();
                let after: String = chars[self.cursor_position..].iter().collect();
                self.condition_columns_input = format!("{}{}{}", before, c, after);
                self.cursor_position += 1;
                self.clear_selection();
                return None;
            }
            if key.code == KeyCode::Delete {
                if !self.delete_selection() {
                    let chars: Vec<char> = self.condition_columns_input.chars().collect();
                    if self.cursor_position < chars.len() {
                        self.condition_columns_input = chars[..self.cursor_position]
                            .iter()
                            .chain(chars[self.cursor_position + 1..].iter())
                            .collect();
                    }
                }
                return None;
            }
        }

        // Handle Ctrl+S to save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            let rule = self.build_style_rule();
            return Some(Action::StyleRuleEditorDialogApplied(rule));
        }

        None
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
