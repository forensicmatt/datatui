use crate::core::column_config::ColumnWidthConfig;
use crate::core::types::DatasetId;
use color_eyre::Result;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Managed dataset backed by DuckDB
///
/// This struct represents a dataset stored as a Parquet file and queried via DuckDB.
/// It provides methods for pagination, querying, and metadata access without loading
/// the entire dataset into memory.
pub struct ManagedDataset {
    conn: Arc<Connection>,
    pub id: DatasetId,
    pub table_name: String,
    pub parquet_path: PathBuf,
    column_config: ColumnWidthConfig,
}

impl ManagedDataset {
    /// Create a new managed dataset
    ///
    /// Registers the Parquet file as a table in DuckDB for querying
    pub fn new(conn: Arc<Connection>, id: DatasetId, parquet_path: PathBuf) -> Result<Self> {
        // Create a valid table name from the dataset ID (replace hyphens with underscores)
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        // Register Parquet file as a view in DuckDB
        // Using a view means we don't copy data, just query the file directly
        conn.execute(
            &format!(
                "CREATE OR REPLACE VIEW {} AS SELECT * FROM read_parquet('{}')",
                table_name,
                parquet_path.display()
            ),
            [],
        )?;

        // Initialize column configuration
        let columns = Self::get_columns_from_parquet(&conn, &table_name)?;
        let column_config = ColumnWidthConfig::from_columns(columns);

        Ok(Self {
            conn,
            id,
            table_name,
            parquet_path,
            column_config,
        })
    }

    /// Helper to get column names from a parquet view
    fn get_columns_from_parquet(conn: &Connection, table_name: &str) -> Result<Vec<String>> {
        let query = format!("DESCRIBE {}", table_name);
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get a page of data for display
    ///
    /// Uses LIMIT/OFFSET for efficient pagination without loading full dataset
    pub fn get_page(&self, offset: usize, limit: usize) -> Result<RecordBatch> {
        let query = format!(
            "SELECT * FROM {} LIMIT {} OFFSET {}",
            self.table_name, limit, offset
        );

        let mut stmt = self.conn.prepare(&query)?;
        let batches = stmt.query_arrow([])?.collect::<Vec<_>>();

        // Combine all batches into one (DuckDB may return multiple small batches)
        if batches.is_empty() {
            // Return empty batch with schema from table
            let empty_query = format!("SELECT * FROM {} LIMIT 0", self.table_name);
            let mut stmt = self.conn.prepare(&empty_query)?;
            let mut arrow = stmt.query_arrow([])?;
            match arrow.next() {
                Some(batch) => Ok(batch),
                None => Err(color_eyre::eyre::eyre!("Failed to get schema")),
            }
        } else {
            // For now, return first batch
            // TODO: Combine batches if needed
            Ok(batches[0].clone())
        }
    }

    /// Get total row count
    pub fn row_count(&self) -> Result<usize> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name);
        let count: i64 = self.conn.query_row(&query, [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get column names
    pub fn column_names(&self) -> Result<Vec<String>> {
        let query = format!("DESCRIBE {}", self.table_name);
        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get column count
    pub fn column_count(&self) -> Result<usize> {
        Ok(self.column_names()?.len())
    }

    /// Execute arbitrary SQL query against this dataset
    pub fn query_sql(&self, sql: &str) -> Result<RecordBatch> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches = stmt.query_arrow([])?.collect::<Vec<_>>();

        if batches.is_empty() {
            return Err(color_eyre::eyre::eyre!("Query returned no results"));
        }

        Ok(batches[0].clone())
    }

    /// Execute arbitrary SQL query with parameters (SECURE - prevents SQL injection)
    ///
    /// Use this method when user input or untrusted data needs to be included in queries.
    /// Parameters are safely bound using DuckDB's prepared statement mechanism.
    ///
    /// # Security
    /// - User-provided values should ALWAYS go through `params`, never string formatting
    /// - Column/table names cannot be parameterized (use `quote_identifier` if needed)
    ///
    /// # Example
    /// ```ignore
    /// let result = dataset.query_sql_with_params(
    ///     "SELECT * FROM table WHERE name LIKE ?",
    ///     &[&format!("%{}%", user_pattern)]
    /// )?;
    /// ```
    pub fn query_sql_with_params(
        &self,
        sql: &str,
        params: &[&dyn duckdb::ToSql],
    ) -> Result<RecordBatch> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches = stmt.query_arrow(params)?.collect::<Vec<_>>();

        if batches.is_empty() {
            return Err(color_eyre::eyre::eyre!("Query returned no results"));
        }

        Ok(batches[0].clone())
    }

    /// Execute query with {table} placeholder substitution
    ///
    /// Replaces all occurrences of {table} with the actual table name
    pub fn execute_query(&self, query_template: &str) -> Result<RecordBatch> {
        let query = query_template.replace("{table}", &self.table_name);
        self.query_sql(&query)
    }

    /// Execute query with {table} placeholder and parameters (SECURE)
    ///
    /// Combines table name substitution with parameter binding for maximum safety.
    /// Use this for queries against dataset tables that include user-provided values.
    ///
    /// # Security
    /// - Table names are substituted (safe - comes from internal state)
    /// - User values go through `params` (safe - properly bound)
    ///
    /// # Example
    /// ```ignore
    /// let result = dataset.execute_query_with_params(
    ///     "SELECT * FROM {table} WHERE column LIKE ?",
    ///     &[&search_pattern]
    /// )?;
    /// ```
    pub fn execute_query_with_params(
        &self,
        query_template: &str,
        params: &[&dyn duckdb::ToSql],
    ) -> Result<RecordBatch> {
        let query = query_template.replace("{table}", &self.table_name);
        self.query_sql_with_params(&query, params)
    }

    // ========================================
    // Column Configuration API
    // ========================================

    /// Get the current column configuration
    pub fn get_column_config(&self) -> &ColumnWidthConfig {
        &self.column_config
    }

    /// Set the entire column configuration
    pub fn set_column_config(&mut self, config: ColumnWidthConfig) -> Result<()> {
        let columns = self.column_names()?;

        // Validate config against current columns
        if !config.validate(&columns) {
            return Err(color_eyre::eyre::eyre!("Invalid column configuration"));
        }

        self.column_config = config;
        Ok(())
    }

    /// Reset column configuration to default
    pub fn reset_column_config(&mut self) {
        if let Ok(columns) = self.column_names() {
            self.column_config = ColumnWidthConfig::from_columns(columns);
        }
    }

    /// Set width for a column (None = auto)
    pub fn set_column_width(&mut self, column: &str, width: Option<u16>) -> Result<()> {
        // Validate column exists
        let columns = self.column_names()?;
        if !columns.contains(&column.to_string()) {
            return Err(color_eyre::eyre::eyre!(
                "Column '{}' does not exist",
                column
            ));
        }

        // Validate width range
        if let Some(w) = width {
            if !(4..=255).contains(&w) {
                return Err(color_eyre::eyre::eyre!("Width must be between 4 and 255"));
            }
        }

        // Set or remove width
        if let Some(w) = width {
            self.column_config
                .manual_widths
                .insert(column.to_string(), w);
        } else {
            self.column_config.manual_widths.remove(column);
        }

        Ok(())
    }

    /// Get width for a column
    pub fn get_column_width(&self, column: &str) -> Option<u16> {
        self.column_config.get_effective_width(column)
    }

    /// Set auto-expand mode
    pub fn set_auto_expand(&mut self, enabled: bool) {
        self.column_config.auto_expand = enabled;
    }

    /// Get auto-expand mode
    pub fn get_auto_expand(&self) -> bool {
        self.column_config.auto_expand
    }

    /// Lock all columns to specific widths (used when disabling auto-expand)
    pub fn lock_all_column_widths(&mut self, current_widths: HashMap<String, u16>) {
        for (col, width) in current_widths {
            if !self.column_config.manual_widths.contains_key(&col) {
                self.column_config.manual_widths.insert(col, width);
            }
        }
    }

    /// Set column visibility
    pub fn set_column_visible(&mut self, column: &str, visible: bool) -> Result<()> {
        // Validate column exists
        let columns = self.column_names()?;
        if !columns.contains(&column.to_string()) {
            return Err(color_eyre::eyre::eyre!(
                "Column '{}' does not exist",
                column
            ));
        }

        // Check if hiding this column would hide all columns
        if !visible {
            let would_be_visible: Vec<_> = columns
                .iter()
                .filter(|c| {
                    if *c == column {
                        false
                    } else {
                        self.column_config.is_column_visible(c)
                    }
                })
                .collect();

            if would_be_visible.is_empty() {
                return Err(color_eyre::eyre::eyre!(
                    "Cannot hide all columns. At least one must be visible."
                ));
            }
        }

        self.column_config
            .hidden_columns
            .insert(column.to_string(), !visible);
        Ok(())
    }

    /// Check if column is visible
    pub fn is_column_visible(&self, column: &str) -> bool {
        self.column_config.is_column_visible(column)
    }

    /// Get list of visible columns in display order
    pub fn get_visible_columns(&self) -> Vec<String> {
        self.column_config.get_visible_columns()
    }

    /// Get list of hidden columns
    pub fn get_hidden_columns(&self) -> Vec<String> {
        self.column_config.get_hidden_columns()
    }

    /// Reorder columns
    pub fn reorder_columns(&mut self, new_order: Vec<String>) -> Result<()> {
        let columns = self.column_names()?;

        // Validate: new_order must contain exactly the same columns
        if new_order.len() != columns.len() {
            return Err(color_eyre::eyre::eyre!(
                "Column count mismatch: expected {}, got {}",
                columns.len(),
                new_order.len()
            ));
        }

        for col in &columns {
            if !new_order.contains(col) {
                return Err(color_eyre::eyre::eyre!(
                    "Missing column '{}' in new order",
                    col
                ));
            }
        }

        self.column_config.column_order = new_order;
        Ok(())
    }

    /// Move a column to a new position
    pub fn move_column(&mut self, column: &str, new_index: usize) -> Result<()> {
        let current_order = &mut self.column_config.column_order;

        // Find current position
        let current_pos = current_order
            .iter()
            .position(|c| c == column)
            .ok_or_else(|| color_eyre::eyre::eyre!("Column '{}' not found", column))?;

        // Validate new index
        if new_index >= current_order.len() {
            return Err(color_eyre::eyre::eyre!(
                "Invalid index {}: must be < {}",
                new_index,
                current_order.len()
            ));
        }

        // Remove and reinsert
        let col_name = current_order.remove(current_pos);
        current_order.insert(new_index, col_name);

        Ok(())
    }
}

// Clone implementation for sharing datasets across threads
impl Clone for ManagedDataset {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            id: self.id.clone(),
            table_name: self.table_name.clone(),
            parquet_path: self.parquet_path.clone(),
            column_config: self.column_config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_parquet() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let parquet_path = dir.path().join("test.parquet");

        // Create a simple parquet file using DuckDB
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            &format!(
                "COPY (SELECT * FROM (VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Charlie')) AS t(id, name))
                 TO '{}' (FORMAT PARQUET)",
                parquet_path.display()
            ),
            []
        ).unwrap();

        (dir, parquet_path)
    }

    #[test]
    fn test_managed_dataset_creation() {
        let (_dir, parquet_path) = create_test_parquet();
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();

        let dataset = ManagedDataset::new(conn, id, parquet_path).unwrap();

        assert!(dataset.table_name.starts_with("dataset_"));
    }

    #[test]
    fn test_managed_dataset_row_count() {
        let (_dir, parquet_path) = create_test_parquet();
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();

        let dataset = ManagedDataset::new(conn, id, parquet_path).unwrap();
        let count = dataset.row_count().unwrap();

        assert_eq!(count, 3);
    }

    #[test]
    fn test_managed_dataset_column_names() {
        let (_dir, parquet_path) = create_test_parquet();
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();

        let dataset = ManagedDataset::new(conn, id, parquet_path).unwrap();
        let columns = dataset.column_names().unwrap();

        assert_eq!(columns, vec!["id", "name"]);
    }

    #[test]
    fn test_managed_dataset_get_page() {
        let (_dir, parquet_path) = create_test_parquet();
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();

        let dataset = ManagedDataset::new(conn, id, parquet_path).unwrap();
        let page = dataset.get_page(0, 2).unwrap();

        assert_eq!(page.num_rows(), 2);
        assert_eq!(page.num_columns(), 2);
    }

    #[test]
    fn test_managed_dataset_pagination() {
        let (_dir, parquet_path) = create_test_parquet();
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();

        let dataset = ManagedDataset::new(conn, id, parquet_path).unwrap();

        // First page
        let page1 = dataset.get_page(0, 2).unwrap();
        assert_eq!(page1.num_rows(), 2);

        // Second page
        let page2 = dataset.get_page(2, 2).unwrap();
        assert_eq!(page2.num_rows(), 1); // Only 1 row left
    }
}
