use crate::core::DatasetId;
use crate::services::search_service::{FindOptions, SearchMode};
use crate::services::{DataService, LlmService, SearchService};
use crate::tui::components::{
    CellViewer, ColumnOperationsDialog, ColumnWidthDialog, CommandBarDialog,
    DataFrameDetailsDialog, DataTable, EmbeddingProgressDialog, ErrorDialog, FindAllResultsDialog,
    FindDialog, LlmManagementDialog, MapViewerDialog, SortDialog, SqlDialog,
};
use crate::tui::{Action, Command, Component, Focusable, KeyBindings, Theme};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

/// Application state
///
/// Manages the TUI components, event routing, and application lifecycle.
pub struct App {
    /// Data service for backend operations
    data_service: DataService,

    /// Current active component (DataTable for now)
    data_table: Option<DataTable>,

    /// Cell viewer component
    cell_viewer: CellViewer,

    /// Find dialog (when active)
    find_dialog: Option<FindDialog>,

    /// Find All results dialog (when active)
    find_all_results_dialog: Option<FindAllResultsDialog>,

    /// Column width dialog (when active)
    column_width_dialog: Option<ColumnWidthDialog>,

    /// Sort dialog(when active)
    sort_dialog: Option<SortDialog>,

    /// DataFrame details dialog (when active)
    dataframe_details_dialog: Option<DataFrameDetailsDialog>,

    /// Map viewer dialog (when active)
    map_viewer_dialog: Option<MapViewerDialog>,

    /// Command bar dialog (when active)
    command_bar_dialog: Option<CommandBarDialog>,

    /// Error dialog (when active)
    error_dialog: Option<ErrorDialog>,

    /// SQL dialog (when active)
    sql_dialog: Option<SqlDialog>,

    /// LLM management dialog (when active)
    llm_management_dialog: Option<LlmManagementDialog>,

    /// Column operations dialog (when active)
    column_operations_dialog: Option<ColumnOperationsDialog>,

    /// LLM service
    llm_service: LlmService,

    /// Last search parameters (for F3 repeat search)
    last_search: Option<(String, FindOptions, SearchMode)>,

    /// Keybindings configuration
    keybindings: KeyBindings,

    /// Current theme
    theme: Theme,

    /// Whether the app should quit
    should_quit: bool,

    /// Channel receiver for embedding job completion.
    ///
    /// Success payload: `(col_name, dataset_id, rowid_embeddings, hide_col)` — written to
    ///   DuckDB on the main thread via `data_service.add_embedding_column()`.
    /// Failure payload: `Err(message)` — displayed in ErrorDialog.
    embedding_result_rx_typed: Option<
        Receiver<Result<(String, crate::core::DatasetId, Vec<(i64, Vec<f32>)>, bool), String>>,
    >,

    /// Progress dialog shown while an embedding job is running.
    embedding_progress_dialog: Option<EmbeddingProgressDialog>,

    /// Receiver end of the progress channel — polled in update().
    embedding_progress_rx: Option<Receiver<(usize, usize)>>,
}

impl App {
    /// Create a new App instance
    pub fn new(workspace_path: impl AsRef<Path>) -> Result<Self> {
        let data_service = DataService::new(workspace_path)?;
        let keybindings = KeyBindings::default();
        let theme = Theme::default();

        // Initialize LLM service
        let config_dir = if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "datatui")
        {
            proj_dirs.config_dir().to_path_buf()
        } else {
            // Fallback to current directory if we can't get project dirs
            std::env::current_dir()?
        };
        let llm_service = LlmService::new(config_dir)?;

        Ok(Self {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            column_width_dialog: None,
            sort_dialog: None,
            dataframe_details_dialog: None,
            map_viewer_dialog: None,
            command_bar_dialog: None,
            error_dialog: None,
            sql_dialog: None,
            llm_management_dialog: None,
            column_operations_dialog: None,
            llm_service,
            last_search: None,
            keybindings,
            theme,
            should_quit: false,
            embedding_result_rx_typed: None,
            embedding_progress_dialog: None,
            embedding_progress_rx: None,
        })
    }

    /// Load a dataset into the data table
    pub fn load_dataset(&mut self, dataset_id: &DatasetId) -> Result<()> {
        let dataset = self.data_service.get_dataset(dataset_id)?;
        let mut table = DataTable::new(dataset)?;
        table.set_focused(true);
        self.data_table = Some(table);
        Ok(())
    }

    /// Handle a dialog result from FindDialog
    fn handle_dialog_result(
        &mut self,
        result: crate::tui::components::find_dialog::DialogResult,
    ) -> Result<()> {
        use crate::tui::components::find_dialog::DialogResult;

        match result {
            DialogResult::ExecuteFindNext {
                pattern,
                options,
                mode,
                ..
            } => {
                // Get current cursor position from table
                if let Some(table) = &mut self.data_table {
                    let dataset = table.dataset();
                    let (start_row, start_col) = table.get_cursor_position();

                    match SearchService::find_next(
                        dataset, &pattern, &options, &mode, start_row, start_col,
                    ) {
                        Ok(Some(result)) => {
                            table.goto_cell(result.row, &result.column)?;
                        }
                        Ok(None) => {
                            if let Some(d) = &mut self.find_dialog {
                                d.set_error("No matches found".to_string());
                            }
                        }
                        Err(e) => {
                            if let Some(d) = &mut self.find_dialog {
                                d.set_error(format!("Search error: {}", e));
                            }
                        }
                    }
                }
            }

            DialogResult::ExecuteCount {
                pattern,
                options,
                mode,
            } => {
                if let Some(table) = &self.data_table {
                    let dataset = table.dataset();

                    match SearchService::count_matches(dataset, &pattern, &options, &mode) {
                        Ok(count) => {
                            if let Some(d) = &mut self.find_dialog {
                                d.set_count(count);
                            }
                        }
                        Err(e) => {
                            if let Some(d) = &mut self.find_dialog {
                                d.set_error(format!("Search error: {}", e));
                            }
                        }
                    }
                }
            }

            DialogResult::ExecuteFindAll {
                pattern,
                options,
                mode,
            } => {
                if let Some(table) = &mut self.data_table {
                    let dataset = table.dataset();

                    let start = std::time::Instant::now();
                    match SearchService::find_all(dataset, &pattern, &options, &mode, 30) {
                        Ok(results) => {
                            let elapsed = start.elapsed();

                            if results.is_empty() {
                                if let Some(d) = &mut self.find_dialog {
                                    d.set_error("No matches found".to_string());
                                }
                            } else {
                                let first_result =
                                    results.first().map(|r| (r.row, r.column.clone()));

                                // Check if dialog already exists
                                if let Some(dialog) = &mut self.find_all_results_dialog {
                                    dialog.add_tab_with_time(pattern.clone(), results, elapsed);
                                } else {
                                    let mut dialog =
                                        FindAllResultsDialog::new(results, pattern.clone());
                                    dialog.set_elapsed_time(elapsed);
                                    dialog.set_focused(true);
                                    self.find_all_results_dialog = Some(dialog);
                                    table.set_focused(false);
                                }

                                if let Some((row, col)) = first_result {
                                    table.goto_cell(row, &col)?;
                                }

                                // Close find dialog
                                self.find_dialog = None;
                            }
                        }
                        Err(e) => {
                            if let Some(d) = &mut self.find_dialog {
                                d.set_error(format!("Search error: {}", e));
                            }
                        }
                    }
                }
            }

            DialogResult::Close => {
                // Just close, already handled
            }
        }

        Ok(())
    }

    /// Handle a column width dialog result
    fn handle_column_dialog_result(
        &mut self,
        result: crate::tui::components::column_width_dialog::DialogResult,
    ) -> Result<()> {
        use crate::tui::components::column_width_dialog::DialogResult as ColDialogResult;

        match result {
            ColDialogResult::ApplyConfig(config) => {
                if let Some(table) = &mut self.data_table {
                    table.dataset_mut().set_column_config(config)?;
                    table.refresh_layout()?;
                }
            }
            ColDialogResult::ReorderColumns(order) => {
                if let Some(table) = &mut self.data_table {
                    table.dataset_mut().reorder_columns(order)?;
                    table.refresh_layout()?;
                }
            }
            ColDialogResult::Close => {
                // Just close, already handled
            }
        }

        Ok(())
    }

    /// Handle a column operations dialog result
    fn handle_column_operations_result(
        &mut self,
        result: crate::tui::components::column_operations_dialog::DialogResult,
    ) -> Result<()> {
        use crate::tui::components::column_operation_options_dialog::ColumnOperationKind;
        use crate::tui::components::column_operations_dialog::DialogResult as ColOpResult;

        match result {
            ColOpResult::Applied(config) => match config.operation {
                ColumnOperationKind::GenerateEmbeddings => {
                    self.dispatch_generate_embeddings(config)?;
                }
                op => {
                    tracing::info!(
                        "Column operation {:?} on '{}' — not yet implemented",
                        op,
                        config.source_column
                    );
                }
            },
            ColOpResult::Cancelled => {}
        }
        Ok(())
    }

    /// Dispatch a GenerateEmbeddings job to a background thread.
    ///
    /// The thread:
    /// 1. Calls `LlmService::generate_embeddings` (blocking HTTP)
    /// 2. Writes embeddings back via `DataService::add_embedding_column`
    ///
    /// Errors are logged; the UI is refreshed on completion.
    fn dispatch_generate_embeddings(
        &mut self,
        config: crate::tui::components::column_operation_options_dialog::ColumnOperationConfig,
    ) -> Result<()> {
        use crate::tui::components::column_operation_options_dialog::OperationOptions;

        let (model_name, num_dimensions) = match &config.options {
            OperationOptions::GenerateEmbeddings {
                model_name,
                num_dimensions,
            } => (model_name.clone(), *num_dimensions),
            _ => return Ok(()),
        };

        let dataset_id = match self.data_table.as_ref() {
            Some(table) => table.dataset().id.clone(),
            None => {
                tracing::warn!("No active dataset — cannot generate embeddings");
                return Ok(());
            }
        };

        // Read texts on the main thread (fast DuckDB read, stays on main thread)
        let rows = match self
            .data_service
            .fetch_column_texts(&dataset_id, &config.source_column)
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(
                    "Failed to read source column '{}': {}",
                    config.source_column,
                    e
                );
                return Ok(());
            }
        };

        if rows.is_empty() {
            tracing::warn!(
                "Source column '{}' has no non-null values — skipping embedding",
                config.source_column
            );
            return Ok(());
        }

        let total = rows.len();
        tracing::info!(
            "Starting embedding job: {} rows, model='{}', provider={:?}",
            total,
            model_name,
            config.provider
        );

        let llm_service = self.llm_service.clone();
        let provider = config.provider;
        let new_col = if config.new_column_name.is_empty() {
            format!("{}_embedding", config.source_column)
        } else {
            config.new_column_name.clone()
        };
        let hide_col = config.hide_new_column;

        // Completion channel.
        // Success: (col_name, dataset_id, paired rowid→embedding, hide_col)
        // Failure: Err(human-readable message)
        type DonePayload =
            Result<(String, crate::core::DatasetId, Vec<(i64, Vec<f32>)>, bool), String>;
        let (done_tx, done_rx) = mpsc::channel::<DonePayload>();
        self.embedding_result_rx_typed = Some(done_rx);

        // Progress channel — per-batch ticks forwarded to the progress dialog
        let (prog_tx, prog_rx) = mpsc::channel::<(usize, usize)>();
        self.embedding_progress_rx = Some(prog_rx);

        // Show the progress overlay immediately
        self.embedding_progress_dialog = Some(EmbeddingProgressDialog::new(
            new_col.clone(),
            config.source_column.clone(),
            total,
        ));

        // Clone dataset_id so we can move it into the thread
        let dataset_id_for_thread = dataset_id.clone();

        std::thread::spawn(move || {
            let (rowids, texts): (Vec<i64>, Vec<String>) = rows.into_iter().unzip();

            // Progress callback sends (done, total) ticks to the main thread
            let prog_cb: crate::services::ProgressCallback = Box::new(move |done, tot| {
                let _ = prog_tx.send((done, tot));
            });

            // ── HTTP only — no DuckDB here ──────────────────────────────────
            // Opening the session file from a second thread would trigger
            // DuckDB's exclusive-write lock and fail with "file in use".
            let embeddings = match llm_service.generate_embeddings(
                texts,
                Some(provider),
                &model_name,
                Some(num_dimensions),
                None,
                Some(prog_cb),
            ) {
                Ok(e) => e,
                Err(e) => {
                    let _ = done_tx.send(Err(format!("Embedding generation failed: {}", e)));
                    return;
                }
            };

            // Pair rowids with their embedding vectors and send to main thread
            let rowid_embeddings: Vec<(i64, Vec<f32>)> =
                rowids.into_iter().zip(embeddings).collect();

            let _ = done_tx.send(Ok((
                new_col,
                dataset_id_for_thread,
                rowid_embeddings,
                hide_col,
            )));
        });

        Ok(())
    }

    fn handle_sort_dialog_result(
        &mut self,
        result: crate::tui::components::sort_dialog::DialogResult,
    ) -> Result<()> {
        use crate::tui::components::sort_dialog::DialogResult as SortDialogResult;

        match result {
            SortDialogResult::ApplySort(sort_columns) => {
                if let Some(table) = &mut self.data_table {
                    if let Err(e) = table.dataset_mut().set_sort_order(sort_columns.clone()) {
                        tracing::error!("Failed to set sort order: {}", e);
                        tracing::error!("Sort columns: {:?}", sort_columns);
                        return Err(e);
                    }
                    if let Err(e) = table.refresh_layout() {
                        tracing::error!("Failed to refresh layout after sort: {}", e);
                        return Err(e);
                    }
                    tracing::debug!("Sort applied successfully: {:?}", sort_columns);
                }
            }
            SortDialogResult::Close => {
                // Just close, already handled
            }
        }

        Ok(())
    }

    /// Handle a command bar dialog result
    fn handle_command_bar_result(
        &mut self,
        result: crate::tui::components::command_bar_dialog::DialogResult,
    ) -> Result<()> {
        use crate::tui::command::{Command, CommandContext};
        use crate::tui::components::command_bar_dialog::DialogResult;

        match result {
            DialogResult::ExecuteCommand(cmd_str) => {
                // Parse the command
                let command = match Command::parse(&cmd_str) {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        // Show error in popup dialog
                        self.error_dialog = Some(ErrorDialog::with_title(
                            format!("Command: {}\n\n{}", cmd_str, err),
                            "Invalid Command".to_string(),
                        ));
                        return Ok(());
                    }
                };

                // Handle help command specially - show help dialog
                if matches!(command, Command::Help) {
                    let help_text = "\
Available Commands:

  :quit, :q              Quit the application
  :find <pattern>        Search for a pattern
  :sort <col> [desc]...  Sort by columns (comma separated)
  :dialog sort           Open sort dialog
  :dialog find           Open find dialog
  :dialog llm            Open LLM management dialog
  :help                  Show this help
  :columns set <c> [w]   Show only specific columns
  :columns hide <c>...   Toggle column visibility
  :columns width <c> <w> Set column width (or auto)
  :goto row <N> [col]    Navigate to row N, optionally column col

Examples:
  :quit                  Exit application
  :find test             Search for 'test'
  :sort name             Sort by 'name' asc
  :sort age desc, name   Sort by 'age' desc, then 'name' asc
  :dialog sort           Open interactive sort dialog
  :dialog find           Open interactive find dialog
  :goto row 10           Go to row 10
  :goto row 5 2          Go to row 5, column 2

Press Esc or Enter to close this dialog.";

                    self.error_dialog = Some(ErrorDialog::with_title(
                        help_text.to_string(),
                        "Command Help".to_string(),
                    ));
                    return Ok(());
                }

                // Create execution context
                let mut ctx = CommandContext {
                    should_quit: &mut self.should_quit,
                    find_dialog: &mut self.find_dialog,
                    sort_dialog: &mut self.sort_dialog,
                    data_table: &mut self.data_table,
                };

                // Execute the command
                if let Err(err) = command.execute(&mut ctx) {
                    // Show error in popup dialog
                    self.error_dialog = Some(ErrorDialog::with_title(
                        format!("Command: {}\n\n{}", cmd_str, err),
                        "Command Error".to_string(),
                    ));
                    return Ok(());
                }

                // Handle commands that require actions
                if let Some(action) = command.requires_action() {
                    self.handle_action(action)?;
                }

                // Close command bar after execution
                self.command_bar_dialog = None;
            }
            DialogResult::Close => {
                self.command_bar_dialog = None;
            }
        }

        Ok(())
    }

    /// Handle a SQL dialog result
    fn handle_sql_dialog_result(
        &mut self,
        result: crate::tui::components::SqlDialogResult,
    ) -> Result<()> {
        use crate::tui::components::SqlDialogResult;
        match result {
            SqlDialogResult::ExecuteQuery(sql) => {
                // Execute the SQL query
                if let Some(table) = &mut self.data_table {
                    if let Err(e) = table.dataset_mut().execute_sql(&sql) {
                        // Show error in dialog
                        if let Some(d) = &mut self.sql_dialog {
                            d.set_error(format!("SQL Error: {}", e));
                        }
                    } else {
                        // Success - close dialog and refresh
                        self.sql_dialog = None;
                        table.reload_schema()?;
                    }
                }
            }
            SqlDialogResult::Close => {
                self.sql_dialog = None;
            }
        }
        Ok(())
    }

    /// Update suggestions for the command bar based on current input
    fn update_command_suggestions(&mut self) {
        if let Some(dialog) = &mut self.command_bar_dialog {
            let input = dialog.command.clone();
            let columns = if let Some(table) = &self.data_table {
                table.get_all_columns()
            } else {
                vec![]
            };

            let suggestions = Command::get_suggestions(&input, &columns);
            dialog.set_suggestions(suggestions);
        }
    }

    /// Handle key events
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        use crate::tui::KeyEventResult;
        use tracing::debug;

        debug!("Handling key event: {:?}", key);

        // Only handle key press events, ignore release/repeat
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // Delegate to active component and handle results
        // We check specific dialogs that produce results via take_result()

        // ErrorDialog (handles differently)
        if let Some(dialog) = &mut self.error_dialog {
            match dialog.handle_key_event(key)? {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
            // If ignored, we might want to let it fall through to global bindings?
            // Previous code returned Ok(())?
            // "if let Some(action) = ... { ... } return Ok(())"
            // So it returned Ok(()) regardless of action?
            // "return Ok(())" means it consumes the key even if ignored?
            // If ErrorDialog is modal, it should probably BLOCK other input.
            // But if it ignores Esc, we verify if global handles it.
            // But if we return Ok(()), global bindings are NOT checked (because handle_key_event returns).
            // So we should implicit fallthrough IF we want global bindings to work.
            // BUT ErrorDialog needs to handle Esc to close!
            // Global keybinding for Esc -> Action::Cancel.
            // So we MUST fall through.
            // So we do `match result ... Ignored => {}` then continue.
        }

        // ... (other dialogs) ...

        // FindDialog
        if self.find_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.find_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };

            if let Some(result) = dr {
                self.handle_dialog_result(result)?;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // CommandBarDialog
        else if self.command_bar_dialog.is_some() {
            // Capture old text to check for changes
            let old_text = self.command_bar_dialog.as_ref().map(|d| d.command.clone());

            let (kr, dr) = if let Some(d) = &mut self.command_bar_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };

            if let Some(result) = dr {
                self.handle_command_bar_result(result)?;
            }

            match kr {
                KeyEventResult::Consumed => {
                    // Check if text changed
                    let new_text = self.command_bar_dialog.as_ref().map(|d| d.command.clone());
                    if old_text != new_text {
                        self.update_command_suggestions();
                    }
                    return Ok(());
                }
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;

                    // Check if text changed (e.g. history navigation)
                    let new_text = self.command_bar_dialog.as_ref().map(|d| d.command.clone());
                    if old_text != new_text {
                        self.update_command_suggestions();
                    }
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // ColumnWidthDialog
        else if self.column_width_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.column_width_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };

            if let Some(result) = dr {
                self.handle_column_dialog_result(result)?;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // SortDialog
        else if self.sort_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.sort_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };

            if let Some(result) = dr {
                self.handle_sort_dialog_result(result)?;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // SqlDialog
        else if self.sql_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.sql_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };

            if let Some(result) = dr {
                self.handle_sql_dialog_result(result)?;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // LlmManagementDialog
        else if self.llm_management_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.llm_management_dialog {
                // First try raw key event for 'd' shortcut
                if d.handle_raw_key_event(key)? == KeyEventResult::Consumed {
                    (KeyEventResult::Consumed, None)
                } else {
                    (
                        d.handle_key_event(key)?,
                        if d.closed { Some(()) } else { None },
                    )
                }
            } else {
                (KeyEventResult::Ignored, None)
            };

            if dr.is_some() {
                self.llm_management_dialog = None;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // ColumnOperationsDialog
        else if self.column_operations_dialog.is_some() {
            let (kr, dr) = if let Some(d) = &mut self.column_operations_dialog {
                (d.handle_key_event(key)?, d.take_result())
            } else {
                (KeyEventResult::Ignored, None)
            };
            if let Some(result) = dr {
                self.handle_column_operations_result(result)?;
            }
            if self
                .column_operations_dialog
                .as_ref()
                .map(|d| d.closed)
                .unwrap_or(false)
            {
                self.column_operations_dialog = None;
            }
            match kr {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        } else if let Some(dialog) = &mut self.find_all_results_dialog {
            match dialog.handle_key_event(key)? {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // MapViewerDialog
        else if let Some(dialog) = &mut self.map_viewer_dialog {
            match dialog.handle_key_event(key)? {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }
        // DataFrameDetailsDialog
        else if let Some(dialog) = &mut self.dataframe_details_dialog {
            match dialog.handle_key_event(key)? {
                KeyEventResult::Consumed => return Ok(()),
                KeyEventResult::Action(a) => {
                    self.handle_action(a)?;
                    return Ok(());
                }
                KeyEventResult::Ignored => {}
            }
        }

        // If not handled by component, check global/scoped keybindings matches
        let scope = if self.error_dialog.is_some() {
            "ErrorDialog"
        } else if self.find_dialog.is_some() {
            "FindDialog"
        } else if self.command_bar_dialog.is_some() {
            "CommandBarDialog"
        } else if self.column_width_dialog.is_some() {
            "ColumnWidthDialog"
        } else if self.sort_dialog.is_some() {
            "SortDialog"
        } else if self.sql_dialog.is_some() {
            "SqlDialog"
        } else if self.llm_management_dialog.is_some() {
            "LlmManagementDialog"
        } else if let Some(col_ops) = &self.column_operations_dialog {
            // If the sub-dialog (options form) is open, use its scope
            if col_ops.has_sub_dialog() {
                "ColumnOperationOptionsDialog"
            } else {
                "ColumnOperationsDialog"
            }
        } else if let Some(ref details) = self.dataframe_details_dialog {
            if details.has_map_viewer() {
                "MapViewerDialog"
            } else {
                "DataFrameDetailsDialog"
            }
        } else if self.map_viewer_dialog.is_some() {
            "MapViewerDialog"
        } else if let Some(dialog) = &self.find_all_results_dialog {
            if dialog.is_focused() {
                "FindAllResults"
            } else {
                "DataTable"
            }
        } else {
            // Let's stick to "Global" or "DataTable" based on context.
            // If no dialog is open, we are likely in DataTable.
            "DataTable"
        };

        if let Some(action) = self.keybindings.get_action(scope, &key) {
            self.handle_action(action)?;
        }

        Ok(())
    }

    /// Handle an action
    fn handle_action(&mut self, action: Action) -> Result<()> {
        // App-level actions
        match action {
            Action::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            Action::NextTab => {
                // If FindAllResultsDialog is active, toggle focus between it and DataTable
                if let Some(dialog) = &mut self.find_all_results_dialog {
                    if let Some(table) = &mut self.data_table {
                        // Toggle focus
                        let dialog_focused = dialog.is_focused();
                        dialog.set_focused(!dialog_focused);
                        table.set_focused(dialog_focused);
                    }
                    return Ok(());
                }
            }
            Action::Find => {
                // Open find dialog, populating from last search if available
                let mut dialog = FindDialog::new();

                // Restore previous search parameters if they exist
                if let Some((pattern, options, mode)) = &self.last_search {
                    dialog.search_pattern = pattern.clone();
                    dialog.search_pattern_cursor = pattern.len();
                    dialog.options = options.clone();
                    dialog.search_mode = mode.clone();
                }

                self.find_dialog = Some(dialog);
                return Ok(());
            }

            Action::OpenColumnWidthDialog => {
                if let Some(table) = &mut self.data_table {
                    let columns = table.get_all_columns();
                    let config = table.dataset().get_column_config().clone();
                    let widths = table.get_calculated_widths()?;

                    let mut dialog = ColumnWidthDialog::new(columns);
                    dialog.set_config(config);
                    dialog.set_calculated_widths(widths);

                    self.column_width_dialog = Some(dialog);
                }
                return Ok(());
            }

            Action::Sort => {
                if let Some(table) = &mut self.data_table {
                    let columns = table.get_all_columns();
                    let (_row, col_idx) = table.get_cursor_position();

                    let mut dialog = SortDialog::new(columns.clone());

                    // Set current column hint for better UX
                    if col_idx < columns.len() {
                        dialog.set_current_column(Some(columns[col_idx].clone()));
                    }

                    if let Ok(sort_order) = table.dataset().get_sort_order() {
                        dialog.set_sort_columns(sort_order);
                    }
                    // }

                    self.sort_dialog = Some(dialog);
                }
                return Ok(());
            }

            Action::OpenDetailsDialog => {
                if let Some(table) = &mut self.data_table {
                    let dataset = table.dataset().clone();
                    let columns = table.get_all_columns();
                    let (_row, col_idx) = table.get_cursor_position();

                    let mut dialog = DataFrameDetailsDialog::new(dataset, columns, col_idx);
                    dialog.set_focused(true);
                    table.set_focused(false);
                    self.dataframe_details_dialog = Some(dialog);
                }
                return Ok(());
            }

            Action::OpenMapViewer => {
                if let Some(table) = &mut self.data_table {
                    // Create map viewer from current row
                    if let Ok(pairs) = table.get_current_row_as_pairs() {
                        let row_idx = table.get_cursor_position().0;
                        let title = format!("Row {}", row_idx + 1); // 1-based index for display
                        let mut dialog = MapViewerDialog::from_pairs(title, pairs);
                        dialog.set_focused(true);
                        table.set_focused(false);
                        self.map_viewer_dialog = Some(dialog);
                    }
                }
                return Ok(());
            }

            Action::OpenCommandBar => {
                self.command_bar_dialog = Some(CommandBarDialog::new());
                return Ok(());
            }

            Action::OpenSqlDialog => {
                if let Some(table) = &mut self.data_table {
                    let columns = table.get_all_columns();
                    let current_sql = table.dataset().get_current_sql();

                    let mut dialog = SqlDialog::new(columns);
                    dialog.set_query_text(current_sql);
                    self.sql_dialog = Some(dialog);
                }
                return Ok(());
            }

            Action::OpenLlmManagementDialog => {
                let dialog = LlmManagementDialog::new(self.llm_service.clone());
                self.llm_management_dialog = Some(dialog);
                return Ok(());
            }

            Action::OpenColumnOperationsDialog => {
                if let Some(table) = &self.data_table {
                    let columns = table.get_all_columns();
                    let (_, col_idx) = table.get_cursor_position();
                    let dialog = ColumnOperationsDialog::new(columns, col_idx);
                    self.column_operations_dialog = Some(dialog);
                }
                return Ok(());
            }

            Action::Cancel => {
                // Close find all results dialog if active
                if self.find_all_results_dialog.is_some() {
                    // Restore focus to table
                    if let Some(table) = &mut self.data_table {
                        table.set_focused(true);
                    }
                    self.find_all_results_dialog = None;
                    return Ok(());
                }

                // Close find dialog if active
                if self.find_dialog.is_some() {
                    self.find_dialog = None;
                    return Ok(());
                }
            }
            Action::Confirm => {
                // Execute search if dialog is active
                if let Some(dialog) = &self.find_dialog {
                    if dialog.search_pattern.is_empty() {
                        // Show error
                        if let Some(d) = &mut self.find_dialog {
                            d.set_error("Search pattern cannot be empty".to_string());
                        }
                        return Ok(());
                    }

                    // Get search parameters
                    let (pattern, options, mode) = dialog.get_search_params();
                    let action_selected = dialog.action_selected;

                    // Store as last search
                    self.last_search = Some((pattern.clone(), options.clone(), mode.clone()));

                    // Execute based on selected action
                    match action_selected {
                        crate::tui::components::find_dialog::FindActionSelected::FindNext => {
                            // Execute find next
                            if let Some(table) = &mut self.data_table {
                                let dataset = table.dataset();
                                let (start_row, start_col) = table.get_cursor_position();

                                match SearchService::find_next(
                                    dataset, &pattern, &options, &mode, start_row, start_col,
                                ) {
                                    Ok(Some(result)) => {
                                        // Navigate to result
                                        table.goto_cell(result.row, &result.column)?;
                                        // Close dialog
                                        self.find_dialog = None;
                                    }
                                    Ok(None) => {
                                        // No results found
                                        if let Some(d) = &mut self.find_dialog {
                                            d.set_error("No matches found".to_string());
                                        }
                                    }
                                    Err(e) => {
                                        // Search error (e.g., invalid regex)
                                        if let Some(d) = &mut self.find_dialog {
                                            d.set_error(format!("Search error: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        crate::tui::components::find_dialog::FindActionSelected::Count => {
                            // Execute count
                            if let Some(table) = &self.data_table {
                                let dataset = table.dataset();

                                match SearchService::count_matches(
                                    dataset, &pattern, &options, &mode,
                                ) {
                                    Ok(count) => {
                                        // Show count in dialog
                                        if let Some(d) = &mut self.find_dialog {
                                            d.set_count(count);
                                        }
                                    }
                                    Err(e) => {
                                        // Search error
                                        if let Some(d) = &mut self.find_dialog {
                                            d.set_error(format!("Search error: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        crate::tui::components::find_dialog::FindActionSelected::FindAll => {
                            // Execute find all
                            if let Some(table) = &mut self.data_table {
                                let dataset = table.dataset();

                                // Track elapsed time
                                let start = std::time::Instant::now();
                                match SearchService::find_all(
                                    dataset, &pattern, &options, &mode, 30,
                                ) {
                                    Ok(results) => {
                                        let elapsed = start.elapsed();

                                        if results.is_empty() {
                                            if let Some(d) = &mut self.find_dialog {
                                                d.set_error("No matches found".to_string());
                                            }
                                        } else {
                                            // Clone first result data before moving results
                                            let first_result =
                                                results.first().map(|r| (r.row, r.column.clone()));

                                            // Check if dialog already exists
                                            if let Some(dialog) = &mut self.find_all_results_dialog
                                            {
                                                // Add new tab to existing dialog
                                                dialog.add_tab_with_time(
                                                    pattern.clone(),
                                                    results,
                                                    elapsed,
                                                );
                                            } else {
                                                // Create new dialog with first tab
                                                let mut dialog = FindAllResultsDialog::new(
                                                    results,
                                                    pattern.clone(),
                                                );
                                                // Set elapsed time
                                                dialog.set_elapsed_time(elapsed);
                                                // Give focus to the dialog initially
                                                dialog.set_focused(true);
                                                self.find_all_results_dialog = Some(dialog);

                                                // Remove focus from table since dialog now has focus
                                                table.set_focused(false);
                                            }

                                            // Jump to first result
                                            if let Some((row, col)) = first_result {
                                                table.goto_cell(row, &col)?;
                                            }

                                            // Remove focus from table since dialog now has focus
                                            table.set_focused(false);

                                            // Close find dialog
                                            self.find_dialog = None;
                                        }
                                    }
                                    Err(e) => {
                                        if let Some(d) = &mut self.find_dialog {
                                            d.set_error(format!("Search error: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    return Ok(());
                }

                // Jump to selected result if find all results dialog is active
                if let Some(dialog) = &self.find_all_results_dialog {
                    if let Some(result) = dialog.get_selected() {
                        if let Some(table) = &mut self.data_table {
                            table.goto_cell(result.row, &result.column)?;
                        }
                    }
                    // Keep dialog open so user can see and navigate to other results
                    return Ok(());
                }
            }
            _ => {}
        }

        // Route to column width dialog if active (MODAL - highest priority)
        if let Some(dialog) = &mut self.column_width_dialog {
            let keep_open = dialog.handle_action(action)?;

            // Check if dialog has a pending result to process
            if let Some(result) = dialog.take_result() {
                self.handle_column_dialog_result(result)?;
            }

            if !keep_open {
                self.column_width_dialog = None;
            }
            return Ok(());
        }

        // Route to sort dialog if active (MODAL)
        if let Some(dialog) = &mut self.sort_dialog {
            let keep_open = dialog.handle_action(action)?;

            // Check if dialog has a pending result to process
            if let Some(result) = dialog.take_result() {
                self.handle_sort_dialog_result(result)?;
            }

            if !keep_open {
                self.sort_dialog = None;
            }
            return Ok(());
        }

        // Route to DataFrame details dialog if active (MODAL)
        if let Some(dialog) = &mut self.dataframe_details_dialog {
            let keep_open = dialog.handle_action(action)?;
            if !keep_open {
                // Restore focus to table
                if let Some(table) = &mut self.data_table {
                    table.set_focused(true);
                }
                self.dataframe_details_dialog = None;
            }
            return Ok(());
        }

        // Route to Map Viewer dialog if active (MODAL)
        if let Some(dialog) = &mut self.map_viewer_dialog {
            let keep_open = dialog.handle_action(action)?;
            if !keep_open {
                // Restore focus to table
                if let Some(table) = &mut self.data_table {
                    table.set_focused(true);
                }
                self.map_viewer_dialog = None;
            }
            return Ok(());
        }

        // Route to error dialog if active (highest priority - modal)
        if let Some(dialog) = &mut self.error_dialog {
            let keep_open = dialog.handle_action(action)?;
            if !keep_open {
                self.error_dialog = None;
            }
            return Ok(());
        }

        // Route to command bar dialog if active
        if self.command_bar_dialog.is_some() {
            // Take ownership to avoid borrow issues
            let mut dialog = self.command_bar_dialog.take().unwrap();
            let keep_open = dialog.handle_action(action)?;

            // Check if dialog has a pending result to process
            if let Some(result) = dialog.take_result() {
                // Process the result (this might create a new dialog with error)
                self.handle_command_bar_result(result)?;
                // If a new dialog was created, it's already in self.command_bar_dialog
                // If not, and we should close, leave it as None
            } else if keep_open {
                // Put the dialog back
                self.command_bar_dialog = Some(dialog);
            }
            // If !keep_open and no result, dialog stays None (closed)

            return Ok(());
        }

        // Route to SQL dialog if active (MODAL)
        if let Some(dialog) = &mut self.sql_dialog {
            let keep_open = dialog.handle_action(action)?;

            // Check if dialog has a pending result to process
            if let Some(result) = dialog.take_result() {
                self.handle_sql_dialog_result(result)?;
            }

            if !keep_open {
                self.sql_dialog = None;
            }
            return Ok(());
        }

        // Route to LLM management dialog if active (MODAL)
        if let Some(dialog) = &mut self.llm_management_dialog {
            let keep_open = dialog.handle_action(action)?;
            if !keep_open || dialog.closed {
                self.llm_management_dialog = None;
            }
            return Ok(());
        }

        // Route to find dialog if active
        if let Some(dialog) = &mut self.find_dialog {
            let keep_open = dialog.handle_action(action)?;

            // Check if dialog has a pending result to process
            if let Some(result) = dialog.take_result() {
                self.handle_dialog_result(result)?;
            }

            if !keep_open {
                self.find_dialog = None;
            }
            return Ok(());
        }

        // Route to find all results dialog if active and focused
        if let Some(dialog) = &mut self.find_all_results_dialog {
            if dialog.is_focused() {
                let keep_open = dialog.handle_action(action)?;
                if !keep_open {
                    // Restore focus to table when dialog closes
                    if let Some(table) = &mut self.data_table {
                        table.set_focused(true);
                    }
                    self.find_all_results_dialog = None;
                }
                return Ok(());
            }
        }

        // Route to column operations dialog if active (MODAL)
        if self.column_operations_dialog.is_some() {
            let (closed, result) = if let Some(d) = &mut self.column_operations_dialog {
                let _handled = d.handle_action(action)?;
                let result = d.take_result();
                (d.closed, result)
            } else {
                (true, None)
            };

            if let Some(result) = result {
                self.handle_column_operations_result(result)?;
            }

            if closed {
                self.column_operations_dialog = None;
            }
            return Ok(());
        }

        // Route to focused component
        if let Some(table) = &mut self.data_table {
            if table.is_focused() {
                table.handle_action(action)?;
            }
        }

        Ok(())
    }

    /// Check if the app should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Update app state (called on every tick)
    pub fn update(&mut self) -> Result<()> {
        if let Some(table) = &mut self.data_table {
            table.update()?;
        }

        // Drain all pending progress ticks (non-blocking)
        // Use a local bool to avoid borrow conflict when updating the dialog
        let mut latest_progress: Option<(usize, usize)> = None;
        if let Some(rx) = &self.embedding_progress_rx {
            loop {
                match rx.try_recv() {
                    Ok(tick) => {
                        latest_progress = Some(tick);
                    }
                    Err(_) => break,
                }
            }
        }
        if let Some((done, total)) = latest_progress {
            if let Some(dlg) = &mut self.embedding_progress_dialog {
                dlg.set_progress(done, total);
            }
        }

        // Poll embedding job completion channel (non-blocking)
        let embedding_done = if let Some(rx) = &self.embedding_result_rx_typed {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Embedding job terminated unexpectedly.".to_string()))
                }
            }
        } else {
            None
        };

        if let Some(result) = embedding_done {
            // Job finished — clear all job state
            self.embedding_result_rx_typed = None;
            self.embedding_progress_rx = None;
            self.embedding_progress_dialog = None;

            match result {
                Ok((col_name, dataset_id, rowid_embeddings, hide_col)) => {
                    tracing::info!("Embedding generation finished, writing to DuckDB...");

                    // Do the DB write on the main thread
                    match self.data_service.add_embedding_column(
                        &dataset_id,
                        &col_name,
                        rowid_embeddings,
                        hide_col,
                    ) {
                        Ok(_) => {
                            tracing::info!(
                                "Embedding column '{}' written successfully, syncing UI state...",
                                col_name
                            );
                            // Reload the DataTable schema and sync the dataset instance
                            if let Some(table) = &mut self.data_table {
                                // Re-fetch the dataset from DataService to get updated config (column_config, etc)
                                if let Ok(updated_ds) = self.data_service.get_dataset(&dataset_id) {
                                    *table.dataset_mut() = updated_ds;
                                }

                                if let Err(e) = table.reload_schema() {
                                    tracing::error!(
                                        "Failed to reload schema after embedding: {}",
                                        e
                                    );
                                    self.error_dialog = Some(ErrorDialog::new(format!(
                                        "Embeddings written to '{}' but failed to refresh view: {}",
                                        col_name, e
                                    )));
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to write embedding column: {}", e);
                            self.error_dialog = Some(ErrorDialog::new(format!(
                                "Failed to write embedding column to database: {}",
                                e
                            )));
                        }
                    }
                }
                Err(msg) => {
                    tracing::error!("Embedding job failed: {}", msg);
                    self.error_dialog = Some(ErrorDialog::new(msg));
                }
            }
        }

        Ok(())
    }

    /// Render the app
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        if let Some(table) = &mut self.data_table {
            // Update cell viewer with current selection first
            if let Ok(cell_info) = table.get_current_cell_info() {
                self.cell_viewer.set_cell_info(Some(cell_info));
            }

            // Calculate the height needed for the cell viewer
            let viewer_height = self.cell_viewer.calculate_height(area.width);

            // Determine layout based on whether find all results panel is active
            let (table_area, results_area) = if self.find_all_results_dialog.is_some() {
                // Split screen: cell viewer (top), table (middle), results panel (bottom)
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(viewer_height), // Cell viewer
                        Constraint::Percentage(70),        // DataTable (80%)
                        Constraint::Percentage(30),        // Results panel (20%)
                    ])
                    .split(area);

                // Render the cell viewer (top)
                self.cell_viewer.render(frame, chunks[0], &self.theme);

                (chunks[1], Some(chunks[2]))
            } else {
                // Normal split: cell viewer (top), table (bottom)
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(viewer_height), Constraint::Min(0)])
                    .split(area);

                // Render the cell viewer (top)
                self.cell_viewer.render(frame, chunks[0], &self.theme);

                (chunks[1], None)
            };

            // Render the data table
            table.render(frame, table_area, &self.theme);

            // Render find all results panel if active
            if let Some(dialog) = &mut self.find_all_results_dialog {
                if let Some(area) = results_area {
                    dialog.render(frame, area, &self.theme);
                }
            }
        } else {
            // TODO: Render welcome screen or file browser
        }

        // Render find dialog overlay on top if active (always overlay)
        if let Some(dialog) = &mut self.find_dialog {
            let dialog_area = Self::centered_rect(60, 50, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render column width dialog overlay if active
        if let Some(dialog) = &mut self.column_width_dialog {
            let dialog_area = Self::centered_rect(70, 70, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render DataFrame details dialog overlay if active
        if let Some(dialog) = &mut self.dataframe_details_dialog {
            let dialog_area = Self::centered_rect(85, 80, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render Map Viewer dialog overlay if active
        if let Some(dialog) = &mut self.map_viewer_dialog {
            let dialog_area = Self::centered_rect(60, 60, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render sort dialog overlay if active
        if let Some(dialog) = &mut self.sort_dialog {
            let dialog_area = Self::centered_rect(60, 60, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        if let Some(dialog) = &mut self.llm_management_dialog {
            let dialog_area = Self::centered_rect(70, 70, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render column operations dialog overlay if active
        if let Some(dialog) = &mut self.column_operations_dialog {
            let dialog_area = Self::centered_rect(70, 80, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render SQL dialog overlay if active
        if let Some(dialog) = &mut self.sql_dialog {
            let dialog_area = Self::centered_rect(90, 90, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render command bar dialog if active (always at bottom, vim-style)
        if let Some(dialog) = &mut self.command_bar_dialog {
            let bar_area = Rect {
                x: area.x,
                y: area.height.saturating_sub(3),
                width: area.width,
                height: 3,
            };
            dialog.render(frame, bar_area, &self.theme);
        }

        // Render error dialog if active (centered overlay, highest priority)
        if let Some(dialog) = &mut self.error_dialog {
            let dialog_area = Self::centered_rect(50, 30, area);
            dialog.render(frame, dialog_area, &self.theme);
        }

        // Render embedding progress dialog overlay if active
        if let Some(dialog) = &mut self.embedding_progress_dialog {
            // A compact bar: 60% wide, 9 rows tall, centred
            let dialog_area = Self::centered_rect_absolute(area, 60, 9);
            dialog.render(frame, dialog_area, &self.theme);
        }
    }

    /// Helper to create centered rectangle
    fn centered_rect(percent_w: u16, percent_h: u16, area: Rect) -> Rect {
        let width = (area.width * percent_w) / 100;
        let height = (area.height * percent_h) / 100;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Helper to create a centred rect with percentage width and fixed row height.
    fn centered_rect_absolute(area: Rect, percent_w: u16, rows: u16) -> Rect {
        let width = (area.width * percent_w) / 100;
        let height = rows.min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Get reference to cell viewer configuration
    pub fn cell_viewer_config(&self) -> &crate::tui::components::ViewerConfig {
        self.cell_viewer.config()
    }

    /// Set cell viewer configuration
    pub fn set_cell_viewer_config(&mut self, config: crate::tui::components::ViewerConfig) {
        self.cell_viewer.set_config(config);
    }

    /// Get reference to data service
    pub fn data_service(&self) -> &DataService {
        &self.data_service
    }

    /// Get reference to theme
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Set theme
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Get keybindings
    pub fn keybindings(&self) -> &KeyBindings {
        &self.keybindings
    }

    /// Set keybindings
    pub fn set_keybindings(&mut self, keybindings: KeyBindings) {
        self.keybindings = keybindings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CsvImportOptions;
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_app() -> (App, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        // Create isolated global DB for this test
        let global_db = workspace_path.join("global_test.duckdb");

        // Create test CSV
        let csv_path = workspace_path.join("test.csv");
        let mut file = std::fs::File::create(&csv_path).unwrap();
        writeln!(file, "id,name,value").unwrap();
        writeln!(file, "1,Alice,100").unwrap();
        writeln!(file, "2,Bob,200").unwrap();
        drop(file);

        // Create app with isolated DataService
        let data_service = DataService::new_impl(workspace_path, Some(global_db)).unwrap();
        let keybindings = KeyBindings::default();
        let theme = Theme::default();

        let llm_service = LlmService::new(workspace_path.to_path_buf()).unwrap();

        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            column_width_dialog: None,
            sort_dialog: None,
            dataframe_details_dialog: None,
            map_viewer_dialog: None,
            sql_dialog: None,
            llm_management_dialog: None,
            llm_service,
            command_bar_dialog: None,
            error_dialog: None,
            last_search: None,
            keybindings,
            theme,
            should_quit: false,
        };

        // Import dataset
        let options = CsvImportOptions::default();
        let dataset_id = app.data_service().import_csv(csv_path, options).unwrap();
        app.load_dataset(&dataset_id).unwrap();

        (app, temp_dir)
    }

    #[test]
    fn test_app_creation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let global_db = workspace_path.join("global_test.duckdb");

        let data_service = DataService::new_impl(workspace_path, Some(global_db)).unwrap();
        let llm_service = LlmService::new(workspace_path.to_path_buf()).unwrap();
        let app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            column_width_dialog: None,
            sort_dialog: None,
            dataframe_details_dialog: None,
            map_viewer_dialog: None,
            sql_dialog: None,
            llm_management_dialog: None,
            llm_service,
            command_bar_dialog: None,
            error_dialog: None,
            last_search: None,
            keybindings: KeyBindings::default(),
            theme: Theme::default(),
            should_quit: false,
        };

        assert!(!app.should_quit());
        assert!(app.data_table.is_none());
    }

    #[test]
    fn test_load_dataset() {
        let (app, _temp_dir) = create_test_app();

        assert!(app.data_table.is_some());
        let table = app.data_table.as_ref().unwrap();
        assert!(table.is_focused());
    }

    #[test]
    fn test_quit_action() {
        let (mut app, _temp_dir) = create_test_app();

        assert!(!app.should_quit());

        // Send quit action
        let quit_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        app.handle_key_event(quit_key).unwrap();

        assert!(app.should_quit());
    }

    #[test]
    fn test_navigation_action() {
        let (mut app, _temp_dir) = create_test_app();

        // Send down arrow
        let down_key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key_event(down_key).unwrap();

        // Table should have moved cursor (we can't directly test cursor position without exposing it)
        // But we can verify no error occurred
        assert!(!app.should_quit());
    }

    #[test]
    fn test_theme_management() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let global_db = workspace_path.join("global_test.duckdb");

        let data_service = DataService::new_impl(workspace_path, Some(global_db)).unwrap();
        let llm_service = LlmService::new(workspace_path.to_path_buf()).unwrap();
        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            column_width_dialog: None,
            sort_dialog: None,
            dataframe_details_dialog: None,
            map_viewer_dialog: None,
            sql_dialog: None,
            llm_management_dialog: None,
            llm_service,
            command_bar_dialog: None,
            error_dialog: None,
            last_search: None,
            keybindings: KeyBindings::default(),
            theme: Theme::default(),
            should_quit: false,
        };

        let light_theme = Theme::light();
        app.set_theme(light_theme);

        assert_eq!(app.theme().name, "Light");
    }

    #[test]
    fn test_keybindings_management() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();
        let global_db = workspace_path.join("global_test.duckdb");

        let data_service = DataService::new_impl(workspace_path, Some(global_db)).unwrap();
        let llm_service = LlmService::new(workspace_path.to_path_buf()).unwrap();
        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            column_width_dialog: None,
            sort_dialog: None,
            dataframe_details_dialog: None,
            map_viewer_dialog: None,
            sql_dialog: None,
            llm_management_dialog: None,
            llm_service,
            command_bar_dialog: None,
            error_dialog: None,
            last_search: None,
            keybindings: KeyBindings::default(),
            theme: Theme::default(),
            should_quit: false,
        };

        let custom_bindings = KeyBindings::default();
        app.set_keybindings(custom_bindings);

        assert!(
            app.keybindings()
                .get_keys_for_action("Global", Action::Quit)
                .len()
                > 0
        );
    }
}
