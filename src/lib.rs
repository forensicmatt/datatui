#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_else_if)]

pub mod core;

// Re-export commonly used types
pub use core::{CsvImportOptions, DatasetId, ManagedDataset, ParquetImportOptions, SourceType};
