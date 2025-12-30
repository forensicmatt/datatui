use crate::core::DatasetId;
use crate::services::DataService;
use crate::tui::{Action, Component, DataTable, Focusable, KeyBindings, Theme};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::Frame;
use std::path::Path;

/// Application state
///
/// Manages the TUI components, event routing, and application lifecycle.
pub struct App {
    /// Data service for backend operations
    data_service: DataService,

    /// Current active component (DataTable for now)
    data_table: Option<DataTable>,

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
            _ => {}
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
        let area = frame.size();

        if let Some(table) = &mut self.data_table {
            table.render(frame, area);
        } else {
            // TODO: Render welcome screen or file browser
        }
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
            keybindings: KeyBindings::default(),
            theme: Theme::default(),
            should_quit: false,
        };

        let custom_bindings = KeyBindings::default();
        app.set_keybindings(custom_bindings);

        assert!(app.keybindings().get_keys_for_action(Action::Quit).len() > 0);
    }
}
