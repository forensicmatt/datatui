#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_else_if)]

pub mod core;
pub mod services;

// Re-export commonly used types
pub use core::{CsvImportOptions, DatasetId, ManagedDataset, ParquetImportOptions, SourceType};
pub use services::DataService;
