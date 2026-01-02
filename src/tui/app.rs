use crate::core::DatasetId;
use crate::services::search_service::{FindOptions, SearchMode};
use crate::services::{DataService, SearchService};
use crate::tui::components::{CellViewer, DataTable, FindAllResultsDialog, FindDialog};
use crate::tui::{Action, Component, Focusable, KeyBindings, Theme};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use std::path::Path;

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

    /// Last search parameters (for F3 repeat search)
    last_search: Option<(String, FindOptions, SearchMode)>,

    /// Keybindings configuration
    keybindings: KeyBindings,

    /// Current theme
    theme: Theme,

    /// Whether the app should quit
    should_quit: bool,
}

impl App {
    /// Create a new App instance
    pub fn new(workspace_path: impl AsRef<Path>) -> Result<Self> {
        let data_service = DataService::new(workspace_path)?;
        let keybindings = KeyBindings::default();
        let theme = Theme::default();

        Ok(Self {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            last_search: None,
            keybindings,
            theme,
            should_quit: false,
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

    /// Handle a key event
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        // Only handle key press events, ignore release/repeat
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // If find dialog is active, give it priority for character input (only when Pattern field is active)
        if let Some(dialog) = &mut self.find_dialog {
            // Only handle character/backspace/delete when Pattern field is active
            if dialog.active_field == crate::tui::components::find_dialog::FindDialogField::Pattern
            {
                // Handle character input for pattern field
                if let KeyCode::Char(c) = key.code {
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                    {
                        // Insert character into pattern
                        dialog
                            .search_pattern
                            .insert(dialog.search_pattern_cursor, c);
                        dialog.search_pattern_cursor += 1;
                        return Ok(());
                    }
                } else if key.code == KeyCode::Backspace {
                    // Handle backspace in pattern field
                    if dialog.search_pattern_cursor > 0 && !dialog.search_pattern.is_empty() {
                        dialog
                            .search_pattern
                            .remove(dialog.search_pattern_cursor - 1);
                        dialog.search_pattern_cursor -= 1;
                    }
                    return Ok(());
                } else if key.code == KeyCode::Delete {
                    // Handle delete in pattern field
                    if dialog.search_pattern_cursor < dialog.search_pattern.len() {
                        dialog.search_pattern.remove(dialog.search_pattern_cursor);
                    }
                    return Ok(());
                }
            }
        }

        // Translate key to action
        if let Some(action) = self.keybindings.get_action(&key) {
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

        // Route to find dialog if active
        if let Some(dialog) = &mut self.find_dialog {
            let keep_open = dialog.handle_action(action)?;
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
                self.cell_viewer.render(frame, chunks[0]);

                (chunks[1], Some(chunks[2]))
            } else {
                // Normal split: cell viewer (top), table (bottom)
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(viewer_height), Constraint::Min(0)])
                    .split(area);

                // Render the cell viewer (top)
                self.cell_viewer.render(frame, chunks[0]);

                (chunks[1], None)
            };

            // Render the data table
            table.render(frame, table_area);

            // Render find all results panel if active
            if let Some(dialog) = &mut self.find_all_results_dialog {
                if let Some(area) = results_area {
                    dialog.render(frame, area);
                }
            }
        } else {
            // TODO: Render welcome screen or file browser
        }

        // Render find dialog overlay on top if active (always overlay)
        if let Some(dialog) = &mut self.find_dialog {
            let dialog_area = Self::centered_rect(60, 50, area);
            dialog.render(frame, dialog_area);
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

        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
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
        let app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
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
        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
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
        let mut app = App {
            data_service,
            data_table: None,
            cell_viewer: CellViewer::new(),
            find_dialog: None,
            find_all_results_dialog: None,
            last_search: None,
            keybindings: KeyBindings::default(),
            theme: Theme::default(),
            should_quit: false,
        };

        let custom_bindings = KeyBindings::default();
        app.set_keybindings(custom_bindings);

        assert!(app.keybindings().get_keys_for_action(Action::Quit).len() > 0);
    }
}
