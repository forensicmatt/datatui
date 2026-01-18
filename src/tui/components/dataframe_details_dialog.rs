//! DataFrame Details Dialog Component
//!
//! Provides detailed insights into a DataFrame through tabbed views:
//! - Unique Values: Value counts for selected column
//! - Column Schema: All columns with their data types
//! - Describe: Statistical summary for numeric columns
//! - Heatmap: Correlation matrix for numeric columns
//! - Embeddings: Placeholder for future embeddings support

use crate::core::ManagedDataset;
use crate::tui::components::MapViewerDialog;
use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table,
    },
    Frame,
};
use std::collections::HashMap;

/// Tab selection for details dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetailsTab {
    UniqueValues,
    Columns,
    Describe,
    Heatmap,
    Embeddings,
}

impl DetailsTab {
    /// Get tab label for rendering
    fn label(&self) -> &'static str {
        match self {
            DetailsTab::UniqueValues => "Unique Values",
            DetailsTab::Columns => "Column Schema",
            DetailsTab::Describe => "Describe",
            DetailsTab::Heatmap => "Heatmap",
            DetailsTab::Embeddings => "Embeddings",
        }
    }

    /// Get next tab in sequence
    fn next(&self) -> Self {
        match self {
            DetailsTab::UniqueValues => DetailsTab::Columns,
            DetailsTab::Columns => DetailsTab::Describe,
            DetailsTab::Describe => DetailsTab::Heatmap,
            DetailsTab::Heatmap => DetailsTab::Embeddings,
            DetailsTab::Embeddings => DetailsTab::UniqueValues,
        }
    }

    /// Get previous tab in sequence
    fn prev(&self) -> Self {
        match self {
            DetailsTab::UniqueValues => DetailsTab::Embeddings,
            DetailsTab::Columns => DetailsTab::UniqueValues,
            DetailsTab::Describe => DetailsTab::Columns,
            DetailsTab::Heatmap => DetailsTab::Describe,
            DetailsTab::Embeddings => DetailsTab::Heatmap,
        }
    }
}

/// Sort order for unique values tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOrder {
    ByValue,
    ByCount,
}

/// DataFrame Details Dialog
pub struct DataFrameDetailsDialog {
    /// Dataset reference
    dataset: ManagedDataset,

    /// All columns in the dataset
    columns: Vec<String>,

    /// Currently active tab
    active_tab: DetailsTab,

    /// Currently selected column index (for unique values tab)
    selected_column_idx: usize,

    /// Whether this component has focus
    focused: bool,

    /// Scroll position for current tab
    scroll_offset: usize,

    /// Selected row in current view
    selected_row: usize,

    // Cached data for each tab
    /// Unique values: (value, count) for current column
    unique_values: Vec<(String, i64)>,

    /// Column info: (name, type)
    column_info: Vec<(String, String)>,

    /// Describe stats: column -> stats map
    describe_stats: HashMap<String, DescribeStats>,

    /// Heatmap: correlation matrix
    heatmap_data: Vec<Vec<f64>>,

    /// Heatmap: numeric column names
    heatmap_columns: Vec<String>,

    /// Heatmap: selected X column index
    heatmap_x_idx: usize,

    /// Heatmap: selected Y column index
    heatmap_y_idx: usize,

    /// Sort order for unique values
    sort_order: SortOrder,

    /// Whether data has been loaded for each tab
    loaded_tabs: HashMap<DetailsTab, bool>,

    /// Last known viewport height for paging calculations
    viewport_height: usize,

    /// Map viewer dialog (when active)
    map_viewer: Option<MapViewerDialog>,
}

/// Statistical summary for a numeric column
#[derive(Debug, Clone)]
struct DescribeStats {
    count: i64,
    mean: Option<f64>,
    std: Option<f64>,
    median: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
}

impl DataFrameDetailsDialog {
    /// Create a new DataFrame details dialog
    pub fn new(dataset: ManagedDataset, columns: Vec<String>, current_col_idx: usize) -> Self {
        let selected_column_idx = current_col_idx.min(columns.len().saturating_sub(1));

        Self {
            dataset,
            columns,
            active_tab: DetailsTab::UniqueValues,
            selected_column_idx,
            focused: false,
            scroll_offset: 0,
            selected_row: 0,
            unique_values: Vec::new(),
            column_info: Vec::new(),
            describe_stats: HashMap::new(),
            heatmap_data: Vec::new(),
            heatmap_columns: Vec::new(),
            heatmap_x_idx: 0,
            heatmap_y_idx: 0,
            sort_order: SortOrder::ByCount,
            loaded_tabs: HashMap::new(),
            viewport_height: 20, // Default, will be updated on first render
            map_viewer: None,
        }
    }

    /// Check if map viewer dialog is active
    pub fn has_map_viewer(&self) -> bool {
        self.map_viewer.is_some()
    }

    /// Switch to next tab
    fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
        self.scroll_offset = 0;
        self.selected_row = 0;
        self.load_tab_data();
    }

    /// Switch to previous tab
    fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
        self.scroll_offset = 0;
        self.selected_row = 0;
        self.load_tab_data();
    }

    /// Load data for current tab if not already loaded
    fn load_tab_data(&mut self) {
        if self
            .loaded_tabs
            .get(&self.active_tab)
            .copied()
            .unwrap_or(false)
        {
            return; // Already loaded
        }

        match self.active_tab {
            DetailsTab::UniqueValues => {
                let _ = self.load_unique_values();
            }
            DetailsTab::Columns => {
                let _ = self.load_column_info();
            }
            DetailsTab::Describe => {
                let _ = self.load_describe_stats();
            }
            DetailsTab::Heatmap => {
                let _ = self.load_heatmap();
            }
            DetailsTab::Embeddings => {
                // Placeholder - no data to load
            }
        }

        self.loaded_tabs.insert(self.active_tab, true);
    }

    /// Load unique values for current column
    fn load_unique_values(&mut self) -> Result<()> {
        if self.columns.is_empty() {
            return Ok(());
        }

        let column_name = &self.columns[self.selected_column_idx];

        // Query unique values with counts
        let query = format!(
            "SELECT \"{}\", COUNT(*) as count FROM {} GROUP BY \"{}\" ",
            column_name,
            self.dataset.table_name(),
            column_name
        );

        let query = match self.sort_order {
            SortOrder::ByValue => format!("{} ORDER BY \"{}\" ASC", query, column_name),
            SortOrder::ByCount => format!("{} ORDER BY count DESC, \"{}\" ASC", query, column_name),
        };

        let conn = self.dataset.connection();
        let mut stmt = conn.prepare(&query)?;

        let mut values = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let value: String = row
                .get::<_, Option<String>>(0)?
                .unwrap_or_else(|| "<NULL>".to_string());
            let count: i64 = row.get(1)?;
            values.push((value, count));
        }

        self.unique_values = values;
        Ok(())
    }

    /// Load column information
    fn load_column_info(&mut self) -> Result<()> {
        let query = format!("DESCRIBE {}", self.dataset.table_name());
        let conn = self.dataset.connection();
        let mut stmt = conn.prepare(&query)?;

        let mut info = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let type_str: String = row.get(1)?;
            info.push((name, type_str));
        }

        self.column_info = info;
        Ok(())
    }

    /// Load describe statistics
    fn load_describe_stats(&mut self) -> Result<()> {
        // First, get column types
        let query = format!("DESCRIBE {}", self.dataset.table_name());
        let conn = self.dataset.connection();
        let mut stmt = conn.prepare(&query)?;

        let mut numeric_columns = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let type_str: String = row.get(1)?;

            // Check if numeric type
            if type_str.to_uppercase().contains("INT")
                || type_str.to_uppercase().contains("FLOAT")
                || type_str.to_uppercase().contains("DOUBLE")
                || type_str.to_uppercase().contains("DECIMAL")
                || type_str.to_uppercase().contains("NUMERIC")
            {
                numeric_columns.push(name);
            }
        }

        // Compute stats for each numeric column
        for col in numeric_columns {
            let stats_query = format!(
                "SELECT COUNT(\"{}\"), AVG(\"{}\"), STDDEV(\"{}\"), MEDIAN(\"{}\"), MIN(\"{}\"), MAX(\"{}\") FROM {}",
                col, col, col, col, col, col, self.dataset.table_name()
            );

            let mut stmt = conn.prepare(&stats_query)?;
            let mut rows = stmt.query([])?;

            if let Some(row) = rows.next()? {
                let stats = DescribeStats {
                    count: row.get(0)?,
                    mean: row.get(1).ok(),
                    std: row.get(2).ok(),
                    median: row.get(3).ok(),
                    min: row.get(4).ok(),
                    max: row.get(5).ok(),
                };
                self.describe_stats.insert(col, stats);
            }
        }

        Ok(())
    }

    /// Load heatmap correlation matrix
    fn load_heatmap(&mut self) -> Result<()> {
        // Get numeric columns
        let query = format!("DESCRIBE {}", self.dataset.table_name());
        let conn = self.dataset.connection();
        let mut stmt = conn.prepare(&query)?;

        let mut numeric_columns = Vec::new();
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let type_str: String = row.get(1)?;

            if type_str.to_uppercase().contains("INT")
                || type_str.to_uppercase().contains("FLOAT")
                || type_str.to_uppercase().contains("DOUBLE")
                || type_str.to_uppercase().contains("DECIMAL")
                || type_str.to_uppercase().contains("NUMERIC")
            {
                numeric_columns.push(name);
            }
        }

        if numeric_columns.is_empty() {
            return Ok(());
        }

        let n = numeric_columns.len();
        let mut matrix = vec![vec![0.0; n]; n];

        // Compute correlations
        for (i, col1) in numeric_columns.iter().enumerate() {
            for (j, col2) in numeric_columns.iter().enumerate() {
                if i == j {
                    matrix[i][j] = 1.0; // Perfect correlation with self
                } else {
                    // Compute correlation
                    let corr_query = format!(
                        "SELECT CORR(\"{}\", \"{}\") FROM {}",
                        col1,
                        col2,
                        self.dataset.table_name()
                    );

                    let mut stmt = conn.prepare(&corr_query)?;
                    let mut rows = stmt.query([])?;

                    if let Some(row) = rows.next()? {
                        let corr: Option<f64> = row.get(0).ok();
                        matrix[i][j] = corr.unwrap_or(0.0);
                    }
                }
            }
        }

        self.heatmap_columns = numeric_columns;
        self.heatmap_data = matrix;
        self.heatmap_x_idx = 0;
        self.heatmap_y_idx = if n > 1 { 1 } else { 0 };

        Ok(())
    }

    /// Toggle sort order for unique values
    fn toggle_sort(&mut self) {
        if self.active_tab == DetailsTab::UniqueValues {
            self.sort_order = match self.sort_order {
                SortOrder::ByValue => SortOrder::ByCount,
                SortOrder::ByCount => SortOrder::ByValue,
            };
            // Reload data with new sort order
            self.loaded_tabs.insert(DetailsTab::UniqueValues, false);
            self.load_tab_data();
        }
    }

    /// Change selected column (for unique values tab)
    fn change_column(&mut self, delta: i32) {
        if self.active_tab == DetailsTab::UniqueValues && !self.columns.is_empty() {
            let new_idx = (self.selected_column_idx as i32 + delta)
                .rem_euclid(self.columns.len() as i32) as usize;
            if new_idx != self.selected_column_idx {
                self.selected_column_idx = new_idx;
                // Reload unique values for new column
                self.loaded_tabs.insert(DetailsTab::UniqueValues, false);
                self.load_tab_data();
            }
        }
    }

    /// Calculate optimal column widths based on content
    ///
    /// Returns a tuple of (col1_width, col2_width) that best fits the data
    /// while respecting minimum and maximum constraints
    fn calculate_column_widths(
        &self,
        available_width: u16,
        header1: &str,
        header2: &str,
        data: &[(String, String)],
        min_width: u16,
        max_percent: f32,
    ) -> (u16, u16) {
        // Calculate maximum content width for each column
        let max_len1 = data
            .iter()
            .map(|(s, _)| s.len())
            .max()
            .unwrap_or(0)
            .max(header1.len());

        let max_len2 = data
            .iter()
            .map(|(_, s)| s.len())
            .max()
            .unwrap_or(0)
            .max(header2.len());

        // Add padding (2 chars per column for spacing)
        let desired_width1 = (max_len1 + 2).min((available_width as f32 * max_percent) as usize);
        let desired_width2 = (max_len2 + 2).min((available_width as f32 * 0.5) as usize);

        // Ensure minimum widths
        let width1 = (desired_width1.max(min_width as usize) as u16)
            .min(available_width.saturating_sub(min_width));
        let width2 = available_width.saturating_sub(width1).max(min_width);

        (width1, width2)
    }

    /// Render the dialog
    fn render_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let theme = Theme::default();

        // Clear background
        frame.render_widget(Clear, area);

        // Main block
        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let block = Block::default()
            .title(" DataFrame Details ")
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.normal_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into tab bar and content
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);

        // Render tab bar
        self.render_tab_bar(frame, chunks[0], &theme);

        // Render active tab content
        self.render_tab_content(frame, chunks[1], &theme);

        // Render map viewer overlay if active
        if let Some(viewer) = &mut self.map_viewer {
            let viewer_area = Self::centered_rect(80, 70, area);
            viewer.render(frame, viewer_area);
        }
    }

    /// Create a centered rectangle
    fn centered_rect(percent_w: u16, percent_h: u16, area: Rect) -> Rect {
        let width = (area.width * percent_w) / 100;
        let height = (area.height * percent_h) / 100;
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        Rect {
            x: area.x + x,
            y: area.y + y,
            width,
            height,
        }
    }

    /// Render tab bar
    fn render_tab_bar(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let tabs = [
            DetailsTab::UniqueValues,
            DetailsTab::Columns,
            DetailsTab::Describe,
            DetailsTab::Heatmap,
            DetailsTab::Embeddings,
        ];

        let mut spans = Vec::new();
        for (i, tab) in tabs.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" | "));
            }

            let style = if *tab == self.active_tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            spans.push(Span::styled(tab.label(), style));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }

    /// Render content for active tab
    fn render_tab_content(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Load data if needed
        self.load_tab_data();

        match self.active_tab {
            DetailsTab::UniqueValues => self.render_unique_values(frame, area, theme),
            DetailsTab::Columns => self.render_columns(frame, area, theme),
            DetailsTab::Describe => self.render_describe(frame, area, theme),
            DetailsTab::Heatmap => self.render_heatmap(frame, area, theme),
            DetailsTab::Embeddings => self.render_embeddings(frame, area, theme),
        }
    }

    /// Render unique values tab
    fn render_unique_values(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Column selector line
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        // Column selector
        let current_col = self
            .columns
            .get(self.selected_column_idx)
            .map(|s| s.as_str())
            .unwrap_or("<none>");
        let sort_label = match self.sort_order {
            SortOrder::ByValue => "sorted by value",
            SortOrder::ByCount => "sorted by count",
        };
        let selector = Paragraph::new(format!(
            "Column: {}  ({}) - Use ←/→ to change, 's' to toggle sort",
            current_col, sort_label
        ))
        .style(theme.normal_style());
        frame.render_widget(selector, chunks[0]);

        // Table with values
        let table_area = chunks[1];
        let viewport_height = table_area.height.saturating_sub(1) as usize; // -1 for header
        self.viewport_height = viewport_height; // Store for paging

        // Adjust scroll offset to keep selection visible
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        } else if self.selected_row >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_row.saturating_sub(viewport_height - 1);
        }

        let visible_start = self.scroll_offset.min(self.unique_values.len());
        let visible_end = (self.scroll_offset + viewport_height).min(self.unique_values.len());
        let visible_values = &self.unique_values[visible_start..visible_end];

        let rows: Vec<Row> = visible_values
            .iter()
            .enumerate()
            .map(|(i, (value, count))| {
                let global_idx = visible_start + i;
                let style = if global_idx == self.selected_row {
                    theme.selected_style()
                } else {
                    theme.normal_style()
                };
                Row::new(vec![value.clone(), count.to_string()]).style(style)
            })
            .collect();

        // Calculate optimal column widths based on actual content
        let data_for_width: Vec<(String, String)> = visible_values
            .iter()
            .map(|(val, count)| (val.clone(), count.to_string()))
            .collect();

        let (value_width, count_width) = self.calculate_column_widths(
            table_area.width,
            "Value",
            "Count",
            &data_for_width,
            8,    // Minimum width
            0.75, // Maximum 75% for value column
        );

        let table = Table::new(
            rows,
            [
                Constraint::Length(value_width),
                Constraint::Length(count_width),
            ],
        )
        .header(Row::new(vec!["Value", "Count"]).style(theme.header_style()))
        .style(theme.normal_style());

        frame.render_widget(table, table_area);

        // Render scrollbar if needed
        if self.unique_values.len() > viewport_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(self.unique_values.len()).position(self.selected_row);

            let scrollbar_area = Rect {
                x: table_area.x + table_area.width.saturating_sub(1),
                y: table_area.y + 1,
                width: 1,
                height: table_area.height.saturating_sub(2),
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    /// Render column schema tab
    fn render_columns(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let viewport_height = area.height.saturating_sub(1) as usize;
        self.viewport_height = viewport_height; // Store for paging

        // Adjust scroll
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        } else if self.selected_row >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_row.saturating_sub(viewport_height - 1);
        }

        let visible_start = self.scroll_offset.min(self.column_info.len());
        let visible_end = (self.scroll_offset + viewport_height).min(self.column_info.len());
        let visible_info = &self.column_info[visible_start..visible_end];

        let rows: Vec<Row> = visible_info
            .iter()
            .enumerate()
            .map(|(i, (name, dtype))| {
                let global_idx = visible_start + i;
                let style = if global_idx == self.selected_row {
                    theme.selected_style()
                } else {
                    theme.normal_style()
                };
                Row::new(vec![name.clone(), dtype.clone()]).style(style)
            })
            .collect();

        // Calculate optimal column widths
        let (name_width, type_width) = self.calculate_column_widths(
            area.width,
            "Column",
            "Type",
            &self.column_info,
            10,  // Minimum width
            0.6, // Maximum 60% for column names
        );

        let table = Table::new(
            rows,
            [
                Constraint::Length(name_width),
                Constraint::Length(type_width),
            ],
        )
        .header(Row::new(vec!["Column", "Type"]).style(theme.header_style()))
        .style(theme.normal_style());

        frame.render_widget(table, area);

        // Scrollbar
        if self.column_info.len() > viewport_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(self.column_info.len()).position(self.selected_row);

            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    /// Render describe statistics tab
    fn render_describe(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.describe_stats.is_empty() {
            let msg = Paragraph::new("No numeric columns found")
                .style(theme.normal_style())
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        // Create sorted list of columns
        let mut columns: Vec<_> = self.describe_stats.keys().cloned().collect();
        columns.sort();

        let viewport_height = area.height.saturating_sub(1) as usize;
        self.viewport_height = viewport_height; // Store for paging

        // Adjust scroll
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        } else if self.selected_row >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected_row.saturating_sub(viewport_height - 1);
        }

        let visible_start = self.scroll_offset.min(columns.len());
        let visible_end = (self.scroll_offset + viewport_height).min(columns.len());
        let visible_cols = &columns[visible_start..visible_end];

        let rows: Vec<Row> = visible_cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let global_idx = visible_start + i;
                let stats = self.describe_stats.get(col).unwrap();

                let style = if global_idx == self.selected_row {
                    theme.selected_style()
                } else {
                    theme.normal_style()
                };

                let fmt_float = |opt: Option<f64>| -> String {
                    opt.map(|v| format!("{:.2}", v))
                        .unwrap_or_else(|| "-".to_string())
                };

                Row::new(vec![
                    col.clone(),
                    stats.count.to_string(),
                    fmt_float(stats.mean),
                    fmt_float(stats.std),
                    fmt_float(stats.median),
                    fmt_float(stats.min),
                    fmt_float(stats.max),
                ])
                .style(style)
            })
            .collect();

        // Calculate optimal widths for describe table
        // Column name should adapt to longest name, stats should be consistent
        let max_col_name_len = columns
            .iter()
            .map(|s| s.len())
            .max()
            .unwrap_or(6)
            .max(6) // "Column" header
            + 2; // padding

        let col_name_width = (max_col_name_len as u16).min(area.width / 3);
        let remaining_width = area.width.saturating_sub(col_name_width);

        // Distribute remaining width evenly across 6 stat columns
        let stat_width = (remaining_width / 6).max(10); // Minimum 10 chars per stat

        let table = Table::new(
            rows,
            [
                Constraint::Length(col_name_width),
                Constraint::Length(stat_width),
                Constraint::Length(stat_width),
                Constraint::Length(stat_width),
                Constraint::Length(stat_width),
                Constraint::Length(stat_width),
                Constraint::Length(stat_width),
            ],
        )
        .header(
            Row::new(vec![
                "Column", "Count", "Mean", "Std", "Median", "Min", "Max",
            ])
            .style(theme.header_style()),
        )
        .style(theme.normal_style());

        frame.render_widget(table, area);

        // Scrollbar
        if columns.len() > viewport_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(columns.len()).position(self.selected_row);

            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    /// Render heatmap tab
    fn render_heatmap(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.heatmap_columns.is_empty() {
            let msg = Paragraph::new("No numeric columns for correlation matrix")
                .style(theme.normal_style())
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        // Show current selection
        let x_col = self
            .heatmap_columns
            .get(self.heatmap_x_idx)
            .map(|s| s.as_str())
            .unwrap_or("");
        let y_col = self
            .heatmap_columns
            .get(self.heatmap_y_idx)
            .map(|s| s.as_str())
            .unwrap_or("");
        let corr = self
            .heatmap_data
            .get(self.heatmap_y_idx)
            .and_then(|row| row.get(self.heatmap_x_idx))
            .copied()
            .unwrap_or(0.0);

        let info = Paragraph::new(format!("Correlation: {} × {} = {:.3}", x_col, y_col, corr))
            .style(theme.normal_style());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        frame.render_widget(info, chunks[0]);

        // Render simple text-based heatmap grid
        let grid_area = chunks[1];
        let n = self.heatmap_columns.len();

        // Simple text rendering of correlation matrix
        let mut lines = Vec::new();

        // Header line with column names (truncated)
        let mut header = String::from("        ");
        for col in &self.heatmap_columns {
            let short = if col.len() > 6 {
                format!("{:>6.6}", col)
            } else {
                format!("{:>6}", col)
            };
            header.push_str(&short);
            header.push(' ');
        }
        lines.push(Line::from(header));

        // Data rows
        for (i, y_col) in self.heatmap_columns.iter().enumerate() {
            let mut row_text = format!("{:>6.6}  ", y_col);

            for j in 0..n {
                let corr = self.heatmap_data[i][j];
                let cell = format!("{:>6.2}", corr);
                row_text.push_str(&cell);
                row_text.push(' ');
            }

            lines.push(Line::from(row_text));
        }

        let paragraph = Paragraph::new(lines).style(theme.normal_style());
        frame.render_widget(paragraph, grid_area);
    }

    /// Render embeddings tab (placeholder)
    fn render_embeddings(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let msg = Paragraph::new("Embeddings feature coming soon")
            .style(theme.normal_style())
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(msg, area);
    }
}

impl Component for DataFrameDetailsDialog {
    fn name(&self) -> &str {
        "DataFrameDetailsDialog"
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Route to map viewer first if active
        if let Some(viewer) = &mut self.map_viewer {
            let keep_open = viewer.handle_action(action)?;
            if !keep_open {
                self.map_viewer = None;
            }
            return Ok(true);
        }

        match action {
            Action::OpenMapViewer => {
                // Create a simple demo map from the selected row
                let pairs: Vec<(String, String)> = match self.active_tab {
                    DetailsTab::UniqueValues => {
                        if let Some((value, count)) = self.unique_values.get(self.selected_row) {
                            vec![
                                ("Value".to_string(), value.clone()),
                                ("Count".to_string(), count.to_string()),
                                ("Type".to_string(), "Unique Value Entry".to_string()),
                            ]
                        } else {
                            vec![]
                        }
                    }
                    DetailsTab::Columns => {
                        if let Some((name, dtype)) = self.column_info.get(self.selected_row) {
                            vec![
                                ("Column Name".to_string(), name.clone()),
                                ("Data Type".to_string(), dtype.clone()),
                            ]
                        } else {
                            vec![]
                        }
                    }
                    DetailsTab::Describe => {
                        // Get stats for selected column
                        let mut columns: Vec<_> = self.describe_stats.keys().cloned().collect();
                        columns.sort();
                        if let Some(col_name) = columns.get(self.selected_row) {
                            if let Some(stats) = self.describe_stats.get(col_name) {
                                vec![
                                    ("Column".to_string(), col_name.clone()),
                                    ("Count".to_string(), stats.count.to_string()),
                                    (
                                        "Mean".to_string(),
                                        stats
                                            .mean
                                            .map(|v| format!("{:.2}", v))
                                            .unwrap_or_else(|| "-".to_string()),
                                    ),
                                    (
                                        "Std Dev".to_string(),
                                        stats
                                            .std
                                            .map(|v| format!("{:.2}", v))
                                            .unwrap_or_else(|| "-".to_string()),
                                    ),
                                    (
                                        "Median".to_string(),
                                        stats
                                            .median
                                            .map(|v| format!("{:.2}", v))
                                            .unwrap_or_else(|| "-".to_string()),
                                    ),
                                    (
                                        "Min".to_string(),
                                        stats
                                            .min
                                            .map(|v| format!("{:.2}", v))
                                            .unwrap_or_else(|| "-".to_string()),
                                    ),
                                    (
                                        "Max".to_string(),
                                        stats
                                            .max
                                            .map(|v| format!("{:.2}", v))
                                            .unwrap_or_else(|| "-".to_string()),
                                    ),
                                ]
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                    _ => vec![],
                };

                if !pairs.is_empty() {
                    let title = format!("Details - {} Tab", self.active_tab.label());
                    let mut viewer = MapViewerDialog::from_pairs(title, pairs);
                    viewer.set_focused(true);
                    self.map_viewer = Some(viewer);
                }
                Ok(true)
            }
            Action::SwitchDetailsTabRight => {
                self.next_tab();
                Ok(true)
            }
            Action::SwitchDetailsTabLeft => {
                self.prev_tab();
                Ok(true)
            }
            Action::MoveUp => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                }
                Ok(true)
            }
            Action::MoveDown => {
                let max_row = match self.active_tab {
                    DetailsTab::UniqueValues => self.unique_values.len().saturating_sub(1),
                    DetailsTab::Columns => self.column_info.len().saturating_sub(1),
                    DetailsTab::Describe => self.describe_stats.len().saturating_sub(1),
                    _ => 0,
                };
                if self.selected_row < max_row {
                    self.selected_row += 1;
                }
                Ok(true)
            }
            Action::PageUp => {
                // Page up by the number of currently visible items
                let total_items = match self.active_tab {
                    DetailsTab::UniqueValues => self.unique_values.len(),
                    DetailsTab::Columns => self.column_info.len(),
                    DetailsTab::Describe => self.describe_stats.len(),
                    _ => 0,
                };

                if total_items == 0 {
                    return Ok(true);
                }

                // Calculate how many items are currently visible
                let visible_start = self.scroll_offset.min(total_items);
                let visible_end = (self.scroll_offset + self.viewport_height).min(total_items);
                let visible_count = visible_end.saturating_sub(visible_start).max(1);

                // Move selection up by the number of visible items
                self.selected_row = self.selected_row.saturating_sub(visible_count);
                Ok(true)
            }
            Action::PageDown => {
                // Page down by the number of currently visible items
                let total_items = match self.active_tab {
                    DetailsTab::UniqueValues => self.unique_values.len(),
                    DetailsTab::Columns => self.column_info.len(),
                    DetailsTab::Describe => self.describe_stats.len(),
                    _ => 0,
                };

                if total_items == 0 {
                    return Ok(true);
                }

                // Calculate how many items are currently visible
                let visible_start = self.scroll_offset.min(total_items);
                let visible_end = (self.scroll_offset + self.viewport_height).min(total_items);
                let visible_count = visible_end.saturating_sub(visible_start).max(1);

                // Move selection down by the number of visible items
                let max_row = total_items.saturating_sub(1);
                self.selected_row = (self.selected_row + visible_count).min(max_row);
                Ok(true)
            }
            Action::MoveLeft => {
                if self.active_tab == DetailsTab::UniqueValues {
                    self.change_column(-1);
                }
                Ok(true)
            }
            Action::MoveRight => {
                if self.active_tab == DetailsTab::UniqueValues {
                    self.change_column(1);
                }
                Ok(true)
            }
            Action::ToggleDetailsSort => {
                self.toggle_sort();
                Ok(true)
            }
            Action::Cancel => Ok(false), // Close dialog
            _ => Ok(true),               // Consume other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.render_dialog(frame, area);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::SwitchDetailsTabLeft,
            Action::SwitchDetailsTabRight,
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::PageUp,
            Action::PageDown,
            Action::ToggleDetailsSort,
            Action::OpenMapViewer,
            Action::Cancel,
        ]
    }
}

impl Focusable for DataFrameDetailsDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
