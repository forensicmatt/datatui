use crate::core::column_config::ColumnWidthConfig;
use crate::core::sql_query::{OrderByColumn, QueryBuilder};
use crate::core::types::DatasetId;
use crate::tui::components::SortColumn;
use color_eyre::Result;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;
use std::collections::HashMap;
use std::sync::Arc;

/// Managed dataset backed by DuckDB
///
/// This struct represents a dataset stored as a table in the session database.
/// It provides methods for pagination, querying, and metadata access without loading
/// the entire dataset into memory.
pub struct ManagedDataset {
    conn: Arc<Connection>,
    pub id: DatasetId,
    pub table_name: String,
    column_config: ColumnWidthConfig,
    // Legacy field for backward compatibility (deprecated)
    sort_order: Vec<SortColumn>,
    // NEW: Global SQL query as source of truth
    current_query: QueryBuilder,
}

impl ManagedDataset {
    /// Create a new managed dataset
    ///
    /// The table must already exist in the database
    pub fn new(conn: Arc<Connection>, id: DatasetId, table_name: String) -> Result<Self> {
        // Validate that the table exists
        let table_check = format!(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = '{}'",
            table_name
        );
        let table_exists: i64 = conn.query_row(&table_check, [], |row| row.get(0))?;

        if table_exists == 0 {
            return Err(color_eyre::eyre::eyre!(
                "Table '{}' does not exist in database",
                table_name
            ));
        }

        // Initialize column configuration
        let columns = Self::get_columns_from_table(&conn, &table_name)?;
        let column_config = ColumnWidthConfig::from_columns(columns);

        let current_query = QueryBuilder::new(&table_name);

        Ok(Self {
            conn,
            id,
            table_name: table_name.clone(),
            column_config,
            sort_order: Vec::new(),
            current_query,
        })
    }

    /// Helper to get column names from a table
    fn get_columns_from_table(conn: &Connection, table_name: &str) -> Result<Vec<String>> {
        let query = format!("DESCRIBE {}", table_name);
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get a page of data for display
    ///
    /// Uses LIMIT/OFFSET for efficient pagination without loading full dataset
    pub fn get_page(&self, offset: usize, limit: usize) -> Result<RecordBatch> {
        // Use subquery for pagination to respect original query's constraints (WHERE, ORDER BY, LIMIT)
        let sql = self.current_query.to_sql();
        let query = format!("SELECT * FROM ({}) LIMIT {} OFFSET {}", sql, limit, offset);

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
        // Count rows based on current query (respecting WHERE, LIMIT, etc.)
        let sql = self.current_query.to_sql();
        let query = format!("SELECT COUNT(*) FROM ({})", sql);
        let count: i64 = self.conn.query_row(&query, [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get column names
    pub fn column_names(&self) -> Result<Vec<String>> {
        // Describe the current query to get actual columns
        let sql = self.current_query.to_sql();
        let query = format!("DESCRIBE {}", sql);
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

        // Clean config to match current columns instead of straight strict validation
        // This handles cases where columns might have been dropped or added since config creation
        let mut config = config;
        config.clean_for_columns(&columns);

        // Final sanity check (should pass after clean)
        if !config.validate(&columns) {
            return Err(color_eyre::eyre::eyre!(
                "Invalid column configuration even after cleaning"
            ));
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

    // ========================================
    // Sorting API
    // ========================================

    /// Set sort order for the dataset
    pub fn set_sort_order(&mut self, columns: Vec<SortColumn>) -> Result<()> {
        // Validate that all columns exist
        let available_cols = self.column_names()?;
        for sort_col in &columns {
            if !available_cols.contains(&sort_col.name) {
                return Err(color_eyre::eyre::eyre!(
                    "Column '{}' does not exist",
                    sort_col.name
                ));
            }
        }

        // Update both legacy field and global query
        self.sort_order = columns.clone();

        // Convert SortColumn to OrderByColumn
        let order_by: Vec<OrderByColumn> = columns
            .into_iter()
            .map(|sc| OrderByColumn {
                column: sc.name,
                ascending: sc.ascending,
            })
            .collect();

        self.current_query.set_order_by(order_by);
        Ok(())
    }

    /// Get current sort order
    pub fn get_sort_order(&self) -> Result<Vec<SortColumn>> {
        // Convert from QueryBuilder's OrderByColumn to SortColumn
        let order_by = self.current_query.get_order_by();
        let sort_columns: Vec<SortColumn> = order_by
            .into_iter()
            .map(|obc| SortColumn {
                name: obc.column,
                ascending: obc.ascending,
            })
            .collect();
        Ok(sort_columns)
    }

    /// Clear all sorting
    pub fn clear_sort(&mut self) {
        self.sort_order.clear();
        self.current_query.set_order_by(Vec::new());
    }

    /// Build ORDER BY clause from sort configuration
    ///
    /// Appends rowid as tie-breaker for stable, deterministic sorting
    fn build_order_by_clause(&self) -> String {
        if self.sort_order.is_empty() {
            return String::new();
        }

        let mut clauses: Vec<String> = self
            .sort_order
            .iter()
            .map(|sc| {
                let direction = if sc.ascending { "ASC" } else { "DESC" };
                format!("\"{}\" {}", sc.name, direction)
            })
            .collect();

        // Always append rowid for deterministic ordering
        // (DuckDB parallelizes sorts, causing non-deterministic results without a tie-breaker)
        clauses.push("rowid ASC".to_string());

        format!("ORDER BY {}", clauses.join(", "))
    }

    // ========================================
    // Global Query API (NEW)
    // ========================================

    /// Get the current query builder
    pub fn get_current_query(&self) -> &QueryBuilder {
        &self.current_query
    }

    /// Set the current query (replaces the entire query)
    /// This is used by the SQL dialog to update the dataset view
    pub fn set_current_query(&mut self, query: QueryBuilder) -> Result<()> {
        // Validate that the query targets this dataset's table
        if query.base_table() != self.table_name {
            return Err(color_eyre::eyre::eyre!(
                "Query must target table '{}', got '{}'",
                self.table_name,
                query.base_table()
            ));
        }

        self.current_query = query;

        // Sync back to legacy sort_order for backward compatibility
        let order_by = self.current_query.get_order_by();
        self.sort_order = order_by
            .into_iter()
            .map(|obc| SortColumn {
                name: obc.column,
                ascending: obc.ascending,
            })
            .collect();

        Ok(())
    }

    /// Execute a raw SQL query and update the dataset view
    /// This parses the SQL and sets it as the new current query
    pub fn execute_sql(&mut self, sql: &str) -> Result<()> {
        let query = QueryBuilder::parse(sql, &self.table_name)?;
        self.set_current_query(query)
    }

    /// Get the current SQL query as a string
    pub fn get_current_sql(&self) -> String {
        self.current_query.to_sql()
    }

    // ========================================
    // Backward Compatibility
    // ========================================

    /// Get the table name for this dataset
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Get a reference to the database connection
    ///
    /// This is provided for advanced use cases like dialog components that need
    /// to execute custom queries for statistics and aggregations.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

// Clone implementation for sharing datasets across threads
impl Clone for ManagedDataset {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            id: self.id.clone(),
            table_name: self.table_name.clone(),
            column_config: self.column_config.clone(),
            sort_order: self.sort_order.clone(),
            current_query: self.current_query.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_table(conn: &Connection, table_name: &str) {
        // Create a simple table using DuckDB
        conn.execute(
            &format!(
                "CREATE TABLE {} AS SELECT * FROM (VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Charlie')) AS t(id, name)",
                table_name
            ),
            []
        ).unwrap();
    }

    #[test]
    fn test_managed_dataset_creation() {
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        create_test_table(&conn, &table_name);
        let dataset = ManagedDataset::new(conn, id, table_name.clone()).unwrap();

        assert_eq!(dataset.table_name, table_name);
    }

    #[test]
    fn test_managed_dataset_row_count() {
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        create_test_table(&conn, &table_name);
        let dataset = ManagedDataset::new(conn, id, table_name).unwrap();
        let count = dataset.row_count().unwrap();

        assert_eq!(count, 3);
    }

    #[test]
    fn test_managed_dataset_column_names() {
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        create_test_table(&conn, &table_name);
        let dataset = ManagedDataset::new(conn, id, table_name).unwrap();
        let columns = dataset.column_names().unwrap();

        assert_eq!(columns, vec!["id", "name"]);
    }

    #[test]
    fn test_managed_dataset_get_page() {
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        create_test_table(&conn, &table_name);
        let dataset = ManagedDataset::new(conn, id, table_name).unwrap();
        let page = dataset.get_page(0, 2).unwrap();

        assert_eq!(page.num_rows(), 2);
        assert_eq!(page.num_columns(), 2);
    }

    #[test]
    fn test_managed_dataset_pagination() {
        let conn = Arc::new(Connection::open_in_memory().unwrap());
        let id = DatasetId::new();
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        create_test_table(&conn, &table_name);
        let dataset = ManagedDataset::new(conn, id, table_name).unwrap();

        // First page
        let page1 = dataset.get_page(0, 2).unwrap();
        assert_eq!(page1.num_rows(), 2);

        // Second page
        let page2 = dataset.get_page(2, 2).unwrap();
        assert_eq!(page2.num_rows(), 1); // Only 1 row left
    }
}
