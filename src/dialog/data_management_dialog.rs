//! DataManagementDialog: Dialog for managing all imported data sources and datasets

use std::ffi::OsStr;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Table, Row, Cell, Paragraph, Wrap};
use serde::{Deserialize, Serialize};
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use polars::prelude::*;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Size;
use tracing::{debug, info, warn, error};
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::data_import_types::DataImportConfig;
use crate::dialog::{
    data_import_dialog::DataImportDialog,
    alias_edit_dialog::AliasEditDialog,
};
use crate::components::dialog_layout::split_dialog_area;
use calamine::Reader;
use crate::dialog::MessageDialog;


/// Represents a single dataset within a data source
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub row_count: usize,
    pub column_count: usize,
    pub status: DatasetStatus,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Status of a dataset
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetStatus {
    Pending,    // Not yet imported
    Imported,   // Successfully imported
    Failed,     // Import failed
    Processing, // Currently being imported
}

impl DatasetStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            DatasetStatus::Pending => "Pending",
            DatasetStatus::Imported => "Imported",
            DatasetStatus::Failed => "Failed",
            DatasetStatus::Processing => "Processing",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            DatasetStatus::Pending => Color::Yellow,
            DatasetStatus::Imported => Color::Green,
            DatasetStatus::Failed => Color::Red,
            DatasetStatus::Processing => Color::Blue,
        }
    }
}

/// Represents a data source with its associated datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: usize,
    pub name: String,
    pub file_path: String,
    pub import_type: String,
    pub datasets: Vec<Dataset>,
    pub total_datasets: usize,
    pub imported_datasets: usize,
    pub failed_datasets: usize,
    pub data_import_config: DataImportConfig,
}

impl DataSource {
    /// Create a new DataSource from a DataImportConfig
    pub fn from_import_config(id: usize, config: &DataImportConfig) -> Self {
        let (name, file_path, import_type, datasets) = match config {
            DataImportConfig::Text(text_config) => {
                let name = text_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                let datasets = vec![Dataset {
                    id: Uuid::new_v4().to_string(),
                    name: name.clone(),
                    alias: None,
                    row_count: 0,
                    column_count: 0,
                    status: DatasetStatus::Pending,
                    error_message: None,
                }];
                (name, text_config.file_path.to_string_lossy().to_string(), "Text File".to_string(), datasets)
            }
            DataImportConfig::Excel(excel_config) => {
                let name = excel_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                let datasets = excel_config.options.worksheets.iter().map(|worksheet| Dataset {
                    id: Uuid::new_v4().to_string(),
                    name: worksheet.name.clone(),
                    alias: None,
                    row_count: worksheet.row_count,
                    column_count: worksheet.column_count,
                    status: if worksheet.load { DatasetStatus::Pending } else { DatasetStatus::Failed },
                    error_message: if worksheet.load { None } else { Some("Worksheet not marked for import".to_string()) },
                }).collect();
                (name, excel_config.file_path.to_string_lossy().to_string(), "Excel File".to_string(), datasets)
            }
            DataImportConfig::Sqlite(sqlite_config) => {
                let file_name = sqlite_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                
                let (name, datasets) = if let Some(table_name) = &sqlite_config.table_name {
                    // Specific table import - create a single dataset for this table
                    let dataset_name = format!("{file_name} - {table_name}");
                    let datasets = vec![Dataset {
                        id: Uuid::new_v4().to_string(),
                        name: table_name.clone(),
                        alias: None,
                        row_count: 0,
                        column_count: 0,
                        status: DatasetStatus::Pending,
                        error_message: None,
                    }];
                    (dataset_name, datasets)
                } else {
                    // Legacy behavior - should not be used with the new add_data_source logic
                    let datasets = vec![Dataset {
                        id: Uuid::new_v4().to_string(),
                        name: "All Tables".to_string(),
                        alias: None,
                        row_count: 0,
                        column_count: 0,
                        status: DatasetStatus::Pending,
                        error_message: None,
                    }];
                    (file_name, datasets)
                };
                
                (name, sqlite_config.file_path.to_string_lossy().to_string(), "SQLite Table".to_string(), datasets)
            }
            DataImportConfig::Parquet(parquet_config) => {
                let name = parquet_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                let datasets = vec![Dataset {
                    id: Uuid::new_v4().to_string(),
                    name: name.clone(),
                    alias: None,
                    row_count: 0,
                    column_count: 0,
                    status: DatasetStatus::Pending,
                    error_message: None,
                }];
                (name, parquet_config.file_path.to_string_lossy().to_string(), "Parquet File".to_string(), datasets)
            }
            DataImportConfig::Json(json_config) => {
                let name = json_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                let datasets = vec![Dataset {
                    id: Uuid::new_v4().to_string(),
                    name: name.clone(),
                    alias: None,
                    row_count: 0,
                    column_count: 0,
                    status: DatasetStatus::Pending,
                    error_message: None,
                }];
                (name, json_config.file_path.to_string_lossy().to_string(), "JSON File".to_string(), datasets)
            }
        };

        let total_datasets = datasets.len();
        let imported_datasets = datasets.iter().filter(|d| d.status == DatasetStatus::Imported).count();
        let failed_datasets = datasets.iter().filter(|d| d.status == DatasetStatus::Failed).count();

        Self {
            id,
            name,
            file_path,
            import_type,
            datasets,
            total_datasets,
            imported_datasets,
            failed_datasets,
            data_import_config: config.clone()
        }
    }

    /// Update dataset status
    pub fn update_dataset_status(&mut self, dataset_name: &str, status: DatasetStatus) {
        if let Some(dataset) = self.datasets.iter_mut().find(|d| d.name == dataset_name) {
            dataset.status = status;
        }
        self.update_counts();
    }

    /// Update dataset error message
    pub fn update_dataset_error(&mut self, dataset_name: &str, error: Option<String>) {
        if let Some(dataset) = self.datasets.iter_mut().find(|d| d.name == dataset_name) {
            dataset.error_message = error;
        }
    }

    /// Update dataset with actual data
    pub fn update_dataset_data(&mut self, dataset_name: &str, row_count: usize, column_count: usize) {
        if let Some(dataset) = self.datasets.iter_mut().find(|d| d.name == dataset_name) {
            dataset.row_count = row_count;
            dataset.column_count = column_count;
        }
    }

    /// Update the counts based on current dataset statuses
    fn update_counts(&mut self) {
        self.total_datasets = self.datasets.len();
        self.imported_datasets = self.datasets.iter().filter(|d| d.status == DatasetStatus::Imported).count();
        self.failed_datasets = self.datasets.iter().filter(|d| d.status == DatasetStatus::Failed).count();
    }

    /// Load datasets from this data source into DataFrames, skipping failed ones and
    /// ignoring per-dataset load errors so the caller can proceed rendering available tabs.
    pub fn load_dataframes(&self) -> Result<HashMap<String, LoadedDataset>> {
        let mut result = HashMap::new();

        for dataset in &self.datasets {
            // Skip datasets that are known to have failed
            if dataset.status == DatasetStatus::Failed {
                continue;
            }

            match self.load_dataset(dataset) {
                Ok(df_arc) => {
                    let loaded_dataset = LoadedDataset {
                        data_source: self.clone(),
                        dataset: dataset.clone(),
                        dataframe: df_arc,
                    };
                    result.insert(dataset.id.clone(), loaded_dataset);
                }
                Err(e) => {
                    // Do not propagate error; just skip this dataset so UI can still close/sync
                    warn!("Skipping dataset '{}' due to load error: {}", dataset.name, e);
                    continue;
                }
            }
        }

        Ok(result)
    }

    /// Load a single dataset for this source, using internal cache keyed by dataset.id
    pub fn load_dataset(&self, dataset: &Dataset) -> Result<Arc<DataFrame>> {
        let (df_arc, _warning) = self.load_dataset_with_warning(dataset)?;
        Ok(df_arc)
    }

    /// Load a single dataset for this source with retry-on-dtype-error behavior.
    /// Returns the DataFrame and an optional warning message.
    pub fn load_dataset_with_warning(&self, dataset: &Dataset) -> Result<(Arc<DataFrame>, Option<String>)> {
        let (df, warning): (DataFrame, Option<String>) = match &self.data_import_config {
            DataImportConfig::Text(text_config) => {
                // Build CsvParseOptions from our CsvImportOptions
                let parse_options = CsvParseOptions::default()
                    .with_separator(text_config.options.delimiter as u8)
                    .with_quote_char(text_config.options.quote_char.map(|c| c as u8));

                // Incremental retry loop: coerce only failing columns to Utf8 and retry until success
                let mut coerced = std::collections::BTreeSet::<String>::new();
                let mut last_err: Option<String> = None;
                let mut attempts: usize = 0;
                // Precompile regexes used within the retry loop
                let re_dtype = regex::Regex::new("as dtype `([^`]+)` at column '([^']+)' \\(column number \\d+\\)").ok();
                let re_at = regex::Regex::new("at '([^']+)'").ok();
                loop {
                    attempts += 1;
                    if attempts > 256 {
                        return Err(color_eyre::eyre::eyre!(
                            "Exceeded maximum dtype coercion attempts. Last error: {}",
                            last_err.unwrap_or_else(|| "unknown".to_string())
                        ));
                    }

                    let mut reader = LazyCsvReader::new(&text_config.file_path)
                        .map_parse_options(|_opts| parse_options.clone())
                        .with_has_header(text_config.options.has_header)
                        .with_infer_schema_length(Some(100_000));

                    if !coerced.is_empty() {
                        let schema = Schema::from_iter(
                            coerced.iter().map(
                                |c| {
                                    let col_name = c.as_str().into();
                                    let _dtype = DataType::String;
                                    Field::new(col_name, _dtype)
                                }
                            )
                        );
                        let schema_ref = std::sync::Arc::new(schema);
                        // Override only coerced columns to Utf8
                        reader = reader.with_dtype_overwrite(Some(schema_ref));
                    }

                    match reader.finish() {
                        Ok(lf) => match lf.collect() {
                            Ok(df_ok) => {
                                let warning = if coerced.is_empty() {
                                    None
                                } else {
                                    Some(format!(
                                        "CSV dtype inference failed; coerced columns to Utf8: {}",
                                        coerced.iter().cloned().collect::<Vec<_>>().join(", ")
                                    ))
                                };
                                break (df_ok, warning);
                            }
                            Err(e) => {
                                tracing::error!("CSV dtype inference failed: {:?}", e);
                                let msg = e.to_string();
                                last_err = Some(msg.clone());
                                // Try a richer pattern: capture dtype, column name, and column number
                                let mut extracted_col: Option<String> = None;
                                if let Some(re) = re_dtype.as_ref()
                                    && let Some(caps) = re.captures(&msg)
                                {
                                    let dtype = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                                    let col = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                                    let col_num = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                                    if !col.is_empty() {
                                        extracted_col = Some(col.to_string());
                                        if coerced.insert(col.to_string()) {
                                            tracing::info!(
                                                "CSV dtype inference failed: cannot parse as dtype `{}` at column '{}' (#{}). Coercing to string and retrying.",
                                                dtype,
                                                col,
                                                col_num
                                            );
                                            continue;
                                        }
                                    }
                                }
                                // Fallback: extract just the column name
                                if extracted_col.is_none()
                                    && let Some(re) = re_at.as_ref()
                                    && let Some(caps) = re.captures(&msg)
                                    && let Some(m) = caps.get(1)
                                {
                                    let col = m.as_str().to_string();
                                    if coerced.insert(col.clone()) {
                                        tracing::info!(
                                            "CSV dtype inference failed for column '{}' with error: {}. Coercing to string and retrying.",
                                            col,
                                            msg
                                        );
                                        continue;
                                    }
                                }
                            }
                        },
                        Err(e) => return Err(color_eyre::eyre::eyre!("Failed to parse CSV file: {e}")),
                    }
                }
            }
            DataImportConfig::Excel(excel_config) => {
                let mut workbook = calamine::open_workbook_auto(&excel_config.file_path)
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to open Excel file '{}': {}", excel_config.file_path.display(), e))?;
                let sheet_name = &dataset.name;
                let range = workbook
                    .worksheet_range(sheet_name)
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to read worksheet '{}': {}", sheet_name, e))?;

                let mut rows_as_strings: Vec<Vec<String>> = Vec::new();
                let mut max_cols: usize = 0;
                for row in range.rows() {
                    let mut out_row: Vec<String> = Vec::with_capacity(row.len());
                    for cell in row {
                        let s = match cell {
                            calamine::Data::Empty => String::new(),
                            calamine::Data::String(s) => s.clone(),
                            calamine::Data::Int(i) => i.to_string(),
                            calamine::Data::Float(f) => f.to_string(),
                            calamine::Data::Bool(b) => b.to_string(),
                            calamine::Data::DateTime(d) => d.as_f64().to_string(),
                            calamine::Data::DateTimeIso(s) => s.clone(),
                            calamine::Data::DurationIso(s) => s.clone(),
                            calamine::Data::Error(e) => format!("ERROR: {e:?}"),
                        };
                        out_row.push(s);
                    }
                    max_cols = max_cols.max(out_row.len());
                    rows_as_strings.push(out_row);
                }

                if rows_as_strings.is_empty() {
                    (DataFrame::empty(), None)
                } else {
                    for row in &mut rows_as_strings {
                        if row.len() < max_cols { row.resize(max_cols, String::new()); }
                    }
                    let header_row = rows_as_strings.remove(0);
                    let mut used_names: std::collections::HashSet<String> = std::collections::HashSet::new();
                    let mut column_names: Vec<String> = Vec::with_capacity(max_cols);
                    for (idx, raw_name) in header_row.into_iter().enumerate() {
                        let mut name = raw_name.trim().to_string();
                        if name.is_empty() { name = format!("column_{}", idx + 1); }
                        if used_names.contains(&name) {
                            let mut suffix = 2usize; let base = name.clone();
                            while used_names.contains(&format!("{base}_{suffix}")) { suffix += 1; }
                            name = format!("{base}_{suffix}");
                        }
                        used_names.insert(name.clone());
                        column_names.push(name);
                    }
                    let mut columns: Vec<Vec<String>> = vec![Vec::with_capacity(rows_as_strings.len()); max_cols];
                    for row in rows_as_strings.into_iter() {
                        for (col_idx, value) in row.into_iter().enumerate() { columns[col_idx].push(value); }
                    }
                    let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(max_cols);
                    for (name, values) in column_names.into_iter().zip(columns.into_iter()) {
                        let s = Series::new(name.as_str().into(), values);
                        cols.push(s.into());
                    }
                    let df_excel = DataFrame::new(cols)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame from worksheet '{}': {}", sheet_name, e))?;
                    (df_excel, None)
                }
            }
            DataImportConfig::Sqlite(sqlite_config) => {
                use rusqlite::Connection;
                use polars::prelude::*;
                use rusqlite::types::ValueRef;
                
                // Open SQLite database connection
                let conn = Connection::open(&sqlite_config.file_path)
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to open SQLite database: {}", e))?;
                
                // Determine which table to import
                let table_name = if let Some(specific_table) = &sqlite_config.table_name {
                    // Import the specific table
                    specific_table.clone()
                } else {
                    // Use the original logic for backward compatibility
                    let tables_to_import = if sqlite_config.options.import_all_tables {
                        // Get all user tables from the database
                        let mut stmt = conn.prepare(
                            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
                        ).map_err(|e| color_eyre::eyre::eyre!("Failed to prepare SQL statement: {}", e))?;
                        
                        let table_rows = stmt.query_map([], |row| {
                            row.get::<_, String>(0)
                        }).map_err(|e| color_eyre::eyre::eyre!("Failed to execute query: {}", e))?;
                        
                        let mut tables = Vec::new();
                        for table_result in table_rows {
                            let table_name = table_result
                                .map_err(|e| color_eyre::eyre::eyre!("Failed to read table name: {}", e))?;
                            tables.push(table_name);
                        }
                        tables
                    } else {
                        sqlite_config.options.selected_tables.clone()
                    };
                    
                    if tables_to_import.is_empty() {
                        return Err(color_eyre::eyre::eyre!("No tables selected for import"));
                    }
                    
                    // Import the first table as the primary DataFrame
                    tables_to_import[0].clone()
                };
                
                // Query all data from the table
                let query = format!("SELECT * FROM [{}]", table_name.replace("'", "''"));
                let mut stmt = conn.prepare(&query)
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to prepare query for table '{}': {}", table_name, e))?;
                
                // Get column information
                let column_count = stmt.column_count();
                let column_names: Vec<String> = (0..column_count)
                    .map(|i| stmt.column_name(i).unwrap_or("unknown").to_string())
                    .collect();
                
                // Fetch all rows as typed values, infer column dtypes, and build DataFrame without string-casting
                #[derive(Clone, Debug)]
                enum SqlValue {
                    Null,
                    Integer(i64),
                    Real(f64),
                    Text(String),
                    Blob(Vec<u8>),
                }

                #[derive(Copy, Clone, Debug, PartialEq, Eq)]
                enum ColType {
                    Unknown,
                    Int64,
                    Float64,
                    Utf8,
                }

                let rows = stmt
                    .query_map([], |row| {
                        let mut values: Vec<SqlValue> = Vec::with_capacity(column_count);
                        for i in 0..column_count {
                            let v = match row.get_ref(i)? {
                                ValueRef::Null => SqlValue::Null,
                                ValueRef::Integer(x) => SqlValue::Integer(x),
                                ValueRef::Real(f) => SqlValue::Real(f),
                                ValueRef::Text(bytes) => {
                                    let s = std::str::from_utf8(bytes).unwrap_or("").to_string();
                                    SqlValue::Text(s)
                                }
                                ValueRef::Blob(b) => SqlValue::Blob(b.to_vec()),
                            };
                            values.push(v);
                        }
                        Ok(values)
                    })
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to execute query: {}", e))?;

                let mut data_rows: Vec<Vec<SqlValue>> = Vec::new();
                for row_result in rows {
                    let row = row_result
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to read row: {}", e))?;
                    data_rows.push(row);
                }

                // Infer column types with simple promotion rules and boolean detection
                let mut col_types: Vec<ColType> = vec![ColType::Unknown; column_count];
                let mut bool_candidate: Vec<bool> = vec![true; column_count];
                for row in &data_rows {
                    for (i, val) in row.iter().enumerate() {
                        match val {
                            SqlValue::Null => {}
                            SqlValue::Integer(x) => {
                                match col_types[i] {
                                    ColType::Unknown => col_types[i] = ColType::Int64,
                                    ColType::Float64 | ColType::Utf8 | ColType::Int64 => {}
                                }
                                if *x != 0 && *x != 1 { bool_candidate[i] = false; }
                            }
                            SqlValue::Real(_) => {
                                match col_types[i] {
                                    ColType::Unknown | ColType::Int64 => col_types[i] = ColType::Float64,
                                    ColType::Float64 | ColType::Utf8 => {}
                                }
                                bool_candidate[i] = false;
                            }
                            SqlValue::Text(_) => {
                                col_types[i] = ColType::Utf8;
                                bool_candidate[i] = false;
                            }
                            SqlValue::Blob(_) => {
                                // Fallback to Utf8 for blobs to avoid losing other types; encode as debug string
                                col_types[i] = ColType::Utf8;
                                bool_candidate[i] = false;
                            }
                        }
                    }
                }

                // Default unknown columns to Utf8 (all-null becomes Utf8 with None values)
                for ct in &mut col_types { if *ct == ColType::Unknown { *ct = ColType::Utf8; } }

                // Build columns by dtype, preserving native numeric/bool types
                let mut columns = Vec::with_capacity(column_count as usize);
                for (i, col_name) in column_names.iter().enumerate() {
                    match col_types[i] {
                        ColType::Int64 if bool_candidate[i] => {
                            let data: Vec<Option<bool>> = data_rows
                                .iter()
                                .map(|row| match row[i] {
                                    SqlValue::Null => None,
                                    SqlValue::Integer(x) => Some(x != 0),
                                    _ => None,
                                })
                                .collect();
                            let series = Series::new(col_name.clone().into(), data);
                            columns.push(series.into());
                        }
                        ColType::Int64 => {
                            let data: Vec<Option<i64>> = data_rows
                                .iter()
                                .map(|row| match row[i] {
                                    SqlValue::Null => None,
                                    SqlValue::Integer(x) => Some(x),
                                    _ => None,
                                })
                                .collect();
                            let series = Series::new(col_name.clone().into(), data);
                            columns.push(series.into());
                        }
                        ColType::Float64 => {
                            let data: Vec<Option<f64>> = data_rows
                                .iter()
                                .map(|row| match row[i] {
                                    SqlValue::Null => None,
                                    SqlValue::Integer(x) => Some(x as f64),
                                    SqlValue::Real(f) => Some(f),
                                    _ => None,
                                })
                                .collect();
                            let series = Series::new(col_name.clone().into(), data);
                            columns.push(series.into());
                        }
                        ColType::Unknown => {
                            // All-null or unresolved columns: treat as Utf8 with None
                            let data: Vec<Option<String>> = data_rows
                                .iter()
                                .map(|row| match &row[i] {
                                    SqlValue::Null => None,
                                    SqlValue::Integer(x) => Some(x.to_string()),
                                    SqlValue::Real(f) => Some(f.to_string()),
                                    SqlValue::Text(s) => Some(s.clone()),
                                    SqlValue::Blob(b) => Some(format!("{b:?}")),
                                })
                                .collect();
                            let series = Series::new(col_name.clone().into(), data);
                            columns.push(series.into());
                        }
                        ColType::Utf8 => {
                            let data: Vec<Option<String>> = data_rows
                                .iter()
                                .map(|row| match &row[i] {
                                    SqlValue::Null => None,
                                    SqlValue::Integer(x) => Some(x.to_string()),
                                    SqlValue::Real(f) => Some(f.to_string()),
                                    SqlValue::Text(s) => Some(s.clone()),
                                    SqlValue::Blob(b) => Some(format!("{b:?}")),
                                })
                                .collect();
                            let series = Series::new(col_name.clone().into(), data);
                            columns.push(series.into());
                        }
                    }
                }
                
                let df_sqlite = DataFrame::new(columns)
                    .map_err(|e| {
                        error!("Failed to build DataFrame from SQLite table '{}': {}", table_name, e);
                        color_eyre::eyre::eyre!(
                            "Failed to build DataFrame from SQLite table '{}': {}", table_name, e
                        )
                })?;
                
                (df_sqlite, Some(table_name.clone()))
            }
            DataImportConfig::Parquet(parquet_config) => {
                let df_pq = polars::prelude::ParquetReader::new(std::fs::File::open(&parquet_config.file_path)?)
                    .finish()
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to read Parquet file: {e}"))?;
                (df_pq, None)
            }
            DataImportConfig::Json(json_config) => {
                // For simplicity, read as a Series of strings per JSON object, then expand to columns
                // Strategy:
                // - If ndjson: read file line by line, parse each line as JSON object (map), collect keys, build string columns
                // - Else: read whole file as JSON value; if it's an array of objects, do same
                use std::io::{BufRead, BufReader, Read};
                use serde_json::Value as JsonValue;

                let path = &json_config.file_path;
                let mut objects: Vec<serde_json::Map<String, JsonValue>> = Vec::new();
                if json_config.options.ndjson {
                    let file = std::fs::File::open(path)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to open NDJSON file: {e}"))?;
                    let reader = BufReader::new(file);
                    for line_res in reader.lines() {
                        let line = line_res.map_err(|e| color_eyre::eyre::eyre!("Failed to read NDJSON line: {e}"))?;
                        if line.trim().is_empty() { continue; }
                        let val: JsonValue = serde_json::from_str(&line)
                            .map_err(|e| color_eyre::eyre::eyre!("Failed to parse NDJSON line as JSON object: {e}"))?;
                        if let JsonValue::Object(map) = val {
                            objects.push(map);
                        } else {
                            return Err(color_eyre::eyre::eyre!("NDJSON line is not a JSON object"));
                        }
                    }
                } else {
                    let mut file = std::fs::File::open(path)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to open JSON file: {e}"))?;
                    let mut buf = String::new();
                    file.read_to_string(&mut buf)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to read JSON file: {e}"))?;
                    let val: JsonValue = serde_json::from_str(&buf)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse JSON: {e}"))?;
                    match val {
                        JsonValue::Array(arr) => {
                            for v in arr {
                                if let JsonValue::Object(map) = v {
                                    objects.push(map);
                                } else {
                                    return Err(color_eyre::eyre::eyre!("JSON array elements must be objects"));
                                }
                            }
                        }
                        JsonValue::Object(map) => {
                            objects.push(map);
                        }
                        _ => {
                            return Err(color_eyre::eyre::eyre!("Top-level JSON must be object or array of objects"));
                        }
                    }
                }

                // Build set of all keys
                let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for obj in &objects {
                    for k in obj.keys() { keys.insert(k.clone()); }
                }

                // For each key, build a Utf8 column by stringifying scalar values; objects/arrays are JSON-stringified
                let mut columns: Vec<polars::prelude::Column> = Vec::with_capacity(keys.len());
                for key in keys.iter() {
                    let mut col_vals: Vec<String> = Vec::with_capacity(objects.len());
                    for obj in &objects {
                        let v = obj.get(key).cloned().unwrap_or(JsonValue::Null);
                        let s = match v {
                            JsonValue::Null => String::new(),
                            JsonValue::Bool(b) => b.to_string(),
                            JsonValue::Number(n) => n.to_string(),
                            JsonValue::String(s) => s,
                            other => other.to_string(),
                        };
                        col_vals.push(s);
                    }
                    let s = polars::prelude::Series::new(key.as_str().into(), col_vals);
                    columns.push(s.into());
                }

                let df_json = DataFrame::new(columns)
                    .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame from JSON: {e}"))?;
                (df_json, None)
            }
        };

        Ok((Arc::new(df), warning))
    }
}

/// Represents a loaded dataset with its associated data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedDataset {
    pub data_source: DataSource,
    pub dataset: Dataset,
    pub dataframe: Arc<DataFrame>,
}

impl LoadedDataset {
    /// Get the ID for the loaded dataset
    pub fn id(&self) -> String {
        self.dataset.id.clone()
    }

    /// Get the display name for the loaded dataset
    pub fn display_name(&self) -> String {
        self.dataset.alias.as_ref().unwrap_or(&self.dataset.name).clone()
    }
}

/// DataManagementDialog: UI for managing all imported data sources and datasets
#[derive(Debug, Serialize, Deserialize)]
pub struct DataManagementDialog {
    pub data_sources: Vec<DataSource>,
    pub selected_dataset_index: usize,
    pub scroll_offset: usize,
    pub show_instructions: bool,
    pub data_import_dialog: Option<DataImportDialog>,
    #[serde(skip)]
    pub alias_edit_dialog: Option<AliasEditDialog>,
    #[serde(skip)]
    pub message_dialog: Option<MessageDialog>,
    #[serde(skip)]
    pub config: Config,
    // Busy/progress overlay for queued imports
    #[serde(skip)]
    pub busy_active: bool,
    #[serde(skip)]
    pub busy_message: String,
    #[serde(skip)]
    pub busy_progress: f64,
    #[serde(skip)]
    pub queue_total: usize,
    #[serde(skip)]
    pub queue_done: usize,
    #[serde(skip)]
    pub pending_queue: Vec<(usize, String)>, // (source_id, dataset_name)
    #[serde(skip)]
    pub load_errors: Vec<String>,
    #[serde(skip)]
    pub current_loading: Option<String>,
}

impl Default for DataManagementDialog {
    fn default() -> Self { Self::new() }
}

impl DataManagementDialog {
    /// Create a new DataManagementDialog
    pub fn new() -> Self {
        Self {
            data_sources: Vec::new(),
            selected_dataset_index: 0,
            scroll_offset: 0,
            show_instructions: true,
            data_import_dialog: None,
            alias_edit_dialog: None,
            message_dialog: None,
            config: Config::default(),
            busy_active: false,
            busy_message: String::new(),
            busy_progress: 0.0,
            queue_total: 0,
            queue_done: 0,
            pending_queue: Vec::new(),
            load_errors: Vec::new(),
            current_loading: None,
        }
    }

    /// Build instructions string from configured keybindings (Global + DataManagement)
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::DataManagement, crate::action::Action::EditSelectedAlias),
            (crate::config::Mode::DataManagement, crate::action::Action::DeleteSelectedSource),
            (crate::config::Mode::DataManagement, crate::action::Action::OpenDataImportDialog),
            (crate::config::Mode::DataManagement, crate::action::Action::LoadAllPendingDatasets),
        ])
    }

    /// Add a new data source from a DataImportConfig
    pub fn add_data_source(&mut self, config: DataImportConfig) {
        // Special handling for SQLite to create individual data sources for each table
        if let DataImportConfig::Sqlite(sqlite_config) = &config {
            if sqlite_config.options.import_all_tables {
                // Create a separate DataSource for each table
                if let Ok(table_names) = DataManagementDialog::get_sqlite_table_names(&sqlite_config.file_path) {
                    for table_name in table_names {
                        let table_config = crate::data_import_types::DataImportConfig::sqlite_table(
                            sqlite_config.file_path.clone(),
                            sqlite_config.options.clone(),
                            table_name
                        );
                        let id = self.data_sources.len();
                        let data_source: DataSource = DataSource::from_import_config(id, &table_config);
                        self.data_sources.push(data_source);
                    }
                    return;
                } else {
                    // Fallback to original behavior if we can't read tables
                    let id = self.data_sources.len();
                    let data_source: DataSource = DataSource::from_import_config(id, &config);
                    self.data_sources.push(data_source);
                    return;
                }
            } else if !sqlite_config.options.selected_tables.is_empty() {
                // Create a separate DataSource for each selected table
                for table_name in &sqlite_config.options.selected_tables {
                    let table_config = crate::data_import_types::DataImportConfig::sqlite_table(
                        sqlite_config.file_path.clone(),
                        sqlite_config.options.clone(),
                        table_name.clone()
                    );
                    let id = self.data_sources.len();
                    let data_source: DataSource = DataSource::from_import_config(id, &table_config);
                    self.data_sources.push(data_source);
                }
                return;
            }
        }
        
        // Default behavior for all other import types
        let id = self.data_sources.len();
        let data_source: DataSource = DataSource::from_import_config(id, &config);
        self.data_sources.push(data_source);
    }

    /// Get table names from a SQLite database
    fn get_sqlite_table_names(file_path: &std::path::PathBuf) -> Result<Vec<String>> {
        use rusqlite::Connection;
        
        let conn = Connection::open(file_path)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to open SQLite database: {}", e))?;
        
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ).map_err(|e| color_eyre::eyre::eyre!("Failed to prepare SQL statement: {}", e))?;
        
        let table_rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| color_eyre::eyre::eyre!("Failed to execute query: {}", e))?;
        
        let mut tables = Vec::new();
        for table_result in table_rows {
            let table_name = table_result
                .map_err(|e| color_eyre::eyre::eyre!("Failed to read table name: {}", e))?;
            tables.push(table_name);
        }
        
        Ok(tables)
    }

    /// Extend or merge data sources with the provided list
    /// - Existing sources are matched by (file_path, import_type) and replaced
    /// - New sources are appended
    /// - Source IDs are reassigned to be sequential starting at 0
    /// - The selected dataset index is clamped to a valid range
    pub fn extend_data_sources(&mut self, sources: Vec<DataSource>) {
        // Build an index for existing sources by a stable key
        let mut key_to_index: HashMap<(String, String), usize> = HashMap::new();
        for (idx, src) in self.data_sources.iter().enumerate() {
            key_to_index.insert((src.file_path.clone(), src.import_type.clone()), idx);
        }

        // Merge/append incoming sources
        for src in sources.into_iter() {
            let key = (src.file_path.clone(), src.import_type.clone());
            if let Some(&idx) = key_to_index.get(&key) {
                // Replace existing entry
                self.data_sources[idx] = src;
            } else {
                // Append and record index
                self.data_sources.push(src);
                let new_idx = self.data_sources.len() - 1;
                key_to_index.insert(key, new_idx);
            }
        }

        // Reassign sequential IDs
        for (idx, src) in self.data_sources.iter_mut().enumerate() {
            src.id = idx;
        }

        // Clamp selection
        let total_datasets = self.get_all_datasets().len();
        if total_datasets == 0 || self.selected_dataset_index >= total_datasets {
            self.selected_dataset_index = 0;
        }
    }

    /// Remove a data source by ID
    pub fn remove_data_source(&mut self, id: usize) {
        self.data_sources.retain(|source| source.id != id);
        // Reassign IDs
        for (index, source) in self.data_sources.iter_mut().enumerate() {
            source.id = index;
        }
        // Reset selection if out of bounds
        let total_datasets = self.get_all_datasets().len();
        if !self.data_sources.is_empty() && self.selected_dataset_index >= total_datasets {
            self.selected_dataset_index = 0;
        }
    }

    /// Get all datasets from all sources as a flat list with source info
    pub fn get_all_datasets(&self) -> Vec<(usize, &DataSource, &Dataset)> {
        let mut all_datasets = Vec::new();
        for source in &self.data_sources {
            for dataset in &source.datasets {
                all_datasets.push((source.id, source, dataset));
            }
        }
        all_datasets
    }

    /// Get the currently selected dataset with its source info
    pub fn selected_dataset(&self) -> Option<(usize, &DataSource, &Dataset)> {
        let all_datasets = self.get_all_datasets();
        all_datasets.get(self.selected_dataset_index).copied()
    }

    /// Update dataset status for a specific source and dataset
    pub fn update_dataset_status(&mut self, source_id: usize, dataset_name: &str, status: DatasetStatus) {
        if let Some(source) = self.data_sources.iter_mut().find(|s| s.id == source_id) {
            source.update_dataset_status(dataset_name, status);
        }
    }

    /// Update dataset data for a specific source and dataset
    pub fn update_dataset_data(&mut self, source_id: usize, dataset_name: &str, row_count: usize, column_count: usize) {
        if let Some(source) = self.data_sources.iter_mut().find(|s| s.id == source_id) {
            source.update_dataset_data(dataset_name, row_count, column_count);
        }
    }

    /// Update dataset error message for a specific source and dataset
    pub fn update_dataset_error(&mut self, source_id: usize, dataset_name: &str, error: Option<String>) {
        if let Some(source) = self.data_sources.iter_mut().find(|s| s.id == source_id) {
            source.update_dataset_error(dataset_name, error);
        }
    }

    /// Update dataset alias for a specific source and dataset
    pub fn update_dataset_alias(&mut self, source_id: usize, dataset_id: &str, alias: Option<String>) {
        if let Some(source) = self.data_sources.iter_mut().find(|s| s.id == source_id)
            && let Some(dataset) = source.datasets.iter_mut().find(|d| d.id == dataset_id)
        {
            dataset.alias = alias;
        }
    }

    /// Load all pending datasets into the dataframe_mapping (used for both manual and auto-loading)
    pub fn load_all_pending_datasets(&mut self) -> Result<()> {
        // Collect all pending datasets first to avoid borrow checker issues
        let mut pending_datasets = Vec::new();
        let mut load_errors: Vec<String> = Vec::new();
        for data_source in &self.data_sources {
            for dataset in &data_source.datasets {
                if dataset.status == DatasetStatus::Pending {
                    pending_datasets.push((data_source.id, dataset.name.clone(), dataset.id.clone()));
                }
            }
        }
        
        // Process each pending dataset
        for (source_id, dataset_name, _dataset_id) in pending_datasets {
            // Set status to Processing
            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Processing);
            // Clear any previous error
            self.update_dataset_error(source_id, &dataset_name, None);

            // Find the data source and dataset
            if let Some(ds_ref) = self.data_sources.iter().find(|s| s.id == source_id) {
                let dataset_cloned = ds_ref.datasets.iter().find(|d| d.name == dataset_name).cloned();
                if let Some(dataset) = dataset_cloned {
                    // Use unified loader on DataSource
                    match ds_ref.load_dataset(&dataset) {
                        Ok(dataframe) => {
                            // Successfully loaded
                            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Imported);
                            
                            // Update dataset data with actual row/column counts
                            let row_count = dataframe.height();
                            let column_count = dataframe.width();
                            self.update_dataset_data(source_id, &dataset_name, row_count, column_count);
                        }
                        Err(e) => {
                            // Failed to load
                            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Failed);
                            self.update_dataset_error(source_id, &dataset_name, Some(e.to_string()));
                            // Collect error for message dialog after processing all datasets
                            if let Some(ds_ref_name) = self.data_sources.iter().find(|s| s.id == source_id).map(|s| s.name.clone()) {
                                load_errors.push(format!("Source '{ds_ref_name}', Dataset '{dataset_name}': {e}"));
                            } else {
                                load_errors.push(format!("Dataset '{dataset_name}': {e}"));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Begin queued import of all pending datasets, showing a busy overlay and
    /// processing one dataset per Render update.
    pub fn begin_queued_import(&mut self) -> Result<()> {
        // Build queue of all Pending datasets
        self.pending_queue.clear();
        self.load_errors.clear();
        for data_source in &self.data_sources {
            for dataset in &data_source.datasets {
                if dataset.status == DatasetStatus::Pending {
                    self.pending_queue.push((data_source.id, dataset.name.clone()));
                }
            }
        }
        self.queue_total = self.pending_queue.len();
        self.queue_done = 0;
        self.current_loading = None;
        if self.queue_total == 0 {
            return Ok(());
        }
        self.busy_active = true;
        self.busy_message = format!("Importing datasets (0/{})", self.queue_total);
        self.busy_progress = 0.0;
        Ok(())
    }

    /// Process the next dataset in the queue. Should be called on Render after UI draw
    /// so the overlay is visible while the load happens.
    pub fn process_next_in_queue(&mut self) -> Result<()> {
        if !self.busy_active { return Ok(()); }
        if let Some((source_id, dataset_name)) = self.pending_queue.first().cloned() {
            // Phase 1: show message for the current item, then return to allow a frame to render it
            if self.current_loading.as_deref() != Some(&dataset_name) {
                self.current_loading = Some(dataset_name.clone());
                let display_done = self.queue_done.min(self.queue_total);
                self.busy_message = format!("Loading '{}' ({}/{})", dataset_name, display_done + 1, self.queue_total);
                return Ok(());
            }
            // Mark Processing and clear previous error
            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Processing);
            self.update_dataset_error(source_id, &dataset_name, None);

            // Find source and dataset snapshot
            if let Some(ds_ref) = self.data_sources.iter().find(|s| s.id == source_id) {
                let dataset_cloned = ds_ref.datasets.iter().find(|d| d.name == dataset_name).cloned();
                if let Some(dataset) = dataset_cloned {
                    match ds_ref.load_dataset(&dataset) {
                        Ok(dataframe) => {
                            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Imported);
                            let row_count = dataframe.height();
                            let column_count = dataframe.width();
                            self.update_dataset_data(source_id, &dataset_name, row_count, column_count);
                        }
                        Err(e) => {
                            self.update_dataset_status(source_id, &dataset_name, DatasetStatus::Failed);
                            self.update_dataset_error(source_id, &dataset_name, Some(e.to_string()));
                            if let Some(ds_ref_name) = self.data_sources.iter().find(|s| s.id == source_id).map(|s| s.name.clone()) {
                                self.load_errors.push(format!("Source '{ds_ref_name}', Dataset '{dataset_name}': {e}"));
                            } else {
                                self.load_errors.push(format!("Dataset '{dataset_name}': {e}"));
                            }
                        }
                    }
                }
            }

            // Dequeue and update counters/message
            let _ = self.pending_queue.remove(0);
            self.queue_done = self.queue_done.saturating_add(1);
            self.current_loading = None;
            if self.queue_done < self.queue_total {
                // Peek next item to show name if available
                if let Some((_, next_name)) = self.pending_queue.first() {
                    self.busy_message = format!("Loading '{}' ({}/{})", next_name, self.queue_done + 1, self.queue_total);
                } else {
                    self.busy_message = format!("Importing datasets ({}/{})", self.queue_done, self.queue_total);
                }
            } else {
                // Completed
                self.busy_active = false;
                self.busy_message.clear();
                self.busy_progress = 0.0;
                if !self.load_errors.is_empty() {
                    let message = self.load_errors.join("\n");
                    error!("Dataset Load Errors: {}", message);

                    let mut msg = MessageDialog::with_title(message, "Dataset Load Errors");
                    msg.register_config_handler(self.config.clone())?;
                    self.message_dialog = Some(msg);
                }
            }
        }

        Ok(())
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the background for the popup
        Clear.render(area, buf);

        let instructions = if self.show_instructions { self.build_instructions_from_config() } else { String::new() };
        let layout = split_dialog_area(area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        
        let block = Block::default()
            .title("Data Management")
            .borders(Borders::ALL);
        
        let inner_area = block.inner(content_area);
        block.render(content_area, buf);

        // Check if we should render the data import dialog
        if let Some(ref import_dialog) = self.data_import_dialog {
            // Create a centered dialog area with 5% margin on all sides
            let margin_x = (area.width as f32 * 0.05) as u16;
            let margin_y = (area.height as f32 * 0.05) as u16;
            let import_dialog_area = Rect::new(
                area.x + margin_x,
                area.y + margin_y,
                area.width.saturating_sub(margin_x * 2),
                area.height.saturating_sub(margin_y * 2),
            );
            self.render_datasets_table(inner_area, buf);
            self.render_instructions(&instructions, instructions_area, buf);
            import_dialog.render(import_dialog_area, buf);
        } else if let Some(ref alias_dialog) = self.alias_edit_dialog {
            // Create a smaller centered dialog area for alias editing
            let dialog_width = 50.min(area.width.saturating_sub(4));
            let dialog_height = 10.min(area.height.saturating_sub(4));
            let alias_dialog_area = Rect::new(
                area.x + (area.width.saturating_sub(dialog_width)) / 2,
                area.y + (area.height.saturating_sub(dialog_height)) / 2,
                dialog_width,
                dialog_height,
            );
            self.render_datasets_table(inner_area, buf);
            self.render_instructions(&instructions, instructions_area, buf);
            alias_dialog.render(alias_dialog_area, buf);
        } else {
            self.render_datasets_table(inner_area, buf);
            self.render_instructions(&instructions, instructions_area, buf);
        }

        // Overlay message dialog if active
        if let Some(ref msg) = self.message_dialog {
            // Calculate a centered area for the message dialog, sized to fit the message content.
            // We'll use a similar approach as MessageDialog::modal_area, but do it here for overlay.
            let max_width = area.width.clamp(30, 60);
            let wrap_width = max_width.saturating_sub(4) as usize;
            let wrapped_lines = textwrap::wrap(&msg.message, wrap_width);
            let content_lines = wrapped_lines.len() as u16;
            let height = content_lines
                .saturating_add(4) // borders + padding
                .clamp(5, area.height.saturating_sub(4));
            let width = max_width;
            let x = area.x + (area.width.saturating_sub(width)) / 2;
            let y = area.y + (area.height.saturating_sub(height)) / 2;
            let msg_area = Rect { x, y, width, height };
            Clear.render(msg_area, buf);
            msg.render(msg_area, buf);
        }

        // Render busy/progress overlay if active (always on top)
        if self.busy_active {
            use ratatui::widgets::Gauge;
            let popup_area = Rect::new(
                area.x + area.width / 6,
                area.y + area.height / 2 - 2,
                area.width - area.width / 3,
                5,
            );
            Clear.render(popup_area, buf);
            let ratio = if self.queue_total > 0 {
                (self.queue_done as f64 / self.queue_total as f64).clamp(0.0, 1.0)
            } else {
                self.busy_progress.clamp(0.0, 1.0)
            };
            let gauge = Gauge::default()
                .block(Block::default().title(self.busy_message.clone()).borders(Borders::ALL))
                .ratio(ratio)
                .label("Importing...");
            gauge.render(popup_area, buf);
        }
    }

    fn render_instructions(&self, instructions: &str, instructions_area: Option<Rect>, buf: &mut Buffer) {
        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    /// Render the unified datasets table
    fn render_datasets_table(&self, area: Rect, buf: &mut Buffer) {
        let all_datasets = self.get_all_datasets();
        
        if all_datasets.is_empty() {
            let message = "No datasets available.\nUse the import dialogs to add data sources.";
            let paragraph = Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            paragraph.render(area, buf);
            return;
        }

        // Create table headers
        let headers = Row::new(vec![
            Cell::from("Source"),
            Cell::from("Dataset"),
            Cell::from("Alias"),
            Cell::from("Type"),
            Cell::from("Status"),
            Cell::from("Rows"),
            Cell::from("Columns"),
            Cell::from("File Path"),
        ]).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        // Create table rows with zebra striping by source
        let rows: Vec<Row> = all_datasets.iter().enumerate().map(|(index, (source_id, source, dataset))| {
            let is_selected = index == self.selected_dataset_index;
            
            // Determine zebra stripe based on source
            let is_even_source = source_id.is_multiple_of(2);
            let zebra_bg = if is_even_source { Color::Rgb(40, 40, 40) } else { Color::Black };
            
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default().fg(dataset.status.color()).bg(zebra_bg)
            };

            Row::new(vec![
                Cell::from(source.name.as_str()),
                Cell::from(dataset.name.as_str()),
                Cell::from(dataset.alias.as_deref().unwrap_or("")),
                Cell::from(source.import_type.as_str()),
                Cell::from(dataset.status.display_name()),
                Cell::from(format!("{row_count}", row_count = dataset.row_count)),
                Cell::from(format!("{col_count}", col_count = dataset.column_count)),
                Cell::from(source.file_path.as_str()),
            ]).style(style)
        }).collect();

        // Create and render table
        let data_set_table = Table::new(rows, &[
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(20),
        ])
        .header(headers)
        .block(Block::default().borders(Borders::NONE));

        Widget::render(data_set_table, area, buf);
    }
}

impl Component for DataManagementDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
        self.config = _config;
        // Propagate to child dialogs if they exist
        if let Some(ref mut d) = self.data_import_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.alias_edit_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.message_dialog { let _ = d.register_config_handler(self.config.clone()); }
        Ok(())
    }

    fn init(&mut self, _area: Size) -> Result<()> {
        Ok(())
    }

    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        if let Some(Event::Key(key)) = event {
            self.handle_key_event(key)
        } else {
            Ok(None)
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        debug!("DataManagementDialog handle_key_event: {:?}", key);

        // Handle message dialog if it's open
        if let Some(ref mut msg) = self.message_dialog {
            if let Some(action) = msg.handle_key_event(key)? {
                match action {
                    Action::DialogClose => {
                        self.message_dialog = None;
                        return Ok(None);
                    }
                    _ => return Ok(Some(action)),
                }
            }
            return Ok(None);
        }

        // Handle alias edit dialog if it's open
        if let Some(ref mut alias_dialog) = self.alias_edit_dialog {
            if let Some(action) = alias_dialog.handle_key_event(key)? {
                match action {
                    Action::DialogClose => {
                        self.alias_edit_dialog = None;
                        return Ok(None);
                    }
                    Action::EditDatasetAlias { source_id, dataset_id, alias } => {
                        // Update the dataset alias
                        self.update_dataset_alias(source_id, &dataset_id, alias);
                        self.alias_edit_dialog = None;
                        return Ok(None);
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        // Handle data import dialog if it's open
        if let Some(ref mut import_dialog) = self.data_import_dialog {
            if let Some(action) = import_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseDataImportDialog => {
                        self.data_import_dialog = None;
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config } => {
                        // Add the data source from the import config
                        self.add_data_source(config);
                        self.data_import_dialog = None;
                    // Begin queued import; progress advances on Render updates
                    let _ = self.begin_queued_import();
                    return Ok(None);
                    }
                    Action::ConfirmDataImport => {
                        // Handle the confirmation action and auto-load data
                        self.data_import_dialog = None;
                    // Begin queued import; progress advances on Render updates
                    let _ = self.begin_queued_import();
                    return Ok(None);
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }
        
        if let Some(nav_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            debug!("DataManagementDialog action_for_key<Global>: {:?}", nav_action);
            match nav_action {
                Action::Escape => {
                    return Ok(Some(Action::CloseDataManagementDialog));
                }
                Action::Up => {
                    if self.selected_dataset_index > 0 {
                        self.selected_dataset_index = self.selected_dataset_index.saturating_sub(1);
                    }
                    return Ok(None);
                }
                Action::Down => {
                    let total_datasets = self.get_all_datasets().len();
                    if self.selected_dataset_index < total_datasets.saturating_sub(1) {
                        self.selected_dataset_index = self.selected_dataset_index.saturating_add(1);
                    }
                    return Ok(None);
                }
                _ => {
                    info!("DataManagementDialog unhandled Global action: {:?} for key: {:?}", nav_action, key);
                }
            }
        }

        if let Some(dm_action) = self.config.action_for_key(crate::config::Mode::DataManagement, key) {
            debug!("DataManagementDialog action_for_key<DataManagement>: {:?}", dm_action);
            match dm_action {
                Action::DeleteSelectedSource => {
                    if let Some((source_id, _source, _dataset)) = self.selected_dataset() {
                        self.remove_data_source(source_id);
                        return Ok(Some(Action::RemoveDataSource { source_id }));
                    }
                    return Ok(None);
                }
                Action::OpenDataImportDialog => {
                    let mut _dialog = DataImportDialog::new();
                    _dialog.register_config_handler(self.config.clone())?;
                    self.data_import_dialog = Some(_dialog);
                    return Ok(None);
                }
                Action::LoadAllPendingDatasets => {
                    self.load_all_pending_datasets()?;
                    return Ok(None);
                }
                Action::EditSelectedAlias => {
                    if let Some((source_id, _source, dataset)) = self.selected_dataset() {
                        let mut _dialog = AliasEditDialog::new(
                            source_id,
                            dataset.id.clone(),
                            dataset.name.clone(),
                            dataset.alias.clone(),
                        );
                        _dialog.register_config_handler(self.config.clone())?;
                        self.alias_edit_dialog = Some(_dialog);
                    }
                    return Ok(None);
                }
                _ => {
                    info!("DataManagementDialog unhandled DataManagement action: {:?} for key: {:?}", dm_action, key);
                }
            }
        }

        Ok(None)
    }

    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        match _action {
            Action::Tick => {
                if self.busy_active {
                    self.busy_progress += 0.02;
                    if self.busy_progress >= 1.0 { self.busy_progress = 0.0; }
                }
                Ok(None)
            }
            Action::Render => {
                if self.busy_active {
                    // Process the next item after the overlay has been drawn
                    self.process_next_in_queue()?;
                }
                Ok(None)
            }
            Action::StartBlockingImport => {
                // Run a blocking import loop that updates the gauge after each dataset
                while self.busy_active {
                    // Draw updated UI first
                    // Signal outer loop to render once
                    // Note: The main loop calls render continuously; we just process next item here
                    self.process_next_in_queue()?;
                    // Update progress for smoother gauge movement between items
                    self.busy_progress += 0.2;
                    if self.busy_progress >= 1.0 { self.busy_progress = 0.0; }
                    // Yield to allow the terminal to draw between steps
                    // (No true async here; small sleep to allow redraws)
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                Ok(None)
            }
            _ => Ok(None)
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
} 

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::dialog::csv_options_dialog::CsvImportOptions;

    #[test]
    fn test_load_dataframes_text_config() {
        // Create a test DataSource with Text import config
        let csv_options = CsvImportOptions::default();
        let text_config = crate::data_import_types::TextImportConfig {
            file_path: PathBuf::from("examples/pokemon_data.csv"),
            options: csv_options,
        };
        
        let data_source = DataSource::from_import_config(0, &DataImportConfig::Text(text_config));
        
        // Test that load_dataframes returns a Result
        let result = data_source.load_dataframes();
        assert!(result.is_ok(), "load_dataframes should return Ok for valid text config");
        
        let dataframes = result.unwrap();
        assert!(!dataframes.is_empty(), "Should load at least one dataset");
        
        // Verify the structure of LoadedDataset
        for (dataset_id, loaded_dataset) in dataframes {
            assert_eq!(loaded_dataset.dataset.id, dataset_id);
            assert!(!loaded_dataset.dataframe.get_column_names().is_empty());
        }
    }

    #[test]
    fn test_load_all_pending_datasets() {
        let mut dialog = DataManagementDialog::new();
        
        // Create a test DataSource with Text import config
        let csv_options = CsvImportOptions::default();
        let text_config = crate::data_import_types::TextImportConfig {
            file_path: PathBuf::from("examples/pokemon_data.csv"),
            options: csv_options,
        };
        
        // Add the data source
        dialog.add_data_source(DataImportConfig::Text(text_config));
        
        // Load all pending datasets
        let result = dialog.load_all_pending_datasets();
        assert!(result.is_ok(), "load_all_pending_datasets should return Ok");
        
        // Check that dataset status was updated to Imported
        for data_source in &dialog.data_sources {
            for dataset in &data_source.datasets {
                assert_eq!(dataset.status, DatasetStatus::Imported, "Dataset should be marked as imported");
                assert!(dataset.row_count > 0, "Dataset should have row count updated");
                assert!(dataset.column_count > 0, "Dataset should have column count updated");
            }
        }
    }

    #[test]
    fn test_dataset_alias_functionality() {
        let mut dialog = DataManagementDialog::new();
        
        // Create a test DataSource with Text import config
        let csv_options = CsvImportOptions::default();
        let text_config = crate::data_import_types::TextImportConfig {
            file_path: PathBuf::from("examples/pokemon_data.csv"),
            options: csv_options,
        };
        
        // Add the data source
        dialog.add_data_source(DataImportConfig::Text(text_config));
        
        // Get the dataset ID first to avoid borrowing conflicts
        let dataset_id = dialog.data_sources[0].datasets[0].id.clone();
        
        // Initially, alias should be None
        assert_eq!(dialog.data_sources[0].datasets[0].alias, None);
        
        // Update the alias
        dialog.update_dataset_alias(0, &dataset_id, Some("My Custom Alias".to_string()));
        
        // Verify the alias was updated
        assert_eq!(dialog.data_sources[0].datasets[0].alias, Some("My Custom Alias".to_string()));
        
        // Test clearing the alias
        dialog.update_dataset_alias(0, &dataset_id, None);
        assert_eq!(dialog.data_sources[0].datasets[0].alias, None);
    }
} 