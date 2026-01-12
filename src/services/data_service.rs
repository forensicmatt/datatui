use crate::core::{
    schema::{init_global_schema, init_session_schema},
    types::{CsvImportOptions, DatasetId, SourceType},
    DatasetRecord, JsonImportOptions, ManagedDataset,
};
use color_eyre::Result;
use duckdb::Connection;
use glob::glob;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// DataService manages dataset imports and session database
///
/// This service is responsible for:
/// - Importing CSV/Parquet files into the session database
/// - Loading data directly into database tables
/// - Managing session and global databases
/// - Providing access to datasets
pub struct DataService {
    /// Global DuckDB connection for user-level config/history
    global_conn: Arc<Connection>,

    /// Session DuckDB connection for dataset metadata and data tables
    session_conn: Arc<Connection>,

    /// Unique session identifier
    session_id: String,

    /// Path to the session directory
    session_path: PathBuf,

    /// In-memory cache of loaded datasets
    datasets: Arc<Mutex<HashMap<DatasetId, ManagedDataset>>>,
}

impl DataService {
    /// Create a new DataService for the given session
    ///
    /// Create a new DataService for the given session
    pub fn new(session_path: impl AsRef<Path>) -> Result<Self> {
        Self::new_impl(session_path.as_ref(), None)
    }

    /// Internal constructor with optional global DB path (for testing)
    pub(crate) fn new_impl(session_path: &Path, global_db_path: Option<PathBuf>) -> Result<Self> {
        // Open global DuckDB database
        let global_db_path = global_db_path.unwrap_or_else(|| {
            directories::BaseDirs::new()
                .ok_or_else(|| color_eyre::eyre::eyre!("Failed to get home directory"))
                .and_then(|base_dirs| {
                    Ok(base_dirs.home_dir().join(".datatui").join("global.duckdb"))
                })
                .unwrap_or_else(|_| PathBuf::from(".datatui/global.duckdb"))
        });

        if let Some(parent) = global_db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let global_conn = Arc::new(Connection::open(&global_db_path)?);
        init_global_schema(&global_conn)?;

        // Generate unique session ID to prevent conflicts between multiple instances
        let session_id = uuid::Uuid::new_v4().to_string();

        // Open session DuckDB database with unique name
        let session_db_path = session_path
            .join(".datatui")
            .join(format!("session_{}.duckdb", session_id));
        if let Some(parent) = session_db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let session_conn = Arc::new(Connection::open(&session_db_path)?);
        init_session_schema(&session_conn)?;

        Ok(Self {
            global_conn,
            session_conn,
            session_id,
            session_path: session_path.to_owned(),
            datasets: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Import a CSV file into the session database
    ///
    /// This method:
    /// 1. Uses DuckDB to read the CSV and load directly into a table
    /// 2. Stores metadata in session database
    /// 3. Creates a ManagedDataset for querying
    /// 4. Supports glob patterns for loading multiple files (e.g., "data/*.csv")
    pub fn import_csv(&self, path: PathBuf, options: CsvImportOptions) -> Result<DatasetId> {
        // Check if path contains glob patterns
        let path_str = path.to_string_lossy();
        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            return self.import_csv_glob(&path_str, options);
        }

        let dataset_id = DatasetId::new();
        let dataset_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Create table name from dataset ID
        let table_name = format!("dataset_{}", dataset_id.as_str().replace("-", "_"));

        // Use DuckDB to load CSV directly into a table
        let delimiter = if options.delimiter == '\t' {
            "\\t".to_string()
        } else {
            options.delimiter.to_string()
        };

        let quote = options
            .quote_char
            .map(|c| format!(", quote = '{}'", c))
            .unwrap_or_default();

        let query = format!(
            "CREATE TABLE {} AS SELECT * FROM read_csv('{}', header = {}, delim = '{}'{}) ",
            table_name,
            path.display(),
            options.has_header,
            delimiter,
            quote
        );

        self.session_conn.execute(&query, [])?;

        // Get row and column counts from the table
        let (row_count, col_count) = self.get_table_stats(&table_name)?;

        // Create and store metadata (no parquet_path needed)
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Csv,
            Some(path.to_string_lossy().to_string()),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.session_conn)?;

        // Create managed dataset
        let dataset =
            ManagedDataset::new(self.session_conn.clone(), dataset_id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Import a Parquet file into the session database
    ///
    /// Loads the Parquet file directly into a table in the session database
    pub fn import_parquet(&self, path: PathBuf) -> Result<DatasetId> {
        let dataset_id = DatasetId::new();
        let dataset_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Create table name from dataset ID
        let table_name = format!("dataset_{}", dataset_id.as_str().replace("-", "_"));

        // Use DuckDB to load Parquet directly into a table
        let query = format!(
            "CREATE TABLE {} AS SELECT * FROM read_parquet('{}')",
            table_name,
            path.display()
        );
        self.session_conn.execute(&query, [])?;

        // Get row and column counts from the table
        let (row_count, col_count) = self.get_table_stats(&table_name)?;

        // Create and store metadata (no parquet_path needed)
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Parquet,
            Some(path.to_string_lossy().to_string()),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.session_conn)?;

        // Create managed dataset
        let dataset =
            ManagedDataset::new(self.session_conn.clone(), dataset_id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Get statistics (row count, column count) from a table
    fn get_table_stats(&self, table_name: &str) -> Result<(u64, u32)> {
        // Get row count
        let count_query = format!("SELECT COUNT(*) FROM {}", table_name);
        let row_count: i64 = self
            .session_conn
            .query_row(&count_query, [], |row| row.get(0))?;

        // Get column count
        let cols_query = format!("DESCRIBE {}", table_name);
        let mut stmt = self.session_conn.prepare(&cols_query)?;
        let col_count = stmt.query_map([], |_| Ok(()))?.count();

        Ok((row_count as u64, col_count as u32))
    }

    /// Get a dataset by ID
    ///
    /// Returns a cached dataset if available, otherwise loads from session database
    pub fn get_dataset(&self, id: &DatasetId) -> Result<ManagedDataset> {
        // Check cache first
        {
            let datasets = self
                .datasets
                .lock()
                .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?;
            if let Some(dataset) = datasets.get(id) {
                return Ok(dataset.clone());
            }
        }

        // Load from database
        let _record = DatasetRecord::load(&self.session_conn, &id.as_str())?;

        // Create table name from dataset ID
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        let dataset = ManagedDataset::new(self.session_conn.clone(), id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(id.clone(), dataset.clone());

        Ok(dataset)
    }

    /// List all datasets in the session
    pub fn list_datasets(&self) -> Result<Vec<DatasetRecord>> {
        DatasetRecord::load_all(&self.session_conn)
    }

    /// Delete a dataset from the session
    pub fn delete_dataset(&self, id: &DatasetId) -> Result<()> {
        // Create table name from dataset ID
        let table_name = format!("dataset_{}", id.as_str().replace("-", "_"));

        // Drop the table
        self.session_conn
            .execute(&format!("DROP TABLE IF EXISTS {}", table_name), [])?;

        // Delete metadata from database
        self.session_conn
            .execute("DELETE FROM datasets WHERE id = ?", [id.as_str()])?;

        // Remove from cache
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .remove(id);

        Ok(())
    }

    /// Get the session path
    pub fn session_path(&self) -> &Path {
        &self.session_path
    }

    /// Get the unique session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Import a JSON file into the session database
    ///
    /// This method supports:
    /// - Standard JSON arrays or objects
    /// - NDJSON (newline-delimited JSON)
    /// - Custom record extraction using JMESPath-like syntax
    /// - Glob patterns for loading multiple files
    ///
    /// # Arguments
    ///
    /// * `path` - File path or glob pattern (e.g., "data/*.json")
    /// * `options` - JSON import options (format, record expression)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use datatui::core::JsonImportOptions;
    /// # use std::path::PathBuf;
    /// # let service = unimplemented!();
    /// // Load NDJSON file
    /// let opts = JsonImportOptions::ndjson();
    /// service.import_json(PathBuf::from("data.ndjson"), opts)?;
    ///
    /// // Load JSON with nested records
    /// let opts = JsonImportOptions::with_records_expr("data.items");
    /// service.import_json(PathBuf::from("api_response.json"), opts)?;
    ///
    /// // Load multiple JSON files
    /// let opts = JsonImportOptions::default();
    /// service.import_json(PathBuf::from("data/*.json"), opts)?;
    /// # Ok::<(), color_eyre::Report>(())
    /// ```
    pub fn import_json(&self, path: PathBuf, options: JsonImportOptions) -> Result<DatasetId> {
        // Check if path contains glob patterns
        let path_str = path.to_string_lossy();
        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            self.import_json_glob(&path_str, options)
        } else {
            self.import_json_single(path, options)
        }
    }

    /// Import a single JSON file
    fn import_json_single(&self, path: PathBuf, options: JsonImportOptions) -> Result<DatasetId> {
        let dataset_id = DatasetId::new();
        let dataset_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let table_name = format!("dataset_{}", dataset_id.as_str().replace("-", "_"));

        // Build the DuckDB read_json or read_ndjson query
        let query = if options.ndjson {
            // For NDJSON, each line is a separate record
            // Get file size and add 20% buffer
            let file_size = std::fs::metadata(&path)
                .map(|m| m.len())
                .unwrap_or(268435456);
            let max_size = (file_size as f64 * 1.2) as u64;

            format!(
                "CREATE TABLE {} AS SELECT * FROM read_json_auto('{}', format='newline_delimited', maximum_object_size={})",
                table_name,
                path.display(),
                max_size
            )
        } else {
            // For regular JSON, check if we need to extract from a nested path
            if options.records_expr == "@" {
                // Direct read - assumes root is an array of records
                // Get file size and add 20% buffer
                let file_size = std::fs::metadata(&path)
                    .map(|m| m.len())
                    .unwrap_or(268435456);
                let max_size = (file_size as f64 * 1.2) as u64;

                format!(
                    "CREATE TABLE {} AS SELECT * FROM read_json_auto('{}', maximum_object_size={})",
                    table_name,
                    path.display(),
                    max_size
                )
            } else {
                // For nested paths like "Records", use DuckDB's struct field access
                let field_name = options.records_expr.clone();

                // Get file size and add 20% buffer for DuckDB's internal overhead
                let file_size = std::fs::metadata(&path)
                    .map(|m| m.len())
                    .unwrap_or(268435456); // Fallback to 256MB if we can't read size
                let max_size = (file_size as f64 * 1.2) as u64;

                format!(
                    "CREATE TABLE {} AS
                    WITH json_data AS (
                        SELECT * FROM read_json_auto('{}', maximum_object_size={})
                    )
                    SELECT item.* FROM (
                        SELECT unnest(\"{}\") as item
                        FROM json_data
                    )",
                    table_name,
                    path.display(),
                    max_size,
                    field_name
                )
            }
        };

        self.session_conn.execute(&query, [])?;

        // Get row and column counts from the table
        let (row_count, col_count) = self.get_table_stats(&table_name)?;

        // Create and store metadata
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Json,
            Some(path.to_string_lossy().to_string()),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.session_conn)?;

        // Create managed dataset
        let dataset =
            ManagedDataset::new(self.session_conn.clone(), dataset_id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Import multiple JSON files using a glob pattern
    fn import_json_glob(&self, pattern: &str, options: JsonImportOptions) -> Result<DatasetId> {
        // Expand glob pattern to get list of files
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in glob(pattern)? {
            match entry {
                Ok(path) => paths.push(path),
                Err(e) => {
                    tracing::warn!("Error reading glob entry: {}", e);
                }
            }
        }

        if paths.is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "No files matched pattern: {}",
                pattern
            ));
        }

        let dataset_id = DatasetId::new();
        let dataset_name = format!("{} files", paths.len());
        let table_name = format!("dataset_{}", dataset_id.as_str().replace("-", "_"));

        // Build UNION ALL query to combine all files
        if options.ndjson {
            // For NDJSON, read all files
            let file_list: Vec<String> =
                paths.iter().map(|p| format!("'{}'", p.display())).collect();

            // Find the largest file size and add 20% buffer
            let max_file_size = paths
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .max()
                .unwrap_or(268435456);
            let max_size = (max_file_size as f64 * 1.2) as u64;

            let query = format!(
                "CREATE TABLE {} AS SELECT * FROM read_json_auto([{}], format='newline_delimited', maximum_object_size={})",
                table_name,
                file_list.join(", "),
                max_size
            );
            self.session_conn.execute(&query, [])?;
        } else {
            // For regular JSON with multiple files
            let file_list: Vec<String> =
                paths.iter().map(|p| format!("'{}'", p.display())).collect();

            let query = if options.records_expr == "@" {
                // Root level arrays
                // Find the largest file size and add 20% buffer
                let max_file_size = paths
                    .iter()
                    .filter_map(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .max()
                    .unwrap_or(268435456);
                let max_size = (max_file_size as f64 * 1.2) as u64;

                format!(
                    "CREATE TABLE {} AS SELECT * FROM read_json_auto([{}], maximum_object_size={})",
                    table_name,
                    file_list.join(", "),
                    max_size
                )
            } else {
                // Nested path extraction from multiple files
                let field_name = options.records_expr.clone();

                // Find the largest file size and add 20% buffer
                let max_file_size = paths
                    .iter()
                    .filter_map(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .max()
                    .unwrap_or(268435456); // Fallback to 256MB if we can't read any sizes
                let max_size = (max_file_size as f64 * 1.2) as u64;

                format!(
                    "CREATE TABLE {} AS
                    WITH json_data AS (
                        SELECT * FROM read_json_auto([{}], maximum_object_size={})
                    )
                    SELECT item.* FROM (
                        SELECT unnest(\"{}\") as item
                        FROM json_data
                    )",
                    table_name,
                    file_list.join(", "),
                    max_size,
                    field_name
                )
            };
            self.session_conn.execute(&query, [])?;
        }

        // Get row and column counts
        let (row_count, col_count) = self.get_table_stats(&table_name)?;

        // Create metadata
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Json,
            Some(pattern.to_string()),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.session_conn)?;

        // Create managed dataset
        let dataset =
            ManagedDataset::new(self.session_conn.clone(), dataset_id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Import CSV files using a glob pattern
    ///
    /// This allows loading multiple CSV files at once with patterns like "data/*.csv"
    pub fn import_csv_glob(&self, pattern: &str, options: CsvImportOptions) -> Result<DatasetId> {
        // Expand glob pattern
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in glob(pattern)? {
            match entry {
                Ok(path) => paths.push(path),
                Err(e) => {
                    tracing::warn!("Error reading glob entry: {}", e);
                }
            }
        }

        if paths.is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "No files matched pattern: {}",
                pattern
            ));
        }

        let dataset_id = DatasetId::new();
        let dataset_name = format!("{} CSV files", paths.len());
        let table_name = format!("dataset_{}", dataset_id.as_str().replace("-", "_"));

        // Build query to read all CSV files
        let delimiter = if options.delimiter == '\t' {
            "\\t".to_string()
        } else {
            options.delimiter.to_string()
        };

        let quote = options
            .quote_char
            .map(|c| format!(", quote = '{}'", c))
            .unwrap_or_default();

        let file_list: Vec<String> = paths.iter().map(|p| format!("'{}'", p.display())).collect();

        let query = format!(
            "CREATE TABLE {} AS SELECT * FROM read_csv([{}], header = {}, delim = '{}'{}) ",
            table_name,
            file_list.join(", "),
            options.has_header,
            delimiter,
            quote
        );

        self.session_conn.execute(&query, [])?;

        // Get row and column counts
        let (row_count, col_count) = self.get_table_stats(&table_name)?;

        // Create metadata
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Csv,
            Some(pattern.to_string()),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.session_conn)?;

        // Create managed dataset
        let dataset =
            ManagedDataset::new(self.session_conn.clone(), dataset_id.clone(), table_name)?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }
}

// Clean up session database when DataService is dropped
impl Drop for DataService {
    fn drop(&mut self) {
        // Close the connection first by dropping the Arc
        // (connection will be closed when all Arc references are dropped)
        drop(self.session_conn.clone());

        // Delete the session database file
        let session_db_path = self
            .session_path
            .join(".datatui")
            .join(format!("session_{}.duckdb", self.session_id));

        if session_db_path.exists() {
            // Best effort cleanup - ignore errors
            let _ = std::fs::remove_file(&session_db_path);

            // Also try to remove .wal file if it exists (DuckDB write-ahead log)
            let wal_path = session_db_path.with_extension("duckdb.wal");
            if wal_path.exists() {
                let _ = std::fs::remove_file(&wal_path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_csv(dir: &Path) -> PathBuf {
        let csv_path = dir.join("test.csv");
        let mut file = std::fs::File::create(&csv_path).unwrap();
        writeln!(file, "id,name,value").unwrap();
        writeln!(file, "1,Alice,100").unwrap();
        writeln!(file, "2,Bob,200").unwrap();
        writeln!(file, "3,Charlie,300").unwrap();
        csv_path
    }

    /// Create a test DataService with isolated global database
    fn create_test_service(session_path: &Path) -> DataService {
        // Use unique global DB in session to avoid file locking between tests
        let global_db = session_path.join("test_global.duckdb");
        DataService::new_impl(session_path, Some(global_db)).unwrap()
    }

    #[test]
    fn test_data_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let service = create_test_service(temp_dir.path());

        assert_eq!(service.session_path(), temp_dir.path());
    }

    #[test]
    fn test_import_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = create_test_csv(temp_dir.path());

        let service = create_test_service(temp_dir.path());
        let options = CsvImportOptions::default();

        let dataset_id = service.import_csv(csv_path, options).unwrap();

        // Verify dataset exists
        let dataset = service.get_dataset(&dataset_id).unwrap();
        assert_eq!(dataset.row_count().unwrap(), 3);
        assert_eq!(dataset.column_count().unwrap(), 3);
    }

    #[test]
    fn test_list_datasets() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = create_test_csv(temp_dir.path());

        let service = create_test_service(temp_dir.path());
        let options = CsvImportOptions::default();

        service
            .import_csv(csv_path.clone(), options.clone())
            .unwrap();
        service.import_csv(csv_path, options).unwrap();

        let datasets = service.list_datasets().unwrap();
        assert_eq!(datasets.len(), 2);
    }

    #[test]
    fn test_delete_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = create_test_csv(temp_dir.path());

        let service = create_test_service(temp_dir.path());
        let options = CsvImportOptions::default();

        let dataset_id = service.import_csv(csv_path, options).unwrap();

        // Delete it
        service.delete_dataset(&dataset_id).unwrap();

        // Verify it's gone
        let datasets = service.list_datasets().unwrap();
        assert_eq!(datasets.len(), 0);
    }

    #[test]
    fn test_csv_with_custom_delimiter() {
        let temp_dir = TempDir::new().unwrap();
        let tsv_path = temp_dir.path().join("test.tsv");

        let mut file = std::fs::File::create(&tsv_path).unwrap();
        writeln!(file, "id\tname\tvalue").unwrap();
        writeln!(file, "1\tAlice\t100").unwrap();
        writeln!(file, "2\tBob\t200").unwrap();

        let service = create_test_service(temp_dir.path());
        let options = CsvImportOptions {
            has_header: true,
            delimiter: '\t',
            quote_char: None,
        };

        let dataset_id = service.import_csv(tsv_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 2);
        assert_eq!(dataset.column_count().unwrap(), 3);
    }

    #[test]
    fn test_import_json() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test.json");

        let mut file = std::fs::File::create(&json_path).unwrap();
        writeln!(
            file,
            r#"[{{"id": 1, "name": "Alice"}}, {{"id": 2, "name": "Bob"}}]"#
        )
        .unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::default();

        let dataset_id = service.import_json(json_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 2);
        assert_eq!(dataset.column_count().unwrap(), 2);
    }

    #[test]
    fn test_import_ndjson() {
        let temp_dir = TempDir::new().unwrap();
        let ndjson_path = temp_dir.path().join("test.ndjson");

        let mut file = std::fs::File::create(&ndjson_path).unwrap();
        writeln!(file, r#"{{"id": 1, "name": "Alice"}}"#).unwrap();
        writeln!(file, r#"{{"id": 2, "name": "Bob"}}"#).unwrap();
        writeln!(file, r#"{{"id": 3, "name": "Charlie"}}"#).unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::ndjson();

        let dataset_id = service.import_json(ndjson_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 3);
        assert_eq!(dataset.column_count().unwrap(), 2);
    }

    #[test]
    fn test_import_json_nested_single_level() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("nested.json");

        let mut file = std::fs::File::create(&json_path).unwrap();
        writeln!(
            file,
            r#"{{"data": [{{"id": 1, "value": 100}}, {{"id": 2, "value": 200}}]}}"#
        )
        .unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::with_records_expr("data");

        let dataset_id = service.import_json(json_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 2);
        assert_eq!(dataset.column_count().unwrap(), 2);
    }

    #[test]
    fn test_import_json_nested_multi_level() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("deeply_nested.json");

        let mut file = std::fs::File::create(&json_path).unwrap();
        writeln!(
            file,
            r#"{{"response": {{"results": [{{"id": 1, "name": "Alice"}}, {{"id": 2, "name": "Bob"}}]}}}}"#
        )
        .unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::with_records_expr("response.results");

        let dataset_id = service.import_json(json_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 2);
        assert_eq!(dataset.column_count().unwrap(), 2);
    }

    #[test]
    fn test_import_json_with_records_attribute() {
        // This tests the user's specific use case: {"Records": [...]}
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("records.json");

        let mut file = std::fs::File::create(&json_path).unwrap();
        writeln!(
            file,
            r#"{{"Records": [{{"id": 1, "name": "Alice", "email": "alice@example.com"}}, {{"id": 2, "name": "Bob", "email": "bob@example.com"}}]}}"#
        )
        .unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::with_records_expr("Records");

        let dataset_id = service.import_json(json_path, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        assert_eq!(dataset.row_count().unwrap(), 2);
        assert_eq!(dataset.column_count().unwrap(), 3); // id, name, email
    }

    #[test]
    fn test_import_csv_glob() {
        let temp_dir = TempDir::new().unwrap();

        // Create first CSV file
        let csv1 = temp_dir.path().join("data1.csv");
        let mut file1 = std::fs::File::create(&csv1).unwrap();
        writeln!(file1, "id,name").unwrap();
        writeln!(file1, "1,Alice").unwrap();

        // Create second CSV file
        let csv2 = temp_dir.path().join("data2.csv");
        let mut file2 = std::fs::File::create(&csv2).unwrap();
        writeln!(file2, "id,name").unwrap();
        writeln!(file2, "2,Bob").unwrap();

        let service = create_test_service(temp_dir.path());
        let options = CsvImportOptions::default();

        let pattern = temp_dir
            .path()
            .join("data*.csv")
            .to_string_lossy()
            .to_string();
        let dataset_id = service.import_csv_glob(&pattern, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        // Should have combined rows from both files
        assert_eq!(dataset.row_count().unwrap(), 2);
    }

    #[test]
    fn test_import_json_glob() {
        let temp_dir = TempDir::new().unwrap();

        // Create first JSON file
        let json1 = temp_dir.path().join("data1.json");
        let mut file1 = std::fs::File::create(&json1).unwrap();
        writeln!(file1, r#"[{{"id": 1, "name": "Alice"}}]"#).unwrap();

        // Create second JSON file
        let json2 = temp_dir.path().join("data2.json");
        let mut file2 = std::fs::File::create(&json2).unwrap();
        writeln!(file2, r#"[{{"id": 2, "name": "Bob"}}]"#).unwrap();

        let service = create_test_service(temp_dir.path());
        let options = JsonImportOptions::default();

        let pattern = temp_dir
            .path()
            .join("data*.json")
            .to_string_lossy()
            .to_string();
        let dataset_id = service.import_json_glob(&pattern, options).unwrap();
        let dataset = service.get_dataset(&dataset_id).unwrap();

        // Should have combined rows from both files
        assert_eq!(dataset.row_count().unwrap(), 2);
    }
}
