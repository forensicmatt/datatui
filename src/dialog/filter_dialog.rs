//! FilterDialog: Popup dialog for configuring column filters on a DataFrame
use serde::{Deserialize, Serialize};
use strum::Display;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use crate::action::Action;
use crate::config::Config;

use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
// use std::cell::RefCell; // no longer used
use polars::prelude::*;
use std::fs::File;
use std::path::Path;
use serde_json;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use crate::components::dialog_layout::split_dialog_area;
use tracing::error;

/// Filter condition for a column
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum FilterCondition {
    Contains { value: String, case_sensitive: bool },
    Regex { pattern: String, case_sensitive: bool },
    Equals { value: String, case_sensitive: bool },
    GreaterThan { value: String },
    LessThan { value: String },
    GreaterThanOrEqual { value: String },
    LessThanOrEqual { value: String },
    IsEmpty,
    IsNotEmpty,
    NotNull,
    IsNull,
    // Extend with more types as needed
}

/// Filter applied to a column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnFilter {
    pub column: String,
    pub condition: FilterCondition,
}

/// Recursive filter expression: single condition or AND/OR group
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum FilterExpr {
    Condition(ColumnFilter),
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
}

/// Dialog mode: list, add, or edit filter
#[derive(Debug)]
pub enum FilterDialogMode {
    List,
    Add,
    Edit(usize), // index of filter being edited
    AddGroup,    // new: for group creation
    FileBrowser(FileBrowserDialog), // new: for save/load
}

/// FilterDialogField: Enum for filtering dialog fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDialogField {
    Column,
    Type,
    Value,
    CaseSensitive,
}

/// FilterDialog: UI for configuring column filters
#[derive(Debug)]
pub struct FilterDialog {
    pub columns: Vec<String>,
    root_expr: FilterExpr, // replaces filters: Vec<ColumnFilter>
    pub mode: FilterDialogMode,
    // List mode state
    pub selected_path: Vec<usize>, // path to selected node in tree
    pub scroll_offset: usize,
    // Add/Edit mode state
    pub add_column_index: usize,
    pub add_condition: Option<FilterCondition>,
    pub add_value: String,
    pub add_case_sensitive: bool,
    pub focus_field: FilterDialogField,
    // Store last rendered lines and selected_idx for navigation
    // last_rendered_lines: RefCell<Option<(Vec<(usize, String, bool, Vec<usize>)>, usize)>>,
    // New: path where to insert or edit a condition
    pub add_insertion_path: Option<Vec<usize>>,
    pub add_group_and: bool, // new: true=AND, false=OR for AddGroup mode
    pub show_instructions: bool, // new: show instructions area (default true)
    pub config: Config,
}

impl FilterDialog {
    /// Create a new FilterDialog
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            root_expr: FilterExpr::And(vec![]),
            mode: FilterDialogMode::List,
            selected_path: vec![],
            scroll_offset: 0,
            add_column_index: 0,
            add_condition: None,
            add_value: String::new(),
            add_case_sensitive: false,
            focus_field: FilterDialogField::Column,
            // last_rendered_lines: RefCell::new(None),
            add_insertion_path: None,
            add_group_and: true,
            show_instructions: true,
            config: Config::default(),
        }
    }

    /// Get a reference to the root filter expression
    pub fn get_root_expr(&self) -> &FilterExpr {
        &self.root_expr
    }

    /// Replace the root filter expression and reset dialog navigation/cache state
    pub fn set_root_expr(&mut self, expr: FilterExpr) {
        self.root_expr = expr;
        self.selected_path.clear();
        self.scroll_offset = 0;
        self.add_insertion_path = None;
        self.add_group_and = true;
        self.show_instructions = true;
        // Invalidate cached rendered lines so they are recomputed for the new expression
        // *self.last_rendered_lines.borrow_mut() = None;
        // Ensure we're in list mode after restore
        self.mode = FilterDialogMode::List;
    }

    /// Render the dialog (UI for List and Add/Edit modes)
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);

        // Build dynamic instructions from config
        let instructions = self.build_instructions_from_config();

        // Outer container with double border and title "Filter"
        let outer_block = Block::default()
            .title("Filter")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        let block = Block::default()
            .title(match &self.mode {
                FilterDialogMode::List => "Conditional Rules",
                FilterDialogMode::Add => "Add Filter",
                FilterDialogMode::Edit(_) => "Edit Filter",
                FilterDialogMode::AddGroup => "Add Group",
                FilterDialogMode::FileBrowser(_) => "Save Filter",
            })
            .borders(Borders::ALL);
        block.render(content_area, buf);
        // --- End dynamic instructions logic ---

        // Calculate max_rows for list rendering
        let mut max_rows = (content_area.height.saturating_sub(2)) as usize;
        if max_rows == 0 { max_rows = 1; }
        // Where to start rendering labels on the x axis
        let start_x = content_area.x + 1;
        match &self.mode {
            FilterDialogMode::List => {
                let list_start_y = content_area.y + 1; // +1 for border
                // Render filter tree as indented lines
                let mut lines = Vec::new();
                let mut path = Vec::new();
                self.root_expr.render_lines(&mut path, &self.selected_path, 0, &mut lines);
                // Store lines for navigation
                if lines.is_empty() {
                    buf.set_string(start_x, list_start_y, "No filters applied.", Style::default().fg(Color::DarkGray));
                } else {
                    let end = (self.scroll_offset + max_rows).min(lines.len());
                    for (vis_idx, i) in (self.scroll_offset..end).enumerate() {
                        let y = list_start_y + vis_idx as u16;
                        let (indent, label, is_selected, _line_path) = &lines[i];
                        let mut style = Style::default();
                        if *is_selected {
                            style = style.fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD);
                        } else if i % 2 == 0 {
                            style = style.bg(Color::Rgb(30,30,30));
                        }
                        let indent_str = "  ".repeat(*indent);
                        buf.set_string(start_x, y, format!("{indent_str}{label}"), style);
                    }
                }
            }
            FilterDialogMode::Add => {
                let field_y = content_area.y + 2;
                let highlight = |field| {
                    if self.focus_field == field {
                        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    }
                };
                // Field 1: Column
                let col_label = format!(
                    "Column: {}",
                    self.columns.get(self.add_column_index)
                        .unwrap_or(&"".to_string())
                );
                buf.set_string(start_x, field_y, col_label, highlight(FilterDialogField::Column));
                // Field 2: Type
                let type_label = format!("Type: {}", match self.add_condition {
                    Some(FilterCondition::Contains { .. }) => "Contains",
                    Some(FilterCondition::Regex { .. }) => "Regex",
                    Some(FilterCondition::Equals { .. }) => "Equals",
                    Some(FilterCondition::GreaterThan { .. }) => "Greater Than",
                    Some(FilterCondition::LessThan { .. }) => "Less Than",
                    Some(FilterCondition::GreaterThanOrEqual { .. }) => "Greater Than or Equal",
                    Some(FilterCondition::LessThanOrEqual { .. }) => "Less Than or Equal",
                    Some(FilterCondition::IsEmpty) => "Is Empty",
                    Some(FilterCondition::IsNotEmpty) => "Is Not Empty",
                    Some(FilterCondition::NotNull) => "Not Null",
                    Some(FilterCondition::IsNull) => "Is Null",
                    None => "<select>",
                });
                buf.set_string(start_x, field_y + 2, type_label, highlight(FilterDialogField::Type));
                // Field 3: Value
                let value_label = format!("Value: {}", self.add_value);
                buf.set_string(start_x, field_y + 4, value_label, highlight(FilterDialogField::Value));
                // Field 4: Case Sensitive
                let cs_label = format!("Case Sensitive: {}", if self.add_case_sensitive { "Yes" } else { "No" });
                buf.set_string(start_x, field_y + 6, cs_label, highlight(FilterDialogField::CaseSensitive));
            }
            FilterDialogMode::Edit(_idx) => {
                let field_y = content_area.y + 2;
                let highlight = |field| {
                    if self.focus_field == field {
                        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    }
                };
                // Field 1: Column
                let col_label = format!("Column: {}", self.columns.get(self.add_column_index).unwrap_or(&"".to_string()));
                buf.set_string(start_x, field_y, col_label, highlight(FilterDialogField::Column));
                // Field 2: Type
                let type_label = format!("Type: {}", match self.add_condition {
                    Some(FilterCondition::Contains { .. }) => "Contains",
                    Some(FilterCondition::Regex { .. }) => "Regex",
                    Some(FilterCondition::Equals { .. }) => "Equals",
                    Some(FilterCondition::GreaterThan { .. }) => "Greater Than",
                    Some(FilterCondition::LessThan { .. }) => "Less Than",
                    Some(FilterCondition::GreaterThanOrEqual { .. }) => "Greater Than or Equal",
                    Some(FilterCondition::LessThanOrEqual { .. }) => "Less Than or Equal",
                    Some(FilterCondition::IsEmpty) => "Is Empty",
                    Some(FilterCondition::IsNotEmpty) => "Is Not Empty",
                    Some(FilterCondition::NotNull) => "Not Null",
                    Some(FilterCondition::IsNull) => "Is Null",
                    None => "<select>",
                });
                buf.set_string(start_x, field_y + 2, type_label, highlight(FilterDialogField::Type));
                // Field 3: Value
                let value_label = format!("Value: {}", self.add_value);
                buf.set_string(start_x, field_y + 4, value_label, highlight(FilterDialogField::Value));
                // Field 4: Case Sensitive
                let cs_label = format!("Case Sensitive: {}", if self.add_case_sensitive { "Yes" } else { "No" });
                buf.set_string(start_x, field_y + 6, cs_label, highlight(FilterDialogField::CaseSensitive));
            }
            FilterDialogMode::AddGroup => {
                let label_y = content_area.y + 2;
                let label = format!("Type: {}", if self.add_group_and { "AND" } else { "OR" });
                buf.set_string(start_x, label_y, label, Style::default().add_modifier(Modifier::BOLD));
            }
            FilterDialogMode::FileBrowser(browser) => {
                browser.render(inner_area, buf);
                // Do not render FilterDialog's instructions in FileBrowser mode
                return max_rows;
            }
        }
        // --- Render instructions at the bottom with a border ---
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
            }
        // --- End instructions ---
        max_rows
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
        // Handle Ctrl+I to toggle instructions
        if key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.show_instructions = !self.show_instructions;
                return None;
            }
        
        // First, honor config-driven actions (Global + Filter)
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::DialogClose);
                }
                Action::Enter => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            return Some(Action::FilterDialogApplied(self.root_expr.clone()));
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            let column = self.columns.get(self.add_column_index).cloned().unwrap_or_default();
                            let condition = match self.add_condition.clone().unwrap_or(FilterCondition::Contains { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive }) {
                                FilterCondition::Contains { .. } => FilterCondition::Contains { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
                                FilterCondition::Regex { .. } => FilterCondition::Regex { pattern: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
                                FilterCondition::Equals { .. } => FilterCondition::Equals { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
                                FilterCondition::GreaterThan { .. } => FilterCondition::GreaterThan { value: self.add_value.clone() },
                                FilterCondition::LessThan { .. } => FilterCondition::LessThan { value: self.add_value.clone() },
                                FilterCondition::GreaterThanOrEqual { .. } => FilterCondition::GreaterThanOrEqual { value: self.add_value.clone() },
                                FilterCondition::LessThanOrEqual { .. } => FilterCondition::LessThanOrEqual { value: self.add_value.clone() },
                                FilterCondition::IsEmpty => FilterCondition::IsEmpty,
                                FilterCondition::IsNotEmpty => FilterCondition::IsNotEmpty,
                                FilterCondition::NotNull => FilterCondition::NotNull,
                                FilterCondition::IsNull => FilterCondition::IsNull,
                            };
                            let filter = ColumnFilter { column, condition };
                            if let Some(path) = self.add_insertion_path.clone() {
                                match self.mode {
                                    FilterDialogMode::Add => {
                                        insert_condition_at(&mut self.root_expr, &path, FilterExpr::Condition(filter));
                                        self.selected_path = path;
                                    }
                                    FilterDialogMode::Edit(_) => {
                                        replace_condition_at(&mut self.root_expr, &path, FilterExpr::Condition(filter));
                                        self.selected_path = path;
                                    }
                                    _ => {}
                                }
                            }
                            self.mode = FilterDialogMode::List;
                            self.add_insertion_path = None;
                            return None;
                        }
                        FilterDialogMode::AddGroup => {
                            if let Some(path) = self.add_insertion_path.clone() {
                                let node = self.root_expr.get_mut(&path);
                                match node {
                                    Some(FilterExpr::Condition(_)) => {
                                        let replaced = std::mem::replace(node.unwrap(), FilterExpr::And(vec![]));
                                        match replaced {
                                            FilterExpr::Condition(cf) => {
                                                let group = if self.add_group_and {
                                                    FilterExpr::And(vec![FilterExpr::Condition(cf.clone())])
                                                } else {
                                                    FilterExpr::Or(vec![FilterExpr::Condition(cf.clone())])
                                                };
                                                replace_condition_at(&mut self.root_expr, &path, group);
                                                self.selected_path = path;
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                    Some(FilterExpr::And(children)) | Some(FilterExpr::Or(children)) => {
                                        let mut child_path = path.clone();
                                        child_path.push(children.len());
                                        let group = if self.add_group_and {
                                            FilterExpr::And(vec![])
                                        } else {
                                            FilterExpr::Or(vec![])
                                        };
                                        insert_condition_at(&mut self.root_expr, &child_path, group);
                                        self.selected_path = child_path;
                                    }
                                    _ => {}
                                }
                            }
                            self.mode = FilterDialogMode::List;
                            self.add_insertion_path = None;
                            return None;
                        }
                        _ => {}
                    }
                }
                Action::Up => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            let mut lines = Vec::new();
                            let mut path = Vec::new();
                            self.root_expr.render_lines(&mut path, &self.selected_path, 0, &mut lines);
                            let idx = lines.iter().position(|(_, _, is_selected, _)| *is_selected).unwrap_or(0);
                            if !lines.is_empty() {
                                let new_idx = if idx == 0 { lines.len() - 1 } else { idx - 1 };
                                self.selected_path = lines[new_idx].3.clone();
                                if new_idx < self.scroll_offset {
                                    self.scroll_offset = new_idx;
                                }
                            }
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            self.focus_field = match self.focus_field {
                                FilterDialogField::Column => FilterDialogField::CaseSensitive,
                                FilterDialogField::Type => FilterDialogField::Column,
                                FilterDialogField::Value => FilterDialogField::Type,
                                FilterDialogField::CaseSensitive => FilterDialogField::Value,
                            };
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Down => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            let mut lines = Vec::new();
                            let mut path = Vec::new();
                            self.root_expr.render_lines(&mut path, &self.selected_path, 0, &mut lines);
                            let idx = lines.iter().position(|(_, _, is_selected, _)| *is_selected).unwrap_or(0);
                            if !lines.is_empty() {
                                let new_idx = if idx + 1 >= lines.len() { 0 } else { idx + 1 };
                                self.selected_path = lines[new_idx].3.clone();
                                if new_idx >= self.scroll_offset + max_rows {
                                    self.scroll_offset = new_idx + 1 - max_rows;
                                }
                            }
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            self.focus_field = match self.focus_field {
                                FilterDialogField::Column => FilterDialogField::Type,
                                FilterDialogField::Type => FilterDialogField::Value,
                                FilterDialogField::Value => FilterDialogField::CaseSensitive,
                                FilterDialogField::CaseSensitive => FilterDialogField::Column,
                            };
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Left => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            if !self.selected_path.is_empty() {
                                self.selected_path.pop();
                            }
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            match self.focus_field {
                                FilterDialogField::Column => {
                                    if self.columns.is_empty() {
                                        self.add_column_index = 0;
                                    } else if self.add_column_index == 0 {
                                        self.add_column_index = self.columns.len() - 1;
                                    } else {
                                        self.add_column_index -= 1;
                                    }
                                }
                                FilterDialogField::Type => {
                                    self.add_condition = match self.add_condition {
                                        Some(FilterCondition::Contains { .. }) | None => Some(FilterCondition::NotNull),
                                        Some(FilterCondition::NotNull) => Some(FilterCondition::IsNotEmpty),
                                        Some(FilterCondition::IsNotEmpty) => Some(FilterCondition::IsNull),
                                        Some(FilterCondition::IsNull) => Some(FilterCondition::IsEmpty),
                                        Some(FilterCondition::IsEmpty) => Some(FilterCondition::LessThanOrEqual { value: self.add_value.clone() }),
                                        Some(FilterCondition::LessThanOrEqual { .. }) => Some(FilterCondition::LessThan { value: self.add_value.clone() }),
                                        Some(FilterCondition::LessThan { .. }) => Some(FilterCondition::GreaterThanOrEqual { value: self.add_value.clone() }),
                                        Some(FilterCondition::GreaterThanOrEqual { .. }) => Some(FilterCondition::GreaterThan { value: self.add_value.clone() }),
                                        Some(FilterCondition::GreaterThan { .. }) => Some(FilterCondition::Equals { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                        Some(FilterCondition::Equals { .. }) => Some(FilterCondition::Regex { pattern: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                        Some(FilterCondition::Regex { .. }) => Some(FilterCondition::Contains { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                    };
                                }
                                FilterDialogField::CaseSensitive => {
                                    self.add_case_sensitive = !self.add_case_sensitive;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Right => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            let node = self.root_expr.get_mut(&self.selected_path);
                            if let Some(FilterExpr::And(children)) | Some(FilterExpr::Or(children)) = node
                                && !children.is_empty() {
                                    self.selected_path.push(0);
                                }
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            match self.focus_field {
                                FilterDialogField::Column => {
                                    if self.columns.is_empty() {
                                        self.add_column_index = 0;
                                    } else {
                                        self.add_column_index = (self.add_column_index + 1) % self.columns.len();
                                    }
                                }
                                FilterDialogField::Type => {
                                    self.add_condition = match self.add_condition {
                                        Some(FilterCondition::Contains { .. }) => Some(FilterCondition::Regex { pattern: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                        Some(FilterCondition::Regex { .. }) => Some(FilterCondition::Equals { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                        Some(FilterCondition::Equals { .. }) => Some(FilterCondition::GreaterThan { value: self.add_value.clone() }),
                                        Some(FilterCondition::GreaterThan { .. }) => Some(FilterCondition::GreaterThanOrEqual { value: self.add_value.clone() }),
                                        Some(FilterCondition::GreaterThanOrEqual { .. }) => Some(FilterCondition::LessThan { value: self.add_value.clone() }),
                                        Some(FilterCondition::LessThan { .. }) => Some(FilterCondition::LessThanOrEqual { value: self.add_value.clone() }),
                                        Some(FilterCondition::LessThanOrEqual { .. }) => Some(FilterCondition::IsEmpty),
                                        Some(FilterCondition::IsEmpty) => Some(FilterCondition::IsNull),
                                        Some(FilterCondition::IsNull) => Some(FilterCondition::IsNotEmpty),
                                        Some(FilterCondition::IsNotEmpty) => Some(FilterCondition::NotNull),
                                        Some(FilterCondition::NotNull) | None => Some(FilterCondition::Contains { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive }),
                                    };
                                }
                                FilterDialogField::CaseSensitive => {
                                    self.add_case_sensitive = !self.add_case_sensitive;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    return None;
                }
                Action::Backspace => {
                    if matches!(&self.mode, FilterDialogMode::Add | FilterDialogMode::Edit(_)) {
                        if self.focus_field == FilterDialogField::Value {
                            self.add_value.pop();
                        }
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Next, check for Filter mode specific actions
        if let Some(filter_action) = self.config.action_for_key(crate::config::Mode::Filter, key) {
            match filter_action {
                Action::AddFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let node = self.root_expr.get_mut(&self.selected_path);
                        match node {
                            Some(FilterExpr::And(children)) | Some(FilterExpr::Or(children)) => {
                                let mut insertion_path = self.selected_path.clone();
                                insertion_path.push(children.len());
                                self.mode = FilterDialogMode::Add;
                                self.add_insertion_path = Some(insertion_path);
                                self.focus_field = FilterDialogField::Column;
                                self.add_condition = None;
                                self.add_value.clear();
                                self.add_case_sensitive = false;
                            }
                            Some(FilterExpr::Condition(_)) => {
                                if let Some(parent_path) = parent_path_of(&self.selected_path) {
                                    let mut sibling_path = parent_path.clone();
                                    if let Some(last) = sibling_path.last_mut() {
                                        *last += 1;
                                    }
                                    self.mode = FilterDialogMode::Add;
                                    self.add_insertion_path = Some(sibling_path);
                                    self.focus_field = FilterDialogField::Column;
                                    self.add_condition = None;
                                    self.add_value.clear();
                                    self.add_case_sensitive = false;
                                }
                            }
                            _ => {}
                        }
                    }
                    return None;
                }
                Action::EditFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let mut lines = Vec::new();
                        let mut path = Vec::new();
                        self.root_expr.render_lines(&mut path, &self.selected_path, 0, &mut lines);
                        let idx = lines.iter().position(|(_, _, is_selected, _)| *is_selected).unwrap_or(0);
                        let node = self.root_expr.get_mut(&self.selected_path);
                        match node {
                            Some(FilterExpr::Condition(col_filter)) => {
                                self.mode = FilterDialogMode::Edit(idx);
                                self.add_insertion_path = Some(self.selected_path.clone());
                                self.focus_field = FilterDialogField::Column;
                                self.add_column_index = self.columns.iter().position(|c| c == &col_filter.column).unwrap_or(0);
                                self.add_condition = Some(col_filter.condition.clone());
                                self.add_value = match &col_filter.condition {
                                    FilterCondition::Contains { value, .. } => value.clone(),
                                    FilterCondition::Regex { pattern, .. } => pattern.clone(),
                                    FilterCondition::Equals { value, .. } => value.clone(),
                                    FilterCondition::GreaterThan { value } => value.clone(),
                                    FilterCondition::LessThan { value } => value.clone(),
                                    FilterCondition::GreaterThanOrEqual { value } => value.clone(),
                                    FilterCondition::LessThanOrEqual { value } => value.clone(),
                                    FilterCondition::IsEmpty => "".to_string(),
                                    FilterCondition::IsNotEmpty => "".to_string(),
                                    FilterCondition::NotNull => "".to_string(),
                                    FilterCondition::IsNull => "".to_string(),
                                };
                                self.add_case_sensitive = match &col_filter.condition {
                                    FilterCondition::Contains { case_sensitive, .. }
                                    | FilterCondition::Regex { case_sensitive, .. }
                                    | FilterCondition::Equals { case_sensitive, .. } => *case_sensitive,
                                    _ => false
                                };
                            }
                            Some(FilterExpr::And(children)) => {
                                let new_children = children.clone();
                                *node.unwrap() = FilterExpr::Or(new_children);
                            }
                            Some(FilterExpr::Or(children)) => {
                                let new_children = children.clone();
                                *node.unwrap() = FilterExpr::And(new_children);
                            }
                            _ => {}
                        }
                    }
                    return None;
                }
                Action::DeleteFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let new_sel = remove_node_at(&mut self.root_expr, &self.selected_path);
                        self.selected_path = new_sel;
                    }
                    return None;
                }
                Action::AddFilterGroup => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let node = self.root_expr.get_mut(&self.selected_path);
                        match node {
                            Some(FilterExpr::And(_)) | Some(FilterExpr::Or(_)) => {
                                self.mode = FilterDialogMode::AddGroup;
                                self.add_insertion_path = Some(self.selected_path.clone());
                                self.add_group_and = true;
                            }
                            Some(FilterExpr::Condition(_)) => {
                                self.mode = FilterDialogMode::AddGroup;
                                self.add_insertion_path = Some(self.selected_path.clone());
                                self.add_group_and = true;
                            }
                            _ => {}
                        }
                    }
                    return None;
                }
                Action::SaveFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let browser = FileBrowserDialog::new(None, Some(vec!["json"]), false, FileBrowserMode::Save);
                        self.mode = FilterDialogMode::FileBrowser(browser);
                    }
                    return None;
                }
                Action::LoadFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let browser = FileBrowserDialog::new(None, Some(vec!["json"]), false, FileBrowserMode::Load);
                        self.mode = FilterDialogMode::FileBrowser(browser);
                    }
                    return None;
                }
                Action::ResetFilters => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        self.root_expr = FilterExpr::And(vec![]);
                        self.selected_path.clear();
                        self.scroll_offset = 0;
                    }
                    return None;
                }
                Action::ToggleFilterGroupType => {
                    if matches!(self.mode, FilterDialogMode::AddGroup) {
                        self.add_group_and = !self.add_group_and;
                    }
                    return None;
                }
                _ => {}
            }
        }

        match &mut self.mode {
            FilterDialogMode::List => {
                let mut lines = Vec::new();
                let mut path = Vec::new();
                self.root_expr.render_lines(&mut path, &self.selected_path, 0, &mut lines);
                let _idx = lines.iter().position(|(_, _, is_selected, _)| *is_selected).unwrap_or(0);

                // Fallback for any unhandled keys in List mode
            }
            FilterDialogMode::Add => {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(c) => {
                            if self.focus_field == FilterDialogField::Value {
                                self.add_value.push(c);
                            }
                        }
                        _ => {}
                    }
                }
            }
            FilterDialogMode::Edit(_idx) => {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(c) => {
                            if self.focus_field == FilterDialogField::Value {
                                self.add_value.push(c);
                            }
                        }
                        _ => {}
                    }
                }
            }
            FilterDialogMode::AddGroup => {
                // All AddGroup functionality handled by config actions
            }
            FilterDialogMode::FileBrowser(browser) => {
                if let Some(action) = browser.handle_key_event(key) {
                    match action {
                        FileBrowserAction::Selected(path) => {
                            match browser.mode {
                                FileBrowserMode::Save => {
                                    match self.save_to_file(&path) {
                                        Ok(_) => {
                                            self.mode = FilterDialogMode::List;
                                        }
                                        Err(e) => {
                                            self.mode = FilterDialogMode::List;
                                            error!("Failed to save filter: {}", e);
                                        }
                                    }
                                }
                                FileBrowserMode::Load => {
                                    match self.load_from_file(&path) {
                                        Ok(_) => {
                                            self.mode = FilterDialogMode::List;
                                        }
                                        Err(e) => {
                                            self.mode = FilterDialogMode::List;
                                            error!("Failed to load filter: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        FileBrowserAction::Cancelled => {
                            self.mode = FilterDialogMode::List;
                        }
                    }
                }
            }
        }
        None
    }

    /// Set the columns and the current column index for Add/Edit mode
    pub fn set_columns(&mut self, columns: Vec<String>, current_index: usize) {
        self.columns = columns;
        self.add_column_index = current_index.min(self.columns.len().saturating_sub(1));
    }

    /// Save the current filter expression to a file as JSON
    pub fn save_to_file(&self, path: &Path) -> color_eyre::Result<()> {
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, &self.root_expr)?;
        Ok(())
    }

    /// Load a filter expression from a file and set it as the current filter
    pub fn load_from_file(&mut self, path: &Path) -> color_eyre::Result<()> {
        let file = File::open(path)?;
        let expr: FilterExpr = serde_json::from_reader(file)?;
        self.set_root_expr(expr);
        Ok(())
    }

    /// Build instructions string from configured keybindings for Filter mode
    fn build_instructions_from_config(&self) -> String {
        use std::fmt::Write as _;
        fn fmt_key_event(key: &crossterm::event::KeyEvent) -> String {
            use crossterm::event::{KeyCode, KeyModifiers};
            let mut parts: Vec<&'static str> = Vec::with_capacity(3);
            if key.modifiers.contains(KeyModifiers::CONTROL) { parts.push("Ctrl"); }
            if key.modifiers.contains(KeyModifiers::ALT) { parts.push("Alt"); }
            if key.modifiers.contains(KeyModifiers::SHIFT) { parts.push("Shift"); }
            let key_part = match key.code {
                KeyCode::Char(' ') => "Space".to_string(),
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) { c.to_ascii_uppercase().to_string() } else { c.to_string() }
                }
                KeyCode::Left => "Left".to_string(),
                KeyCode::Right => "Right".to_string(),
                KeyCode::Up => "Up".to_string(),
                KeyCode::Down => "Down".to_string(),
                KeyCode::Enter => "Enter".to_string(),
                KeyCode::Esc => "Esc".to_string(),
                KeyCode::Tab => "Tab".to_string(),
                KeyCode::BackTab => "BackTab".to_string(),
                KeyCode::Delete => "Delete".to_string(),
                KeyCode::Insert => "Insert".to_string(),
                KeyCode::Home => "Home".to_string(),
                KeyCode::End => "End".to_string(),
                KeyCode::PageUp => "PageUp".to_string(),
                KeyCode::PageDown => "PageDown".to_string(),
                KeyCode::F(n) => format!("F{n}"),
                _ => "?".to_string(),
            };
            if parts.is_empty() { key_part } else { format!("{}+{}", parts.join("+"), key_part) }
        }

        fn fmt_sequence(seq: &[crossterm::event::KeyEvent]) -> String {
            let parts: Vec<String> = seq.iter().map(fmt_key_event).collect();
            parts.join(", ")
        }

        let mut segments: Vec<String> = Vec::new();

        match &self.mode {
            FilterDialogMode::List => {
                // Global actions for List mode
                if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
                    let global_actions: &[(Action, &str)] = &[
                        (Action::Up, "Move"),
                        (Action::Down, "Move"),
                        (Action::Left, "In"),
                        (Action::Right, "Out"),
                        (Action::Enter, "OK"),
                        (Action::Escape, "Cancel"),
                    ];

                    for (action, label) in global_actions {
                        let mut keys_for_action: Vec<&Vec<crossterm::event::KeyEvent>> = global_bindings
                            .iter()
                            .filter_map(|(seq, a)| if a == action { Some(seq) } else { None })
                            .collect();
                        keys_for_action.sort_by_key(|seq| seq.len());
                        if let Some(first) = keys_for_action.first() {
                            let key_text = fmt_sequence(first);
                            match action {
                                Action::Up | Action::Down => {
                                    if segments.iter().any(|s| s.contains("Move")) { continue; }
                                    segments.push(format!("{}/Down: {}", key_text.replace("Down", "Up"), label));
                                }
                                Action::Left | Action::Right => {
                                    if segments.iter().any(|s| s.contains("←/→:")) { continue; }
                                    segments.push("←/→:In/Out".to_string());
                                }
                                _ => segments.push(format!("{}: {}", key_text, label)),
                            }
                        }
                    }
                }

                // Filter-specific actions for List mode
                if let Some(filter_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Filter) {
                    let filter_actions: &[(Action, &str)] = &[
                        (Action::AddFilter, "Add"),
                        (Action::EditFilter, "Edit"),
                        (Action::DeleteFilter, "Del"),
                        (Action::AddFilterGroup, "Group"),
                        (Action::SaveFilter, "Save"),
                        (Action::LoadFilter, "Load"),
                        (Action::ResetFilters, "Reset"),
                    ];

                    for (action, label) in filter_actions {
                        let mut keys_for_action: Vec<&Vec<crossterm::event::KeyEvent>> = filter_bindings
                            .iter()
                            .filter_map(|(seq, a)| if a == action { Some(seq) } else { None })
                            .collect();
                        keys_for_action.sort_by_key(|seq| seq.len());
                        if let Some(first) = keys_for_action.first() {
                            let key_text = fmt_sequence(first);
                            segments.push(format!("{}: {}", key_text, label));
                        }
                    }
                }
            }
            FilterDialogMode::Add => {
                segments.push("Enter: OK".to_string());
                segments.push("Esc: Cancel".to_string());
            }
            FilterDialogMode::Edit(_) => {
                segments.push("Enter: OK".to_string());
                segments.push("Esc: Cancel".to_string());
            }
            FilterDialogMode::AddGroup => {
                if let Some(filter_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Filter) {
                    if let Some(tab_binding) = filter_bindings.iter().find(|(_, a)| **a == Action::ToggleFilterGroupType) {
                        let key_text = fmt_sequence(&tab_binding.0);
                        segments.push(format!("{}: Toggle AND/OR", key_text));
                    }
                }
                segments.push("Enter: OK".to_string());
                segments.push("Esc: Cancel".to_string());
            }
            FilterDialogMode::FileBrowser(_) => {
                segments.push("Enter: OK".to_string());
                segments.push("Esc: Cancel".to_string());
            }
        }

        // Join with double space for readability
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{}", seg);
        }
        out
    }
}

impl Component for FilterDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { self.config = _config; Ok(()) }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}

impl ColumnFilter {
    /// Format a filter as a summary string for the list
    pub fn summary(&self) -> String {
        let cs = match &self.condition {
            FilterCondition::Contains { case_sensitive, .. }
            | FilterCondition::Regex { case_sensitive, .. }
            | FilterCondition::Equals { case_sensitive, .. } => {
                if *case_sensitive { "[Aa]" } else { "[aA]" }
            },
            _ => ""
        };
        match &self.condition {
            FilterCondition::Contains { value, .. } => format!("{} contains \"{}\" {}", self.column, value, cs),
            FilterCondition::Regex { pattern, .. } => format!("{} matches /{}/ {}", self.column, pattern, cs),
            FilterCondition::Equals { value, .. } => format!("{} = \"{}\" {}", self.column, value, cs),
            FilterCondition::GreaterThan { value } => format!("{} > {}", self.column, value),
            FilterCondition::LessThan { value } => format!("{} < {}", self.column, value),
            FilterCondition::GreaterThanOrEqual { value } => format!("{} >= {}", self.column, value),
            FilterCondition::LessThanOrEqual { value } => format!("{} <= {}", self.column, value),
            FilterCondition::IsEmpty => format!("{} is empty", self.column),
            FilterCondition::IsNotEmpty => format!("{} is not empty", self.column),
            FilterCondition::NotNull => format!("{} is not null", self.column),
            FilterCondition::IsNull => format!("{} is null", self.column),
        }
    }

    /// Create a boolean mask for this filter condition
    pub fn create_mask(&self, df: &DataFrame) -> color_eyre::Result<BooleanChunked> {
        let column = df.column(&self.column)?;
        let column_type = column.dtype();
        
        // We have to handle the different types of columns differently.
        match &self.condition {
            FilterCondition::Contains { value, case_sensitive } => {
                match column_type {
                    DataType::String => {
                        if *case_sensitive {
                            column.str()?
                                .contains_literal(value)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        } else {
                            column.str()?
                                .to_lowercase()
                                .contains_literal(&value.to_lowercase())
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        }
                    }
                    DataType::Int32 | DataType::Int64 | DataType::Float32 | DataType::Float64 | DataType::Boolean => {
                        // Convert the integer column to string for contains matching
                        let str_series = column.cast(&DataType::String)?;
                        if *case_sensitive {
                           str_series.str()?
                                .contains_literal(value)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        } else {
                            str_series.str()?
                                .to_lowercase()
                                .contains_literal(&value.to_lowercase()) 
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        }
                    }
                    _ => {
                        // TODO: Handle other types of columns
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            }
            FilterCondition::NotNull => {
                // For any dtype, return mask of not-null values
                Ok(column.is_not_null())
            }
            FilterCondition::IsNull => {
                // For any dtype, return mask of null values
                Ok(column.is_null())
            }
            FilterCondition::IsEmpty => {
                // For any dtype, return mask of empty values
                Ok( BooleanChunked::full("".into(), true, df.height()))
            }
            FilterCondition::IsNotEmpty => {
                // For any dtype, return mask of not-empty values
                Ok( BooleanChunked::full("".into(), false, df.height()))
            }
            FilterCondition::Regex { pattern, case_sensitive } => {
                match column_type {
                    DataType::String => {
                        if *case_sensitive {
                            column.str()?
                                .contains(pattern, false)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        } else {
                            column.str()?
                                .to_lowercase()
                                .contains(pattern, true)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        }
                    }
                    DataType::Int32 | DataType::Int64 | DataType::Float32 | DataType::Float64 | DataType::Boolean => {
                        // Convert the column to string for regex matching
                        let str_series = column.cast(&DataType::String)?;
                        if *case_sensitive {
                            str_series.str()?
                                .contains(pattern, false)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        } else {
                            str_series.str()?
                                .to_lowercase()
                                .contains(pattern, true)
                                .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
                        }
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            }
            FilterCondition::Equals { value, case_sensitive } => {
                match column_type {
                    DataType::String => {
                        if *case_sensitive {
                            Ok(column.str()?.equal(value.as_str()))
                        } else {
                            let value_lc = value.to_lowercase();
                            Ok(column.str()?.to_lowercase().equal(value_lc.as_str()))
                        }
                    }
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let numeric_value = value.parse::<i128>().unwrap();
                        let series = column.cast(&DataType::Int128)?;
                        Ok(series.i64()?
                            .equal(numeric_value))
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let numeric_value = value.parse::<u64>().unwrap();
                        let series = column.cast(&DataType::UInt64)?;
                        Ok(series.u64()?
                            .equal(numeric_value))
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let numeric_value = value.parse::<f64>().unwrap();
                        let series = column.cast(&DataType::Float64)?;
                        Ok(series.f64()?
                            .equal(numeric_value))
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            },
            FilterCondition::GreaterThan { value } => {
                match column_type {
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let numeric_value = value.parse::<i64>().unwrap();
                        Ok(column.i64()?
                            .gt(numeric_value))
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let numeric_value = value.parse::<u64>().unwrap();
                        Ok(column.u64()?
                            .gt(numeric_value))
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let numeric_value = value.parse::<f64>().unwrap();
                        Ok(column.f64()?
                            .gt(numeric_value))
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            },
            FilterCondition::LessThan { value } => {
                match column_type {
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let numeric_value = value.parse::<i64>().unwrap();
                        Ok(column.i64()?
                            .lt(numeric_value))
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let numeric_value = value.parse::<u64>().unwrap();
                        Ok(column.u64()?
                            .lt(numeric_value))
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let numeric_value = value.parse::<f64>().unwrap();
                        Ok(column.f64()?
                            .lt(numeric_value))
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            },
            FilterCondition::GreaterThanOrEqual { value } => {
                match column_type {
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let numeric_value = value.parse::<i64>().unwrap();
                        Ok(column.i64()?
                            .gt_eq(numeric_value))
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let numeric_value = value.parse::<u64>().unwrap();
                        Ok(column.u64()?
                            .gt_eq(numeric_value))
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let numeric_value = value.parse::<f64>().unwrap();
                        Ok(column.f64()?
                            .gt_eq(numeric_value))
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            },
            FilterCondition::LessThanOrEqual { value } => {
                match column_type {
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let numeric_value = value.parse::<i64>().unwrap();
                        Ok(column.i64()?
                            .lt_eq(numeric_value))
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let numeric_value = value.parse::<u64>().unwrap();
                        Ok(column.u64()?
                            .lt_eq(numeric_value))
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let numeric_value = value.parse::<f64>().unwrap();
                        Ok(column.f64()?
                            .lt_eq(numeric_value))
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type: {}", column_type))
                    }
                }
            },
        }
    }
}

impl FilterExpr {
    /// Recursively render the filter tree as lines of (indent, label, is_selected)
    pub fn render_lines(&self, path: &mut Vec<usize>, selected_path: &[usize], indent: usize, lines: &mut Vec<(usize, String, bool, Vec<usize>)>) {
        let is_selected = path == selected_path;
        match self {
            FilterExpr::Condition(cond) => {
                lines.push((indent, cond.summary(), is_selected, path.clone()));
            }
            FilterExpr::And(children) => {
                let label = if indent == 0 { "Root AND".to_string() } else { "AND".to_string() };
                lines.push((indent, label, is_selected, path.clone()));
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    child.render_lines(path, selected_path, indent + 1, lines);
                    path.pop();
                }
            }
            FilterExpr::Or(children) => {
                lines.push((indent, "OR".to_string(), is_selected, path.clone()));
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    child.render_lines(path, selected_path, indent + 1, lines);
                    path.pop();
                }
            }
        }
    }

    /// Create a boolean mask for this filter expression
    pub fn create_mask(&self, df: &DataFrame) -> color_eyre::Result<BooleanChunked> {
        match self {
            FilterExpr::Condition(filter) => {
                filter.create_mask(df)
                    .map_err(|e| color_eyre::eyre::eyre!("Filter error: {}", e))
            }
            FilterExpr::And(children) => {
                if children.is_empty() {
                    // Empty AND returns all true (no filtering)
                    Ok(BooleanChunked::full("".into(), true, df.height()))
                } else {
                    // Combine all child masks with AND
                    let mut result = children[0].create_mask(df)?;
                    for child in &children[1..] {
                        let child_mask = child.create_mask(df)?;
                        result = result & child_mask;
                    }
                    Ok(result)
                }
            }
            FilterExpr::Or(children) => {
                if children.is_empty() {
                    // Empty OR returns all false (no rows match)
                    Ok(BooleanChunked::full("".into(), false, df.height()))
                } else {
                    // Combine all child masks with OR
                    let mut result = children[0].create_mask(df)?;
                    for child in &children[1..] {
                        let child_mask = child.create_mask(df)?;
                        result = result | child_mask;
                    }
                    Ok(result)
                }
            }
        }
    }

    /// Get mutable reference to node at path, or None if invalid
    pub fn get_mut(&mut self, path: &[usize]) -> Option<&mut FilterExpr> {
        let mut node = self;
        for &i in path {
            match node {
                FilterExpr::And(children) | FilterExpr::Or(children) => node = children.get_mut(i)?,
                FilterExpr::Condition(_) => return None,
            }
        }
        Some(node)
    }
    /// Get number of children if group, else 0
    pub fn child_count(&self) -> usize {
        match self {
            FilterExpr::And(children) | FilterExpr::Or(children) => children.len(),
            FilterExpr::Condition(_) => 0,
        }
    }
}

// Helper: get parent path of a node
fn parent_path_of(path: &[usize]) -> Option<Vec<usize>> {
    if path.is_empty() { None } else { Some(path[..path.len()-1].to_vec()) }
}

// Helper: insert a condition at a path (as child or sibling)
fn insert_condition_at(expr: &mut FilterExpr, path: &[usize], new_node: FilterExpr) {
    if path.is_empty() {
        // Insert at root (should be group)
        if let FilterExpr::And(children) | FilterExpr::Or(children) = expr {
            children.push(new_node);
        }
    } else {
        let (head, tail) = path.split_first().unwrap();
        match expr {
            FilterExpr::And(children) | FilterExpr::Or(children) => {
                if tail.is_empty() {
                    // If group is empty or index is out of bounds, push as child
                    if *head >= children.len() {
                        children.push(new_node);
                    } else {
                        children.insert(*head, new_node);
                    }
                } else if let Some(child) = children.get_mut(*head) {
                    insert_condition_at(child, tail, new_node);
                }
            }
            _ => {}
        }
    }
}

// Helper: replace a condition at a path
fn replace_condition_at(expr: &mut FilterExpr, path: &[usize], new_node: FilterExpr) {
    if path.is_empty() {
        *expr = new_node;
    } else {
        let (head, tail) = path.split_first().unwrap();
        match expr {
            FilterExpr::And(children) | FilterExpr::Or(children) => {
                if tail.is_empty() {
                    children[*head] = new_node;
                } else if let Some(child) = children.get_mut(*head) {
                    replace_condition_at(child, tail, new_node);
                }
            }
            _ => {}
        }
    }
}

// Remove node at path, return new selection path
fn remove_node_at(expr: &mut FilterExpr, path: &[usize]) -> Vec<usize> {
    if path.is_empty() {
        // Don't remove root
        return vec![];
    }
    let (head, tail) = path.split_first().unwrap();
    match expr {
        FilterExpr::And(children) | FilterExpr::Or(children) => {
            if tail.is_empty() {
                let idx = *head;
                children.remove(idx);
                // Select previous sibling, next sibling, or parent
                if idx > 0 {
                    let mut new_path = path.to_vec();
                    *new_path.last_mut().unwrap() -= 1;
                    new_path
                } else if !children.is_empty() {
                    let mut new_path = path.to_vec();
                    *new_path.last_mut().unwrap() = 0;
                    new_path
                } else {
                    parent_path_of(path).unwrap_or_default()
                }
            } else if let Some(child) = children.get_mut(*head) {
                remove_node_at(child, tail)
            } else {
                parent_path_of(path).unwrap_or_default()
            }
        }
        _ => parent_path_of(path).unwrap_or_default(),
    }
} 