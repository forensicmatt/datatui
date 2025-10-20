//! DataImportTypes: Enum and structs for different data import configurations

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::dialog::csv_options_dialog::CsvImportOptions;
use crate::dialog::xlsx_options_dialog::XlsxImportOptions;
use crate::dialog::sqlite_options_dialog::SqliteImportOptions;
use crate::dialog::parquet_options_dialog::ParquetImportOptions;
use crate::dialog::json_options_dialog::JsonImportOptions;

/// Text file import configuration (CSV, TSV, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextImportConfig {
    pub file_path: PathBuf,
    pub options: CsvImportOptions,
    #[serde(default)]
    pub additional_paths: Vec<PathBuf>,
    #[serde(default)]
    pub merge: bool,
}

/// Excel file import configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcelImportConfig {
    pub file_path: PathBuf,
    pub options: XlsxImportOptions,
}

/// SQLite database import configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteImportConfig {
    pub file_path: PathBuf,
    pub options: SqliteImportOptions,
    pub table_name: Option<String>, // Specific table to import (None means use options to determine)
}

/// Parquet file import configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParquetImportConfig {
    pub file_path: PathBuf,
    pub options: ParquetImportOptions,
}

/// JSON file import configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonImportConfig {
    pub file_path: PathBuf,
    pub options: JsonImportOptions,
    #[serde(default)]
    pub additional_paths: Vec<PathBuf>,
    #[serde(default)]
    pub merge: bool,
}

/// Enum that can store different types of import configurations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataImportConfig {
    Text(TextImportConfig),
    Excel(ExcelImportConfig),
    Sqlite(SqliteImportConfig),
    Parquet(ParquetImportConfig),
    Json(JsonImportConfig),
}

impl DataImportConfig {
    /// Get the file path for any import configuration
    pub fn file_path(&self) -> &PathBuf {
        match self {
            DataImportConfig::Text(config) => &config.file_path,
            DataImportConfig::Excel(config) => &config.file_path,
            DataImportConfig::Sqlite(config) => &config.file_path,
            DataImportConfig::Parquet(config) => &config.file_path,
            DataImportConfig::Json(config) => &config.file_path,
        }
    }

    /// Get a display name for the import type
    pub fn import_type_name(&self) -> &'static str {
        match self {
            DataImportConfig::Text(_) => "Text File",
            DataImportConfig::Excel(_) => "Excel File",
            DataImportConfig::Sqlite(_) => "SQLite Database",
            DataImportConfig::Parquet(_) => "Parquet File",
            DataImportConfig::Json(_) => "JSON File",
        }
    }

    /// Create a text import configuration from a file path and options
    pub fn text(file_path: PathBuf, options: CsvImportOptions) -> Self {
        DataImportConfig::Text(TextImportConfig {
            file_path,
            options,
            additional_paths: Vec::new(),
            merge: false,
        })
    }

    /// Create an excel import configuration from a file path and options
    pub fn excel(file_path: PathBuf, options: XlsxImportOptions) -> Self {
        DataImportConfig::Excel(ExcelImportConfig {
            file_path,
            options,
        })
    }

    /// Create a sqlite import configuration from a file path and options
    pub fn sqlite(file_path: PathBuf, options: SqliteImportOptions) -> Self {
        DataImportConfig::Sqlite(SqliteImportConfig {
            file_path,
            options,
            table_name: None,
        })
    }

    /// Create a sqlite import configuration for a specific table
    pub fn sqlite_table(file_path: PathBuf, options: SqliteImportOptions, table_name: String) -> Self {
        DataImportConfig::Sqlite(SqliteImportConfig {
            file_path,
            options,
            table_name: Some(table_name),
        })
    }

    /// Create a parquet import configuration from a file path and options
    pub fn parquet(file_path: PathBuf, options: ParquetImportOptions) -> Self {
        DataImportConfig::Parquet(ParquetImportConfig {
            file_path,
            options,
        })
    }

    /// Create a json import configuration from a file path and options
    pub fn json(file_path: PathBuf, options: JsonImportOptions) -> Self {
        DataImportConfig::Json(JsonImportConfig {
            file_path,
            options,
            additional_paths: Vec::new(),
            merge: false,
        })
    }
} 