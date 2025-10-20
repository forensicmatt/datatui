//! XlsxOptionsDialog: Dialog for configuring Excel import options

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use crate::components::dialog_layout::split_dialog_area;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent, KeyCode};
use ratatui::Frame;
use tui_textarea::Input as TuiInput;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode};
use tui_textarea::TextArea;
use arboard::Clipboard;
use tracing::error;

use crate::excel_operations::{ExcelOperations, WorksheetInfo};

/// XLSX import options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct XlsxImportOptions {
    pub worksheets: Vec<WorksheetInfo>,
}

/// XlsxOptionsDialog: Dialog for configuring Excel import options
#[derive(Debug, Serialize, Deserialize)]
pub struct XlsxOptionsDialog {
    pub file_path: String,
    pub xlsx_options: XlsxImportOptions,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    pub selected_worksheet_index: usize, // Index of selected worksheet in the table
    pub finish_button_selected: bool, // Whether the finish button is selected
    pub show_instructions: bool, // Whether to show instructions area
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    pub config: Config,
}

impl XlsxOptionsDialog {
    /// Create a new XlsxOptionsDialog
    pub fn new(file_path: String, xlsx_options: XlsxImportOptions) -> Self {
        let mut file_path_input = TextArea::default();
        file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        file_path_input.insert_str(&file_path);
        
        Self {
            file_path,
            xlsx_options,
            file_path_focused: true,
            browse_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected_worksheet_index: 0,
            finish_button_selected: false,
            show_instructions: true,
            file_path_input,
            file_browser: None,
            config: Config::default(),
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

    /// Get the current XLSX options
    pub fn get_xlsx_options(&self) -> &XlsxImportOptions {
        &self.xlsx_options
    }

    /// Get the current XLSX options as mutable
    pub fn get_xlsx_options_mut(&mut self) -> &mut XlsxImportOptions {
        &mut self.xlsx_options
    }

        /// Create a DataImportConfig from the current dialog state
    pub fn create_import_config(&self) -> crate::data_import_types::DataImportConfig {
        use crate::data_import_types::DataImportConfig;

        let file_path = PathBuf::from(&self.file_path);
        // Filter worksheets that are marked for loading
        let _selected_worksheets: Vec<String> = self.xlsx_options.worksheets
            .iter()
            .filter(|w| w.load)
            .map(|w| w.name.clone())
            .collect();
        
        // Create a new XlsxImportOptions with only the selected worksheets
        let mut filtered_options = self.xlsx_options.clone();
        filtered_options.worksheets.retain(|w| w.load);
        
        DataImportConfig::excel(file_path, filtered_options)
    }

    /// Load worksheets from the current Excel file
    pub fn load_worksheets(&mut self) -> Result<()> {
        if !self.file_path.is_empty() {
            let file_path = PathBuf::from(&self.file_path);
            if ExcelOperations::is_valid_excel_file(&file_path) {
                match ExcelOperations::read_worksheet_info(&file_path) {
                    Ok(worksheets) => {
                        self.xlsx_options.worksheets = worksheets;
                        self.selected_worksheet_index = 0;
                    }
                    Err(e) => {
                        // Handle error - could log or show a message
                        error!("Error loading Excel file: {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    /// Update the file path
    fn update_file_path(&mut self, path: String) {
        self.file_path = path;
    }

    /// Toggle worksheet load status
    fn toggle_worksheet_load(&mut self, index: usize) {
        if index < self.xlsx_options.worksheets.len() {
            self.xlsx_options.worksheets[index].load = !self.xlsx_options.worksheets[index].load;
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
            (crate::config::Mode::XlsxOptionsDialog, crate::action::Action::OpenXlsxFileBrowser),
            (crate::config::Mode::XlsxOptionsDialog, crate::action::Action::PasteFilePath),
            (crate::config::Mode::XlsxOptionsDialog, crate::action::Action::ToggleWorksheetLoad),
        ])
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the background for the popup
        Clear.render(area, buf);
        
        // If file browser mode is active, render the file browser
        if self.file_browser_mode {
            if let Some(browser) = &self.file_browser { browser.render(area, buf); }
            return;
        }
        
        let _block = Block::default()
            .title("Excel Import Options")
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

        // Render file path input and [Browse] within a single bordered block,
        // shrinking the input area to avoid overlapping the [Browse] text
        let file_path_area = chunks[0];
        let outer_block = Block::default()
            .title("File Path")
            .borders(Borders::ALL);
        outer_block.render(file_path_area, buf);

        // Compute inner content area (inside the border)
        let inner_x = file_path_area.x.saturating_add(1);
        let inner_y = file_path_area.y.saturating_add(1);
        let inner_w = file_path_area.width.saturating_sub(2);
        let inner_h = file_path_area.height.saturating_sub(2);

        let browse_text = "[Browse]";
        let reserved_for_browse: u16 = (browse_text.len() as u16).saturating_add(1); // 1 space padding
        let input_w = inner_w.saturating_sub(reserved_for_browse);

        // Input area is the inner area minus the reserved width for the browse text
        let input_area = Rect {
            x: inner_x,
            y: inner_y,
            width: input_w,
            height: inner_h,
        };

        // Render the TextArea without its own borders, within the input area
        if !self.file_path_focused {
            let mut textarea_copy = self.file_path_input.clone();
            textarea_copy.set_block(Block::default());
            textarea_copy.set_cursor_style(Style::default().fg(Color::Gray)); // Hide cursor
            textarea_copy.render(input_area, buf);
        } else {
            let mut textarea_copy = self.file_path_input.clone();
            textarea_copy.set_block(Block::default());
            textarea_copy.render(input_area, buf);
        }

        // Render browse button inside the inner area on the right
        let browse_x = inner_x
            .saturating_add(inner_w.saturating_sub(browse_text.len() as u16));
        let browse_style = if self.file_path_focused {
            Style::default().fg(Color::Gray)
        } else if self.browse_button_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(browse_x, inner_y, browse_text, browse_style);

        // Render options content
        let options_area = chunks[1];
        
        // Render worksheets table
        if !self.xlsx_options.worksheets.is_empty() {
            // Table header
            let header_y = options_area.y + 1;
            let header_style = Style::default().fg(Color::Yellow);
            
            // Column headers
            buf.set_string(options_area.x + 1, header_y, "Load", header_style);
            buf.set_string(options_area.x + 8, header_y, "Name", header_style);
            buf.set_string(options_area.x + 30, header_y, "Rows", header_style);
            buf.set_string(options_area.x + 40, header_y, "Cols", header_style);
            buf.set_string(options_area.x + 50, header_y, "Cells", header_style);
            
            // Render each worksheet
            for (i, worksheet) in self.xlsx_options.worksheets.iter().enumerate() {
                let y = header_y + 1 + i as u16;
                
                // Determine row style based on selection
                let row_style = if i == self.selected_worksheet_index && !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default()
                };
                
                // Load checkbox
                let load_text = if worksheet.load { "[x]" } else { "[ ]" };
                buf.set_string(options_area.x + 1, y, load_text, row_style);
                
                // Worksheet name
                buf.set_string(options_area.x + 8, y, &worksheet.name, row_style);
                
                // Row count
                let rows_text = format!("{}", worksheet.row_count);
                buf.set_string(options_area.x + 30, y, &rows_text, row_style);
                
                // Column count
                let cols_text = format!("{}", worksheet.column_count);
                buf.set_string(options_area.x + 40, y, &cols_text, row_style);
                
                // Non-empty cells count
                let cells_text = format!("{}", worksheet.non_empty_cells);
                buf.set_string(options_area.x + 50, y, &cells_text, row_style);
            }
        } else {
            // No worksheets loaded
            let no_data_text = "No Excel file loaded or no worksheets found";
            buf.set_string(options_area.x + 1, options_area.y + 1, no_data_text, Style::default().fg(Color::Gray));
        }

        // Render the [Finish] button at the bottom right of the full dialog area
        let finish_text = "[Finish]";
        let finish_x = main_layout.content_area.x + main_layout.content_area.width.saturating_sub(finish_text.len() as u16 + 2);
        let finish_y = main_layout.content_area.y + main_layout.content_area.height.saturating_sub(2);
        let finish_style = if self.finish_button_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(finish_x, finish_y, finish_text, finish_style);

        // Render the options block border
        let options_block = Block::default()
            .borders(Borders::ALL)
            .title("Worksheets");
        options_block.render(options_area, buf);

        // Render the options block border
        let options_block = Block::default()
            .borders(Borders::ALL)
            .title("Excel Options");
        options_block.render(options_area, buf);

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

impl Component for XlsxOptionsDialog {
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
                        // Load worksheets from the selected file
                        let _ = self.load_worksheets();
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

        let result = if key.kind == crossterm::event::KeyEventKind::Press {
            // Get all configured actions once at the start
            let global_action = self.config.action_for_key(crate::config::Mode::Global, key);
            let xlsx_dialog_action = self.config.action_for_key(crate::config::Mode::XlsxOptionsDialog, key);

            // First, check for XlsxOptionsDialog-specific actions
            if let Some(dialog_action) = &xlsx_dialog_action {
                match dialog_action {
                    Action::OpenXlsxFileBrowser => {
                        // Open file browser
                        let mut browser = FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["xlsx", "xls"]),
                            false,
                            FileBrowserMode::Load
                        );
                        browser.register_config_handler(self.config.clone());
                        self.file_browser = Some(browser);
                        self.file_browser_mode = true;
                        return Ok(None);
                    }
                    Action::PasteFilePath => {
                        // Paste clipboard text into the File Path when focused
                        if self.file_path_focused
                            && let Ok(mut clipboard) = Clipboard::new()
                            && let Ok(text) = clipboard.get_text() {
                            let first_line = text.lines().next().unwrap_or("").to_string();
                            self.set_file_path(first_line);
                            let _ = self.load_worksheets();
                        }
                        return Ok(None);
                    }
                    Action::ToggleWorksheetLoad => {
                        // Toggle worksheet load status when a worksheet is selected
                        if !self.file_path_focused
                            && !self.browse_button_selected
                            && !self.finish_button_selected
                            && !self.xlsx_options.worksheets.is_empty() {
                            self.toggle_worksheet_load(self.selected_worksheet_index);
                        }
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // Next, check Global actions with special handling
            if let Some(global_action) = &global_action {
                match global_action {
                    Action::Escape => {
                        return Ok(Some(Action::CloseXlsxOptionsDialog));
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return Ok(None);
                    }
                    Action::Tab => {
                        // Tab moves between file path and browse button
                        if self.file_path_focused {
                            self.file_path_focused = false;
                            self.browse_button_selected = true;
                        } else {
                            self.file_path_focused = true;
                            self.browse_button_selected = false;
                        }
                        return Ok(None);
                    }
                    Action::Right => {
                        if self.file_path_focused {
                            // Always move focus from File Path to [Browse]
                            self.file_path_focused = false;
                            self.browse_button_selected = true;
                        }
                        return Ok(None);
                    }
                    Action::Left => {
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
                        return Ok(None);
                    }
                    Action::Up => {
                        if self.finish_button_selected {
                            // If finish button is selected, up arrow goes to last worksheet or browse button
                            self.finish_button_selected = false;
                            if !self.xlsx_options.worksheets.is_empty() {
                                self.selected_worksheet_index = self.xlsx_options.worksheets.len().saturating_sub(1);
                            } else {
                                // If no worksheets, go to browse button
                                self.browse_button_selected = true;
                            }
                        } else if self.file_path_focused {
                            // When file path is focused, up arrow moves to worksheets only if there are worksheets
                            if !self.xlsx_options.worksheets.is_empty() {
                                self.file_path_focused = false;
                                self.browse_button_selected = false;
                                self.selected_worksheet_index = 0;
                            }
                            // If no worksheets, keep file path focused
                        } else if self.browse_button_selected {
                            // If browse button is selected, up arrow goes back to file path
                            self.file_path_focused = true;
                            self.browse_button_selected = false;
                        } else if !self.xlsx_options.worksheets.is_empty() {
                            // Navigate worksheet selection
                            if self.selected_worksheet_index > 0 {
                                self.selected_worksheet_index = self.selected_worksheet_index.saturating_sub(1);
                            } else {
                                // If at first worksheet, go back to browse button
                                self.browse_button_selected = true;
                            }
                        }
                        return Ok(None);
                    }
                    Action::Down => {
                        if self.file_path_focused {
                            // When file path is focused, down arrow moves to worksheets only if there are worksheets
                            if !self.xlsx_options.worksheets.is_empty() {
                                self.file_path_focused = false;
                                self.browse_button_selected = false;
                                self.selected_worksheet_index = 0;
                            }
                            // If no worksheets, keep file path focused
                        } else if self.browse_button_selected {
                            // If browse button is selected, down arrow moves to worksheets only if there are worksheets
                            if !self.xlsx_options.worksheets.is_empty() {
                                self.browse_button_selected = false;
                                self.selected_worksheet_index = 0;
                            } else {
                                // If no worksheets, move to finish button
                                self.browse_button_selected = false;
                                self.finish_button_selected = true;
                            }
                        } else if !self.xlsx_options.worksheets.is_empty() {
                            // Navigate worksheet selection
                            if self.selected_worksheet_index < self.xlsx_options.worksheets.len().saturating_sub(1) {
                                self.selected_worksheet_index = self.selected_worksheet_index.saturating_add(1);
                            } else {
                                // If at last worksheet, move to finish button
                                self.selected_worksheet_index = 0; // Reset worksheet selection
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
                                Some(vec!["xlsx", "xls"]),
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
                        } else if self.file_path_focused {
                            // If file path is focused and Enter is pressed, select the finish button
                            self.file_path_focused = false;
                            self.finish_button_selected = true;
                            return Ok(None);
                        } else {
                            // If no button is selected and Enter is pressed, select the finish button
                            self.finish_button_selected = true;
                            return Ok(None);
                        }
                    }
                    _ => {}
                }
            }

            match key.code {
                KeyCode::Backspace => {
                    if self.file_path_focused {
                        // Handle backspace to delete characters in file path
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                    }
                    None
                }
                KeyCode::Char(_c) => {
                    if self.file_path_focused {
                        // Handle text input for file path
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                        None
                    } else {
                        None
                    }
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