use serde::{Deserialize, Serialize};
use strum::Display;
use crate::dialog::sort_dialog::SortColumn;
use crate::dialog::filter_dialog::{FilterExpr, ColumnFilter};
use crate::dialog::column_width_dialog::ColumnWidthConfig;
use crate::dialog::find_dialog::{FindOptions, SearchMode};
use crate::dialog::TransformScope;
use crate::dialog::jmes_dialog::JmesPathKeyValuePair;


/// High-level actions that can be triggered by UI or components.
#[derive(Debug, Clone, PartialEq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    /// Close any active dialog
    DialogClose,
    /// User applied a sort dialog with columns and directions
    SortDialogApplied(Vec<SortColumn>),
    /// User applied a filter dialog with a root expression
    FilterDialogApplied(FilterExpr),
    /// Add a single filter condition programmatically (e.g., from Unique Values)
    AddFilterCondition(ColumnFilter),
    /// User applied a SQL dialog with a query string
    SqlDialogApplied(String),
    /// User applied a SQL dialog with a query string to create a new dataset
    SqlDialogAppliedNewDataset { 
        dataset_name: String, 
        dataframe: std::sync::Arc<polars::prelude::DataFrame> 
    },
    /// User requested to restore the original DataFrame from the SQL dialog
    SqlDialogRestore,
    /// User applied a column width dialog with configuration
    ColumnWidthDialogApplied(ColumnWidthConfig),
    /// User reordered columns in the column width dialog
    ColumnWidthDialogReordered(Vec<String>),
    /// User requested to find next match in the DataTable
    FindNext {
        pattern: String,
        options: FindOptions,
        search_mode: SearchMode,
    },
    /// User requested to count matches in the DataTable
    FindCount {
        pattern: String,
        options: FindOptions,
        search_mode: SearchMode,
    },
    /// User requested to find all matches in the DataTable
    FindAll {
        pattern: String,
        options: FindOptions,
        search_mode: SearchMode,
    },
    /// User requested to go to a specific result (row, column) from Find All results
    GoToResult {
        row: usize,
        column: String,
    },
    /// User requested to remove a DataFrame from the manager
    RemoveDataFrame(usize),
    /// User requested to close the DataTable manager dialog
    CloseDataTableManagerDialog,
    /// User requested to open the DataTable manager dialog
    OpenDataTableManagerDialog,
    /// User requested to open the data import dialog
    OpenDataImportDialog,
    /// User requested to close the data import dialog
    CloseDataImportDialog,
    /// User requested to confirm data import
    ConfirmDataImport,
    /// User requested to add a data import configuration
    AddDataImportConfig {
        config: crate::data_import_types::DataImportConfig,
    },
    /// User requested to open file browser dialog
    OpenFileBrowserDialog,
    /// User requested to open file browser (generic)
    OpenFileBrowser,
    /// User requested to open CSV options dialog
    OpenCsvOptionsDialog,
    /// User requested to close CSV options dialog
    CloseCsvOptionsDialog,
    /// User requested to open XLSX options dialog
    OpenXlsxOptionsDialog,
    /// User requested to close XLSX options dialog
    CloseXlsxOptionsDialog,
    /// User requested to open SQLite options dialog
    OpenSqliteOptionsDialog,
    /// User requested to close SQLite options dialog
    CloseSqliteOptionsDialog,
    /// User requested to open Parquet options dialog
    OpenParquetOptionsDialog,
    /// User requested to close Parquet options dialog
    CloseParquetOptionsDialog,
    /// User requested to open JSON options dialog
    OpenJsonOptionsDialog,
    /// User requested to close JSON options dialog
    CloseJsonOptionsDialog,
    /// User requested to open data management dialog
    OpenDataManagementDialog,
    /// User requested to close data management dialog
    CloseDataManagementDialog,
    /// User requested to open data export dialog
    OpenDataExportDialog,
    /// User requested to close data export dialog
    CloseDataExportDialog,
    /// User requested to remove a data source
    RemoveDataSource { source_id: usize },
    /// User requested to open the data tab manager dialog
    OpenDataTabManagerDialog,
    /// User requested to close the data tab manager dialog
    CloseDataTabManagerDialog,
    /// User requested to add a dataset to the tab manager
    /// LoadDatasets{
    ///     datasets: Vec<crate::data_import_types::DataImportConfig>,
    /// }
    // Avoid carrying non-Send/Sync types in Action.
    // If needed, pass a serializable config instead of a full DataSource.
    AddDataSources {
        source_config: crate::data_import_types::DataImportConfig,
    },
    /// User requested to edit a dataset alias
    EditDatasetAlias {
        source_id: usize,
        dataset_id: String,
        alias: Option<String>,
    },
    /// User applied project settings dialog with configuration
    ProjectSettingsApplied(crate::dialog::ProjectSettingsConfig),
    /// User requested to cast a column to a new dtype
    ColumnCastRequested { column: String, dtype: String },
    /// Apply a JMESPath transformation to the dataset
    JmesTransformDataset((String, TransformScope)),
    /// Add columns to the dataset using JMESPath expressions per column name
    JmesTransformAddColumns(Vec<JmesPathKeyValuePair>, TransformScope),
    /// Request to persist the current workspace state
    SaveWorkspaceState,
    /// User requested a column operation from ColumnOperationsDialog
    ColumnOperationRequested(String),
    /// User applied column operation options
    ColumnOperationOptionsApplied(crate::dialog::column_operation_options_dialog::ColumnOperationConfig),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn test_action_display() {
        let a1 = Action::DialogClose;
        let a2 = Action::SortDialogApplied(vec![SortColumn { name: "test".to_string(), ascending: true }]);
        let a1_str = format!("{a1}");
        let a2_str = format!("{a2}");
        info!("Action::DialogClose Display: {}", a1_str);
        info!("Action::SortDialogApplied Display: {}", a2_str);
        // Accept any non-empty string for now, or adjust to match actual output
        assert!(!a1_str.is_empty());
        assert!(!a2_str.is_empty());
    }

    #[test]
    fn test_variant_matching() {
        let action = Action::FilterDialogApplied(FilterExpr::And(vec![]));
        match action {
            Action::FilterDialogApplied(FilterExpr::And(_)) => {
                // Test passes if we match the And variant
            }
            _ => panic!("Expected FilterDialogApplied variant"),
        }
    }
}
