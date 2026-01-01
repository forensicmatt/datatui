use crate::core::types::DatasetId;
use color_eyre::Result;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;
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

        Ok(Self {
            conn,
            id,
            table_name,
            parquet_path,
        })
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
}

// Clone implementation for sharing datasets across threads
impl Clone for ManagedDataset {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            id: self.id.clone(),
            table_name: self.table_name.clone(),
            parquet_path: self.parquet_path.clone(),
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
