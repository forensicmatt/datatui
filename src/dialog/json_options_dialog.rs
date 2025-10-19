//! JsonOptionsDialog: Dialog for configuring JSON import options (supports JSON array or NDJSON)

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode};
use tui_textarea::TextArea;
use arboard::Clipboard;
use tracing::info;

/// JSON import options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonImportOptions {
    /// If true, treat file as NDJSON (each line is a JSON object); otherwise expect a JSON array/object
    pub ndjson: bool,
    /// JMESPath expression that yields the records to load (default: "@")
    pub records_expr: String,
}

impl Default for JsonImportOptions {
    fn default() -> Self {
        Self { ndjson: false, records_expr: "@".to_string() }
    }
}

/// JsonOptionsDialog: Dialog for selecting a JSON file and format
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonOptionsDialog {
    pub file_path: String,
    pub json_options: JsonImportOptions,
    pub file_path_focused: bool,
    pub records_expr_focused: bool,
    pub ndjson_option_selected: bool,
    pub browse_button_selected: bool,
    pub finish_button_selected: bool,
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    pub show_instructions: bool,
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub records_expr_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    pub key_config: Config,
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
        let mut records_expr_input = TextArea::default();
        records_expr_input.set_block(
            Block::default()
                .title("JMESPath Records Expression (default: @)")
                .borders(Borders::ALL)
        );
        records_expr_input.insert_str(&json_options.records_expr);

        Self {
            file_path,
            json_options,
            file_path_focused: true,
            records_expr_focused: false,
            ndjson_option_selected: false,
            browse_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            show_instructions: true,
            file_path_input,
            records_expr_input,
            file_browser: None,
            key_config: Config::default(),
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

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.key_config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Tab),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::JsonOptionsDialog, crate::action::Action::OpenJsonFileBrowser),
            (crate::config::Mode::JsonOptionsDialog, crate::action::Action::PasteJsonFilePath),
            (crate::config::Mode::JsonOptionsDialog, crate::action::Action::ToggleNdjson),
        ])
    }

    /// Try to autodetect JSON format and a sensible records expression from a file path.
    /// Returns (Option<ndjson>, Option<records_expr>)
    fn autodetect_json_settings(path_str: &str) -> Option<(Option<bool>, Option<String>)> {
        use std::io::{BufRead, BufReader, Read};
        use serde_json::Value as JsonValue;
        let path = std::path::Path::new(path_str);

        // Prefer full-file parse first; if this succeeds, it's regular JSON (not NDJSON)
        if let Ok(mut file) = std::fs::File::open(path) {
            let mut buf = String::new();
            if file.read_to_string(&mut buf).is_ok() {
                if let Ok(root) = serde_json::from_str::<JsonValue>(&buf) {
                    // If root is array of objects -> records '@'
                    if let JsonValue::Array(arr) = &root {
                        if arr.iter().all(|v| v.is_object()) {
                            info!("json autodetect: top-level array of objects -> '@'");
                            return Some((Some(false), Some("@".to_string())));
                        }
                    }
                    // If root is object, try common keys that are arrays of objects
                    if let JsonValue::Object(map) = &root {
                        // Prioritized list of common record keys
                        let candidates = [
                            "records", "Records", "items", "Items", "data", "Data", "rows", "Rows", "result", "results", "value", "values"
                        ];
                        for key in candidates {
                            if let Some(v) = map.get(key) {
                                if let JsonValue::Array(arr) = v {
                                    if arr.iter().all(|e| e.is_object()) {
                                        info!("json autodetect: found array-of-objects at key '{}'", key);
                                        return Some((Some(false), Some(key.to_string())));
                                    }
                                }
                            }
                        }
                        // Try to find first array-of-objects to use as key
                        for (k, v) in map.iter() {
                            if let JsonValue::Array(arr) = v {
                                if arr.iter().all(|e| e.is_object()) {
                                    info!("json autodetect: using first array-of-objects key '{}'", k);
                                    return Some((Some(false), Some(k.clone())));
                                }
                            }
                        }
                    }
                    // Fallback to '@'
                    info!("json autodetect: fallback to '@'");
                    return Some((Some(false), Some("@".to_string())));
                }
            }
        }

        // If the whole file doesn't parse as JSON, sample multiple non-empty lines for NDJSON
        if let Ok(file) = std::fs::File::open(path) {
            let reader = BufReader::new(file);
            let mut total_non_empty: usize = 0;
            let mut valid_object_lines: usize = 0;
            for line_res in reader.lines().take(500) {
                if let Ok(line) = line_res {
                    let t = line.trim();
                    if t.is_empty() { continue; }
                    total_non_empty += 1;
                    if serde_json::from_str::<JsonValue>(t).ok().is_some_and(|v| v.is_object()) {
                        valid_object_lines += 1;
                    }
                    if total_non_empty >= 5 { break; }
                }
            }
            // Heuristic: at least 3 non-empty lines sampled and >= 2 valid JSON object lines
            if total_non_empty >= 3 && valid_object_lines >= 2 {
                info!("json autodetect: detected NDJSON for {} ({} valid object lines)", path_str, valid_object_lines);
                return Some((Some(true), None));
            }
        }

        None
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
        
        // Get dynamic instructions
        let instructions = self.build_instructions_from_config();
        
        // Use split_dialog_area to handle instructions
        let layout = split_dialog_area(area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        
        let _block = Block::default()
            .title("JSON Import Options")
            .borders(Borders::ALL);

        // Create a layout with the file path input at the top and options below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // File path input
                Constraint::Length(3), // records expr input
                Constraint::Min(0),    // Options/content
            ])
            .split(content_area);

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

        // Render records expression input row
        let records_area = chunks[1];
        let records_block = Block::default()
            .title("JMESPath Records Expression (e.g., @, Records, data.items)")
            .borders(Borders::ALL);
        records_block.render(records_area, buf);
        let rec_inner = Rect {
            x: records_area.x.saturating_add(1),
            y: records_area.y.saturating_add(1),
            width: records_area.width.saturating_sub(2),
            height: records_area.height.saturating_sub(2),
        };
        if !self.records_expr_focused {
            let mut rec_copy = self.records_expr_input.clone();
            rec_copy.set_block(Block::default());
            rec_copy.set_cursor_style(Style::default().fg(Color::Gray));
            rec_copy.render(rec_inner, buf);
        } else {
            let mut rec_copy = self.records_expr_input.clone();
            rec_copy.set_block(Block::default());
            rec_copy.render(rec_inner, buf);
        }

        // Render options content: NDJSON toggle and Finish
        let options_area = chunks[2];
        let ndjson_label = format!("NDJSON (one JSON object per line): {}", if self.json_options.ndjson { "On" } else { "Off" });
        let ndjson_style = if self.ndjson_option_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default()
        };
        buf.set_string(options_area.x + 1, options_area.y + 1, ndjson_label, ndjson_style);

        // Render the [Finish] button at the bottom right of the content area
        let finish_text = "[Finish]";
        let finish_x = content_area.x + content_area.width.saturating_sub(finish_text.len() as u16 + 2);
        let finish_y = content_area.y + content_area.height.saturating_sub(2);
        let finish_style = if self.finish_button_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        buf.set_string(finish_x, finish_y, finish_text, finish_style);

        // Render instructions if enabled and available
        if self.show_instructions && let Some(instructions_area) = instructions_area {
            use ratatui::widgets::{Paragraph, Wrap};
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }
}

impl Component for JsonOptionsDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.key_config = config;
        // Register config with file_browser if it exists
        if let Some(browser) = &mut self.file_browser {
            browser.register_config_handler(self.key_config.clone());
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
        use crossterm::event::KeyCode;
        
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
                        // Autodetect records expression and JSONL based on selected file
                        if let Some((det_ndjson, det_expr)) = Self::autodetect_json_settings(&self.file_path) {
                            let mut opts = self.json_options.clone();
                            if let Some(is_ndjson) = det_ndjson { opts.ndjson = is_ndjson; }
                            if let Some(expr) = det_expr {
                                opts.records_expr = expr.clone();
                                // Update records expr input field to show detected value
                                self.records_expr_input = TextArea::from(vec![expr]);
                                self.records_expr_input.set_block(
                                    Block::default()
                                        .title("JMESPath Records Expression (default: @)")
                                        .borders(Borders::ALL)
                                );
                            }
                            self.json_options = opts;
                        }
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
            // First, honor config-driven Global actions
            if let Some(global_action) = self.key_config.action_for_key(crate::config::Mode::Global, key) {
                match global_action {
                    Action::Escape => {
                        return Ok(Some(Action::CloseJsonOptionsDialog));
                    }
                    Action::Tab => {
                        // Cycle: file_path -> records_expr -> browse -> finish -> file_path
                        if self.file_path_focused {
                            self.file_path_focused = false;
                            self.records_expr_focused = true;
                            self.browse_button_selected = false;
                            self.finish_button_selected = false;
                        } else if self.records_expr_focused {
                            self.file_path_focused = false;
                            self.records_expr_focused = false;
                            self.browse_button_selected = true;
                            self.finish_button_selected = false;
                        } else if self.browse_button_selected {
                            self.file_path_focused = false;
                            self.records_expr_focused = false;
                            self.browse_button_selected = false;
                            self.finish_button_selected = true;
                        } else {
                            self.file_path_focused = true;
                            self.records_expr_focused = false;
                            self.browse_button_selected = false;
                            self.finish_button_selected = false;
                        }
                        return Ok(None);
                    }
                    Action::Enter => {
                        if self.browse_button_selected {
                            // Open file browser
                            self.file_browser = Some(FileBrowserDialog::new(
                                Some(self.file_browser_path.clone()),
                                Some(vec!["json", "ndjson"]),
                                false,
                                FileBrowserMode::Load
                            ));
                            // Register config with the new file browser
                            if let Some(browser) = &mut self.file_browser {
                                browser.register_config_handler(self.key_config.clone());
                            }
                            self.file_browser_mode = true;
                            return Ok(None);
                        } else if self.finish_button_selected {
                            // Finish button pressed - create import config and return it
                            let config = self.create_import_config();
                            return Ok(Some(Action::AddDataImportConfig { config }));
                        } else if self.file_path_focused {
                            // If file path is focused and Enter is pressed, select the finish button
                            self.file_path_focused = false;
                            self.records_expr_focused = true;
                            self.finish_button_selected = false;
                            return Ok(None);
                        } else if self.records_expr_focused {
                            // Move from records expr to finish button for convenience
                            self.records_expr_focused = false;
                            self.finish_button_selected = true;
                            return Ok(None);
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
                                self.records_expr_focused = true;
                                self.browse_button_selected = false;
                                self.finish_button_selected = false;
                            } else {
                                // Let the TextArea handle the right arrow normally
                                use tui_textarea::Input as TuiInput;
                                let input: TuiInput = key.into();
                                self.file_path_input.input(input);
                                self.update_file_path(self.file_path_input.lines().join("\n"));
                            }
                        } else if self.records_expr_focused {
                            // Only move to Browse when cursor is at end-of-text; otherwise move cursor right within field
                            let lines = self.records_expr_input.lines();
                            let cursor_pos = self.records_expr_input.cursor();
                            if cursor_pos.0 == lines.len().saturating_sub(1) &&
                               cursor_pos.1 >= lines.last().unwrap_or(&String::new()).len() {
                                self.records_expr_focused = false;
                                self.browse_button_selected = true;
                                self.finish_button_selected = false;
                            } else {
                                use tui_textarea::Input as TuiInput;
                                let input: TuiInput = key.into();
                                self.records_expr_input.input(input);
                                // Do not change focus; allow cursor to advance
                            }
                        } else if self.ndjson_option_selected {
                            // Move from NDJSON option to Finish
                            self.ndjson_option_selected = false;
                            self.finish_button_selected = true;
                        } else if self.browse_button_selected {
                            // Move to finish button
                            self.browse_button_selected = false;
                            self.finish_button_selected = true;
                        }
                        return Ok(None);
                    }
                    Action::Left => {
                        if self.finish_button_selected {
                            // Move from Finish back to options (NDJSON)
                            self.finish_button_selected = false;
                            self.ndjson_option_selected = true;
                            self.browse_button_selected = false;
                            self.records_expr_focused = false;
                            self.file_path_focused = false;
                        } else if self.browse_button_selected {
                            // Move from browse button to records expr
                            self.records_expr_focused = true;
                            self.browse_button_selected = false;
                        } else if self.file_path_focused {
                            // Let the TextArea handle the left arrow normally
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.update_file_path(self.file_path_input.lines().join("\n"));
                        } else if self.records_expr_focused {
                            // Let TextArea handle normally
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.records_expr_input.input(input);
                            let expr = self.records_expr_input.lines().join("\n");
                            let mut opts = self.json_options.clone();
                            opts.records_expr = expr;
                            self.json_options = opts;
                        }
                        return Ok(None);
                    }
                    Action::Up => {
                        // Reverse of Down behavior
                        if self.ndjson_option_selected {
                            self.ndjson_option_selected = false;
                            self.records_expr_focused = true;
                        } else if self.records_expr_focused {
                            // Only move up to File Path when cursor is at column 0 on first line
                            let cursor_pos = self.records_expr_input.cursor();
                            if cursor_pos.0 == 0 && cursor_pos.1 == 0 {
                                self.records_expr_focused = false;
                                self.file_path_focused = true;
                            } else {
                                // Let TextArea handle the left arrow normally when not at start
                                use tui_textarea::Input as TuiInput;
                                let input: TuiInput = key.into();
                                self.records_expr_input.input(input);
                            }
                        } else if self.finish_button_selected {
                            self.finish_button_selected = false;
                            self.browse_button_selected = true;
                        }
                        return Ok(None);
                    }
                    Action::Down => {
                        // When File Path is selected, Down -> Records expression
                        if self.file_path_focused {
                            self.file_path_focused = false;
                            self.records_expr_focused = true;
                            self.ndjson_option_selected = false;
                            self.browse_button_selected = false;
                            self.finish_button_selected = false;
                        } else if self.records_expr_focused {
                            // When Records expression is selected, Down -> NDJSON option
                            self.records_expr_focused = false;
                            self.ndjson_option_selected = true;
                            self.browse_button_selected = false;
                            self.finish_button_selected = false;
                        } else if self.browse_button_selected {
                            // When Browse is selected, Down -> Finish
                            self.browse_button_selected = false;
                            self.finish_button_selected = true;
                        }
                        return Ok(None);
                    }
                    Action::Backspace => {
                        if self.file_path_focused {
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.update_file_path(self.file_path_input.lines().join("\n"));
                        } else if self.records_expr_focused {
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.records_expr_input.input(input);
                            let expr = self.records_expr_input.lines().join("\n");
                            let mut opts = self.json_options.clone();
                            opts.records_expr = expr;
                            self.json_options = opts;
                        }
                        return Ok(None);
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // Next, check for JsonOptionsDialog specific actions
            if let Some(dialog_action) = self.key_config.action_for_key(crate::config::Mode::JsonOptionsDialog, key) {
                match dialog_action {
                    Action::OpenJsonFileBrowser => {
                        // Open file browser
                        self.file_browser = Some(FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["json", "ndjson"]),
                            false,
                            FileBrowserMode::Load
                        ));
                        // Register config with the new file browser
                        if let Some(browser) = &mut self.file_browser {
                            browser.register_config_handler(self.key_config.clone());
                        }
                        self.file_browser_mode = true;
                        return Ok(None);
                    }
                    Action::PasteJsonFilePath => {
                        // Paste clipboard text into the File Path when focused
                        if self.file_path_focused
                            && let Ok(mut clipboard) = Clipboard::new()
                            && let Ok(text) = clipboard.get_text() {
                            let first_line = text.lines().next().unwrap_or("").to_string();
                            self.file_path = first_line.clone();
                            self.file_path_input = TextArea::from(vec![first_line]);
                            self.file_path_input.set_block(
                                Block::default()
                                    .title("File Path")
                                    .borders(Borders::ALL)
                            );
                        }
                        return Ok(None);
                    }
                    Action::ToggleNdjson => {
                        // Toggle NDJSON option
                        let mut opts = self.json_options.clone();
                        opts.ndjson = !opts.ndjson;
                        self.json_options = opts;
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            // Fallback for character input when editing fields
            if let KeyCode::Char(c) = key.code {
                // Space toggles NDJSON when the NDJSON option is selected
                if self.ndjson_option_selected && c == ' ' {
                    let mut opts = self.json_options.clone();
                    opts.ndjson = !opts.ndjson;
                    self.json_options = opts;
                    return Ok(None);
                }
                if self.file_path_focused {
                    use tui_textarea::Input as TuiInput;
                    let input: TuiInput = key.into();
                    self.file_path_input.input(input);
                    self.update_file_path(self.file_path_input.lines().join("\n"));
                    return Ok(None);
                } else if self.records_expr_focused {
                    use tui_textarea::Input as TuiInput;
                    let input: TuiInput = key.into();
                    self.records_expr_input.input(input);
                    let expr = self.records_expr_input.lines().join("\n");
                    let mut opts = self.json_options.clone();
                    opts.records_expr = expr;
                    self.json_options = opts;
                    return Ok(None);
                } else {
                    return Ok(None);
                }
            }
            None
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


