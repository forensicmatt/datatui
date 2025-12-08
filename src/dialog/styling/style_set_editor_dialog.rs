//! StyleSetEditorDialog: Dialog for editing StyleSet metadata and managing rules
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{StyleSet, StyleRule, StyleLogic, Condition};
use crate::dialog::styling::style_rule_editor_dialog::StyleRuleEditorDialog;
use ratatui::style::Color;
use arboard::Clipboard;
use uuid::Uuid;

/// Focus field in the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSetEditorField {
    Id,
    Name,
    Categories,
    Description,
    Tags,
    Rules,
}

/// Dialog mode
#[derive(Debug)]
pub enum StyleSetEditorMode {
    Editing,
    RuleEditor(Box<StyleRuleEditorDialog>),
}

/// StyleSetEditorDialog: UI for editing a StyleSet
#[derive(Debug)]
pub struct StyleSetEditorDialog {
    /// The StyleSet being edited
    pub style_set: StyleSet,
    /// Text input for name
    pub name_input: String,
    /// Text input for categories (comma-separated)
    pub categories_input: String,
    /// Text input for description
    pub description_input: String,
    /// Text input for tags (comma-separated)
    pub tags_input: String,
    /// Current focus field
    pub focus_field: StyleSetEditorField,
    /// Cursor position in current text field
    pub cursor_position: usize,
    /// Selection start for text input
    pub selection_start: Option<usize>,
    /// Selection end for text input
    pub selection_end: Option<usize>,
    /// Selected rule index (when in Rules field)
    pub selected_rule_index: usize,
    /// Scroll offset for rules list
    pub rules_scroll_offset: usize,
    /// Available columns for filter expressions
    pub columns: Vec<String>,
    /// Dialog mode
    pub mode: StyleSetEditorMode,
    /// Index of rule being edited (None = new rule)
    pub editing_rule_index: Option<usize>,
    /// Show instructions
    pub show_instructions: bool,
    /// Config
    pub config: Config,
}

impl StyleSetEditorDialog {
    /// Create a new StyleSetEditorDialog for an existing StyleSet
    pub fn new(style_set: StyleSet, columns: Vec<String>) -> Self {
        let name_input = style_set.name.clone();
        let categories_input = style_set.categories.clone()
            .map(|v| v.join(", "))
            .unwrap_or_default();
        let description_input = style_set.description.clone();
        let tags_input = style_set.tags.clone()
            .map(|v| v.join(", "))
            .unwrap_or_default();

        Self {
            style_set,
            name_input,
            categories_input,
            description_input,
            tags_input,
            focus_field: StyleSetEditorField::Name,
            cursor_position: 0,
            selection_start: None,
            selection_end: None,
            selected_rule_index: 0,
            rules_scroll_offset: 0,
            columns,
            mode: StyleSetEditorMode::Editing,
            editing_rule_index: None,
            show_instructions: true,
            config: Config::default(),
        }
    }

    /// Create a new StyleSetEditorDialog for a new StyleSet
    pub fn new_empty(columns: Vec<String>) -> Self {
        let id = Uuid::new_v4().to_string();
        let style_set = StyleSet {
            id,
            name: String::new(),
            categories: None,
            tags: None,
            description: String::new(),
            yaml_path: None,
            rules: vec![],
            schema_hint: None,
        };
        Self::new(style_set, columns)
    }

    /// Build the resulting StyleSet from current inputs
    pub fn build_style_set(&self) -> StyleSet {
        let categories = if self.categories_input.trim().is_empty() {
            None
        } else {
            Some(
                self.categories_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            )
        };

        let tags = if self.tags_input.trim().is_empty() {
            None
        } else {
            Some(
                self.tags_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            )
        };

        StyleSet {
            id: self.style_set.id.clone(),
            name: self.name_input.clone(),
            categories,
            description: self.description_input.clone(),
            tags,
            yaml_path: self.style_set.yaml_path.clone(),
            rules: self.style_set.rules.clone(),
            schema_hint: self.style_set.schema_hint.clone(),
        }
    }

    /// Get current text field value based on focus
    fn get_current_field_value(&self) -> &str {
        match self.focus_field {
            StyleSetEditorField::Id => &self.style_set.id,
            StyleSetEditorField::Name => &self.name_input,
            StyleSetEditorField::Categories => &self.categories_input,
            StyleSetEditorField::Description => &self.description_input,
            StyleSetEditorField::Tags => &self.tags_input,
            StyleSetEditorField::Rules => "",
        }
    }

    /// Set current text field value based on focus
    fn set_current_field_value(&mut self, value: String) {
        match self.focus_field {
            StyleSetEditorField::Id => {} // ID is read-only
            StyleSetEditorField::Name => self.name_input = value,
            StyleSetEditorField::Categories => self.categories_input = value,
            StyleSetEditorField::Description => self.description_input = value,
            StyleSetEditorField::Tags => self.tags_input = value,
            StyleSetEditorField::Rules => {}
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
        let len = self.get_current_field_value().chars().count();
        self.selection_start = Some(0);
        self.selection_end = Some(len);
        self.cursor_position = len;
    }

    /// Delete the selected text if a selection exists
    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.get_selection_range() {
            let current = self.get_current_field_value().to_string();
            let chars: Vec<char> = current.chars().collect();
            let new_value: String = chars[..start].iter().chain(chars[end..].iter()).collect();
            self.set_current_field_value(new_value);
            self.cursor_position = start;
            self.clear_selection();
            true
        } else {
            false
        }
    }

    /// Copy text to clipboard
    fn copy_to_clipboard(&self) {
        let current = self.get_current_field_value();
        let text_to_copy = if let Some((start, end)) = self.get_selection_range() {
            let chars: Vec<char> = current.chars().collect();
            chars[start..end].iter().collect::<String>()
        } else {
            current.to_string()
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

        let current = self.get_current_field_value().to_string();
        if current.is_empty() || self.cursor_position == 0 {
            return;
        }

        let chars: Vec<char> = current.chars().collect();
        let mut pos = self.cursor_position.min(chars.len());

        if pos == 0 {
            return;
        }

        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        let word_start = if pos > 0 {
            let mut start = pos;
            if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' {
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                    start -= 1;
                }
            } else {
                start = pos - 1;
            }
            start
        } else {
            0
        };

        let new_value: String = chars[..word_start].iter().chain(chars[self.cursor_position..].iter()).collect();
        self.set_current_field_value(new_value);
        self.cursor_position = word_start;
    }

    /// Check if current field is a text input field
    fn is_text_field(&self) -> bool {
        matches!(
            self.focus_field,
            StyleSetEditorField::Name |
            StyleSetEditorField::Categories |
            StyleSetEditorField::Description |
            StyleSetEditorField::Tags
        )
    }

    /// Get a summary of a rule for display
    fn get_rule_summary(rule: &StyleRule) -> String {
        let name = rule.name.as_ref().map(|n| n.as_str()).unwrap_or("(unnamed)");
        
        let logic_summary = match &rule.logic {
            StyleLogic::Conditional(cond) => {
                let cond_str = match &cond.condition {
                    Condition::Filter { columns, .. } => {
                        let cols = columns.as_ref().map(|v| v.join(", ")).unwrap_or_else(|| "all".to_string());
                        format!("Filter[{}]", cols)
                    }
                    Condition::Regex { pattern, .. } => format!("Regex({})", pattern),
                };
                let app_count = cond.applications.len();
                let scope_str = if app_count > 0 {
                    cond.applications[0].scope.display_name()
                } else {
                    "?"
                };
                format!("{} → {} ({}app)", cond_str, scope_str, app_count)
            }
            StyleLogic::Gradient(g) => format!("Gradient({})", g.source_column),
            StyleLogic::Categorical(c) => format!("Categorical({})", c.source_column),
        };

        format!("{}: {} [p:{}]", name, logic_summary, rule.priority)
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match &self.mode {
            StyleSetEditorMode::Editing => {
                let field_hint = match self.focus_field {
                    StyleSetEditorField::Id => "ID is read-only",
                    StyleSetEditorField::Name => "Enter name",
                    StyleSetEditorField::Categories => "Comma-separated categories",
                    StyleSetEditorField::Description => "Enter description",
                    StyleSetEditorField::Tags => "Comma-separated tags",
                    StyleSetEditorField::Rules => "+: Add, -: Delete, Enter: Edit, Ctrl+↑/↓: Move",
                };
                format!(
                    "{}  {}",
                    field_hint,
                    self.config.actions_to_instructions(&[
                        (Mode::Global, Action::Up),
                        (Mode::Global, Action::Down),
                        (Mode::Global, Action::Escape),
                        (Mode::StyleSetEditorDialog, Action::SaveStyleSet),
                    ])
                )
            }
            StyleSetEditorMode::RuleEditor(_) => {
                "Editing rule...".to_string()
            }
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // If in rule editor mode, render that instead
        if let StyleSetEditorMode::RuleEditor(editor) = &self.mode {
            editor.render(area, buf);
            return;
        }

        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Set Editor")
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
            .title("Edit Style Set")
            .borders(Borders::ALL);
        let inner = block.inner(content_area);
        block.render(content_area, buf);

        let start_x = inner.x;
        let mut y = inner.y;

        let highlight = |field: StyleSetEditorField| -> Style {
            if self.focus_field == field {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let label_style = Style::default().fg(Color::Gray);
        let value_style = Style::default().fg(Color::White);

        // ID field (read-only)
        buf.set_string(start_x, y, "ID:", label_style);
        buf.set_string(start_x + 15, y, &self.style_set.id, 
            if self.focus_field == StyleSetEditorField::Id {
                Style::default().fg(Color::DarkGray).bg(Color::Rgb(40, 40, 40))
            } else {
                Style::default().fg(Color::DarkGray)
            }
        );
        y += 1;

        // Name field
        buf.set_string(start_x, y, "Name:", highlight(StyleSetEditorField::Name));
        self.render_text_field(start_x + 15, y, &self.name_input, StyleSetEditorField::Name, buf);
        y += 1;

        // Categories field
        buf.set_string(start_x, y, "Categories:", highlight(StyleSetEditorField::Categories));
        self.render_text_field(start_x + 15, y, &self.categories_input, StyleSetEditorField::Categories, buf);
        y += 1;

        // Description field
        buf.set_string(start_x, y, "Description:", highlight(StyleSetEditorField::Description));
        self.render_text_field(start_x + 15, y, &self.description_input, StyleSetEditorField::Description, buf);
        y += 1;

        // Tags field
        buf.set_string(start_x, y, "Tags:", highlight(StyleSetEditorField::Tags));
        self.render_text_field(start_x + 15, y, &self.tags_input, StyleSetEditorField::Tags, buf);
        y += 2;

        // Rules section
        buf.set_string(start_x, y, "Rules:", highlight(StyleSetEditorField::Rules));
        buf.set_string(start_x + 10, y, &format!("({} rule{})", 
            self.style_set.rules.len(),
            if self.style_set.rules.len() == 1 { "" } else { "s" }
        ), label_style);
        y += 1;

        // Render rules list
        let rules_area_height = inner.height.saturating_sub(y - inner.y).saturating_sub(1) as usize;
        let is_rules_focused = self.focus_field == StyleSetEditorField::Rules;

        if self.style_set.rules.is_empty() {
            buf.set_string(start_x + 2, y, "No rules defined", Style::default().fg(Color::DarkGray));
        } else {
            let end = (self.rules_scroll_offset + rules_area_height).min(self.style_set.rules.len());
            for (vis_idx, i) in (self.rules_scroll_offset..end).enumerate() {
                let rule = &self.style_set.rules[i];
                let is_selected = is_rules_focused && i == self.selected_rule_index;
                
                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    value_style
                };

                let prefix = if is_selected { "► " } else { "  " };
                let summary = Self::get_rule_summary(rule);
                let line = format!("{}{}: {}", prefix, i + 1, summary);
                
                // Truncate to fit
                let max_width = inner.width.saturating_sub(2) as usize;
                let display_line = if line.chars().count() > max_width {
                    format!("{}...", line.chars().take(max_width - 3).collect::<String>())
                } else {
                    line
                };
                
                buf.set_string(start_x, y + vis_idx as u16, &display_line, style);
            }
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

    /// Render a text field with cursor and selection
    fn render_text_field(&self, x: u16, y: u16, value: &str, field: StyleSetEditorField, buf: &mut Buffer) {
        let is_focused = self.focus_field == field;
        let style = if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Cyan)
        };

        if is_focused && self.is_text_field() {
            // Render with selection/cursor
            if let Some((sel_start, sel_end)) = self.get_selection_range() {
                let chars: Vec<char> = value.chars().collect();
                let mut x_pos = x;

                if sel_start > 0 {
                    let before: String = chars[..sel_start].iter().collect();
                    buf.set_string(x_pos, y, &before, style);
                    x_pos += before.len() as u16;
                }

                let selected: String = chars[sel_start..sel_end].iter().collect();
                let selection_style = Style::default().fg(Color::Black).bg(Color::White);
                buf.set_string(x_pos, y, &selected, selection_style);
                x_pos += selected.len() as u16;

                if sel_end < chars.len() {
                    let after: String = chars[sel_end..].iter().collect();
                    buf.set_string(x_pos, y, &after, style);
                }
            } else {
                // Draw text (no placeholder when focused)
                buf.set_string(x, y, value, style);
                // Render block cursor
                let cursor_x = x + self.cursor_position as u16;
                let cursor_char = value.chars().nth(self.cursor_position).unwrap_or(' ');
                let cursor_style = self.config.style_config.cursor.block();
                buf.set_string(cursor_x, y, cursor_char.to_string(), cursor_style);
            }
        } else {
            // When not focused, show placeholder for empty fields
            let display = if value.is_empty() { "(empty)" } else { value };
            let display_style = if value.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                style
            };
            buf.set_string(x, y, display, display_style);
        }
    }

    /// Handle a key event
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Handle rule editor mode first
        if let StyleSetEditorMode::RuleEditor(editor) = &mut self.mode {
            if let Some(action) = editor.handle_key_event_pub(key) {
                match action {
                    Action::StyleRuleEditorDialogApplied(rule) => {
                        if let Some(idx) = self.editing_rule_index {
                            // Update existing rule
                            if idx < self.style_set.rules.len() {
                                self.style_set.rules[idx] = rule;
                            }
                        } else {
                            // Add new rule
                            self.style_set.rules.push(rule);
                            self.selected_rule_index = self.style_set.rules.len() - 1;
                        }
                        self.mode = StyleSetEditorMode::Editing;
                        self.editing_rule_index = None;
                    }
                    Action::CloseStyleRuleEditorDialog => {
                        self.mode = StyleSetEditorMode::Editing;
                        self.editing_rule_index = None;
                    }
                    _ => {}
                }
            }
            return None;
        }

        // Check Global actions first
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseStyleSetEditorDialog);
                }
                Action::Enter => {
                    if self.focus_field == StyleSetEditorField::Rules && !self.style_set.rules.is_empty() {
                        // Edit selected rule
                        let rule = self.style_set.rules[self.selected_rule_index].clone();
                        let mut editor = StyleRuleEditorDialog::new(rule, self.columns.clone());
                        let _ = editor.register_config_handler(self.config.clone());
                        self.editing_rule_index = Some(self.selected_rule_index);
                        self.mode = StyleSetEditorMode::RuleEditor(Box::new(editor));
                    }
                    return None;
                }
                Action::Up => {
                    match self.focus_field {
                        StyleSetEditorField::Id => {
                            self.focus_field = StyleSetEditorField::Rules;
                            if !self.style_set.rules.is_empty() {
                                self.selected_rule_index = self.style_set.rules.len() - 1;
                            }
                        }
                        StyleSetEditorField::Name => {
                            self.focus_field = StyleSetEditorField::Id;
                            self.cursor_position = 0;
                        }
                        StyleSetEditorField::Categories => {
                            self.focus_field = StyleSetEditorField::Name;
                            self.cursor_position = self.name_input.chars().count();
                        }
                        StyleSetEditorField::Description => {
                            self.focus_field = StyleSetEditorField::Categories;
                            self.cursor_position = self.categories_input.chars().count();
                        }
                        StyleSetEditorField::Tags => {
                            self.focus_field = StyleSetEditorField::Description;
                            self.cursor_position = self.description_input.chars().count();
                        }
                        StyleSetEditorField::Rules => {
                            if self.selected_rule_index > 0 {
                                self.selected_rule_index -= 1;
                                // Update scroll offset if needed
                                if self.selected_rule_index < self.rules_scroll_offset {
                                    self.rules_scroll_offset = self.selected_rule_index;
                                }
                            } else {
                                self.focus_field = StyleSetEditorField::Tags;
                                self.cursor_position = self.tags_input.chars().count();
                            }
                        }
                    }
                    self.clear_selection();
                    return None;
                }
                Action::Down => {
                    match self.focus_field {
                        StyleSetEditorField::Id => {
                            self.focus_field = StyleSetEditorField::Name;
                            self.cursor_position = 0;
                        }
                        StyleSetEditorField::Name => {
                            self.focus_field = StyleSetEditorField::Categories;
                            self.cursor_position = 0;
                        }
                        StyleSetEditorField::Categories => {
                            self.focus_field = StyleSetEditorField::Description;
                            self.cursor_position = 0;
                        }
                        StyleSetEditorField::Description => {
                            self.focus_field = StyleSetEditorField::Tags;
                            self.cursor_position = 0;
                        }
                        StyleSetEditorField::Tags => {
                            self.focus_field = StyleSetEditorField::Rules;
                            self.selected_rule_index = 0;
                            self.rules_scroll_offset = 0;
                        }
                        StyleSetEditorField::Rules => {
                            if self.selected_rule_index < self.style_set.rules.len().saturating_sub(1) {
                                self.selected_rule_index += 1;
                            } else {
                                self.focus_field = StyleSetEditorField::Id;
                            }
                        }
                    }
                    self.clear_selection();
                    return None;
                }
                Action::Left => {
                    if self.is_text_field() && self.cursor_position > 0 {
                        self.cursor_position -= 1;
                        self.clear_selection();
                    } else if self.focus_field == StyleSetEditorField::Rules && !self.style_set.rules.is_empty() {
                        // Rotate scope backwards for the selected rule
                        let rule = &mut self.style_set.rules[self.selected_rule_index];
                        if let StyleLogic::Conditional(cond) = &mut rule.logic {
                            if let Some(app) = cond.applications.first_mut() {
                                app.scope = app.scope.prev();
                            }
                        }
                    }
                    return None;
                }
                Action::Right => {
                    if self.is_text_field() {
                        let len = self.get_current_field_value().chars().count();
                        if self.cursor_position < len {
                            self.cursor_position += 1;
                            self.clear_selection();
                        }
                    } else if self.focus_field == StyleSetEditorField::Rules && !self.style_set.rules.is_empty() {
                        // Rotate scope forwards for the selected rule
                        let rule = &mut self.style_set.rules[self.selected_rule_index];
                        if let StyleLogic::Conditional(cond) = &mut rule.logic {
                            if let Some(app) = cond.applications.first_mut() {
                                app.scope = app.scope.next();
                            }
                        }
                    }
                    return None;
                }
                Action::Backspace => {
                    if self.is_text_field() {
                        if !self.delete_selection() && self.cursor_position > 0 {
                            let current = self.get_current_field_value().to_string();
                            let chars: Vec<char> = current.chars().collect();
                            let new_value: String = chars[..self.cursor_position - 1]
                                .iter()
                                .chain(chars[self.cursor_position..].iter())
                                .collect();
                            self.set_current_field_value(new_value);
                            self.cursor_position -= 1;
                        }
                    }
                    return None;
                }
                Action::SelectAllText => {
                    if self.is_text_field() {
                        self.select_all();
                        return None;
                    }
                    // Don't return - let it fall through to dialog-specific actions
                }
                Action::CopyText => {
                    if self.is_text_field() {
                        self.copy_to_clipboard();
                    }
                    return None;
                }
                Action::DeleteWord => {
                    if self.is_text_field() {
                        self.delete_word_backward();
                    }
                    return None;
                }
                Action::Paste => {
                    if self.is_text_field() {
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                self.delete_selection();
                                let current = self.get_current_field_value().to_string();
                                let chars: Vec<char> = current.chars().collect();
                                let before: String = chars[..self.cursor_position].iter().collect();
                                let after: String = chars[self.cursor_position..].iter().collect();
                                self.set_current_field_value(format!("{}{}{}", before, text, after));
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

        // Check StyleSetEditorDialog specific actions
        if let Some(dialog_action) = self.config.action_for_key(Mode::StyleSetEditorDialog, key) {
            match dialog_action {
                Action::AddStyleRule => {
                    let editor = StyleRuleEditorDialog::new_empty(self.columns.clone());
                    let mut editor = editor;
                    let _ = editor.register_config_handler(self.config.clone());
                    self.editing_rule_index = None; // New rule
                    self.mode = StyleSetEditorMode::RuleEditor(Box::new(editor));
                    return None;
                }
                Action::EditStyleRule => {
                    if self.focus_field == StyleSetEditorField::Rules && !self.style_set.rules.is_empty() {
                        let rule = self.style_set.rules[self.selected_rule_index].clone();
                        let mut editor = StyleRuleEditorDialog::new(rule, self.columns.clone());
                        let _ = editor.register_config_handler(self.config.clone());
                        self.editing_rule_index = Some(self.selected_rule_index);
                        self.mode = StyleSetEditorMode::RuleEditor(Box::new(editor));
                    }
                    return None;
                }
                Action::DeleteStyleRule => {
                    if self.focus_field == StyleSetEditorField::Rules && !self.style_set.rules.is_empty() {
                        self.style_set.rules.remove(self.selected_rule_index);
                        if self.selected_rule_index >= self.style_set.rules.len() && self.selected_rule_index > 0 {
                            self.selected_rule_index -= 1;
                        }
                    }
                    return None;
                }
                Action::MoveRuleUp => {
                    if self.focus_field == StyleSetEditorField::Rules 
                        && self.selected_rule_index > 0 
                        && !self.style_set.rules.is_empty() 
                    {
                        self.style_set.rules.swap(self.selected_rule_index, self.selected_rule_index - 1);
                        self.selected_rule_index -= 1;
                    }
                    return None;
                }
                Action::MoveRuleDown => {
                    if self.focus_field == StyleSetEditorField::Rules 
                        && self.selected_rule_index < self.style_set.rules.len() - 1 
                    {
                        self.style_set.rules.swap(self.selected_rule_index, self.selected_rule_index + 1);
                        self.selected_rule_index += 1;
                    }
                    return None;
                }
                Action::SaveStyleSet => {
                    let style_set = self.build_style_set();
                    return Some(Action::StyleSetEditorDialogApplied(style_set));
                }
                _ => {}
            }
        }
        
        // Direct +/- key handling for Rules field
        if self.focus_field == StyleSetEditorField::Rules {
            if key.code == KeyCode::Char('+') || key.code == KeyCode::Insert {
                // Add new rule
                let editor = StyleRuleEditorDialog::new_empty(self.columns.clone());
                let mut editor = editor;
                let _ = editor.register_config_handler(self.config.clone());
                self.editing_rule_index = None; // New rule
                self.mode = StyleSetEditorMode::RuleEditor(Box::new(editor));
                return None;
            }
            if key.code == KeyCode::Char('-') || key.code == KeyCode::Delete {
                // Delete selected rule
                if !self.style_set.rules.is_empty() {
                    self.style_set.rules.remove(self.selected_rule_index);
                    if self.selected_rule_index >= self.style_set.rules.len() && self.selected_rule_index > 0 {
                        self.selected_rule_index -= 1;
                    }
                }
                return None;
            }
        }

        // Handle character input for text fields
        if self.is_text_field() {
            if let KeyCode::Char(c) = key.code {
                self.delete_selection();
                let current = self.get_current_field_value().to_string();
                let chars: Vec<char> = current.chars().collect();
                let before: String = chars[..self.cursor_position].iter().collect();
                let after: String = chars[self.cursor_position..].iter().collect();
                self.set_current_field_value(format!("{}{}{}", before, c, after));
                self.cursor_position += 1;
                self.clear_selection();
                return None;
            }
            if key.code == KeyCode::Delete {
                if !self.delete_selection() {
                    let current = self.get_current_field_value().to_string();
                    let chars: Vec<char> = current.chars().collect();
                    if self.cursor_position < chars.len() {
                        let new_value: String = chars[..self.cursor_position]
                            .iter()
                            .chain(chars[self.cursor_position + 1..].iter())
                            .collect();
                        self.set_current_field_value(new_value);
                    }
                }
                return None;
            }
        }

        // Handle Ctrl+A for adding rule (alternative)
        if key.code == KeyCode::Char('a') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            if self.focus_field == StyleSetEditorField::Rules {
                let mut editor = StyleRuleEditorDialog::new_empty(self.columns.clone());
                let _ = editor.register_config_handler(self.config.clone());
                self.editing_rule_index = None;
                self.mode = StyleSetEditorMode::RuleEditor(Box::new(editor));
            }
            return None;
        }

        // Handle Ctrl+S to save
        if key.code == KeyCode::Char('s') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
            let style_set = self.build_style_set();
            return Some(Action::StyleSetEditorDialogApplied(style_set));
        }

        None
    }
}

impl Component for StyleSetEditorDialog {
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

