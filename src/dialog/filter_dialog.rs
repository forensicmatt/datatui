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
#[cfg_attr(feature = "json_schema", derive(schemars::JsonSchema))]
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
    // Phase 1: New condition types
    /// Range check: value between min and max
    Between { min: String, max: String, inclusive: bool },
    /// Set membership: value in list of values
    InList { values: Vec<String>, case_sensitive: bool },
    /// Negation: NOT condition
    Not(Box<FilterCondition>),
    /// Phase 2: Column comparison
    CompareColumns { other_column: String, operator: CompareOp },
    /// Phase 2: String length check
    StringLength { operator: CompareOp, length: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ConditionKind {
    Contains,
    Regex,
    Equals,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    IsEmpty,
    IsNull,
    IsNotEmpty,
    NotNull,
    Between,
    InList,
    CompareColumns,
    StringLength,
}

const CONDITION_CYCLE: [ConditionKind; 15] = [
    ConditionKind::Contains,
    ConditionKind::StringLength,
    ConditionKind::CompareColumns,
    ConditionKind::InList,
    ConditionKind::Between,
    ConditionKind::Regex,
    ConditionKind::Equals,
    ConditionKind::GreaterThan,
    ConditionKind::GreaterThanOrEqual,
    ConditionKind::LessThan,
    ConditionKind::LessThanOrEqual,
    ConditionKind::IsEmpty,
    ConditionKind::IsNull,
    ConditionKind::IsNotEmpty,
    ConditionKind::NotNull,
];

fn condition_cycle() -> &'static [ConditionKind] {
    &CONDITION_CYCLE
}

fn condition_kind(condition: Option<&FilterCondition>) -> ConditionKind {
    match condition {
        Some(FilterCondition::Contains { .. }) | Some(FilterCondition::Not(_)) | None => ConditionKind::Contains,
        Some(FilterCondition::Regex { .. }) => ConditionKind::Regex,
        Some(FilterCondition::Equals { .. }) => ConditionKind::Equals,
        Some(FilterCondition::GreaterThan { .. }) => ConditionKind::GreaterThan,
        Some(FilterCondition::LessThan { .. }) => ConditionKind::LessThan,
        Some(FilterCondition::GreaterThanOrEqual { .. }) => ConditionKind::GreaterThanOrEqual,
        Some(FilterCondition::LessThanOrEqual { .. }) => ConditionKind::LessThanOrEqual,
        Some(FilterCondition::IsEmpty) => ConditionKind::IsEmpty,
        Some(FilterCondition::IsNotEmpty) => ConditionKind::IsNotEmpty,
        Some(FilterCondition::NotNull) => ConditionKind::NotNull,
        Some(FilterCondition::IsNull) => ConditionKind::IsNull,
        Some(FilterCondition::Between { .. }) => ConditionKind::Between,
        Some(FilterCondition::InList { .. }) => ConditionKind::InList,
        Some(FilterCondition::CompareColumns { .. }) => ConditionKind::CompareColumns,
        Some(FilterCondition::StringLength { .. }) => ConditionKind::StringLength,
    }
}

fn next_kind(kind: ConditionKind) -> ConditionKind {
    let cycle = condition_cycle();
    let idx = cycle.iter().position(|k| *k == kind).unwrap_or(0);
    cycle[(idx + 1) % cycle.len()]
}

fn prev_kind(kind: ConditionKind) -> ConditionKind {
    let cycle = condition_cycle();
    let idx = cycle.iter().position(|k| *k == kind).unwrap_or(0);
    if idx == 0 {
        cycle[cycle.len() - 1]
    } else {
        cycle[idx - 1]
    }
}

/// Comparison operator for advanced conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json_schema", derive(schemars::JsonSchema))]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Lte,
    Gte,
}

/// Filter applied to a column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json_schema", derive(schemars::JsonSchema))]
pub struct ColumnFilter {
    pub column: String,
    pub condition: FilterCondition,
}

/// Recursive filter expression: single condition or AND/OR group
#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "json_schema", derive(schemars::JsonSchema))]
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
    FileBrowser(Box<FileBrowserDialog>), // new: for save/load
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
    // Free column mode: allows typing a JMESPath query for the column
    pub enabled_free_column: bool,
    pub add_column_text: String, // stores the free-typed column (JMESPath query)
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
            enabled_free_column: false,
            add_column_text: String::new(),
        }
    }

    /// Enable free column mode, allowing the user to type a JMESPath query for the column
    pub fn with_free_column(mut self) -> Self {
        self.enabled_free_column = true;
        self
    }

    /// Set free column mode, allowing the user to type a JMESPath query for the column
    pub fn set_free_column(&mut self, enabled: bool) {
        self.enabled_free_column = enabled;
    }

    fn condition_from_kind(&self, kind: ConditionKind) -> FilterCondition {
        match kind {
            ConditionKind::Contains => FilterCondition::Contains { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
            ConditionKind::Regex => FilterCondition::Regex { pattern: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
            ConditionKind::Equals => FilterCondition::Equals { value: self.add_value.clone(), case_sensitive: self.add_case_sensitive },
            ConditionKind::GreaterThan => FilterCondition::GreaterThan { value: self.add_value.clone() },
            ConditionKind::GreaterThanOrEqual => FilterCondition::GreaterThanOrEqual { value: self.add_value.clone() },
            ConditionKind::LessThan => FilterCondition::LessThan { value: self.add_value.clone() },
            ConditionKind::LessThanOrEqual => FilterCondition::LessThanOrEqual { value: self.add_value.clone() },
            ConditionKind::IsEmpty => FilterCondition::IsEmpty,
            ConditionKind::IsNull => FilterCondition::IsNull,
            ConditionKind::IsNotEmpty => FilterCondition::IsNotEmpty,
            ConditionKind::NotNull => FilterCondition::NotNull,
            ConditionKind::Between => FilterCondition::Between { min: String::new(), max: String::new(), inclusive: true },
            ConditionKind::InList => FilterCondition::InList { values: vec![], case_sensitive: self.add_case_sensitive },
            ConditionKind::CompareColumns => FilterCondition::CompareColumns { other_column: String::new(), operator: CompareOp::Eq },
            ConditionKind::StringLength => FilterCondition::StringLength { operator: CompareOp::Eq, length: 0 },
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
        self.add_column_text.clear();
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
                let col_label = if self.enabled_free_column {
                    format!("Column: {}", self.add_column_text)
                } else {
                    format!(
                        "Column: {}",
                        self.columns.get(self.add_column_index)
                            .unwrap_or(&"".to_string())
                    )
                };
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
                    Some(FilterCondition::Between { .. }) => "Between",
                    Some(FilterCondition::InList { .. }) => "In List",
                    Some(FilterCondition::Not(_)) => "Not",
                    Some(FilterCondition::CompareColumns { .. }) => "Compare Columns",
                    Some(FilterCondition::StringLength { .. }) => "String Length",
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
                let col_label = if self.enabled_free_column {
                    format!("Column: {}", self.add_column_text)
                } else {
                    format!("Column: {}", self.columns.get(self.add_column_index).unwrap_or(&"".to_string()))
                };
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
                    Some(FilterCondition::Between { .. }) => "Between",
                    Some(FilterCondition::InList { .. }) => "In List",
                    Some(FilterCondition::Not(_)) => "Not",
                    Some(FilterCondition::CompareColumns { .. }) => "Compare Columns",
                    Some(FilterCondition::StringLength { .. }) => "String Length",
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
        use crossterm::event::KeyCode;
        
        // Handle FileBrowser mode first - if file browser is open, pass keys to it
        if let FilterDialogMode::FileBrowser(browser) = &mut self.mode {
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
            return None;
        }
        
        // First, honor config-driven actions (Global + Filter)
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    match &self.mode {
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) | FilterDialogMode::AddGroup => {
                            self.mode = FilterDialogMode::List;
                            self.add_insertion_path = None;
                            return None;
                        }
                        _ => {
                            return Some(Action::DialogClose);
                        }
                    }
                }
                Action::Enter => {
                    match &self.mode {
                        FilterDialogMode::List => {
                            return Some(Action::FilterDialogApplied(self.root_expr.clone()));
                        }
                        FilterDialogMode::Add | FilterDialogMode::Edit(_) => {
                            let column = if self.enabled_free_column {
                                self.add_column_text.clone()
                            } else {
                                self.columns.get(self.add_column_index).cloned().unwrap_or_default()
                            };
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
                                // Phase 1: New conditions - parse value as needed
                                FilterCondition::Between { inclusive, .. } => {
                                    // Parse value as "min,max" or use defaults
                                    let parts: Vec<&str> = self.add_value.split(',').collect();
                                    let min = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();
                                    let max = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                                    FilterCondition::Between { min, max, inclusive }
                                },
                                FilterCondition::InList { case_sensitive, .. } => {
                                    // Parse value as comma-separated list
                                    let values: Vec<String> = self.add_value.split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                    FilterCondition::InList { values, case_sensitive }
                                },
                                FilterCondition::Not(inner) => FilterCondition::Not(inner),
                                // Phase 2: Advanced conditions
                                FilterCondition::CompareColumns { operator, .. } => {
                                    FilterCondition::CompareColumns { other_column: self.add_value.clone(), operator }
                                },
                                FilterCondition::StringLength { operator, .. } => {
                                    let length = self.add_value.parse().unwrap_or(0);
                                    FilterCondition::StringLength { operator, length }
                                },
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
                                    // When cycling columns, also update free column text
                                    if self.enabled_free_column {
                                        if let Some(col) = self.columns.get(self.add_column_index) {
                                            self.add_column_text = col.clone();
                                        }
                                    }
                                }
                                FilterDialogField::Type => {
                                    let current = condition_kind(self.add_condition.as_ref());
                                    let previous = prev_kind(current);
                                    self.add_condition = Some(self.condition_from_kind(previous));
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
                                    // When cycling columns, also update free column text
                                    if self.enabled_free_column {
                                        if let Some(col) = self.columns.get(self.add_column_index) {
                                            self.add_column_text = col.clone();
                                        }
                                    }
                                }
                                FilterDialogField::Type => {
                                    let current = condition_kind(self.add_condition.as_ref());
                                    let next = next_kind(current);
                                    self.add_condition = Some(self.condition_from_kind(next));
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
                        match self.focus_field {
                            FilterDialogField::Value => {
                                self.add_value.pop();
                            }
                            FilterDialogField::Column if self.enabled_free_column => {
                                self.add_column_text.pop();
                            }
                            _ => {}
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
                                self.add_column_text.clear();
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
                                    self.add_column_text.clear();
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
                                self.add_column_text = col_filter.column.clone();
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
                                    FilterCondition::Between { min, max, .. } => format!("{},{}", min, max),
                                    FilterCondition::InList { values, .. } => values.join(","),
                                    FilterCondition::Not(_) => "".to_string(),
                                    FilterCondition::CompareColumns { other_column, .. } => other_column.clone(),
                                    FilterCondition::StringLength { length, .. } => length.to_string(),
                                };
                                self.add_case_sensitive = match &col_filter.condition {
                                    FilterCondition::Contains { case_sensitive, .. }
                                    | FilterCondition::Regex { case_sensitive, .. }
                                    | FilterCondition::Equals { case_sensitive, .. }
                                    | FilterCondition::InList { case_sensitive, .. } => *case_sensitive,
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
                        let mut browser = FileBrowserDialog::new(None, Some(vec!["json"]), false, FileBrowserMode::Save);
                        browser.register_config_handler(self.config.clone());
                        self.mode = FilterDialogMode::FileBrowser(Box::new(browser));
                    }
                    return None;
                }
                Action::LoadFilter => {
                    if matches!(self.mode, FilterDialogMode::List) {
                        let mut browser = FileBrowserDialog::new(None, Some(vec!["json"]), false, FileBrowserMode::Load);
                        browser.register_config_handler(self.config.clone());
                        self.mode = FilterDialogMode::FileBrowser(Box::new(browser));
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
                if key.kind == KeyEventKind::Press
                    && let KeyCode::Char(c) = key.code {
                        match self.focus_field {
                            FilterDialogField::Value => {
                                self.add_value.push(c);
                            }
                            FilterDialogField::Column if self.enabled_free_column => {
                                self.add_column_text.push(c);
                            }
                            _ => {}
                        }
                    }
            }
            FilterDialogMode::Edit(_idx) => {
                if key.kind == KeyEventKind::Press
                    && let KeyCode::Char(c) = key.code {
                        match self.focus_field {
                            FilterDialogField::Value => {
                                self.add_value.push(c);
                            }
                            FilterDialogField::Column if self.enabled_free_column => {
                                self.add_column_text.push(c);
                            }
                            _ => {}
                        }
                    }
            }
            FilterDialogMode::AddGroup => {
                // All AddGroup functionality handled by config actions above
            }
            FilterDialogMode::FileBrowser(_) => {
                // FileBrowser mode is handled at the top of the function
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
        match &self.mode {
            FilterDialogMode::List => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::Global, crate::action::Action::Enter),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                    (crate::config::Mode::Filter, crate::action::Action::AddFilter),
                    (crate::config::Mode::Filter, crate::action::Action::EditFilter),
                    (crate::config::Mode::Filter, crate::action::Action::DeleteFilter),
                    (crate::config::Mode::Filter, crate::action::Action::AddFilterGroup),
                    (crate::config::Mode::Filter, crate::action::Action::SaveFilter),
                    (crate::config::Mode::Filter, crate::action::Action::LoadFilter),
                    (crate::config::Mode::Filter, crate::action::Action::ResetFilters),
                ])
            }
            FilterDialogMode::Add => {
                "Enter: OK  Esc: Cancel".to_string()
            }
            FilterDialogMode::Edit(_) => {
                "Enter: OK  Esc: Cancel".to_string()
            }
            FilterDialogMode::AddGroup => {
                let instructions = self.config.actions_to_instructions(&[
                    (crate::config::Mode::Filter, crate::action::Action::ToggleFilterGroupType),
                    (crate::config::Mode::Global, crate::action::Action::Enter),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ]);
                if instructions.is_empty() {
                    "Enter: OK  Esc: Cancel".to_string()
                } else {
                    format!("{instructions}  Enter: OK  Esc: Cancel")
                }
            }
            FilterDialogMode::FileBrowser(_) => {
                "Enter: OK  Esc: Cancel".to_string()
            }
        }
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
            | FilterCondition::Equals { case_sensitive, .. }
            | FilterCondition::InList { case_sensitive, .. } => {
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
            // Phase 1: New conditions
            FilterCondition::Between { min, max, inclusive } => {
                let op = if *inclusive { "between" } else { "between (exclusive)" };
                format!("{} {} {} and {}", self.column, op, min, max)
            },
            FilterCondition::InList { values, .. } => {
                let display_values = if values.len() > 3 {
                    format!("{}, {}... ({} total)", values[0], values[1], values.len())
                } else {
                    values.join(", ")
                };
                format!("{} in [{}] {}", self.column, display_values, cs)
            },
            FilterCondition::Not(inner) => {
                // Create a temporary ColumnFilter to get inner summary
                let inner_filter = ColumnFilter {
                    column: self.column.clone(),
                    condition: (**inner).clone(),
                };
                format!("NOT ({})", inner_filter.summary())
            },
            // Phase 2: Advanced conditions
            FilterCondition::CompareColumns { other_column, operator } => {
                let op_str = match operator {
                    CompareOp::Eq => "=",
                    CompareOp::Ne => "≠",
                    CompareOp::Lt => "<",
                    CompareOp::Gt => ">",
                    CompareOp::Lte => "≤",
                    CompareOp::Gte => "≥",
                };
                format!("{} {} {}", self.column, op_str, other_column)
            },
            FilterCondition::StringLength { operator, length } => {
                let op_str = match operator {
                    CompareOp::Eq => "=",
                    CompareOp::Ne => "≠",
                    CompareOp::Lt => "<",
                    CompareOp::Gt => ">",
                    CompareOp::Lte => "≤",
                    CompareOp::Gte => "≥",
                };
                format!("len({}) {} {}", self.column, op_str, length)
            },
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
            // Phase 1: New conditions
            FilterCondition::Between { min, max, inclusive } => {
                match column_type {
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let min_val = min.parse::<i64>().map_err(|_| color_eyre::eyre::eyre!("Invalid min value: {}", min))?;
                        let max_val = max.parse::<i64>().map_err(|_| color_eyre::eyre::eyre!("Invalid max value: {}", max))?;
                        let col = column.i64()?;
                        if *inclusive {
                            Ok(col.gt_eq(min_val) & col.lt_eq(max_val))
                        } else {
                            Ok(col.gt(min_val) & col.lt(max_val))
                        }
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let min_val = min.parse::<u64>().map_err(|_| color_eyre::eyre::eyre!("Invalid min value: {}", min))?;
                        let max_val = max.parse::<u64>().map_err(|_| color_eyre::eyre::eyre!("Invalid max value: {}", max))?;
                        let col = column.u64()?;
                        if *inclusive {
                            Ok(col.gt_eq(min_val) & col.lt_eq(max_val))
                        } else {
                            Ok(col.gt(min_val) & col.lt(max_val))
                        }
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let min_val = min.parse::<f64>().map_err(|_| color_eyre::eyre::eyre!("Invalid min value: {}", min))?;
                        let max_val = max.parse::<f64>().map_err(|_| color_eyre::eyre::eyre!("Invalid max value: {}", max))?;
                        let col = column.f64()?;
                        if *inclusive {
                            Ok(col.gt_eq(min_val) & col.lt_eq(max_val))
                        } else {
                            Ok(col.gt(min_val) & col.lt(max_val))
                        }
                    }
                    DataType::String => {
                        // String comparison for between
                        let col = column.str()?;
                        let min_str = min.as_str();
                        let max_str = max.as_str();
                        if *inclusive {
                            Ok(col.gt_eq(min_str) & col.lt_eq(max_str))
                        } else {
                            Ok(col.gt(min_str) & col.lt(max_str))
                        }
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type for Between: {}", column_type))
                    }
                }
            },
            FilterCondition::InList { values, case_sensitive } => {
                match column_type {
                    DataType::String => {
                        let col = column.str()?;
                        if *case_sensitive {
                            // Create a mask for each value and OR them together
                            let mut result = BooleanChunked::full("".into(), false, df.height());
                            for v in values {
                                result = result | col.equal(v.as_str());
                            }
                            Ok(result)
                        } else {
                            let col_lower = col.to_lowercase();
                            let mut result = BooleanChunked::full("".into(), false, df.height());
                            for v in values {
                                result = result | col_lower.equal(v.to_lowercase().as_str());
                            }
                            Ok(result)
                        }
                    }
                    DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        let col = column.i64()?;
                        let mut result = BooleanChunked::full("".into(), false, df.height());
                        for v in values {
                            if let Ok(num) = v.parse::<i64>() {
                                result = result | col.equal(num);
                            }
                        }
                        Ok(result)
                    }
                    DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        let col = column.u64()?;
                        let mut result = BooleanChunked::full("".into(), false, df.height());
                        for v in values {
                            if let Ok(num) = v.parse::<u64>() {
                                result = result | col.equal(num);
                            }
                        }
                        Ok(result)
                    }
                    DataType::Float32 | DataType::Float64 => {
                        let col = column.f64()?;
                        let mut result = BooleanChunked::full("".into(), false, df.height());
                        for v in values {
                            if let Ok(num) = v.parse::<f64>() {
                                result = result | col.equal(num);
                            }
                        }
                        Ok(result)
                    }
                    _ => {
                        Err(color_eyre::eyre::eyre!("Unsupported column type for InList: {}", column_type))
                    }
                }
            },
            FilterCondition::Not(inner) => {
                // Create a temporary ColumnFilter with the inner condition
                let inner_filter = ColumnFilter {
                    column: self.column.clone(),
                    condition: (**inner).clone(),
                };
                let inner_mask = inner_filter.create_mask(df)?;
                Ok(!inner_mask)
            },
            // Phase 2: Advanced conditions
            FilterCondition::CompareColumns { other_column, operator } => {
                let other = df.column(other_column)?;
                
                // Try numeric comparison first
                if let (Ok(col_f64), Ok(other_f64)) = (column.cast(&DataType::Float64), other.cast(&DataType::Float64)) {
                    let col = col_f64.f64()?;
                    let other = other_f64.f64()?;
                    match operator {
                        CompareOp::Eq => Ok(col.equal(other)),
                        CompareOp::Ne => Ok(col.not_equal(other)),
                        CompareOp::Lt => Ok(col.lt(other)),
                        CompareOp::Gt => Ok(col.gt(other)),
                        CompareOp::Lte => Ok(col.lt_eq(other)),
                        CompareOp::Gte => Ok(col.gt_eq(other)),
                    }
                } else {
                    // Fall back to string comparison
                    let col = column.cast(&DataType::String)?;
                    let other = other.cast(&DataType::String)?;
                    let col_str = col.str()?;
                    let other_str = other.str()?;
                    match operator {
                        CompareOp::Eq => Ok(col_str.equal(other_str)),
                        CompareOp::Ne => Ok(col_str.not_equal(other_str)),
                        CompareOp::Lt => Ok(col_str.lt(other_str)),
                        CompareOp::Gt => Ok(col_str.gt(other_str)),
                        CompareOp::Lte => Ok(col_str.lt_eq(other_str)),
                        CompareOp::Gte => Ok(col_str.gt_eq(other_str)),
                    }
                }
            },
            FilterCondition::StringLength { operator, length } => {
                let col = column.cast(&DataType::String)?;
                let str_col = col.str()?;
                let lengths = str_col.str_len_chars();
                let len_val = *length as u32;
                match operator {
                    CompareOp::Eq => Ok(lengths.equal(len_val)),
                    CompareOp::Ne => Ok(lengths.not_equal(len_val)),
                    CompareOp::Lt => Ok(lengths.lt(len_val)),
                    CompareOp::Gt => Ok(lengths.gt(len_val)),
                    CompareOp::Lte => Ok(lengths.lt_eq(len_val)),
                    CompareOp::Gte => Ok(lengths.gt_eq(len_val)),
                }
            },
        }
    }

    /// Evaluate this filter condition against a single row's data
    /// row_data: Map of column names to string values
    pub fn evaluate_row(&self, row_data: &std::collections::BTreeMap<String, String>) -> color_eyre::Result<bool> {
        let cell_value = row_data.get(&self.column)
            .map(|s| s.as_str())
            .unwrap_or("");

        match &self.condition {
            FilterCondition::Contains { value, case_sensitive } => {
                if *case_sensitive {
                    Ok(cell_value.contains(value))
                } else {
                    Ok(cell_value.to_lowercase().contains(&value.to_lowercase()))
                }
            }
            FilterCondition::Regex { pattern, case_sensitive } => {
                use regex::Regex;
                let re = if *case_sensitive {
                    Regex::new(pattern)
                } else {
                    Regex::new(&format!("(?i){}", pattern))
                }.map_err(|e| color_eyre::eyre::eyre!("Invalid regex pattern: {}", e))?;
                Ok(re.is_match(cell_value))
            }
            FilterCondition::Equals { value, case_sensitive } => {
                if *case_sensitive {
                    Ok(cell_value == value)
                } else {
                    Ok(cell_value.to_lowercase() == value.to_lowercase())
                }
            }
            FilterCondition::GreaterThan { value } => {
                // Try to parse as number and compare
                if let Ok(cell_num) = cell_value.parse::<f64>() {
                    if let Ok(val_num) = value.parse::<f64>() {
                        return Ok(cell_num > val_num);
                    }
                }
                // Fall back to string comparison
                Ok(cell_value > value.as_str())
            }
            FilterCondition::LessThan { value } => {
                // Try to parse as number and compare
                if let Ok(cell_num) = cell_value.parse::<f64>() {
                    if let Ok(val_num) = value.parse::<f64>() {
                        return Ok(cell_num < val_num);
                    }
                }
                // Fall back to string comparison
                Ok(cell_value < value.as_str())
            }
            FilterCondition::GreaterThanOrEqual { value } => {
                // Try to parse as number and compare
                if let Ok(cell_num) = cell_value.parse::<f64>() {
                    if let Ok(val_num) = value.parse::<f64>() {
                        return Ok(cell_num >= val_num);
                    }
                }
                // Fall back to string comparison
                Ok(cell_value >= value.as_str())
            }
            FilterCondition::LessThanOrEqual { value } => {
                // Try to parse as number and compare
                if let Ok(cell_num) = cell_value.parse::<f64>() {
                    if let Ok(val_num) = value.parse::<f64>() {
                        return Ok(cell_num <= val_num);
                    }
                }
                // Fall back to string comparison
                Ok(cell_value <= value.as_str())
            }
            FilterCondition::IsEmpty => {
                Ok(cell_value.is_empty())
            }
            FilterCondition::IsNotEmpty => {
                Ok(!cell_value.is_empty())
            }
            FilterCondition::NotNull => {
                Ok(!cell_value.is_empty() && cell_value != "null" && cell_value != "NULL")
            }
            FilterCondition::IsNull => {
                Ok(cell_value.is_empty() || cell_value == "null" || cell_value == "NULL")
            }
            // Phase 1: New conditions
            FilterCondition::Between { min, max, inclusive } => {
                // Try numeric comparison first
                if let Ok(cell_num) = cell_value.parse::<f64>() {
                    if let (Ok(min_num), Ok(max_num)) = (min.parse::<f64>(), max.parse::<f64>()) {
                        return if *inclusive {
                            Ok(cell_num >= min_num && cell_num <= max_num)
                        } else {
                            Ok(cell_num > min_num && cell_num < max_num)
                        };
                    }
                }
                // Fall back to string comparison
                if *inclusive {
                    Ok(cell_value >= min.as_str() && cell_value <= max.as_str())
                } else {
                    Ok(cell_value > min.as_str() && cell_value < max.as_str())
                }
            }
            FilterCondition::InList { values, case_sensitive } => {
                if *case_sensitive {
                    Ok(values.iter().any(|v| v == cell_value))
                } else {
                    let cell_lower = cell_value.to_lowercase();
                    Ok(values.iter().any(|v| v.to_lowercase() == cell_lower))
                }
            }
            FilterCondition::Not(inner) => {
                let inner_filter = ColumnFilter {
                    column: self.column.clone(),
                    condition: (**inner).clone(),
                };
                let inner_result = inner_filter.evaluate_row(row_data)?;
                Ok(!inner_result)
            }
            // Phase 2: Advanced conditions
            FilterCondition::CompareColumns { other_column, operator } => {
                let other_value = row_data.get(other_column)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                
                // Try numeric comparison first
                if let (Ok(cell_num), Ok(other_num)) = (cell_value.parse::<f64>(), other_value.parse::<f64>()) {
                    return match operator {
                        CompareOp::Eq => Ok((cell_num - other_num).abs() < f64::EPSILON),
                        CompareOp::Ne => Ok((cell_num - other_num).abs() >= f64::EPSILON),
                        CompareOp::Lt => Ok(cell_num < other_num),
                        CompareOp::Gt => Ok(cell_num > other_num),
                        CompareOp::Lte => Ok(cell_num <= other_num),
                        CompareOp::Gte => Ok(cell_num >= other_num),
                    };
                }
                // Fall back to string comparison
                match operator {
                    CompareOp::Eq => Ok(cell_value == other_value),
                    CompareOp::Ne => Ok(cell_value != other_value),
                    CompareOp::Lt => Ok(cell_value < other_value),
                    CompareOp::Gt => Ok(cell_value > other_value),
                    CompareOp::Lte => Ok(cell_value <= other_value),
                    CompareOp::Gte => Ok(cell_value >= other_value),
                }
            }
            FilterCondition::StringLength { operator, length } => {
                let cell_len = cell_value.chars().count();
                match operator {
                    CompareOp::Eq => Ok(cell_len == *length),
                    CompareOp::Ne => Ok(cell_len != *length),
                    CompareOp::Lt => Ok(cell_len < *length),
                    CompareOp::Gt => Ok(cell_len > *length),
                    CompareOp::Lte => Ok(cell_len <= *length),
                    CompareOp::Gte => Ok(cell_len >= *length),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn forward_and_backward_are_inverses() {
        for kind in condition_cycle() {
            let next = next_kind(*kind);
            let back = prev_kind(next);
            assert_eq!(*kind, back);

            let prev = prev_kind(*kind);
            let forward = next_kind(prev);
            assert_eq!(*kind, forward);
        }
    }

    #[test]
    fn full_cycle_visits_each_kind_once() {
        let mut seen = HashSet::new();
        let mut current = condition_cycle()[0];

        for _ in 0..condition_cycle().len() {
            assert!(seen.insert(current));
            current = next_kind(current);
        }

        assert_eq!(current, condition_cycle()[0]);
        assert_eq!(seen.len(), condition_cycle().len());
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

    /// Evaluate this filter expression against a single row's data
    /// row_data: Map of column names to string values
    pub fn evaluate_row(&self, row_data: &std::collections::BTreeMap<String, String>) -> color_eyre::Result<bool> {
        match self {
            FilterExpr::Condition(filter) => {
                filter.evaluate_row(row_data)
            }
            FilterExpr::And(children) => {
                if children.is_empty() {
                    Ok(true) // Empty AND returns true
                } else {
                    // All children must be true
                    for child in children {
                        if !child.evaluate_row(row_data)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
            }
            FilterExpr::Or(children) => {
                if children.is_empty() {
                    Ok(false) // Empty OR returns false
                } else {
                    // At least one child must be true
                    for child in children {
                        if child.evaluate_row(row_data)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
            }
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