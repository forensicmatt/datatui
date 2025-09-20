//! Excel operations using the calamine library

use calamine::{open_workbook_auto, Reader, Data};
use std::path::Path;
use color_eyre::Result;
use serde::{Deserialize, Serialize};

/// Represents a worksheet in an Excel file
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorksheetInfo {
    pub name: String,
    pub load: bool,
    pub row_count: usize,
    pub column_count: usize,
    pub non_empty_cells: usize,
}

impl WorksheetInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            load: true, // Default to loading
            row_count: 0,
            column_count: 0,
            non_empty_cells: 0,
        }
    }
}

/// Excel operations struct for reading Excel files
pub struct ExcelOperations;

impl ExcelOperations {
    /// Read an Excel file and extract worksheet information
    pub fn read_worksheet_info(file_path: &Path) -> Result<Vec<WorksheetInfo>> {
        let mut workbook = open_workbook_auto(file_path)?;
        let mut worksheets = Vec::new();

        // Get all sheet names
        let sheet_names = workbook.sheet_names().to_owned();
        
        for sheet_name in sheet_names {
            let mut worksheet_info = WorksheetInfo::new(sheet_name.clone());
            
            // Try to get the worksheet range
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                let (rows, cols) = range.get_size();
                worksheet_info.row_count = rows;
                worksheet_info.column_count = cols;
                
                // Count non-empty cells
                let non_empty_cells = range.used_cells().count();
                worksheet_info.non_empty_cells = non_empty_cells;
            }
            
            worksheets.push(worksheet_info);
        }

        Ok(worksheets)
    }

    /// Get a preview of worksheet data (first few rows)
    pub fn get_worksheet_preview(file_path: &Path, sheet_name: &str, max_rows: usize) -> Result<Vec<Vec<String>>> {
        let mut workbook = open_workbook_auto(file_path)?;
        let mut preview = Vec::new();

        if let Ok(range) = workbook.worksheet_range(sheet_name) {
            for (row_idx, row) in range.rows().enumerate() {
                if row_idx >= max_rows {
                    break;
                }
                
                let mut preview_row = Vec::new();
                for cell in row {
                    let cell_value = match cell {
                        Data::Empty => String::new(),
                        Data::String(s) => s.clone(),
                        Data::Int(i) => i.to_string(),
                        Data::Float(f) => f.to_string(),
                        Data::Bool(b) => b.to_string(),
                        Data::DateTime(d) => d.as_f64().to_string(),
                        Data::DateTimeIso(s) => s.clone(),
                        Data::DurationIso(s) => s.clone(),
                        Data::Error(e) => format!("ERROR: {e:?}"),
                    };
                    preview_row.push(cell_value);
                }
                preview.push(preview_row);
            }
        }

        Ok(preview)
    }

    /// Check if a file is a valid Excel file
    pub fn is_valid_excel_file(file_path: &Path) -> bool {
        if let Some(extension) = file_path.extension() && let Some(ext_str) = extension.to_str() {
            return matches!(ext_str.to_lowercase().as_str(), "xlsx" | "xlsm" | "xlsb" | "xls");
        }
        false
    }
} 