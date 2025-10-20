//! SqliteOptionsDialog: Dialog for configuring SQLite import options

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tracing::error;
use crate::components::dialog_layout::split_dialog_area;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode};
use tui_textarea::TextArea;

/// SQLite import options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqliteImportOptions {
    pub selected_tables: Vec<String>,
    pub import_all_tables: bool,
}

impl Default for SqliteImportOptions {
    fn default() -> Self {
        Self {
            selected_tables: Vec::new(),
            import_all_tables: true,
        }
    }
}

/// SqliteOptionsDialog: Dialog for configuring SQLite import options
#[derive(Debug, Serialize, Deserialize)]
pub struct SqliteOptionsDialog {
    pub file_path: String,
    pub sqlite_options: SqliteImportOptions,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub finish_button_selected: bool, // Whether the finish button is selected
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    pub available_tables: Vec<String>,
    pub selected_table_index: usize,
    pub show_instructions: bool, // Whether to show instructions area
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    pub config: Config,
}

impl SqliteOptionsDialog {
    /// Create a new SqliteOptionsDialog
    pub fn new(file_path: String, sqlite_options: SqliteImportOptions) -> Self {
        let mut file_path_input = TextArea::default();
        file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        file_path_input.insert_str(&file_path);
        
        let mut dialog = Self {
            file_path,
            sqlite_options,
            file_path_focused: true,
            browse_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            available_tables: Vec::new(),
            selected_table_index: 0,
            show_instructions: true,
            file_path_input,
            file_browser: None,
            config: Config::default(),
        };
        
        // Load available tables if file path is provided
        let _ = dialog.load_available_tables();
        
        dialog
    }

    /// Get the current file path
    pub fn get_file_path(&self) -> &str {
        &self.file_path
    }

    /// Set the file path
    pub fn set_file_path(&mut self, path: String) {
        self.file_path = path.clone();
        self.file_path_input = TextArea::from(vec![path]);
        self.file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        // Load available tables when file path changes
        let _ = self.load_available_tables();
    }

    /// Get the current SQLite options
    pub fn get_sqlite_options(&self) -> &SqliteImportOptions {
        &self.sqlite_options
    }

    /// Get the current SQLite options as mutable
    pub fn get_sqlite_options_mut(&mut self) -> &mut SqliteImportOptions {
        &mut self.sqlite_options
    }

        /// Create a DataImportConfig from the current dialog state
    pub fn create_import_config(&self) -> crate::data_import_types::DataImportConfig {
        use crate::data_import_types::DataImportConfig;

        let file_path = PathBuf::from(&self.file_path);
        DataImportConfig::sqlite(file_path, self.sqlite_options.clone())
    }

    /// Set available tables
    pub fn set_available_tables(&mut self, tables: Vec<String>) {
        self.available_tables = tables.clone();
        // When import_all_tables is true, select all available tables by default
        if self.sqlite_options.import_all_tables {
            self.sqlite_options.selected_tables = tables;
        }
    }

    /// Load available tables from SQLite database
    pub fn load_available_tables(&mut self) -> Result<()> {
        if self.file_path.is_empty() || !std::path::Path::new(&self.file_path).exists() {
            self.available_tables.clear();
            self.sqlite_options.selected_tables.clear();
            return Ok(());
        }

        // Try to use polars to query the SQLite database for table names
        match self.query_sqlite_tables() {
            Ok(tables) => {
                self.set_available_tables(tables);
            }
            Err(error) => {
                error!("Failed to query SQLite tables: {}", error);
                // If polars fails, fall back to empty tables list
                self.available_tables.clear();
                self.sqlite_options.selected_tables.clear();
            }
        }
        Ok(())
    }

    /// Query SQLite database for table names using rusqlite
    fn query_sqlite_tables(&self) -> Result<Vec<String>> {
        use rusqlite::Connection;
        
        let mut tables = Vec::new();
        
        // Open connection to SQLite database
        let conn = Connection::open(&self.file_path)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to open SQLite database: {}", e))?;
        
        // Query for all user tables (excluding system tables)
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ).map_err(|e| color_eyre::eyre::eyre!("Failed to prepare SQL statement: {}", e))?;
        
        let table_rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| color_eyre::eyre::eyre!("Failed to execute query: {}", e))?;
        
        for table_result in table_rows {
            let table_name = table_result
                .map_err(|e| color_eyre::eyre::eyre!("Failed to read table name: {}", e))?;
            tables.push(table_name);
        }
        
        Ok(tables)
    }

    /// Update the file path
    fn update_file_path(&mut self, path: String) {
        self.file_path = path;
        // Load available tables when file path changes
        let _ = self.load_available_tables();
    }

    /// Toggle import all tables
    fn toggle_import_all_tables(&mut self) {
        self.sqlite_options.import_all_tables = !self.sqlite_options.import_all_tables;
    }

    /// Toggle table selection
    fn toggle_table_selection(&mut self, table_name: &str) {
        if self.sqlite_options.selected_tables.contains(&table_name.to_string()) {
            self.sqlite_options.selected_tables.retain(|s| s != table_name);
        } else {
            self.sqlite_options.selected_tables.push(table_name.to_string());
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Tab),
            (crate::config::Mode::Global, crate::action::Action::Up),
            (crate::config::Mode::Global, crate::action::Action::Down),
            (crate::config::Mode::Global, crate::action::Action::Left),
            (crate::config::Mode::Global, crate::action::Action::Right),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::SqliteOptionsDialog, crate::action::Action::OpenSqliteFileBrowser),
            (crate::config::Mode::SqliteOptionsDialog, crate::action::Action::ToggleTableSelection),
        ])
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the background for the popup
        Clear.render(area, buf);
        
        // If file browser mode is active, render the file browser
        if self.file_browser_mode {
            if let Some(browser) = &self.file_browser {
                browser.render(area, buf);
            }
            return;
        }
        
        let _block = Block::default()
            .title("SQLite Import Options")
            .borders(Borders::ALL);

        // Use split_dialog_area to handle instructions layout
        let instructions = self.build_instructions_from_config();
        let main_layout = split_dialog_area(area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        
        // Split the content area for file path and options
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // File path input
                Constraint::Min(0),    // Options content
            ])
            .split(main_layout.content_area);

        // Render file path input
        let file_path_area = chunks[0];
        if !self.file_path_focused {
            let mut textarea_copy = self.file_path_input.clone();   
            textarea_copy.set_cursor_style(Style::default().fg(Color::Gray)); // Hide cursor
            textarea_copy.render(file_path_area, buf);
        } else {
            self.file_path_input.render(file_path_area, buf);
        }

        // Render browse button
        let browse_text = "[Browse]";
        let browse_x = file_path_area.x + file_path_area.width.saturating_sub(browse_text.len() as u16 + 1);
        let browse_style = if self.file_path_focused {
            Style::default().fg(Color::Gray)
        } else if self.browse_button_selected {
            // Browse button is selected
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(browse_x, file_path_area.y + 1, browse_text, browse_style);

        // Render options content
        let options_area = chunks[1];
        
        // Render import all tables option
        let import_all_text = format!("Import All Tables: {}", self.sqlite_options.import_all_tables);
        let import_all_style = if !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected && self.selected_table_index == 0 {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default()
        };
        buf.set_string(options_area.x + 1, options_area.y + 1, import_all_text, import_all_style);

        // Render table selection if not importing all tables
        if !self.sqlite_options.import_all_tables {
            let table_title = "Select Tables:";
            buf.set_string(options_area.x + 1, options_area.y + 3, table_title, Style::default());

            for (i, table) in self.available_tables.iter().enumerate() {
                let y = options_area.y + 4 + i as u16;
                let is_selected = self.sqlite_options.selected_tables.contains(table);
                let checkbox = if is_selected { "[x]" } else { "[ ]" };
                let table_text = format!("{checkbox} {table}");
                
                let style = if i == (self.selected_table_index.saturating_sub(1)) && !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected && !self.sqlite_options.import_all_tables {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default()
                };
                
                buf.set_string(options_area.x + 2, y, table_text, style);
            }
        }

        // Render the options block border
        let options_block = Block::default()
            .borders(Borders::ALL)
            .title("SQLite Options");
        options_block.render(options_area, buf);

        // Render the [Finish] button at the bottom right of the content area
        let finish_text = "[Finish]";
        let finish_x = main_layout.content_area.x + main_layout.content_area.width.saturating_sub(finish_text.len() as u16 + 2);
        let finish_y = main_layout.content_area.y + main_layout.content_area.height.saturating_sub(2);
        let finish_style = if self.finish_button_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(finish_x, finish_y, finish_text, finish_style);

        // Render instructions area if available
        if let Some(instructions_area) = main_layout.instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }
}

impl Component for SqliteOptionsDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
        self.config = _config;
        // Propagate to FileBrowserDialog if it exists
        if let Some(ref mut browser) = self.file_browser {
            browser.register_config_handler(self.config.clone());
        }
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
        // Handle file browser events if file browser mode is active
        if self.file_browser_mode {
            if let Some(browser) = &mut self.file_browser
                && let Some(action) = browser.handle_key_event(key) {
                match action {
                    crate::dialog::file_browser_dialog::FileBrowserAction::Selected(path) => {
                        // Update the file path with the selected file
                        self.file_path = path.to_string_lossy().to_string();
                        self.update_file_path(self.file_path.clone());
                        // Update the TextArea to reflect the new file path
                        self.file_path_input = TextArea::from(vec![self.file_path.clone()]);
                        self.file_path_input.set_block(
                            Block::default()
                                .title("File Path")
                                .borders(Borders::ALL)
                        );
                        self.file_browser_mode = false;
                        self.file_browser = None;
                        return Ok(None);
                    }
                    crate::dialog::file_browser_dialog::FileBrowserAction::Cancelled => {
                        // Cancel file browser
                        self.file_browser_mode = false;
                        self.file_browser = None;
                        return Ok(None);
                    }
                }
            }
            return Ok(None);
        }

        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(None);
        }

        // Get config-driven actions once
        let global_action = self.config.action_for_key(crate::config::Mode::Global, key);
        let sqlite_dialog_action = self.config.action_for_key(crate::config::Mode::SqliteOptionsDialog, key);

        // First, honor config-driven Global actions
        if let Some(global_action) = &global_action {
            match global_action {
                Action::Escape => {
                    return Ok(Some(Action::CloseSqliteOptionsDialog));
                }
                Action::ToggleInstructions => {
                    self.show_instructions = !self.show_instructions;
                    return Ok(None);
                }
                Action::Backspace => {
                    if self.file_path_focused {
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                    }
                    return Ok(None);
                }
                Action::Tab => {
                    // Tab only moves between file path and browse button
                    if self.file_path_focused {
                        self.file_path_focused = false; // Move to browse button
                        self.browse_button_selected = true;
                        self.selected_table_index = 0; // Reset option selection
                    } else {
                        self.file_path_focused = true; // Move back to file path
                        self.browse_button_selected = false;
                        self.selected_table_index = 0; // Reset option selection
                    }
                    return Ok(None);
                }
                Action::Right => {
                    if self.file_path_focused {
                        // Check if cursor is at the end of the text
                        let lines = self.file_path_input.lines();
                        let cursor_pos = self.file_path_input.cursor();
                        
                        if cursor_pos.0 == lines.len().saturating_sub(1) && 
                           cursor_pos.1 >= lines.last().unwrap_or(&String::new()).len() {
                            // Cursor is at the end, move to browse button
                            self.file_path_focused = false;
                            self.browse_button_selected = true;
                        } else {
                            // Let the TextArea handle the right arrow normally
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.update_file_path(self.file_path_input.lines().join("\n"));
                        }
                    } else if !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected {
                        // If an option is selected, right arrow moves to finish button
                        self.finish_button_selected = true;
                    }
                    return Ok(None);
                }
                Action::Left => {
                    if self.finish_button_selected {
                        // Move from finish button back to options
                        self.finish_button_selected = false;
                    } else if self.browse_button_selected {
                        // Move from browse button to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    } else if self.file_path_focused {
                        // Let the TextArea handle the left arrow normally
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                    }
                    return Ok(None);
                }
                Action::Up => {
                    if self.finish_button_selected {
                        // Move from finish button to last option
                        self.finish_button_selected = false;
                        if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                            self.selected_table_index = self.available_tables.len(); // Last table + 1 for "Import All Tables"
                        } else {
                            self.selected_table_index = 0; // "Import All Tables" option
                        }
                    } else if self.browse_button_selected {
                        // If browse button is selected, up arrow goes back to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    } else if self.file_path_focused {
                        // When file path is focused, up arrow moves to last option
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                        if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                            self.selected_table_index = self.available_tables.len(); // Last table + 1
                        } else {
                            self.selected_table_index = 0; // "Import All Tables" option
                        }
                    } else {
                        // Navigate options (Import All Tables + individual tables)
                        if self.selected_table_index > 0 {
                            self.selected_table_index = self.selected_table_index.saturating_sub(1);
                        } else {
                            // At first option, go back to file path
                            self.file_path_focused = true;
                        }
                    }
                    return Ok(None);
                }
                Action::Down => {
                    if self.file_path_focused {
                        // When file path is focused, down arrow moves to options
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                        self.selected_table_index = 0; // Start at "Import All Tables"
                    } else if self.browse_button_selected {
                        // If browse button is selected, down arrow moves to finish button
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                    } else {
                        // Navigate options
                        let max_index = if self.sqlite_options.import_all_tables {
                            0 // Only "Import All Tables" option
                        } else {
                            self.available_tables.len() // "Import All Tables" + individual tables
                        };
                        
                        if self.selected_table_index < max_index {
                            self.selected_table_index = self.selected_table_index.saturating_add(1);
                        } else {
                            // At last option, move to finish button
                            self.finish_button_selected = true;
                        }
                    }
                    return Ok(None);
                }
                Action::Enter => {
                    if self.browse_button_selected {
                        // Open file browser
                        let mut browser = FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["db", "sqlite", "sqlite3"]),
                            false,
                            FileBrowserMode::Load
                        );
                        browser.register_config_handler(self.config.clone());
                        self.file_browser = Some(browser);
                        self.file_browser_mode = true;
                        return Ok(None);
                    } else if self.finish_button_selected {
                        // Finish button pressed - create import config and return it
                        let config = self.create_import_config();
                        return Ok(Some(Action::AddDataImportConfig { config }));
                    } else if !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected {
                        // Toggle import all tables or table selection
                        if self.selected_table_index == 0 {
                            // "Import All Tables" option
                            self.toggle_import_all_tables();
                        } else if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                            let table_index = self.selected_table_index.saturating_sub(1);
                            if table_index < self.available_tables.len() {
                                let table_name = self.available_tables[table_index].clone();
                                self.toggle_table_selection(&table_name);
                            }
                        }
                        return Ok(None);
                    }
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Next, check for SqliteOptionsDialog-specific actions
        if let Some(dialog_action) = &sqlite_dialog_action {
            match dialog_action {
                Action::OpenSqliteFileBrowser => {
                    // Ctrl+B: Open file browser
                    let mut browser = FileBrowserDialog::new(
                        Some(self.file_browser_path.clone()),
                        Some(vec!["db", "sqlite", "sqlite3"]),
                        false,
                        FileBrowserMode::Load
                    );
                    browser.register_config_handler(self.config.clone());
                    self.file_browser = Some(browser);
                    self.file_browser_mode = true;
                    return Ok(None);
                }
                Action::ToggleTableSelection => {
                    // Space: Toggle table selection or import all tables
                    if !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected {
                        if self.selected_table_index == 0 {
                            // "Import All Tables" option
                            self.toggle_import_all_tables();
                        } else if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                            let table_index = self.selected_table_index.saturating_sub(1);
                            if table_index < self.available_tables.len() {
                                let table_name = self.available_tables[table_index].clone();
                                self.toggle_table_selection(&table_name);
                            }
                        }
                    }
                    return Ok(None);
                }
                Action::Paste => {
                    // Paste clipboard text into the File Path when focused
                    if self.file_path_focused
                        && let Ok(mut clipboard) = arboard::Clipboard::new()
                            && let Ok(text) = clipboard.get_text() {
                                let first_line = text.lines().next().unwrap_or("").to_string();
                                self.set_file_path(first_line);
                            }
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Fallback for character input or other unhandled keys
        if let KeyCode::Char(_c) = key.code
            && self.file_path_focused {
                // Handle text input for file path
                use tui_textarea::Input as TuiInput;
                let input: TuiInput = key.into();
                self.file_path_input.input(input);
                self.update_file_path(self.file_path_input.lines().join("\n"));
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