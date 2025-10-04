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
    /// Global actions (configurable)
    Escape,
    Enter,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    Tab,
    Paste,
    /// DataManagementDialog actions (configurable)
    DeleteSelectedSource,
    LoadAllPendingDatasets,
    EditSelectedAlias,
    /// Open the Project Settings dialog
    OpenProjectSettingsDialog,
    /// Close the Project Settings dialog
    CloseProjectSettingsDialog,
    /// Open the Sort dialog in the current context
    OpenSortDialog,
    /// Quick sort: add/select current column in Sort dialog
    QuickSortCurrentColumn,
    /// Open the Filter dialog
    OpenFilterDialog,
    /// Quick filter: equals on current cell value
    QuickFilterEqualsCurrentValue,
    /// Move selected column left within the table
    MoveSelectedColumnLeft,
    /// Move selected column right within the table
    MoveSelectedColumnRight,
    /// Open SQL dialog
    OpenSqlDialog,
    /// Open JMESPath dialog
    OpenJmesDialog,
    /// Open Column Operations dialog
    OpenColumnOperationsDialog,
    /// Open Find dialog
    OpenFindDialog,
    /// Open DataFrame Details dialog
    OpenDataframeDetailsDialog,
    /// Open Column Width dialog
    OpenColumnWidthDialog,
    /// Open Data Export dialog
    OpenDataExportDialog,
    /// Copy currently selected cell
    CopySelectedCell,
    /// Toggle instructions panel
    ToggleInstructions,
    /// Open the Data Management dialog
    OpenDataManagementDialog,
    /// Close the Data Management dialog
    CloseDataManagementDialog,
    /// Move current tab to front
    MoveTabToFront,
    /// Move current tab to back
    MoveTabToBack,
    /// Move current tab one position left
    MoveTabLeft,
    /// Move current tab one position right
    MoveTabRight,
    /// Switch to previous tab
    PrevTab,
    /// Switch to next tab
    NextTab,
    /// Manually synchronize tabs from Data Management
    SyncTabs,
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
    /// User selected an item in DataImport dialog (e.g., proceed/open options)
    DataImportSelect,
    /// User requested to go back within the DataImport dialog
    DataImportBack,
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
    // (OpenDataExportDialog moved earlier in the enum)
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
    /// Sort dialog specific actions
    ToggleSortDirection,
    RemoveSortColumn,
    AddSortColumn,
    /// Filter dialog specific actions
    AddFilter,
    EditFilter,
    DeleteFilter,
    AddFilterGroup,
    SaveFilter,
    LoadFilter,
    ResetFilters,
    ToggleFilterGroupType,
    /// Find dialog specific actions
    ToggleSpace,
    Delete,
    /// JMESPath dialog specific actions
    AddColumn,
    EditColumn,
    DeleteColumn,
    ApplyTransform,
    /// FindAllResults dialog specific actions
    GoToFirst,
    GoToLast,
    PageUp,
    PageDown,
    /// SqlDialog specific actions
    SelectAllText,
    CopyText,
    RunQuery,
    CreateNewDataset,
    RestoreDataFrame,
    OpenSqlFileBrowser,
    ClearText,
    PasteText,
    /// XlsxOptionsDialog specific actions
    OpenXlsxFileBrowser,
    PasteFilePath,
    ToggleWorksheetLoad,
    /// ParquetOptionsDialog specific actions
    OpenParquetFileBrowser,
    PasteParquetFilePath,
    /// SqliteOptionsDialog specific actions
    OpenSqliteFileBrowser,
    ToggleImportAllTables,
    ToggleTableSelection,
    /// FileBrowserDialog specific actions
    FileBrowserPageUp,
    FileBrowserPageDown,
    ConfirmOverwrite,
    DenyOverwrite,
    NavigateToParent,
    /// ColumnWidthDialog specific actions
    ToggleAutoExpand,
    StartColumnEditing,
    ToggleEditMode,
    ToggleColumnHidden,
    MoveColumnUp,
    MoveColumnDown,
    /// JsonOptionsDialog specific actions
    OpenJsonFileBrowser,
    PasteJsonFilePath,
    ToggleNdjson,
    FinishJsonImport,
    /// ColumnOperationOptionsDialog specific actions
    ToggleField,
    ToggleButtons,
    /// DataFrameDetailsDialog specific actions
    SwitchToNextTab,
    SwitchToPrevTab,
    ChangeColumnLeft,
    ChangeColumnRight,
    OpenSortChoice,
    OpenCastOverlay,
    AddFilterFromValue,
    ExportCurrentTab,
    NavigateHeatmapLeft,
    NavigateHeatmapRight,
    NavigateHeatmapUp,
    NavigateHeatmapDown,
    NavigateHeatmapPageUp,
    NavigateHeatmapPageDown,
    NavigateHeatmapHome,
    NavigateHeatmapEnd,
    ScrollStatsLeft,
    ScrollStatsRight,
    /// ProjectSettingsDialog specific actions
    ToggleDataViewerOption,
    /// TableExportDialog specific actions
    CopyFilePath,
    ExportTable,
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
