//! JsonOptionsDialog: Dialog for configuring JSON import options (supports JSON array or NDJSON)

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
use arboard::Clipboard;

/// JSON import options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonImportOptions {
    /// If true, treat file as NDJSON (each line is a JSON object); otherwise expect a JSON array of objects
    pub ndjson: bool,
}

impl Default for JsonImportOptions {
    fn default() -> Self {
        Self { ndjson: false }
    }
}

/// JsonOptionsDialog: Dialog for selecting a JSON file and format
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonOptionsDialog {
    pub file_path: String,
    pub json_options: JsonImportOptions,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub finish_button_selected: bool,
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
}

impl JsonOptionsDialog {
    /// Create a new JsonOptionsDialog
    pub fn new(file_path: String, json_options: JsonImportOptions) -> Self {
        let mut file_path_input = TextArea::default();
        file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        file_path_input.insert_str(&file_path);
        
        Self {
            file_path,
            json_options,
            file_path_focused: true,
            browse_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_path_input,
            file_browser: None,
        }
    }

    /// Create a DataImportConfig from the current dialog state
    pub fn create_import_config(&self) -> crate::data_import_types::DataImportConfig {
        use crate::data_import_types::DataImportConfig;
        let file_path = PathBuf::from(&self.file_path);
        DataImportConfig::json(file_path, self.json_options.clone())
    }

    /// Update the file path
    fn update_file_path(&mut self, path: String) {
        self.file_path = path;
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
            .title("JSON Import Options")
            .borders(Borders::ALL);

        // Create a layout with the file path input at the top and options below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // File path input
                Constraint::Min(0),    // Options/content
            ])
            .split(area);

        // Render file path input and [Browse] within a single bordered block
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

        // Render options content: NDJSON toggle and Finish
        let options_area = chunks[1];
        let ndjson_label = format!("NDJSON (one JSON object per line): {}", if self.json_options.ndjson { "On" } else { "Off" });
        buf.set_string(options_area.x + 1, options_area.y + 1, ndjson_label, Style::default());

        // Render the [Finish] button at the bottom right of the full dialog area
        let finish_text = "[Finish]";
        let finish_x = area.x + area.width.saturating_sub(finish_text.len() as u16 + 2);
        let finish_y = area.y + area.height.saturating_sub(2);
        let finish_style = if self.finish_button_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(finish_x, finish_y, finish_text, finish_style);
    }
}

impl Component for JsonOptionsDialog {
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
                    // Cycle between file path, browse button, finish button
                    if self.file_path_focused {
                        self.file_path_focused = false;
                        self.browse_button_selected = true;
                        self.finish_button_selected = false;
                    } else if self.browse_button_selected {
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                    } else {
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                        self.finish_button_selected = false;
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
                            self.finish_button_selected = false;
                        } else {
                            // Let the TextArea handle the right arrow normally
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.update_file_path(self.file_path_input.lines().join("\n"));
                        }
                    } else if self.browse_button_selected {
                        // Move to finish button
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                    }
                    None
                }
                KeyCode::Left => {
                    if self.finish_button_selected {
                        // Move from finish button back to browse button
                        self.finish_button_selected = false;
                        self.browse_button_selected = true;
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
                    None
                }
                KeyCode::Up => {
                    if self.finish_button_selected {
                        self.finish_button_selected = false;
                        self.browse_button_selected = true;
                    } else if self.browse_button_selected {
                        self.browse_button_selected = false;
                        self.file_path_focused = true;
                    }
                    None
                }
                KeyCode::Down => {
                    if self.file_path_focused {
                        self.file_path_focused = false;
                        self.browse_button_selected = true;
                    } else if self.browse_button_selected {
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                    }
                    None
                }
                KeyCode::Enter => {
                    if self.browse_button_selected {
                        // Open file browser
                        self.file_browser = Some(FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["json", "ndjson"]),
                            false,
                            FileBrowserMode::Load
                        ));
                        self.file_browser_mode = true;
                        None
                    } else if self.finish_button_selected {
                        // Finish button pressed - create import config and return it
                        let config = self.create_import_config();
                        Some(Action::AddDataImportConfig { config })
                    } else if self.file_path_focused {
                        // If file path is focused and Enter is pressed, select the finish button
                        self.file_path_focused = false;
                        self.finish_button_selected = true;
                        None
                    } else {
                        None
                    }
                }
                KeyCode::Char('b') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    // Ctrl+B: Open file browser
                    self.file_browser = Some(FileBrowserDialog::new(
                        Some(self.file_browser_path.clone()),
                        Some(vec!["json", "ndjson"]),
                        false,
                        FileBrowserMode::Load
                    ));
                    self.file_browser_mode = true;
                    None
                }
                KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    // Ctrl+P: Paste clipboard text into the File Path when focused
                    if self.file_path_focused {
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                let first_line = text.lines().next().unwrap_or("").to_string();
                                self.file_path = first_line.clone();
                                self.file_path_input = TextArea::from(vec![first_line]);
                                self.file_path_input.set_block(
                                    Block::default()
                                        .title("File Path")
                                        .borders(Borders::ALL)
                                );
                            }
                        }
                    }
                    None
                }
                KeyCode::Char('n') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    // Ctrl+N: Toggle NDJSON option
                    let mut opts = self.json_options.clone();
                    opts.ndjson = !opts.ndjson;
                    self.json_options = opts;
                    None
                }
                KeyCode::Backspace => {
                    if self.file_path_focused {
                        // Handle backspace to delete characters in file path
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                    }
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
                    Some(Action::CloseJsonOptionsDialog)
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


