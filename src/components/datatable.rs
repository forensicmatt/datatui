//! DataTable: Interactive table widget for displaying Polars DataFrames in Ratatui
//!
//! This component renders a DataFrame as a scrollable, selectable table with support for theming and extensibility.
//! It is designed to be decoupled, reusable, and integrate with the Component trait and Action system.
//!
//! Extension points: custom cell rendering, sorting/filtering hooks, advanced navigation, etc.
use crate::style::StyleConfig;
use crate::components::Component;
use crate::dataframe::manager::ManagedDataFrame;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use std::sync::Arc;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent};
use ratatui::widgets::{Table, Row, Cell, Block, Borders};
use ratatui::prelude::{Frame, Rect, Size};
use ratatui::layout::Constraint;
use tokio::sync::mpsc::UnboundedSender;
use ratatui::style::{Modifier, Style};
use polars::prelude::DataFrame;
use crate::dialog::find_dialog::{FindOptions, SearchMode};
use crate::dialog::column_width_dialog::ColumnWidthConfig;
use crate::dialog::styling::{StyleSet, matches_column};
use polars::prelude::{AnyValue};
use regex::Regex;
use serde_json::{Value, Number};
use std::collections::BTreeMap;



/// Convert a Polars AnyValue into a display string
fn anyvalue_to_display_string(value: &AnyValue) -> String {
    match value {
        AnyValue::Null => "".to_string(),
        AnyValue::String(s) => s.to_string(),
        other => format!("{other}"),
    }
}


/// Convert a Polars AnyValue into a serde_json::Value
fn anyvalue_to_json(val: &AnyValue) -> Value {
    match val {
        AnyValue::Null => Value::Null,
        AnyValue::Boolean(b) => Value::Bool(*b),
        AnyValue::String(s) => Value::String(s.to_string()),
        AnyValue::UInt8(n) => Value::Number((*n).into()),
        AnyValue::UInt16(n) => Value::Number((*n).into()),
        AnyValue::UInt32(n) => Value::Number((*n).into()),
        AnyValue::UInt64(n) => Value::Number((*n).into()),
        AnyValue::Int8(n) => Value::Number((*n).into()),
        AnyValue::Int16(n) => Value::Number((*n).into()),
        AnyValue::Int32(n) => Value::Number((*n).into()),
        AnyValue::Int64(n) => Value::Number((*n).into()),
        AnyValue::Int128(n) => Value::String(n.to_string()),
        AnyValue::Float32(n) => Value::Number(Number::from_f64(*n as f64).unwrap()),
        AnyValue::Float64(n) => Value::Number(Number::from_f64(*n).unwrap()),
        AnyValue::Date(d) => Value::String(d.to_string()),
        AnyValue::Datetime(ts, unit, tz) => Value::String(format!("{ts:?} {unit:?} {tz:?}")),
        AnyValue::DatetimeOwned(ts, unit, tz) => Value::String(format!("{ts:?} {unit:?} {tz:?}")),
        AnyValue::Duration(d, unit) => Value::String(format!("{d:?} {unit:?}")),
        AnyValue::Time(t) => Value::String(t.to_string()),
        AnyValue::Categorical(id, mapping, _) => Value::String(mapping.get(*id).to_string()),
        AnyValue::CategoricalOwned(id, mapping, _) => Value::String(mapping.get(*id).to_string()),
        AnyValue::Enum(id, mapping, _) => Value::String(mapping.get(*id).to_string()),
        AnyValue::EnumOwned(id, mapping, _) => Value::String(mapping.get(*id).to_string()),
        AnyValue::List(s) => {
            // Convert each element in the list to a proper JSON value using anyvalue_to_json
            let vals: Vec<Value> = s.iter().map(|v| anyvalue_to_json(&v)).collect();
            Value::Array(vals)
        },
        AnyValue::Struct(_, arr, fields) => Value::String(format!("{arr:?} {fields:?}")),
        AnyValue::StructOwned(data) => Value::String(format!("{data:?}")),
        AnyValue::StringOwned(s) => Value::String(s.to_string()),
        AnyValue::Binary(b) => Value::String(format!("{b:?}")),
        AnyValue::BinaryOwned(b) => Value::String(format!("{b:?}")),
        AnyValue::Decimal(n, scale) => Value::String(format!("{n} {scale}")),
    }
}


#[allow(dead_code)]
fn value_to_display_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => {
            other.to_string()
        },
    }
}

/// Represents the selection state in the table (row, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSelection {
    pub row: usize,
    pub col: usize,
}

/// Represents the scroll position in the table (vertical, horizontal)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableScroll {
    pub y: usize,
    pub x: usize,
}

/// DataTable: Interactive widget for displaying a DataFrame as a table
#[derive(Debug)]
pub struct DataTable {
    /// Owned managed DataFrame to display
    pub dataframe: ManagedDataFrame,
    /// Current selection (row, column)
    pub selection: TableSelection,
    /// Current scroll position (vertical, horizontal)
    pub scroll: TableScroll,
    /// Style configuration for theming
    pub style: StyleConfig,
    /// Last rendered area height (for paging/scroll logic)
    pub last_area_height: u16,
    /// Last rendered area width (for horizontal scroll logic)
    pub last_area_width: u16,
    /// Enabled style sets for conditional styling
    pub style_sets: Vec<StyleSet>,
}

impl DataTable {
    /// Minimum column width in characters for horizontal scrolling
    const MIN_COL_WIDTH: u16 = 4;
    /// Maximum column width in characters for display
    const MAX_COL_WIDTH: u16 = 255;

    /// Create a new DataTable for the given DataFrame and style
    pub fn new(dataframe: ManagedDataFrame, style: StyleConfig) -> Self {
        Self {
            dataframe,
            selection: TableSelection { row: 0, col: 0 },
            scroll: TableScroll { y: 0, x: 0 },
            style,
            last_area_height: 0,
            last_area_width: 0,
            style_sets: Vec::new(),
        }
    }

    /// Set the style sets to apply
    pub fn set_style_sets(&mut self, style_sets: Vec<StyleSet>) {
        self.style_sets = style_sets;
    }
    
    /// Set the column width configuration
    pub fn set_column_width_config(&mut self, config: ColumnWidthConfig) {
        self.dataframe.set_column_width_config(config);
    }

    /// Get the column width configuration
    pub fn get_column_width_config(&self) -> ColumnWidthConfig {
        self.dataframe.column_width_config.clone()
    }

    /// Reset the current DataFrame to the base lazy frame
    pub fn reset_current_df(&mut self) {
        self.dataframe.reset_current_df();
    }

    /// Set the current DataFrame
    pub fn set_current_df(&mut self, df: DataFrame) {
        self.dataframe.set_current_df(df);
    }

    /// Get the value of the currently selected cell as a string.
    ///
    /// Returns an empty string if the selection is out of bounds or the value cannot be retrieved.
    pub fn selected_cell_value(&self) -> Result<String> {
        let visible_columns = self.get_visible_columns()?;
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let row = self.selection.row;
        let col = self.selection.col;

        if row < df.height() && col < visible_columns.len() {
            let col_name = &visible_columns[col];
            if let Ok(series) = df.column(col_name)
                && let Ok(val) = series.get(row) {
                    return Ok(val.str_value().to_string());
                }
        }
        Ok(String::new())
    }

    /// Get the value of the currently selected cell as a string.
    ///
    /// Returns an empty string if the selection is out of bounds or the value cannot be retrieved.
    pub fn selected_cell_json_value(&self) -> Result<Value> {
        let visible_columns = self.get_visible_columns()?;
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let row = self.selection.row;
        let col = self.selection.col;
        
        if row < df.height() && col < visible_columns.len() {
            let col_name = &visible_columns[col];
            if let Ok(series) = df.column(col_name)
                && let Ok(val) = series.get(row) {
                    let v = anyvalue_to_json(&val);
                    return Ok(v);
                }
        }
        Ok(Value::Null)
    }

    /// Helper to determine visible columns and their widths given a col_start and area width
    fn visible_col_range(
        &self,
        df: &polars::prelude::DataFrame,
        columns: &[String],
        area_width: u16,
        row_start: usize,
        row_end: usize,
        col_start: usize,
    ) -> (usize, usize, Vec<u16>) {
        let total_cols = columns.len();
        let mut col_widths = Vec::new();
        let mut total_width = 0u16;
        let mut col_end = col_start;
        let cell_padding = 0;
        
        while col_end < total_cols {
            let col_name = &columns[col_end];
            
            // Determine column width based on configuration
            let col_width = {
                // First check if there's a manual width set (takes precedence)
                if let Some(manual_width) = self.dataframe.column_width_config.manual_widths.get(col_name) {
                    *manual_width
                } else if self.dataframe.column_width_config.auto_expand {
                    // Auto-expand mode: calculate based on content
                    let mut max_len = Self::MIN_COL_WIDTH as usize;
                    for i in row_start..row_end {
                        let val = df.column(col_name)
                            .ok()
                            .and_then(|s| s.get(i).ok())
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let cell_len = val.chars().count() + cell_padding;
                        if cell_len > max_len {
                            max_len = cell_len;
                        }
                    }
                    max_len.clamp(Self::MIN_COL_WIDTH as usize, Self::MAX_COL_WIDTH as usize) as u16
                } else {
                    // Manual mode but no width set: fallback to auto-calculation
                    let mut max_len = Self::MIN_COL_WIDTH as usize;
                    for i in row_start..row_end {
                        let val = df.column(col_name)
                            .ok()
                            .and_then(|s| s.get(i).ok())
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let cell_len = val.chars().count() + cell_padding;
                        if cell_len > max_len {
                            max_len = cell_len;
                        }
                    }
                    max_len.clamp(Self::MIN_COL_WIDTH as usize, Self::MAX_COL_WIDTH as usize) as u16
                }
            };
            
            // Calculate how much width is actually available for this column
            let remaining_width = area_width.saturating_sub(total_width);
            
            // If no remaining width, we can't fit any more columns
            if remaining_width == 0 {
                break;
            }
            
            // Use the smaller of the column's desired width or remaining width
            let actual_width = std::cmp::min(col_width, remaining_width);
            
            // Only add the column if we can fit at least the minimum width
            if actual_width >= Self::MIN_COL_WIDTH {
                col_widths.push(actual_width);
                total_width += actual_width;
                col_end += 1;
            } else {
                // If we can't even fit the minimum width, stop
                break;
            }
        }
        (col_start, col_end, col_widths)
    }

    /// Calculate the optimal scroll position to show a column at the leftmost edge
    fn calculate_optimal_scroll_for_column(
        &self,
        _df: &polars::prelude::DataFrame,
        _columns: &[String],
        target_col: usize,
        _area_width: u16,
        _row_start: usize,
        _row_end: usize,
    ) -> usize {
        // Simply position the target column at the beginning of the visible area
        // This ensures the column is at the leftmost edge when scrolled to
        target_col
    }

    /// Compute the desired width for a specific column index within `columns`,
    /// using the same logic as `visible_col_range` but for a single column.
    fn desired_column_width(
        &self,
        df: &polars::prelude::DataFrame,
        columns: &[String],
        col_index: usize,
        row_start: usize,
        row_end: usize,
    ) -> u16 {
        if col_index >= columns.len() {
            return Self::MIN_COL_WIDTH;
        }
        let col_name = &columns[col_index];
        if let Some(manual_width) = self.dataframe.column_width_config.manual_widths.get(col_name) {
            return *manual_width;
        }
        let mut max_len = Self::MIN_COL_WIDTH as usize;
        for i in row_start..row_end {
            let val = df.column(col_name)
                .ok()
                .and_then(|s| s.get(i).ok())
                .map(|v| v.to_string())
                .unwrap_or_default();
            let cell_len = val.chars().count();
            if cell_len > max_len {
                max_len = cell_len;
            }
        }
        max_len
            .clamp(Self::MIN_COL_WIDTH as usize, Self::MAX_COL_WIDTH as usize) as u16
    }

    /// Get visible columns (excluding hidden ones)
    pub fn get_visible_columns(&self) -> Result<Vec<String>> {
        let df = self.get_dataframe()?;

        let all_columns: Vec<String> = df.get_column_names_owned()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        Ok(all_columns.into_iter()
            .filter(|col_name| {
                !self.dataframe.column_width_config.hidden_columns
                    .get(col_name)
                    .unwrap_or(&false)
            })
            .collect())
    }

    /// Get the dataframe
    pub fn get_dataframe(&self) -> Result<Arc<DataFrame>> {
        self.dataframe.get_dataframe()
    }

    /// Adjust selection to be within bounds of visible columns
    fn adjust_selection_for_visible_columns(&mut self) -> Result<()> {
        let visible_columns = self.get_visible_columns()?;
        let ncols = visible_columns.len();
        if ncols == 0 {
            // No visible columns, reset selection
            self.selection.col = 0;
            self.scroll.x = 0;
        } else if self.selection.col >= ncols {
            // Selection is out of bounds, adjust to last visible column
            self.selection.col = ncols.saturating_sub(1);
            self.scroll.x = self.selection.col;
        }
        Ok(())
    }

    /// Search for the next cell matching the pattern, using options and search_mode.
    /// Returns Ok(Some((row, col))) if found, Ok(None) if not, or Err if error.
    pub fn find_next(&self, pattern: &str, options: &FindOptions, search_mode: &SearchMode) -> color_eyre::Result<Option<(usize, usize)>> {
        let visible_columns = self.get_visible_columns()?;
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let nrows = df.height();
        let ncols = visible_columns.len();
        if nrows == 0 || ncols == 0 || pattern.is_empty() {
            return Ok(None);
        }
        let start_row = self.selection.row;
        let start_col = self.selection.col;
        let mut indices = Vec::new();
        // Flatten the table into (row, col) pairs in search order
        if options.backward {
            // Backward: search in reverse order
            let mut r = start_row;
            let mut c = start_col;
            loop {
                indices.push((r, c));
                if c == 0 {
                    if r == 0 { break; }
                    r -= 1;
                    c = ncols - 1;
                } else {
                    c -= 1;
                }
                if r == start_row && c == start_col { break; }
            }
            if options.wrap_around {
                r = nrows - 1;
                c = ncols - 1;
                while (r, c) != (start_row, start_col) {
                    indices.push((r, c));
                    if c == 0 {
                        if r == 0 { break; }
                        r -= 1;
                        c = ncols - 1;
                    } else {
                        c -= 1;
                    }
                }
            }
        } else {
            // Forward: search in order
            let mut r = start_row;
            let mut c = start_col;
            loop {
                indices.push((r, c));
                if c + 1 == ncols {
                    if r + 1 == nrows { break; }
                    r += 1;
                    c = 0;
                } else {
                    c += 1;
                }
                if r == start_row && c == start_col { break; }
            }
            if options.wrap_around {
                r = 0;
                c = 0;
                while (r, c) != (start_row, start_col) {
                    indices.push((r, c));
                    if c + 1 == ncols {
                        if r + 1 == nrows { break; }
                        r += 1;
                        c = 0;
                    } else {
                        c += 1;
                    }
                }
            }
        }
        // Remove the current cell from search (start after/before selection)
        if !indices.is_empty() { indices.remove(0); }
        // Prepare matcher
        let matcher: Box<dyn Fn(&str) -> bool> = match search_mode {
            SearchMode::Normal => {
                let pat = if options.match_case { pattern.to_string() } else { pattern.to_lowercase() };
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    let cell_cmp = if options.match_case { cell.to_string() } else { cell.to_lowercase() };
                    if whole_word {
                        cell_cmp == pat
                    } else {
                        cell_cmp.contains(&pat)
                    }
                })
            }
            SearchMode::Regex => {
                let re = if options.match_case {
                    Regex::new(pattern)
                } else {
                    Regex::new(&format!("(?i){pattern}"))
                }?;
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    if whole_word {
                        re.find(cell).is_some_and(|m| m.as_str() == cell)
                    } else {
                        re.is_match(cell)
                    }
                })
            }
        };
        // Search
        for (row, col) in indices {
            let col_name = &visible_columns[col];
            if let Ok(series) = df.column(col_name)
                && let Ok(val) = series.get(row) {
                    let cell_str = val.str_value();
                    if matcher(&cell_str) {
                        return Ok(Some((row, col)));
                    }
                }
        }
        Ok(None)
    }

    /// Count the number of matches for the given pattern, options, and search mode in the visible DataFrame.
    pub fn count_matches(&self, pattern: &str, options: &FindOptions, search_mode: &SearchMode) -> color_eyre::Result<usize> {
        let visible_columns = self.get_visible_columns()?;
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let nrows = df.height();
        let ncols = visible_columns.len();
        if nrows == 0 || ncols == 0 || pattern.is_empty() {
            return Ok(0);
        }
        // Prepare matcher
        let matcher: Box<dyn Fn(&str) -> bool> = match search_mode {
            SearchMode::Normal => {
                let pat = if options.match_case { pattern.to_string() } else { pattern.to_lowercase() };
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    let cell_cmp = if options.match_case { cell.to_string() } else { cell.to_lowercase() };
                    if whole_word {
                        cell_cmp == pat
                    } else {
                        cell_cmp.contains(&pat)
                    }
                })
            }
            SearchMode::Regex => {
                let re = if options.match_case {
                    Regex::new(pattern)
                } else {
                    Regex::new(&format!("(?i){pattern}"))
                }?;
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    if whole_word {
                        re.find(cell).is_some_and(|m| m.as_str() == cell)
                    } else {
                        re.is_match(cell)
                    }
                })
            }
        };
        let mut count = 0;
        for row in 0..nrows {
            for col_name in visible_columns.iter() {
                if let Ok(series) = df.column(col_name)
                    && let Ok(val) = series.get(row) {
                        let cell_str = val.str_value();
                        if matcher(&cell_str) {
                            count += 1;
                        }
                    }
            }
        }
        Ok(count)
    }

    /// Scrolls the table so that the selected cell is visible.
    pub fn scroll_to_selection(&mut self) -> Result<()> {
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let visible_columns = self.get_visible_columns()?;
        let nrows = df.height();
        let ncols = visible_columns.len();
        let header_height = 1;
        let area_height = if self.last_area_height > 0 {
            self.last_area_height.saturating_sub(header_height + 2) as usize
        } else {
            10 // fallback default
        };
        let area_width = if self.last_area_width > 0 {
            self.last_area_width as usize
        } else {
            ncols // fallback
        };
        // Vertical scroll
        if self.selection.row < self.scroll.y {
            self.scroll.y = self.selection.row;
        } else if self.selection.row >= self.scroll.y + area_height {
            self.scroll.y = self.selection.row + 1 - area_height;
        }
        // Horizontal scroll - use the existing logic from handle_key_event
        let row_start = self.scroll.y.min(nrows);
        let row_end = (row_start + area_height).min(nrows);
        let (col_start, col_end, col_widths) = self.visible_col_range(
            df, &visible_columns, area_width as u16, 
            row_start, row_end, self.scroll.x
        );
        let col = self.selection.col;
        if col < col_start {
            self.scroll.x = col;
        } else if col >= col_end {
            self.scroll.x = self.calculate_optimal_scroll_for_column(
                df, &visible_columns, col, area_width as u16, row_start, row_end
            );
        } else if col == col_end.saturating_sub(1) {
            // If the selected column is the last visible one and its desired width exceeds
            // the allocated width, shift it to the left edge to maximize its visible content.
            let allocated_idx = col_end.saturating_sub(col_start + 1);
            if let Some(&allocated_width) = col_widths.get(allocated_idx) {
                let desired_width = self.desired_column_width(df, &visible_columns, col, row_start, row_end);
                if desired_width > allocated_width {
                    self.scroll.x = col;
                }
            }
        }
        Ok(())
    }

    /// Find all matches for the given pattern, options, and search mode in the visible DataFrame.
    /// Returns a vector of FindAllResult with row, column, and context around each match.
    pub fn find_all_matches(
        &self, pattern: &str, options: &FindOptions, search_mode: &SearchMode,
        context_chars: usize
    ) -> color_eyre::Result<Vec<crate::dialog::find_all_results_dialog::FindAllResult>> {
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let visible_columns = self.get_visible_columns()?;
        let nrows = df.height();
        let ncols = visible_columns.len();
        if nrows == 0 || ncols == 0 || pattern.is_empty() {
            return Ok(Vec::new());
        }
        // Prepare matcher (same logic as find_next/count_matches)
        let matcher: Box<dyn Fn(&str) -> bool> = match search_mode {
            SearchMode::Normal => {
                let pat = if options.match_case { pattern.to_string() } else { pattern.to_lowercase() };
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    let cell_cmp = if options.match_case { cell.to_string() } else { cell.to_lowercase() };
                    if whole_word {
                        cell_cmp == pat
                    } else {
                        cell_cmp.contains(&pat)
                    }
                })
            }
            SearchMode::Regex => {
                let re = if options.match_case {
                    Regex::new(pattern)
                } else {
                    Regex::new(&format!("(?i){pattern}"))
                }?;
                let whole_word = options.whole_word;
                Box::new(move |cell: &str| {
                    if whole_word {
                        re.find(cell).is_some_and(|m| m.as_str() == cell)
                    } else {
                        re.is_match(cell)
                    }
                })
            }
        };
        let mut results = Vec::new();
        // Search through all cells in the DataFrame
        for row in 0..nrows {
            for col_name in visible_columns.iter() {
                if let Ok(series) = df.column(col_name)
                    && let Ok(val) = series.get(row) {
                        let cell_str = val.str_value();
                        if matcher(&cell_str) {
                            // Generate context around the match
                            let context = self.generate_context(&cell_str, pattern, context_chars, search_mode, options);
                            results.push(crate::dialog::find_all_results_dialog::FindAllResult {
                                row,
                                column: col_name.clone(),
                                context,
                            });
                        }
                    }
            }
        }
        Ok(results)
    }

    /// Generate context around a match in a cell string.
    /// Returns a string with context_chars characters before and after the match.
    fn generate_context(&self, cell_str: &str, pattern: &str, context_chars: usize, search_mode: &SearchMode, options: &FindOptions) -> String {
        // Find the position of the match in the cell string
        let match_pos = match search_mode {
            SearchMode::Normal => {
                let search_text = if options.match_case { pattern } else { &pattern.to_lowercase() };
                let cell_text = if options.match_case { cell_str } else { &cell_str.to_lowercase() };
                if options.whole_word {
                    // For whole word, find the exact word boundary
                    cell_text.find(search_text)
                } else {
                    // For substring, find any occurrence
                    cell_text.find(search_text)
                }
            }
            SearchMode::Regex => {
                let re = if options.match_case {
                    Regex::new(pattern)
                } else {
                    Regex::new(&format!("(?i){pattern}"))
                };
                if let Ok(re) = re {
                    re.find(cell_str).map(|m| m.start())
                } else {
                    None
                }
            }
        };
        
        if let Some(pos) = match_pos {
            // Calculate start and end positions for context
            let start = pos.saturating_sub(context_chars);
            let end = (pos + pattern.len() + context_chars).min(cell_str.len());
            
            // Extract context with ellipsis if needed
            let mut context = String::new();
            if start > 0 {
                context.push_str("...");
            }
            context.push_str(&cell_str[start..end]);
            if end < cell_str.len() {
                context.push_str("...");
            }
            context
        } else {
            // Fallback: return the full cell string if match position cannot be determined
            cell_str.to_string()
        }
    }
}

impl Component for DataTable {
    /// Register an action handler for sending actions.
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        let _ = tx;
        Ok(())
    }

    /// Register a configuration handler.
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        let _ = config;
        Ok(())
    }

    /// Initialize the component with a specified area.
    fn init(&mut self, area: Size) -> Result<()> {
        let _ = area;
        Ok(())
    }

    /// Handle incoming events and produce actions if necessary.
    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }

    /// Handle key events and produce actions if necessary.
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        use crossterm::event::{KeyCode, KeyModifiers};
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let nrows = df.height();
        let visible_columns = self.get_visible_columns()?;
        let ncols = visible_columns.len();
        let mut sel = self.selection;
        let mut scroll = self.scroll;
        let header_height = 1;

        // Calculate the available height for displaying data rows by subtracting the header height
        // from the total area height. If no area height is set yet (last_area_height <= 0),
        // fall back to a default of 10 rows
        let area_height = if self.last_area_height > 0 {
            self.last_area_height.saturating_sub(header_height + 2) as usize
        } else {
            10 // fallback default
        };
        let area_width = if self.last_area_width > 0 {
            self.last_area_width as usize
        } else {
            ncols // fallback
        };
        let min_col_width = Self::MIN_COL_WIDTH as usize;
        let _max_visible_cols = std::cmp::max(1, area_width / min_col_width);
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Up => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if sel.row >= area_height {
                            sel.row -= area_height;
                        } else {
                            sel.row = 0;
                        }
                    } else if sel.row > 0 {
                        sel.row -= 1;
                    }
                }
                KeyCode::Down => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        sel.row = (sel.row + area_height).min(nrows.saturating_sub(1));
                    } else if sel.row + 1 < nrows {
                        sel.row += 1;
                    }
                }
                KeyCode::Left => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if sel.col >= area_width {
                            sel.col -= area_width;
                        } else {
                            sel.col = 0;
                        }
                    } else if sel.col > 0 {
                        sel.col -= 1;
                    }
                }
                KeyCode::Right => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        sel.col = (sel.col + area_width).min(ncols.saturating_sub(1));
                    } else if sel.col + 1 < ncols {
                        sel.col += 1;
                    }
                }
                KeyCode::Home => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        sel.col = 0;
                    } else {
                        sel.row = 0;
                    }
                }
                KeyCode::End => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        sel.col = ncols.saturating_sub(1);
                    } else {
                        sel.row = nrows.saturating_sub(1);
                    }
                }
                _ => {}
            }
        }
        // Ensure selection is in bounds
        sel.row = sel.row.min(nrows.saturating_sub(1));
        sel.col = sel.col.min(ncols.saturating_sub(1));
        // Update scroll to keep selection visible (vertical)
        if area_height > 0 {
            if sel.row < scroll.y {
                scroll.y = sel.row;
            } else if sel.row >= scroll.y + area_height {
                scroll.y = sel.row + 1 - area_height;
            }
        }
        // --- Horizontal scroll logic ---
        // Use visible_col_range to determine which columns are visible for the current scroll.x
        let row_start = scroll.y.min(nrows);
        let row_end = (row_start + area_height).min(nrows);
        let area_width_u16 = self.last_area_width;
        let (col_start, col_end, col_widths) = self.visible_col_range(
            df, &visible_columns, area_width_u16, row_start, row_end, scroll.x
        );
        // If selection.col is left of visible, scroll left
        if sel.col < col_start {
            scroll.x = sel.col;
        } else if sel.col >= col_end {
            // Use the optimal scroll calculation to ensure the selected column can display its full width
            scroll.x = self.calculate_optimal_scroll_for_column(
                df, &visible_columns, sel.col, area_width_u16, row_start, row_end
            );
        } else if sel.col == col_end.saturating_sub(1) {
            // If the selected column is the last visible one and its desired width exceeds
            // the allocated width, shift it to the left edge to maximize its visible content.
            let allocated_idx = col_end.saturating_sub(col_start + 1);
            if let Some(&allocated_width) = col_widths.get(allocated_idx) {
                let desired_width = self.desired_column_width(df, &visible_columns, sel.col, row_start, row_end);
                if desired_width > allocated_width {
                    scroll.x = sel.col;
                }
            }
        }
        self.selection = sel;
        self.scroll = scroll;
        Ok(None)
    }

    /// Handle mouse events and produce actions if necessary.
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        let _ = mouse;
        Ok(None)
    }

    /// Update the state of the component based on a received action.
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        let _ = action;
        Ok(None)
    }

    /// Render the component on the screen.
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.last_area_height = area.height;
        self.last_area_width = area.width;
        // Adjust selection if needed due to hidden columns
        self.adjust_selection_for_visible_columns()?;
        let df = self.get_dataframe()?;
        let df = df.as_ref();
        let visible_columns = self.get_visible_columns()?;
        let total_rows = df.height();
        let total_cols = visible_columns.len();
        let header_height = 1;
        let max_visible_rows = area.height.saturating_sub(header_height) as usize;
        let row_start = self.scroll.y.min(total_rows);
        let row_end = (row_start + max_visible_rows).min(total_rows);
        let col_start = self.scroll.x.min(total_cols);
        let (col_start, col_end, col_widths) = self.visible_col_range(
            df, &visible_columns, area.width, row_start, row_end, col_start
        );
        let visible_columns_slice = &visible_columns[col_start..col_end];
        
        // Calculate if we need a vertical scroll bar
        let needs_vertical_scroll = total_rows > max_visible_rows;
        let scroll_bar_width = if needs_vertical_scroll { 1 } else { 0 };
        
        // Adjust table area to account for scroll bar
        let table_area = if needs_vertical_scroll {
            Rect::new(
                area.x + scroll_bar_width,
                area.y,
                area.width.saturating_sub(scroll_bar_width),
                area.height,
            )
        } else {
            area
        };
        
        // Draw vertical scroll bar if needed
        if needs_vertical_scroll {
            let scroll_bar_area = Rect::new(
                area.x,
                area.y,
                scroll_bar_width,
                area.height,
            );
            
            // Calculate scroll bar thumb position and size
            let scroll_bar_height = area.height as f64;
            let total_rows_f = total_rows as f64;
            let max_visible_rows_f = max_visible_rows as f64;
            
            // Calculate thumb size as a proportion of the scroll bar height
            let thumb_size_f = (max_visible_rows_f / total_rows_f) * scroll_bar_height;
            let thumb_size = std::cmp::max(1, thumb_size_f.round() as usize);
            
            // Calculate thumb position using floating-point for smoother movement
            let scroll_progress = if total_rows > max_visible_rows {
                self.scroll.y as f64 / (total_rows - max_visible_rows) as f64
            } else {
                0.0
            };
            
            // Calculate available space for thumb movement
            let available_space = scroll_bar_height - thumb_size as f64;
            let thumb_position_f = scroll_progress * available_space;
            
            // Ensure thumb position is within bounds
            let max_thumb_position = (scroll_bar_height as usize).saturating_sub(thumb_size);
            let _thumb_position = thumb_position_f.round() as usize;
            let thumb_position_f = thumb_position_f.min(max_thumb_position as f64);
            
            // Draw scroll bar track
            for y in scroll_bar_area.y..scroll_bar_area.bottom() {
                frame.buffer_mut().set_string(
                    scroll_bar_area.x,
                    y,
                    "│",
                    Style::default().fg(ratatui::style::Color::DarkGray)
                );
            }
            
            // Alternative: Draw a density-based scroll bar for even smoother appearance
            // This creates a more fluid visual by using different characters based on position
            let scroll_bar_y_start = scroll_bar_area.y;
            let scroll_bar_y_end = scroll_bar_area.bottom();
            
            for y in scroll_bar_y_start..scroll_bar_y_end {
                let y_pos = (y - scroll_bar_y_start) as f64;
                
                // Calculate if this position should be part of the thumb
                let thumb_start_f = thumb_position_f;
                let thumb_end_f = thumb_position_f + thumb_size as f64;
                
                let char_to_use = if y_pos >= thumb_start_f && y_pos < thumb_end_f {
                    // Inside thumb - use full block
                    "█"
                } else if y_pos >= thumb_start_f - 0.5 && y_pos < thumb_start_f {
                    // Near top edge - use partial fill
                    "▄"
                } else if y_pos >= thumb_end_f && y_pos < thumb_end_f + 0.5 {
                    // Near bottom edge - use partial fill
                    "▀"
                } else {
                    // Outside thumb - use track character
                    "│"
                };
                
                let style = if y_pos >= thumb_start_f && y_pos < thumb_end_f {
                    Style::default().fg(ratatui::style::Color::Cyan)
                } else if char_to_use != "│" {
                    // Partial fill characters
                    Style::default().fg(ratatui::style::Color::Cyan)
                } else {
                    // Track
                    Style::default().fg(ratatui::style::Color::DarkGray)
                };
                
                frame.buffer_mut().set_string(
                    scroll_bar_area.x,
                    y,
                    char_to_use,
                    style
                );
            }
        }

        // Only build visible rows (avoid full materialization)
        let mut visible_rows: Vec<Vec<AnyValue>> = Vec::with_capacity(row_end - row_start);
        for i in row_start..row_end {
            let mut row: Vec<AnyValue> = Vec::with_capacity(col_end - col_start);
            for col in visible_columns_slice {
                let any_val = df.column(col)
                    .ok()
                    .and_then(|s| s.get(i).ok())
                    .unwrap_or_else(|| AnyValue::Null);
                row.push(any_val);
            }
            visible_rows.push(row);
        }

        // Header with sort indicators
        let header = Row::new(
            visible_columns_slice
                .iter()
                .map(|c| {
                    let col_name = c;
                    let mut label = col_name.clone();
                    if let Some(ref sort_cols) = self.dataframe.last_sort
                        && let Some((sort_idx, sort_col)) = sort_cols.iter()
                                .enumerate()
                                .find(|(_, sc)| sc.name == col_name.clone()) {
                            let arrow = if sort_col.ascending { "↑" } else { "↓" };
                            let prefix = if sort_cols.len() > 1 {
                                format!("{}[{}] ", arrow, sort_idx+1)
                            } else {
                                format!("{arrow} ")
                            };
                            label = format!("{prefix}{label}");
                        }
                    Cell::from(label).style(self.style.table_header)
                })
        );
        let selected_cell_style = Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED | Modifier::UNDERLINED);
    
        // Build row data for style rule evaluation
        let row_widgets: Vec<Row> = visible_rows.iter().enumerate().map(|(i, row)| {
            let global_row = row_start + i;
            
            // Build row data map for style rule evaluation
            let mut row_data = BTreeMap::new();
            for (j, col_name) in visible_columns_slice.iter().enumerate() {
                let value = &row[j];
                let cell_str = anyvalue_to_display_string(value);
                row_data.insert(col_name.clone(), cell_str);
            }
            
            // Evaluate all style rules and collect matched styles
            let mut row_style: Option<ratatui::style::Style> = None;
            let mut cell_styles: Vec<Option<ratatui::style::Style>> = vec![None; visible_columns_slice.len()];
            
            for style_set in &self.style_sets {
                for rule in &style_set.rules {
                    // Filter row_data by column_scope if specified
                    let eval_data: BTreeMap<String, String> = if let Some(ref column_scope) = rule.column_scope {
                        if column_scope.is_empty() {
                            row_data.clone()
                        } else {
                            row_data.iter()
                                .filter(|(col_name, _)| matches_column(col_name, column_scope))
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect()
                        }
                    } else {
                        row_data.clone()
                    };
                    
                    // Evaluate the rule
                    if let Ok(matches) = rule.match_expr.evaluate_row(&eval_data) {
                        if matches {
                            let matched_style = rule.style.style.to_ratatui_style();
                            
                            match rule.style.scope {
                                crate::dialog::styling::ScopeEnum::Row => {
                                    // Apply to entire row
                                    row_style = Some(matched_style);
                                }
                                crate::dialog::styling::ScopeEnum::Cell => {
                                    // Apply to matching cells (those in column_scope)
                                    if let Some(ref column_scope) = rule.column_scope {
                                        for (j, col_name) in visible_columns_slice.iter().enumerate() {
                                            if column_scope.is_empty() || matches_column(col_name, column_scope) {
                                                cell_styles[j] = Some(matched_style);
                                            }
                                        }
                                    } else {
                                        // Apply to all cells if no column_scope
                                        for j in 0..visible_columns_slice.len() {
                                            cell_styles[j] = Some(matched_style);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Create cells with applied styles
            let cells: Vec<Cell> = (0..visible_columns_slice.len()).map(|j| {
                let col_idx = col_start + j;
                let value = &row[j];
                let cell_str = anyvalue_to_display_string(value);
                let mut cell = Cell::from(cell_str);
                
                // Apply cell-specific style if set
                if let Some(ref cell_style) = cell_styles[j] {
                    cell = cell.style(*cell_style);
                } else {
                    cell = cell.style(self.style.table_cell);
                }
                
                // Selected cell style overrides
                if global_row == self.selection.row && col_idx == self.selection.col {
                    cell = cell.style(selected_cell_style);
                }
                cell
            }).collect();
            
            // Create row with applied styles
            let mut r = Row::new(cells);
            
            // Apply row-level style if set
            if let Some(ref rs) = row_style {
                r = r.style(*rs);
            } else {
                // Default row styling
                if global_row == self.selection.row {
                    r = r.style(Style::default().add_modifier(Modifier::REVERSED));
                } else if global_row % 2 == 0 {
                    r = r.style(self.style.table_row_even);
                } else {
                    r = r.style(self.style.table_row_odd);
                }
            }
            r
        }).collect();
        let widths = col_widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i == col_widths.len().saturating_sub(1) {
                    Constraint::Fill(1)
                } else {
                    Constraint::Length(*w)
                }
            })
            .collect::<Vec<_>>();
        let table = Table::new(row_widgets, widths)
            .header(header)
            .block(Block::default()
            .borders(Borders::ALL)
            .style(self.style.table_border));
        frame.render_widget(table, table_area);
        Ok(())
    }
} 

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataframe::manager::ManagedDataFrame;
    use polars::prelude::*;

    #[test]
    fn test_selected_cell_value() {
        // Create a simple test DataFrame
        let s1 = Series::new("col1".into(), &["a", "b", "c"]);
        let s2 = Series::new("col2".into(), &[1, 2, 3]);
        let df = DataFrame::new(vec![s1.into(), s2.into()]).unwrap();
        
        let managed_df = ManagedDataFrame::new(df.clone(), "test".to_string(), None, None);
        
        let style = StyleConfig::default();
        let mut datatable = DataTable::new(managed_df, style);
        
        // Test getting cell value at (0, 0)
        datatable.selection = TableSelection { row: 0, col: 0 };
        assert_eq!(datatable.selected_cell_value().unwrap(), "a");
        
        // Test getting cell value at (1, 1)
        datatable.selection = TableSelection { row: 1, col: 1 };
        assert_eq!(datatable.selected_cell_value().unwrap(), "2");
        
        // Test out of bounds
        datatable.selection = TableSelection { row: 10, col: 0 };
        assert_eq!(datatable.selected_cell_value().unwrap(), "");
    }
} 