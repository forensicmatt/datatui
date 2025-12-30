use crate::core::{
    schema::{init_global_schema, init_workspace_schema},
    types::{CsvImportOptions, DatasetId, ParquetImportOptions, SourceType},
    DatasetRecord, ManagedDataset,
};
use color_eyre::Result;
use duckdb::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// DataService manages dataset imports and workspace database
///
/// This service is responsible for:
/// - Importing CSV/Parquet files into the workspace
/// - Converting data to Parquet format via DuckDB
/// - Managing workspace and global databases
/// - Providing access to datasets
pub struct DataService {
    /// Global DuckDB connection for user-level config/history
    global_conn: Arc<Connection>,

    /// Workspace DuckDB connection for dataset metadata
    workspace_conn: Arc<Connection>,

    /// Path to the workspace directory
    workspace_path: PathBuf,

    /// In-memory cache of loaded datasets
    datasets: Arc<Mutex<HashMap<DatasetId, ManagedDataset>>>,
}

impl DataService {
    /// Create a new DataService for the given workspace
    ///
    /// Initializes both global and workspace databases
    pub fn new(workspace_path: &Path) -> Result<Self> {
        Self::new_impl(workspace_path, None)
    }

    /// Internal constructor that allows specifying global DB path (for tests)
    fn new_impl(workspace_path: &Path, global_db_path: Option<PathBuf>) -> Result<Self> {
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

        // Open workspace DuckDB database
        let workspace_db_path = workspace_path.join(".datatui").join("workspace.duckdb");
        if let Some(parent) = workspace_db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let workspace_conn = Arc::new(Connection::open(&workspace_db_path)?);
        init_workspace_schema(&workspace_conn)?;

        // Create data directory for Parquet files
        std::fs::create_dir_all(workspace_path.join(".datatui").join("data"))?;

        Ok(Self {
            global_conn,
            workspace_conn,
            workspace_path: workspace_path.to_owned(),
            datasets: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Import a CSV file into the workspace
    ///
    /// This method:
    /// 1. Uses DuckDB to read the CSV and convert to Parquet (streaming, memory-efficient)
    /// 2. Stores metadata in workspace database
    /// 3. Creates a ManagedDataset for querying
    pub fn import_csv(&self, path: PathBuf, options: CsvImportOptions) -> Result<DatasetId> {
        let dataset_id = DatasetId::new();
        let dataset_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Create Parquet file path
        let parquet_path = self
            .workspace_path
            .join(".datatui")
            .join("data")
            .join(format!("{}.parquet", dataset_id.as_str()));

        // Use DuckDB to convert CSV to Parquet (streaming, memory-efficient)
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
            "COPY (SELECT * FROM read_csv('{}', header = {}, delim = '{}'{}))\n             TO '{}' (FORMAT PARQUET, COMPRESSION ZSTD)",
            path.display(),
            options.has_header,
            delimiter,
            quote,
            parquet_path.display()
        );

        self.workspace_conn.execute(&query, [])?;

        // Get row and column counts
        let (row_count, col_count) = self.get_parquet_stats(&parquet_path)?;

        // Create and store metadata
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Csv,
            Some(path.to_string_lossy().to_string()),
            parquet_path.to_string_lossy().to_string(),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.workspace_conn)?;

        // Create managed dataset
        let dataset = ManagedDataset::new(
            self.workspace_conn.clone(),
            dataset_id.clone(),
            parquet_path,
        )?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Import a Parquet file into the workspace
    ///
    /// Since the file is already Parquet, this just copies it and creates metadata
    pub fn import_parquet(&self, path: PathBuf) -> Result<DatasetId> {
        let dataset_id = DatasetId::new();
        let dataset_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Create Parquet file path in workspace
        let parquet_path = self
            .workspace_path
            .join(".datatui")
            .join("data")
            .join(format!("{}.parquet", dataset_id.as_str()));

        // Copy the Parquet file
        std::fs::copy(&path, &parquet_path)?;

        // Get row and column counts
        let (row_count, col_count) = self.get_parquet_stats(&parquet_path)?;

        // Create and store metadata
        let mut record = DatasetRecord::new(
            dataset_id.clone(),
            dataset_name,
            SourceType::Parquet,
            Some(path.to_string_lossy().to_string()),
            parquet_path.to_string_lossy().to_string(),
        );
        record.row_count = Some(row_count);
        record.column_count = Some(col_count);
        record.insert(&self.workspace_conn)?;

        // Create managed dataset
        let dataset = ManagedDataset::new(
            self.workspace_conn.clone(),
            dataset_id.clone(),
            parquet_path,
        )?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset.clone());

        Ok(dataset_id)
    }

    /// Get statistics (row count, column count) from a Parquet file
    fn get_parquet_stats(&self, parquet_path: &Path) -> Result<(u64, u32)> {
        // Get row count
        let count_query = format!(
            "SELECT COUNT(*) FROM read_parquet('{}')",
            parquet_path.display()
        );
        let row_count: i64 = self
            .workspace_conn
            .query_row(&count_query, [], |row| row.get(0))?;

        // Get column count
        let cols_query = format!(
            "DESCRIBE (SELECT * FROM read_parquet('{}'))",
            parquet_path.display()
        );
        let mut stmt = self.workspace_conn.prepare(&cols_query)?;
        let col_count = stmt.query_map([], |_| Ok(()))?.count();

        Ok((row_count as u64, col_count as u32))
    }

    /// Get a dataset by ID
    ///
    /// Returns a cached dataset if available, otherwise loads from workspace
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
        let record = DatasetRecord::load(&self.workspace_conn, &id.as_str())?;
        let dataset = ManagedDataset::new(
            self.workspace_conn.clone(),
            record.id.clone(),
            PathBuf::from(&record.parquet_path),
        )?;

        // Cache it
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(id.clone(), dataset.clone());

        Ok(dataset)
    }

    /// List all datasets in the workspace
    pub fn list_datasets(&self) -> Result<Vec<DatasetRecord>> {
        DatasetRecord::load_all(&self.workspace_conn)
    }

    /// Delete a dataset from the workspace
    pub fn delete_dataset(&self, id: &DatasetId) -> Result<()> {
        // Load record to get parquet path
        let record = DatasetRecord::load(&self.workspace_conn, &id.as_str())?;

        // Delete Parquet file
        let parquet_path = PathBuf::from(&record.parquet_path);
        if parquet_path.exists() {
            std::fs::remove_file(parquet_path)?;
        }

        // Delete from database
        self.workspace_conn
            .execute("DELETE FROM datasets WHERE id = ?", [id.as_str()])?;

        // Remove from cache
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .remove(id);

        Ok(())
    }

    /// Get the workspace path
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
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
    fn create_test_service(workspace_path: &Path) -> DataService {
        // Use unique global DB in workspace to avoid file locking between tests
        let global_db = workspace_path.join("test_global.duckdb");
        DataService::new_impl(workspace_path, Some(global_db)).unwrap()
    }

    #[test]
    fn test_data_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let service = create_test_service(temp_dir.path());

        assert_eq!(service.workspace_path(), temp_dir.path());
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
