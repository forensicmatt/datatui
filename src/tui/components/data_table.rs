use crate::core::ManagedDataset;
use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use duckdb::arrow::array::Array;
use duckdb::arrow::array::{Float64Array, Int64Array, StringArray};
use duckdb::arrow::datatypes::DataType;
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table},
    Frame,
};

/// Per-column configuration
#[derive(Debug, Clone)]
pub struct ColumnConfig {
    pub name: String,
    pub auto_size: bool,          // Calculate width from content?
    pub fixed_width: Option<u16>, // Manual override
    pub min_width: u16,           // Minimum constraint
    pub max_width: u16,           // Maximum constraint
    pub visible: bool,            // Show/hide column
    pub order: usize,             // Display order (future: reordering)
}

impl Default for ColumnConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            auto_size: true,
            fixed_width: None,
            min_width: 8,
            max_width: 50,
            visible: true,
            order: 0,
        }
    }
}

/// Viewport/display preferences
#[derive(Debug, Clone)]
pub struct ViewportConfig {
    pub auto_expand: bool,   // Expand columns to fill space?
    pub sample_size: usize,  // Rows to sample (default: 100)
    pub column_padding: u16, // Spaces around content (default: 2)
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            auto_expand: true,
            sample_size: 100,
            column_padding: 2,
        }
    }
}

/// Position in the table (row, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

/// Viewport into the dataset
#[derive(Debug, Clone)]
pub struct Viewport {
    pub top: usize,          // First visible row
    pub left_col: usize,     // First visible column (NEW)
    pub height: usize,       // Visible rows
    pub width: u16,          // Available width in chars (changed from usize)
    pub visible_cols: usize, // Number of columns that fit (NEW)
}

/// DataTable component for displaying tabular data
///
/// Manages viewport, cursor, and rendering of dataset contents.
/// Supports keyboard navigation and lazy loading of data.
pub struct DataTable {
    dataset: ManagedDataset,
    cursor: Position,
    viewport: Viewport,
    focused: bool,
    supported_actions: Vec<Action>,

    // Column configuration
    column_configs: Vec<ColumnConfig>,
    viewport_config: ViewportConfig,

    // Caching
    calculated_widths: Option<Vec<u16>>,
    cache_valid: bool,
}

impl DataTable {
    /// Create a new DataTable for the given dataset
    pub fn new(dataset: ManagedDataset) -> Result<Self> {
        // Initialize column configs from dataset
        let column_names = dataset.column_names()?;
        let column_configs = column_names
            .iter()
            .enumerate()
            .map(|(idx, name)| ColumnConfig {
                name: name.clone(),
                order: idx,
                ..Default::default()
            })
            .collect();

        Ok(Self {
            dataset,
            cursor: Position { row: 0, col: 0 },
            viewport: Viewport {
                top: 0,
                left_col: 0,
                height: 20,
                width: 100,
                visible_cols: 0,
            },
            focused: false,
            supported_actions: vec![
                Action::MoveUp,
                Action::MoveDown,
                Action::MoveLeft,
                Action::MoveRight,
                Action::PageUp,
                Action::PageDown,
                Action::Home,
                Action::End,
                Action::GoToTop,
                Action::GoToBottom,
                Action::Copy,
                Action::CopyWithHeaders,
            ],
            column_configs,
            viewport_config: ViewportConfig::default(),
            calculated_widths: None,
            cache_valid: false,
        })
    }

    /// Get total row count from dataset
    fn row_count(&self) -> Result<usize> {
        self.dataset.row_count()
    }

    /// Get total column count from dataset
    fn column_count(&self) -> Result<usize> {
        self.dataset.column_count()
    }

    /// Update viewport based on terminal area
    fn update_viewport(&mut self, area: Rect) {
        // Account for borders and header
        self.viewport.height = (area.height.saturating_sub(3)) as usize; // -3 for borders and header

        // Account for borders (-2) and vertical scrollbar if needed (-1)
        let total_rows = self.row_count().unwrap_or(0);
        let scrollbar_width = if total_rows > self.viewport.height {
            1
        } else {
            0
        };
        self.viewport.width = area.width.saturating_sub(2 + scrollbar_width);
    }

    /// Ensure cursor is within viewport
    fn ensure_cursor_visible(&mut self) {
        // Vertical scrolling
        if self.cursor.row < self.viewport.top {
            self.viewport.top = self.cursor.row;
        } else if self.cursor.row >= self.viewport.top + self.viewport.height {
            self.viewport.top = self.cursor.row.saturating_sub(self.viewport.height - 1);
        }

        // Horizontal scrolling
        // Note: visible_cols is calculated during render based on left_col,
        // so we need to be careful about using it here
        if self.cursor.col < self.viewport.left_col {
            // Cursor moved left of viewport - scroll left
            self.viewport.left_col = self.cursor.col;
            self.cache_valid = false; // Invalidate cache since viewport changed
        } else if self.viewport.visible_cols > 0
            && self.cursor.col >= self.viewport.left_col + self.viewport.visible_cols
        {
            // Cursor moved right of viewport - scroll right
            // Move left_col forward, but conservatively to ensure cursor is visible
            self.viewport.left_col = self
                .cursor
                .col
                .saturating_sub(self.viewport.visible_cols - 1);
            self.cache_valid = false; // Invalidate cache since viewport changed
        }
    }

    /// Move cursor up
    fn move_up(&mut self) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor down
    fn move_down(&mut self) -> Result<()> {
        let row_count = self.row_count()?;
        if row_count > 0 && self.cursor.row < row_count - 1 {
            self.cursor.row += 1;
            self.ensure_cursor_visible();
        }
        Ok(())
    }

    /// Move cursor left
    fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor right
    fn move_right(&mut self) -> Result<()> {
        let col_count = self.column_count()?;
        if col_count > 0 && self.cursor.col < col_count - 1 {
            self.cursor.col += 1;
            self.ensure_cursor_visible();
        }
        Ok(())
    }

    /// Page up
    fn page_up(&mut self) {
        self.cursor.row = self.cursor.row.saturating_sub(self.viewport.height);
        self.ensure_cursor_visible();
    }

    /// Page down
    fn page_down(&mut self) -> Result<()> {
        let row_count = self.row_count()?;
        if row_count > 0 {
            self.cursor.row = (self.cursor.row + self.viewport.height).min(row_count - 1);
            self.ensure_cursor_visible();
        }
        Ok(())
    }

    /// Go to first row
    fn go_to_top(&mut self) {
        self.cursor.row = 0;
        self.ensure_cursor_visible();
    }

    /// Go to last row
    fn go_to_bottom(&mut self) -> Result<()> {
        let row_count = self.row_count()?;
        if row_count > 0 {
            self.cursor.row = row_count - 1;
            self.ensure_cursor_visible();
        }
        Ok(())
    }

    /// Go to start of row
    fn go_home(&mut self) {
        self.cursor.col = 0;
        self.ensure_cursor_visible();
    }

    /// Go to end of row
    fn go_end(&mut self) -> Result<()> {
        let col_count = self.column_count()?;
        if col_count > 0 {
            self.cursor.col = col_count - 1;
            self.ensure_cursor_visible();
        }
        Ok(())
    }

    /// Get information about the currently selected cell
    pub fn get_current_cell_info(&self) -> Result<super::CellInfo> {
        let column_names = self.dataset.column_names()?;
        let column_name = column_names
            .get(self.cursor.col)
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        // Fetch the data for the current cell
        let batch = self.dataset.get_page(self.cursor.row, 1)?;

        let (value, data_type) = if self.cursor.col < batch.num_columns() {
            let column = batch.column(self.cursor.col);
            let value_str = self.format_cell_value(column, 0); // row 0 since we fetched just 1 row
            let type_str = format!("{:?}", column.data_type());
            (value_str, Some(type_str))
        } else {
            ("Error: Column out of bounds".to_string(), None)
        };

        Ok(super::CellInfo::new(
            self.cursor.row,
            self.cursor.col,
            column_name,
            value,
            data_type,
        ))
    }

    /// Navigate to a specific cell by row and column name
    pub fn goto_cell(&mut self, row: usize, column_name: &str) -> Result<()> {
        let column_names = self.dataset.column_names()?;
        if let Some(col_idx) = column_names.iter().position(|c| c == column_name) {
            let row_count = self.row_count()?;
            self.cursor.row = row.min(row_count.saturating_sub(1));
            self.cursor.col = col_idx.min(column_names.len().saturating_sub(1));
            self.ensure_cursor_visible();
            // Invalidate cache to force re-render with new cursor position
            self.cache_valid = false;
        }
        Ok(())
    }

    /// Get current cursor posit (row, col_index)
    pub fn get_cursor_position(&self) -> (usize, usize) {
        (self.cursor.row, self.cursor.col)
    }

    /// Get reference to dataset (for search operations)
    pub fn dataset(&self) -> &crate::core::ManagedDataset {
        &self.dataset
    }

    // Column Width Management

    /// Calculate column widths based on content and configuration
    fn calculate_widths(&mut self) -> Result<Vec<u16>> {
        let visible_configs: Vec<_> = self
            .column_configs
            .iter()
            .enumerate()
            .filter(|(_, c)| c.visible)
            .collect();

        let mut widths = Vec::new();

        // Sample data for width calculation
        let sample_size = self
            .viewport_config
            .sample_size
            .min(self.row_count()? as usize);
        let sample_batch = if sample_size > 0 {
            Some(self.dataset.get_page(0, sample_size)?)
        } else {
            None
        };

        for (col_idx, config) in &visible_configs {
            let width = if let Some(fixed) = config.fixed_width {
                // Use fixed width
                fixed
            } else if config.auto_size {
                // Calculate from content
                let mut max_width = config.name.len() as u16;

                if let Some(ref batch) = sample_batch {
                    if *col_idx < batch.num_columns() {
                        let column = batch.column(*col_idx);
                        for row_idx in 0..batch.num_rows() {
                            let value_str = self.format_cell_value(column, row_idx);
                            max_width = max_width.max(value_str.len() as u16);
                        }
                    }
                }

                // Apply padding
                max_width += self.viewport_config.column_padding;

                // Apply constraints
                max_width.max(config.min_width).min(config.max_width)
            } else {
                // Fallback to min width
                config.min_width
            };

            widths.push(width);
        }

        // Auto-expand if enabled
        if self.viewport_config.auto_expand {
            self.distribute_remaining_space(&mut widths);
        }

        Ok(widths)
    }

    /// Distribute remaining space among columns
    fn distribute_remaining_space(&self, widths: &mut [u16]) {
        let total: u16 = widths.iter().sum();
        let remaining = self.viewport.width.saturating_sub(total);

        if remaining > 0 && !widths.is_empty() {
            let per_col = remaining / widths.len() as u16;
            for width in widths.iter_mut() {
                *width += per_col;
            }
        }
    }

    /// Get or calculate column widths (with caching)
    fn get_or_calculate_widths(&mut self) -> Result<Vec<u16>> {
        if self.cache_valid {
            if let Some(ref widths) = self.calculated_widths {
                return Ok(widths.clone());
            }
        }

        let widths = self.calculate_widths()?;
        self.calculated_widths = Some(widths.clone());
        self.cache_valid = true;
        Ok(widths)
    }

    /// Invalidate width cache
    fn invalidate_width_cache(&mut self) {
        self.cache_valid = false;
    }

    /// Calculate how many columns fit in viewport
    /// Note: Ratatui's Table widget adds 1 space between columns as a separator
    fn calculate_visible_columns(&mut self, column_widths: &[u16]) {
        let mut accumulated_width = 0u16;
        let mut visible_count = 0;

        for width in column_widths.iter().skip(self.viewport.left_col) {
            // Calculate width including inter-column spacing (1 space between columns)
            let width_with_spacing = if visible_count > 0 {
                width + 1 // Add 1 for the space separator before this column
            } else {
                *width // First column has no separator before it
            };

            if accumulated_width + width_with_spacing > self.viewport.width {
                break;
            }
            accumulated_width += width_with_spacing;
            visible_count += 1;
        }

        self.viewport.visible_cols = visible_count.max(1); // Always show at least 1 column
    }

    /// Format a cell value from an Arrow array
    fn format_cell_value(&self, column: &dyn Array, row_idx: usize) -> String {
        match column.data_type() {
            DataType::Int64 => {
                let array = column.as_any().downcast_ref::<Int64Array>().unwrap();
                if array.is_null(row_idx) {
                    "NULL".to_string()
                } else {
                    array.value(row_idx).to_string()
                }
            }
            DataType::Utf8 => {
                let array = column.as_any().downcast_ref::<StringArray>().unwrap();
                if array.is_null(row_idx) {
                    "NULL".to_string()
                } else {
                    array.value(row_idx).to_string()
                }
            }
            DataType::Float64 => {
                let array = column.as_any().downcast_ref::<Float64Array>().unwrap();
                if array.is_null(row_idx) {
                    "NULL".to_string()
                } else {
                    array.value(row_idx).to_string()
                }
            }
            _ => {
                // Fallback for other types
                format!("{:?}", column.slice(row_idx, 1))
            }
        }
    }
}

impl Component for DataTable {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::MoveUp => {
                self.move_up();
                Ok(true)
            }
            Action::MoveDown => {
                self.move_down()?;
                Ok(true)
            }
            Action::MoveLeft => {
                self.move_left();
                Ok(true)
            }
            Action::MoveRight => {
                self.move_right()?;
                Ok(true)
            }
            Action::PageUp => {
                self.page_up();
                Ok(true)
            }
            Action::PageDown => {
                self.page_down()?;
                Ok(true)
            }
            Action::GoToTop => {
                self.go_to_top();
                Ok(true)
            }
            Action::GoToBottom => {
                self.go_to_bottom()?;
                Ok(true)
            }
            Action::Home => {
                self.go_home();
                Ok(true)
            }
            Action::End => {
                self.go_end()?;
                Ok(true)
            }
            // Other actions not handled
            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.update_viewport(area);

        let theme = Theme::default();

        // Calculate column widths
        let all_widths = match self.get_or_calculate_widths() {
            Ok(w) => w,
            Err(_) => {
                let error_block = Block::default().borders(Borders::ALL).title("Error");
                frame.render_widget(error_block, area);
                return;
            }
        };

        // Calculate visible columns
        self.calculate_visible_columns(&all_widths);

        // Verify cursor is actually visible after calculation
        // (ensure_cursor_visible uses a potentially stale visible_cols value)
        let end_col = (self.viewport.left_col + self.viewport.visible_cols).min(all_widths.len());
        if self.cursor.col >= end_col {
            // Cursor is beyond the visible range - adjust left_col to show cursor
            // Start from cursor column and work backwards to fit as many columns as possible
            self.viewport.left_col = self.cursor.col;

            // Recalculate visible columns from the new position
            self.calculate_visible_columns(&all_widths);

            // If only 1 column fits, try backing up to show more context
            if self.viewport.visible_cols == 1 && self.viewport.left_col > 0 {
                // Try to show cursor column plus some columns before it
                let mut test_left = self.cursor.col.saturating_sub(3); // Try to show 4 columns
                loop {
                    self.viewport.left_col = test_left;
                    self.calculate_visible_columns(&all_widths);
                    let test_end =
                        (self.viewport.left_col + self.viewport.visible_cols).min(all_widths.len());

                    if self.cursor.col < test_end {
                        // Cursor is visible with this left_col
                        break;
                    }

                    if test_left >= self.cursor.col {
                        // Can't go further right
                        break;
                    }

                    test_left += 1;
                }
            }
        }

        // Get visible column range (recalculate after potential adjustment)
        let end_col = (self.viewport.left_col + self.viewport.visible_cols).min(all_widths.len());
        let visible_widths = &all_widths[self.viewport.left_col..end_col];

        // Fetch column names for visible range
        let all_columns = match self.dataset.column_names() {
            Ok(names) => names,
            Err(_) => vec!["Error".to_string()],
        };
        // Ensure end_col doesn't exceed actual column count
        let safe_end_col = end_col.min(all_columns.len());
        let visible_columns =
            &all_columns[self.viewport.left_col.min(all_columns.len())..safe_end_col];

        // Create header from visible columns
        let header_cells: Vec<Cell> = visible_columns
            .iter()
            .map(|name| Cell::from(name.as_str()))
            .collect();
        let header = Row::new(header_cells).style(theme.header_style());

        // Fetch visible rows from dataset
        let rows: Vec<Row> = match self
            .dataset
            .get_page(self.viewport.top, self.viewport.height)
        {
            Ok(batch) => {
                let mut result_rows = Vec::new();

                // Get number of rows in this batch
                let num_rows = batch.num_rows();

                // For each row
                for row_idx in 0..num_rows {
                    let mut cells = Vec::new();

                    // Only render visible columns
                    for col_offset in 0..self.viewport.visible_cols {
                        let actual_col_idx = self.viewport.left_col + col_offset;
                        if actual_col_idx >= batch.num_columns() {
                            break;
                        }

                        let column = batch.column(actual_col_idx);
                        let value = self.format_cell_value(column, row_idx);

                        // Check if this is the currently selected cell
                        let is_selected_cell = self.viewport.top + row_idx == self.cursor.row
                            && actual_col_idx == self.cursor.col;

                        let cell = if is_selected_cell {
                            Cell::from(value).style(theme.selected_cell_style())
                        } else {
                            Cell::from(value)
                        };

                        cells.push(cell);
                    }

                    // Highlight selected row
                    let row_style = if self.viewport.top + row_idx == self.cursor.row {
                        theme.selected_style()
                    } else if row_idx % 2 == 1 {
                        theme.alt_row_style()
                    } else {
                        theme.normal_style()
                    };

                    result_rows.push(Row::new(cells).style(row_style));
                }

                result_rows
            }
            Err(_) => {
                // Error fetching data - show error row
                vec![Row::new(vec![Cell::from("Error loading data")])]
            }
        };

        // Convert widths to ratatui constraints
        // Use Min for the last column so it expands to fill remaining space
        let constraints: Vec<ratatui::layout::Constraint> = visible_widths
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                if i == visible_widths.len() - 1 {
                    // Last column: use Min to fill remaining space
                    ratatui::layout::Constraint::Min(w)
                } else {
                    // Other columns: use exact Length
                    ratatui::layout::Constraint::Length(w)
                }
            })
            .collect();

        let table_block = Block::default()
            .borders(Borders::ALL)
            .title(
                if self.viewport.left_col > 0 || end_col < all_columns.len() {
                    format!(
                        "Data Table [{}/{}] ← Cols {}-{} of {} →",
                        self.cursor.row + 1,
                        self.row_count().unwrap_or(0),
                        self.viewport.left_col + 1,
                        end_col,
                        all_columns.len()
                    )
                } else {
                    format!(
                        "Data Table [{}/{}]",
                        self.cursor.row + 1,
                        self.row_count().unwrap_or(0)
                    )
                },
            )
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            });

        let table = Table::new(rows, constraints)
            .header(header)
            .block(table_block);

        frame.render_widget(table, area);

        // Render vertical scrollbar
        let total_rows = self.row_count().unwrap_or(0);
        if total_rows > self.viewport.height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(total_rows).position(self.cursor.row);

            // Scrollbar area is the right edge of the table area
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1, // Skip top border
                width: 1,
                height: area.height.saturating_sub(2), // Skip top and bottom borders
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }

        // Render horizontal scrollbar
        let total_cols = self.column_count().unwrap_or(0);
        if total_cols > self.viewport.visible_cols {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(Some("←"))
                .end_symbol(Some("→"));

            let mut scrollbar_state = ScrollbarState::new(total_cols).position(self.cursor.col);

            // Scrollbar area is the bottom edge of the table area
            let scrollbar_area = Rect {
                x: area.x + 1, // Skip left border
                y: area.y + area.height.saturating_sub(1),
                width: area.width.saturating_sub(2), // Skip left and right borders
                height: 1,
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &self.supported_actions
    }

    fn name(&self) -> &str {
        "DataTable"
    }
}

impl Focusable for DataTable {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CsvImportOptions;
    use crate::services::DataService;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_dataset() -> (ManagedDataset, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create isolated global DB for this test
        let global_db = workspace_path.join("global_test.duckdb");

        // Create CSV file
        let csv_path = workspace_path.join("test.csv");
        let mut file = std::fs::File::create(&csv_path).unwrap();
        writeln!(file, "id,name,value").unwrap();
        writeln!(file, "1,Alice,100").unwrap();
        writeln!(file, "2,Bob,200").unwrap();
        writeln!(file, "3,Charlie,300").unwrap();
        drop(file);

        // Use DataService with isolated global DB to import
        let service = DataService::new_impl(workspace_path, Some(global_db)).unwrap();
        let options = CsvImportOptions::default();
        let dataset_id = service.import_csv(csv_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        // Keep both dataset and temp_dir alive
        (dataset, temp_dir)
    }

    #[test]
    fn test_cursor_movement() {
        let (dataset, _temp_dir) = create_test_dataset();
        let mut table = DataTable::new(dataset).unwrap();

        // Initial position
        assert_eq!(table.cursor.row, 0);
        assert_eq!(table.cursor.col, 0);

        // Move down
        table.handle_action(Action::MoveDown).unwrap();
        assert_eq!(table.cursor.row, 1);

        // Move right
        table.handle_action(Action::MoveRight).unwrap();
        assert_eq!(table.cursor.col, 1);

        // Move up
        table.handle_action(Action::MoveUp).unwrap();
        assert_eq!(table.cursor.row, 0);

        // Move left
        table.handle_action(Action::MoveLeft).unwrap();
        assert_eq!(table.cursor.col, 0);
    }

    #[test]
    fn test_go_to_top_bottom() {
        let (dataset, _temp_dir) = create_test_dataset();
        let mut table = DataTable::new(dataset).unwrap();

        // Go to bottom
        table.handle_action(Action::GoToBottom).unwrap();
        assert_eq!(table.cursor.row, 2); // 3 rows (0-indexed)

        // Go to top
        table.handle_action(Action::GoToTop).unwrap();
        assert_eq!(table.cursor.row, 0);
    }

    #[test]
    fn test_home_end() {
        let (dataset, _temp_dir) = create_test_dataset();
        let mut table = DataTable::new(dataset).unwrap();

        // Go to end
        table.handle_action(Action::End).unwrap();
        assert_eq!(table.cursor.col, 2); // 3 columns (0-indexed)

        // Go to home
        table.handle_action(Action::Home).unwrap();
        assert_eq!(table.cursor.col, 0);
    }

    #[test]
    fn test_focus() {
        let (dataset, _temp_dir) = create_test_dataset();
        let mut table = DataTable::new(dataset).unwrap();

        assert!(!table.is_focused());
        table.set_focused(true);
        assert!(table.is_focused());
    }

    #[test]
    fn test_supported_actions() {
        let (dataset, _temp_dir) = create_test_dataset();
        let table = DataTable::new(dataset).unwrap();

        let actions = table.supported_actions();
        assert!(actions.contains(&Action::MoveUp));
        assert!(actions.contains(&Action::MoveDown));
        assert!(actions.contains(&Action::Copy));
    }
}
