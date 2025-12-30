use crate::core::ManagedDataset;
use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use duckdb::arrow::array::Array;
use duckdb::arrow::array::{Float64Array, Int64Array, StringArray};
use duckdb::arrow::datatypes::DataType;
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

/// Position in the table (row, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

/// Viewport into the dataset
#[derive(Debug, Clone)]
pub struct Viewport {
    pub top: usize,    // First visible row
    pub left: usize,   // First visible column
    pub height: usize, // Visible rows
    pub width: usize,  // Visible columns
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
}

impl DataTable {
    /// Create a new DataTable for the given dataset
    pub fn new(dataset: ManagedDataset) -> Self {
        Self {
            dataset,
            cursor: Position { row: 0, col: 0 },
            viewport: Viewport {
                top: 0,
                left: 0,
                height: 20, // Will be updated based on terminal size
                width: 10,
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
        }
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
        self.viewport.width = (area.width.saturating_sub(2)) as usize; // -2 for borders
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
        if self.cursor.col < self.viewport.left {
            self.viewport.left = self.cursor.col;
        } else if self.cursor.col >= self.viewport.left + self.viewport.width {
            self.viewport.left = self.cursor.col.saturating_sub(self.viewport.width - 1);
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

        // Fetch actual data from dataset
        let column_names = match self.dataset.column_names() {
            Ok(names) => names,
            Err(_) => vec!["Error".to_string()],
        };

        // Create header
        let header_cells: Vec<Cell> = column_names
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

                    // For each column
                    for col in batch.columns() {
                        // Convert array value to string
                        let value = match col.data_type() {
                            DataType::Int64 => {
                                let array = col.as_any().downcast_ref::<Int64Array>().unwrap();
                                if array.is_null(row_idx) {
                                    "NULL".to_string()
                                } else {
                                    array.value(row_idx).to_string()
                                }
                            }
                            DataType::Utf8 => {
                                let array = col.as_any().downcast_ref::<StringArray>().unwrap();
                                if array.is_null(row_idx) {
                                    "NULL".to_string()
                                } else {
                                    array.value(row_idx).to_string()
                                }
                            }
                            DataType::Float64 => {
                                let array = col.as_any().downcast_ref::<Float64Array>().unwrap();
                                if array.is_null(row_idx) {
                                    "NULL".to_string()
                                } else {
                                    array.value(row_idx).to_string()
                                }
                            }
                            _ => {
                                // Fallback for other types
                                format!("{:?}", col.slice(row_idx, 1))
                            }
                        };

                        cells.push(Cell::from(value));
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

        // Calculate column widths (equal distribution for now)
        let num_cols = column_names.len().max(1);
        let col_width = ratatui::layout::Constraint::Percentage((100 / num_cols) as u16);
        let constraints = vec![col_width; num_cols];

        let table = Table::new(rows, constraints).header(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Data Table [{}/{}]",
                    self.cursor.row + 1,
                    self.row_count().unwrap_or(0)
                ))
                .border_style(if self.focused {
                    theme.focused_border_style()
                } else {
                    theme.border_style()
                }),
        );

        frame.render_widget(table, area);
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
        let mut table = DataTable::new(dataset);

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
        let mut table = DataTable::new(dataset);

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
        let mut table = DataTable::new(dataset);

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
        let mut table = DataTable::new(dataset);

        assert!(!table.is_focused());
        table.set_focused(true);
        assert!(table.is_focused());
    }

    #[test]
    fn test_supported_actions() {
        let (dataset, _temp_dir) = create_test_dataset();
        let table = DataTable::new(dataset);

        let actions = table.supported_actions();
        assert!(actions.contains(&Action::MoveUp));
        assert!(actions.contains(&Action::MoveDown));
        assert!(actions.contains(&Action::Copy));
    }
}
