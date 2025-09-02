//! SqliteOptionsDialog: Dialog for configuring SQLite import options

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};
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
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    pub available_tables: Vec<String>,
    pub selected_table_index: usize,
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
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
        
        Self {
            file_path,
            sqlite_options,
            file_path_focused: true,
            browse_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            available_tables: Vec::new(),
            selected_table_index: 0,
            file_path_input,
            file_browser: None,
        }
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
        self.available_tables = tables;
    }

    /// Update the file path
    fn update_file_path(&mut self, path: String) {
        self.file_path = path;
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

        // Create a layout with the file path input at the top
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // File path input
                Constraint::Min(0),    // Options content
            ])
            .split(area);

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
        let import_all_style = if !self.file_path_focused && !self.browse_button_selected {
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
                let table_text = format!("{} {}", checkbox, table);
                
                let style = if i == self.selected_table_index && !self.file_path_focused && !self.browse_button_selected {
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
    }
}

impl Component for SqliteOptionsDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
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
            if let Some(browser) = &mut self.file_browser {
                if let Some(action) = browser.handle_key_event(key) {
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
            }
            return Ok(None);
        }

        let result = if key.kind == crossterm::event::KeyEventKind::Press {
            match key.code {
                KeyCode::Tab => {
                    // Tab moves between file path and browse button
                    if self.file_path_focused {
                        self.file_path_focused = false;
                        self.browse_button_selected = true;
                    } else {
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    }
                    None
                }
                KeyCode::Right => {
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
                    }
                    None
                }
                KeyCode::Left => {
                    if self.browse_button_selected {
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
                    None
                }
                KeyCode::Up => {
                    if self.file_path_focused {
                        // When file path is focused, up arrow moves to options
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                    } else if self.browse_button_selected {
                        // If browse button is selected, up arrow goes back to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    } else if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                        // Navigate table selection
                        if self.selected_table_index > 0 {
                            self.selected_table_index = self.selected_table_index.saturating_sub(1);
                        }
                    }
                    None
                }
                KeyCode::Down => {
                    if self.file_path_focused {
                        // When file path is focused, down arrow moves to options
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                    } else if self.browse_button_selected {
                        // If browse button is selected, down arrow moves to options
                        self.browse_button_selected = false;
                    } else if !self.sqlite_options.import_all_tables && !self.available_tables.is_empty() {
                        // Navigate table selection
                        if self.selected_table_index < self.available_tables.len().saturating_sub(1) {
                            self.selected_table_index = self.selected_table_index.saturating_add(1);
                        }
                    }
                    None
                }
                KeyCode::Enter => {
                    if self.browse_button_selected {
                        // Open file browser
                        self.file_browser = Some(FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["db", "sqlite", "sqlite3"]),
                            false,
                            FileBrowserMode::Load
                        ));
                        self.file_browser_mode = true;
                        None
                    } else if !self.file_path_focused && !self.browse_button_selected {
                        // Toggle import all tables or table selection
                        if self.sqlite_options.import_all_tables {
                            self.toggle_import_all_tables();
                        } else if !self.available_tables.is_empty() {
                            let table_name = self.available_tables[self.selected_table_index].clone();
                            self.toggle_table_selection(&table_name);
                        }
                        None
                    } else {
                        None
                    }
                }
                KeyCode::Char('b') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    // Ctrl+B: Open file browser
                    self.file_browser = Some(FileBrowserDialog::new(
                        Some(self.file_browser_path.clone()),
                        Some(vec!["db", "sqlite", "sqlite3"]),
                        false,
                        FileBrowserMode::Load
                    ));
                    self.file_browser_mode = true;
                    None
                }
                KeyCode::Char(_c) => {
                    if self.file_path_focused {
                        // Handle text input for file path
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                        None
                    } else {
                        None
                    }
                }
                KeyCode::Esc => {
                    Some(Action::CloseSqliteOptionsDialog)
                }
                _ => None,
            }
        } else {
            None
        };
        Ok(result)
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