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

    /// Get the path to the session DuckDB file.
    pub fn session_db_path(&self) -> PathBuf {
        self.session_path
            .join(".datatui")
            .join(format!("session_{}.duckdb", self.session_id))
    }

    /// Validate a raw SQL query and apply it to the specified dataset.
    ///
    /// This is the service API used by the LLM agent tool — the tool never
    /// opens the database file directly; instead it sends the SQL to the main
    /// thread which calls this method using the existing open connection.
    ///
    /// Returns `Ok(())` on success or an `Err` whose message should be
    /// forwarded back to the LLM so it can self-correct.
    pub fn validate_and_apply_sql(
        &mut self,
        dataset_id: &crate::core::DatasetId,
        sql: &str,
    ) -> Result<()> {
        use crate::core::sql_query::QueryBuilder;

        // 1. Syntax-check via the existing session connection (no new file open).
        self.session_conn
            .prepare(sql)
            .map_err(|e| color_eyre::eyre::eyre!("Invalid SQL syntax: {}", e))?;

        // 2. Parse into a QueryBuilder (validates it is a SELECT targeting this table).
        let table_name = {
            let datasets = self
                .datasets
                .lock()
                .map_err(|e| color_eyre::eyre::eyre!("Dataset lock poisoned: {}", e))?;
            datasets
                .get(dataset_id)
                .map(|ds| ds.table_name().to_string())
                .ok_or_else(|| {
                    color_eyre::eyre::eyre!("Dataset '{}' not found", dataset_id.as_str())
                })?
        };

        let qb = QueryBuilder::parse(sql, &table_name)
            .map_err(|e| color_eyre::eyre::eyre!("Query parse error: {}", e))?;

        // 3. Apply to the dataset.
        let mut datasets = self
            .datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Dataset lock poisoned: {}", e))?;
        let dataset = datasets.get_mut(dataset_id).ok_or_else(|| {
            color_eyre::eyre::eyre!("Dataset '{}' not found", dataset_id.as_str())
        })?;
        dataset.set_current_query(qb)?;

        Ok(())
    }

    /// Run a read-only query for the LLM to gather context.
    ///
    /// The query is always wrapped with a LIMIT (capped at 50 rows) to prevent
    /// unbounded result sets from consuming too many tokens. Results are serialized
    /// as TOON format — a compact, token-efficient representation that is ~18-25%
    /// smaller than JSON, making it ideal for LLM context.
    ///
    /// Uses Arrow RecordBatch output (not DuckDB's `to_json(row(t.*))`) to avoid
    /// the "Can't pack nothing into a struct" error that occurs when wrapping
    /// subqueries.
    pub fn execute_query_for_context(
        &self,
        dataset_id: &crate::core::DatasetId,
        sql: &str,
        mut limit: usize,
    ) -> Result<String> {
        use duckdb::arrow::array::Array;
        use duckdb::arrow::datatypes::DataType;

        // Enforce max limit of 50 rows
        limit = limit.min(50);

        let datasets = self
            .datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Dataset lock poisoned: {}", e))?;
        let _dataset = datasets.get(dataset_id).ok_or_else(|| {
            color_eyre::eyre::eyre!("Dataset '{}' not found", dataset_id.as_str())
        })?;

        // 1. Syntax check against the existing session connection (no new file open)
        self.session_conn
            .prepare(sql)
            .map_err(|e| color_eyre::eyre::eyre!("Invalid SQL syntax: {}", e))?;

        // 2. Execute wrapped in a LIMIT subquery using Arrow output.
        //    Avoid DuckDB's to_json(row(t.*)) which fails on subquery aliases.
        let wrapped_sql = format!("SELECT * FROM ({}) LIMIT {}", sql, limit);
        let mut stmt = self.session_conn.prepare(&wrapped_sql)?;
        let mut arrow_stream = stmt.query_arrow([])?;

        // 3. Collect all batches and pull column names from the schema.
        let mut all_batches = Vec::new();
        let mut schema_opt = None;
        for batch in arrow_stream.by_ref() {
            if schema_opt.is_none() {
                schema_opt = Some(batch.schema());
            }
            all_batches.push(batch);
        }

        let schema = match schema_opt {
            Some(s) => s,
            None => return Ok("No results found.".to_string()),
        };

        let col_names: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();

        // 4. A minimal cell-value formatter that covers the types DuckDB returns.
        fn arrow_cell_to_json(col: &dyn Array, row: usize) -> serde_json::Value {
            use duckdb::arrow::array::*;
            if col.is_null(row) {
                return serde_json::Value::Null;
            }
            match col.data_type() {
                DataType::Utf8 => {
                    let a = col.as_any().downcast_ref::<StringArray>().unwrap();
                    serde_json::Value::String(a.value(row).to_string())
                }
                DataType::LargeUtf8 => {
                    let a = col.as_any().downcast_ref::<LargeStringArray>().unwrap();
                    serde_json::Value::String(a.value(row).to_string())
                }
                DataType::Boolean => {
                    let a = col.as_any().downcast_ref::<BooleanArray>().unwrap();
                    serde_json::Value::Bool(a.value(row))
                }
                DataType::Int8 => serde_json::json!(col.as_any().downcast_ref::<Int8Array>().unwrap().value(row)),
                DataType::Int16 => serde_json::json!(col.as_any().downcast_ref::<Int16Array>().unwrap().value(row)),
                DataType::Int32 => serde_json::json!(col.as_any().downcast_ref::<Int32Array>().unwrap().value(row)),
                DataType::Int64 => serde_json::json!(col.as_any().downcast_ref::<Int64Array>().unwrap().value(row)),
                DataType::UInt8 => serde_json::json!(col.as_any().downcast_ref::<UInt8Array>().unwrap().value(row)),
                DataType::UInt16 => serde_json::json!(col.as_any().downcast_ref::<UInt16Array>().unwrap().value(row)),
                DataType::UInt32 => serde_json::json!(col.as_any().downcast_ref::<UInt32Array>().unwrap().value(row)),
                DataType::UInt64 => serde_json::json!(col.as_any().downcast_ref::<UInt64Array>().unwrap().value(row)),
                DataType::Float32 => serde_json::json!(col.as_any().downcast_ref::<Float32Array>().unwrap().value(row)),
                DataType::Float64 => serde_json::json!(col.as_any().downcast_ref::<Float64Array>().unwrap().value(row)),
                // Decimal types — emit as string to preserve precision
                DataType::Decimal128(_, scale) => {
                    let a = col.as_any().downcast_ref::<Decimal128Array>().unwrap();
                    let raw = a.value(row);
                    let divisor = 10_i128.pow(*scale as u32);
                    let whole = raw / divisor;
                    let frac = (raw % divisor).abs();
                    serde_json::Value::String(format!("{}.{:0>scale$}", whole, frac, scale = *scale as usize))
                }
                // Everything else: format as debug string
                _ => serde_json::Value::String(format!("{:?}", col.slice(row, 1))),
            }
        }

        // 5. Convert each batch row → JSON object keyed by column name.
        let mut json_rows: Vec<serde_json::Value> = Vec::new();
        for batch in &all_batches {
            for row_idx in 0..batch.num_rows() {
                let mut obj = serde_json::Map::new();
                for (col_idx, name) in col_names.iter().enumerate() {
                    let val = arrow_cell_to_json(batch.column(col_idx).as_ref(), row_idx);
                    obj.insert(name.clone(), val);
                }
                json_rows.push(serde_json::Value::Object(obj));
            }
        }

        if json_rows.is_empty() {
            return Ok("No results found.".to_string());
        }

        // 6. Wrap in { "rows": [...] } and encode as TOON.
        //    TOON hoists the column names into the array header, producing compact
        //    comma-separated rows, e.g.:
        //
        //    rows[3]{boardgame,avg_rating}:
        //      Pandemic,8.6
        //      Catan,7.4
        //      Chess,9.0
        let value = serde_json::json!({ "rows": json_rows });
        let toon = toon_format::encode_default(&value)
            .map_err(|e| color_eyre::eyre::eyre!("TOON encode error: {}", e))?;

        Ok(toon)
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
    /// # let service: datatui::services::DataService = unimplemented!();
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

                // Handle dot notation for nested fields (e.g. "response.results" -> "response"."results")
                let unnest_expr = if field_name.contains('.') {
                    field_name
                        .split('.')
                        .map(|part| format!("\"{}\"", part))
                        .collect::<Vec<_>>()
                        .join(".")
                } else {
                    format!("\"{}\"", field_name)
                };

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
                        SELECT unnest({}) as item
                        FROM json_data
                    )",
                    table_name,
                    path.display(),
                    max_size,
                    unnest_expr
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

    /// Fetch all non-null text values from a column, returning (rowid, text) pairs.
    ///
    /// DuckDB's internal `rowid` pseudo-column is used so we can update specific rows
    /// later, even if the table has no surrogate key.
    pub fn fetch_column_texts(
        &self,
        dataset_id: &DatasetId,
        column_name: &str,
    ) -> Result<Vec<(i64, String)>> {
        let dataset = self.get_dataset(dataset_id)?;
        let table = &dataset.table_name;

        // Escape the column name to prevent injection
        let sql = format!(
            "SELECT rowid, CAST(\"{col}\" AS VARCHAR) FROM \"{table}\" WHERE \"{col}\" IS NOT NULL",
            col = column_name.replace('"', "\"\""),
            table = table.replace('"', "\"\"")
        );

        let mut stmt = self.session_conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let text: String = row.get(1)?;
            Ok((rowid, text))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Add a new FLOAT[] embedding column to a dataset table and populate it.
    ///
    /// `rowid_embeddings` must be in the same order as the rows returned by
    /// `fetch_column_texts` (paired rowid → embedding vector).
    ///
    /// If `new_column_name` already exists the method returns an error.
    pub fn add_embedding_column(
        &self,
        dataset_id: &DatasetId,
        new_column_name: &str,
        rowid_embeddings: Vec<(i64, Vec<f32>)>,
        hide_column: bool,
        config: crate::core::EmbeddingColumnConfig,
    ) -> Result<()> {
        let mut dataset = self.get_dataset(dataset_id)?;
        let table = dataset.table_name.clone();

        // Escape names
        let safe_col = new_column_name.replace('"', "\"\"");

        // 1. Add the new column (FLOAT[])
        let alter_sql = format!(
            "ALTER TABLE \"{table}\" ADD COLUMN \"{col}\" FLOAT[]",
            table = table.replace('"', "\"\""),
            col = safe_col,
        );
        self.session_conn.execute(&alter_sql, [])?;

        // 2. Use a temporary table for batch update - more robust and faster than individual updates
        let temp_table = format!("temp_emb_{}", uuid::Uuid::new_v4().simple());
        self.session_conn.execute(
            &format!("CREATE TEMP TABLE {} (rid BIGINT, emb FLOAT[])", temp_table),
            [],
        )?;

        let mut success_count = 0;
        let total_to_update = rowid_embeddings.len();

        {
            // Insert embeddings into temp table using a prepared statement
            let mut insert_stmt = self.session_conn.prepare(&format!(
                "INSERT INTO {} (rid, emb) VALUES (?, ?::FLOAT[])",
                temp_table
            ))?;

            for (rowid, embedding) in &rowid_embeddings {
                // Convert Vec<f32> to a DuckDB array string [val1, val2, ...]
                let array_str = format!(
                    "[{}]",
                    embedding
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );

                if let Err(e) = insert_stmt.execute(duckdb::params![rowid, array_str]) {
                    tracing::error!("Failed to insert rowid {} into temp table: {}", rowid, e);
                }
            }
        }

        // 3. Perform a JOIN update from the temp table to the main table
        // This is the fastest and most reliable way to update a large number of rows in DuckDB
        let update_sql = format!(
            "UPDATE \"{table}\" SET \"{col}\" = t.emb FROM {temp} t WHERE \"{table}\".rowid = t.rid",
            table = table.replace('"', "\"\""),
            col = safe_col,
            temp = temp_table
        );

        match self.session_conn.execute(&update_sql, []) {
            Ok(n) => {
                success_count = n;
            }
            Err(e) => {
                tracing::error!("JOIN update failed: {}", e);
                // Cleanup temp table before returning
                let _ = self
                    .session_conn
                    .execute(&format!("DROP TABLE {}", temp_table), []);
                return Err(e.into());
            }
        }

        // Cleanup temp table
        let _ = self
            .session_conn
            .execute(&format!("DROP TABLE {}", temp_table), []);

        // 4. Save embedding configuration as metadata
        self.set_column_metadata(
            dataset_id,
            new_column_name,
            "embedding_config",
            &config.to_json(),
        )?;
        self.set_column_metadata(dataset_id, new_column_name, "is_embedding", "true")?;

        // 5. Refresh column config on the cached dataset so the new column appears
        dataset.reset_column_config();
        if hide_column {
            let _ = dataset.set_column_visible(new_column_name, false);
        }

        // Write updated dataset back into the cache
        self.datasets
            .lock()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to acquire dataset lock: {}", e))?
            .insert(dataset_id.clone(), dataset);

        // Result summary - using warn priority to ensure visibility in default log levels
        if success_count == 0 && total_to_update > 0 {
            tracing::warn!(
                "CRITICAL: Embedding column '{}' added but 0/{} rows were updated. RowID mismatch?",
                new_column_name,
                total_to_update
            );
        } else {
            tracing::warn!(
                "Successfully added embedding column '{}': {}/{} rows updated",
                new_column_name,
                success_count,
                total_to_update
            );
        }

        Ok(())
    }

    /// Set a metadata value for a column
    pub fn set_column_metadata(
        &self,
        dataset_id: &DatasetId,
        column_name: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        self.session_conn.execute(
            "INSERT INTO column_metadata (dataset_id, column_name, metadata_key, metadata_value)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (dataset_id, column_name, metadata_key) DO UPDATE SET metadata_value = excluded.metadata_value",
            duckdb::params![dataset_id.as_str(), column_name, key, value],
        )?;
        Ok(())
    }

    /// Get a metadata value for a column
    pub fn get_column_metadata(
        &self,
        dataset_id: &DatasetId,
        column_name: &str,
        key: &str,
    ) -> Result<Option<String>> {
        let mut stmt = self.session_conn.prepare(
            "SELECT metadata_value FROM column_metadata WHERE dataset_id = ? AND column_name = ? AND metadata_key = ?"
        )?;
        let mut rows = stmt.query(duckdb::params![dataset_id.as_str(), column_name, key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Get information about all columns in a dataset, including their database types
    /// and any associated metadata (like embedding configs).
    pub fn get_dataset_column_info(
        &self,
        dataset_id: &DatasetId,
    ) -> Result<Vec<crate::core::models::ColumnInfo>> {
        let dataset = self.get_dataset(dataset_id)?;
        let table = &dataset.table_name;

        // Use DuckDB's DESCRIBE to get column names and types
        let query = format!("DESCRIBE \"{}\"", table.replace('"', "\"\""));
        let mut stmt = self.session_conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let data_type: String = row.get(1)?;
            Ok((name, data_type))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (name, data_type) = row?;
            // Check for embedding configuration metadata
            let embedding_config = self
                .get_column_metadata(dataset_id, &name, "embedding_config")?
                .and_then(|json| crate::core::models::EmbeddingColumnConfig::from_json(&json));

            result.push(crate::core::models::ColumnInfo {
                name,
                data_type,
                embedding_config,
            });
        }
        Ok(result)
    }

    /// Add a record to the sort history
    pub fn add_sort_history_record(
        &self,
        record: crate::core::models::SortHistoryRecord,
    ) -> Result<()> {
        record.insert(&self.session_conn)
    }

    /// Get sort history for a dataset
    pub fn get_sort_history(
        &self,
        dataset_id: &DatasetId,
    ) -> Result<Vec<crate::core::models::SortHistoryRecord>> {
        crate::core::models::SortHistoryRecord::load_for_dataset(&self.session_conn, dataset_id)
    }

    /// Apply a similarity sort to a dataset without creating a column.
    /// Returns the name of the calculated column (e.g. "similarity_score").
    pub fn apply_similarity_sort(
        &self,
        dataset_id: &DatasetId,
        source_column: &str,
        query_vector: &[f32],
    ) -> Result<String> {
        let mut dataset = self.get_dataset(dataset_id)?;
        let table_name = &dataset.table_name;
        let score_col = "similarity_score".to_string();

        // 1. Ensure the column exists physically
        // Using information_schema.columns is reliable in DuckDB
        let check_sql = format!(
            "SELECT count(*) FROM information_schema.columns WHERE table_name = '{}' AND column_name = '{}'",
            table_name.replace('"', ""),
            score_col
        );
        let count: i64 = self
            .session_conn
            .query_row(&check_sql, [], |row| row.get(0))?;

        if count == 0 {
            let add_col_sql = format!(
                "ALTER TABLE \"{}\" ADD COLUMN \"{}\" FLOAT",
                table_name.replace('"', "\"\""),
                score_col
            );
            self.session_conn.execute(&add_col_sql, [])?;
        }

        // 2. Update the column with the similarity score
        // We do this once here, so Selective Selects (rendering) are fast
        let vector_str = format!("{:?}", query_vector);
        let update_sql = format!(
            "UPDATE \"{}\" SET \"{}\" = list_cosine_similarity(\"{}\", {})::FLOAT",
            table_name.replace('"', "\"\""),
            score_col,
            source_column.replace('"', "\"\""),
            vector_str
        );
        self.session_conn.execute(&update_sql, [])?;

        // 3. Update the query to sort by the physical column
        let mut query = dataset.get_current_query().clone();

        // Remove from calculated columns if it was there as a virtual column
        // We want to treat it as a regular physical column now
        let mut new_calcs = Vec::new();
        for calc in query.get_calculated_columns() {
            if !calc.contains(&score_col) {
                new_calcs.push(calc.clone());
            }
        }
        query.set_calculated_columns(new_calcs);

        // Apply DESC sort on the physical score column
        query.set_order_by(vec![crate::core::sql_query::OrderByColumn {
            column: score_col.clone(),
            ascending: false,
        }]);

        dataset.set_current_query(query)?;

        // Cache back
        self.datasets
            .lock()
            .unwrap()
            .insert(dataset_id.clone(), dataset);

        Ok(score_col)
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

/// Cloning a DataService shares the same underlying Arc connections and dataset cache.
///
/// This is intentional: background threads can hold a clone to write results back
/// to the same session DuckDB database without extra coordination.
impl Clone for DataService {
    fn clone(&self) -> Self {
        Self {
            global_conn: self.global_conn.clone(),
            session_conn: self.session_conn.clone(),
            session_id: self.session_id.clone(),
            session_path: self.session_path.clone(),
            datasets: self.datasets.clone(),
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
