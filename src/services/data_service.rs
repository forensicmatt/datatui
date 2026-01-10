use crate::core::{
    schema::{init_global_schema, init_session_schema},
    types::{CsvImportOptions, DatasetId, SourceType},
    DatasetRecord, ManagedDataset,
};
use color_eyre::Result;
use duckdb::Connection;
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
    pub fn import_csv(&self, path: PathBuf, options: CsvImportOptions) -> Result<DatasetId> {
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
}
