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
use tracing::info;
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
                let name = sqlite_config.file_path.file_name()
                    .unwrap_or_else(|| OsStr::new("Unknown"))
                    .to_string_lossy()
                    .to_string();
                let datasets = if sqlite_config.options.import_all_tables {
                    vec![Dataset {
                        id: Uuid::new_v4().to_string(),
                        name: "All Tables".to_string(),
                        alias: None,
                        row_count: 0,
                        column_count: 0,
                        status: DatasetStatus::Pending,
                        error_message: None,
                    }]
                } else {
                    sqlite_config.options.selected_tables.iter().map(|table| Dataset {
                        id: Uuid::new_v4().to_string(),
                        name: table.clone(),
                        alias: None,
                        row_count: 0,
                        column_count: 0,
                        status: DatasetStatus::Pending,
                        error_message: None,
                    }).collect()
                };
                (name, sqlite_config.file_path.to_string_lossy().to_string(), "SQLite Database".to_string(), datasets)
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

    /// Load all datasets from this data source into DataFrames
    pub fn load_dataframes(&self) -> Result<HashMap<String, LoadedDataset>> {
        let mut result = HashMap::new();

        for dataset in &self.datasets {
            // only attempt to load datasets marked Pending or with empty cache entry
            let df_arc = self.load_dataset(dataset)?;
            let loaded_dataset = LoadedDataset {
                data_source: self.clone(),
                dataset: dataset.clone(),
                dataframe: df_arc,
            };
            result.insert(dataset.id.clone(), loaded_dataset);
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
            DataImportConfig::Sqlite(_) => {
                return Err(color_eyre::eyre::eyre!("SQLite import not yet supported"));
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
        }
    }

    /// Add a new data source from a DataImportConfig
    pub fn add_data_source(&mut self, config: DataImportConfig) {
        let id = self.data_sources.len();
        let data_source: DataSource = DataSource::from_import_config(id, &config);
        self.data_sources.push(data_source);
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
        
        // After processing, if any errors occurred, display them to the user
        if !load_errors.is_empty() {
            let message = load_errors.join("\n");
            self.message_dialog = Some(MessageDialog::with_title(message, "Dataset Load Errors"));
        }
        
        Ok(())
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the background for the popup
        Clear.render(area, buf);

        let instructions = "Up/Down: Navigate  Ctrl+e: Edit Alias  Ctrl+d: Delete Source  Ctrl+a: Add Data Source  Ctrl+i: Toggle Instructions / View Error  Esc: Close";
        let layout = split_dialog_area(area, self.show_instructions, Some(instructions));
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
            self.render_instructions(instructions, instructions_area, buf);
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
            self.render_instructions(instructions, instructions_area, buf);
            alias_dialog.render(alias_dialog_area, buf);
        } else {
            self.render_datasets_table(inner_area, buf);
            self.render_instructions(instructions, instructions_area, buf);
        }

        // Overlay message dialog if active
        if let Some(ref msg) = self.message_dialog {
            msg.render(area, buf);
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
        info!("DataManagementDialog handle_key_event: {:?}", key);

        if let Some(nav_action) = self.config.action_for_key(crate::config::Mode::Navigation, key) {
            info!("DataManagementDialog action_for_key<Navigation>: {:?}", nav_action);
            match nav_action {
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
                    info!("DataManagementDialog unhandled Navigation action: {:?} for key: {:?}", nav_action, key);
                }
            }
        }
        if let Some(dm_action) = self.config.action_for_key(crate::config::Mode::DataManagement, key) {
            info!("DataManagementDialog action_for_key<DataManagement>: {:?}", dm_action);
            match dm_action {
                Action::CloseDataManagementDialog => {
                    return Ok(Some(Action::CloseDataManagementDialog))
                }
                Action::DeleteSelectedSource => {
                    if let Some((source_id, _source, _dataset)) = self.selected_dataset() {
                        self.remove_data_source(source_id);
                        return Ok(Some(Action::RemoveDataSource { source_id }));
                    }
                    return Ok(None);
                }
                Action::OpenDataImportDialog => {
                    self.data_import_dialog = Some(DataImportDialog::new());
                    return Ok(None);
                }
                Action::LoadAllPendingDatasets => {
                    self.load_all_pending_datasets()?;
                    return Ok(None);
                }
                Action::EditSelectedAlias => {
                    if let Some((source_id, _source, dataset)) = self.selected_dataset() {
                        self.alias_edit_dialog = Some(AliasEditDialog::new(
                            source_id,
                            dataset.id.clone(),
                            dataset.name.clone(),
                            dataset.alias.clone(),
                        ));
                    }
                    return Ok(None);
                }
                _ => {
                    info!("DataManagementDialog unhandled DataManagement action: {:?} for key: {:?}", dm_action, key);
                }
            }
        }

        info!("BLAH: {:?}", key);

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
                        // Auto-load all pending datasets after adding data source
                        if let Err(e) = self.load_all_pending_datasets() { return Ok(Some(Action::Error(format!("Failed to auto-load datasets: {e}")))); }
                        return Ok(None);
                    }
                    Action::ConfirmDataImport => {
                        // Handle the confirmation action and auto-load data
                        self.data_import_dialog = None;
                        // Auto-load all pending datasets after import
                        if let Err(e) = self.load_all_pending_datasets() { return Ok(Some(Action::Error(format!("Failed to auto-load datasets: {e}")))); }
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        Ok(None)
    }

    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
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