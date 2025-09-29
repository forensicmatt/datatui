//! CsvOptionsDialog: Dialog for configuring CSV import options

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
use crate::dialog::file_browser_dialog::FileBrowserAction;
use crate::data_import_types::DataImportConfig;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode};
use tui_textarea::TextArea;
use arboard::Clipboard;

/// CSV/TSV import options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CsvImportOptions {
    pub delimiter: char,
    pub has_header: bool,
    pub quote_char: Option<char>,
    pub escape_char: Option<char>,
}

impl Default for CsvImportOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: true,
            quote_char: Some('"'),
            escape_char: Some('\\'),
        }
    }
}

/// CsvOptionsDialog: Dialog for configuring CSV import options
#[derive(Debug, Serialize, Deserialize)]
pub struct CsvOptionsDialog {
    pub file_path: String,
    pub csv_options: CsvImportOptions,
    pub file_path_focused: bool,
    pub option_selected: usize, // Which CSV option is currently selected (0-3)
    pub editing_option: bool, // Whether we're currently editing a CSV option
    pub browse_button_selected: bool, // Whether the browse button is selected
    pub finish_button_selected: bool, // Whether the finish button is selected
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    pub config: Config,
}

impl CsvOptionsDialog {
    /// Create a new CsvOptionsDialog
    pub fn new(file_path: String, csv_options: CsvImportOptions) -> Self {
        let mut file_path_input = TextArea::default();
        file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        file_path_input.insert_str(&file_path);
        
        Self {
            file_path,
            csv_options,
            file_path_focused: true,
            option_selected: 0,
            editing_option: false,
            browse_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
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

    /// Get the current CSV options
    pub fn get_csv_options(&self) -> &CsvImportOptions {
        &self.csv_options
    }

    /// Get the current CSV options as mutable
    pub fn get_csv_options_mut(&mut self) -> &mut CsvImportOptions {
        &mut self.csv_options
    }

        /// Create a DataImportConfig from the current dialog state
    pub fn create_import_config(&self) -> DataImportConfig {
        let file_path = PathBuf::from(&self.file_path);
        DataImportConfig::text(file_path, self.csv_options.clone())
    }

    /// Update the file path
    fn update_file_path(&mut self, path: String) {
        self.file_path = path;
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        use std::fmt::Write as _;
        fn fmt_key_event(key: &crossterm::event::KeyEvent) -> String {
            use crossterm::event::{KeyCode, KeyModifiers};
            let mut parts: Vec<&'static str> = Vec::with_capacity(3);
            if key.modifiers.contains(KeyModifiers::CONTROL) { parts.push("Ctrl"); }
            if key.modifiers.contains(KeyModifiers::ALT) { parts.push("Alt"); }
            if key.modifiers.contains(KeyModifiers::SHIFT) { parts.push("Shift"); }
            let key_part = match key.code {
                KeyCode::Char(' ') => "Space".to_string(),
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) { c.to_ascii_uppercase().to_string() } else { c.to_string() }
                }
                KeyCode::Left => "Left".to_string(),
                KeyCode::Right => "Right".to_string(),
                KeyCode::Up => "Up".to_string(),
                KeyCode::Down => "Down".to_string(),
                KeyCode::Enter => "Enter".to_string(),
                KeyCode::Esc => "Esc".to_string(),
                KeyCode::Tab => "Tab".to_string(),
                KeyCode::BackTab => "BackTab".to_string(),
                KeyCode::Delete => "Delete".to_string(),
                KeyCode::Insert => "Insert".to_string(),
                KeyCode::Home => "Home".to_string(),
                KeyCode::End => "End".to_string(),
                KeyCode::PageUp => "PageUp".to_string(),
                KeyCode::PageDown => "PageDown".to_string(),
                KeyCode::F(n) => format!("F{n}"),
                _ => "?".to_string(),
            };
            if parts.is_empty() { key_part } else { format!("{}+{}", parts.join("+"), key_part) }
        }

        fn fmt_sequence(seq: &[crossterm::event::KeyEvent]) -> String {
            let parts: Vec<String> = seq.iter().map(fmt_key_event).collect();
            parts.join(", ")
        }

        let mut segments: Vec<String> = Vec::new();

        // Global actions
        if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
            let global_actions: &[(Action, &str)] = &[
                (Action::Up, "Move"),
                (Action::Down, "Move"),
                (Action::Enter, "Select"),
                (Action::Escape, "Cancel"),
            ];

            for (action, label) in global_actions {
                let mut keys_for_action: Vec<&Vec<crossterm::event::KeyEvent>> = global_bindings
                    .iter()
                    .filter_map(|(seq, a)| if a == action { Some(seq) } else { None })
                    .collect();
                keys_for_action.sort_by_key(|seq| seq.len());
                if let Some(first) = keys_for_action.first() {
                    let key_text = fmt_sequence(first);
                    match action {
                        Action::Up | Action::Down => {
                            if segments.iter().any(|s| s.contains("Move")) { continue; }
                            segments.push(format!("{}/Down: {}", key_text.replace("Down", "Up"), label));
                        }
                        _ => segments.push(format!("{}: {}", key_text, label)),
                    }
                }
            }
        }

        // CsvOptions-specific actions
        if let Some(csv_bindings) = self.config.keybindings.0.get(&crate::config::Mode::CsvOptions) {
            let csv_actions: &[(Action, &str)] = &[
                (Action::Tab, "Navigate"),
                (Action::OpenFileBrowser, "Browse"),
                (Action::Paste, "Paste"),
            ];

            for (action, label) in csv_actions {
                let mut keys_for_action: Vec<&Vec<crossterm::event::KeyEvent>> = csv_bindings
                    .iter()
                    .filter_map(|(seq, a)| if a == action { Some(seq) } else { None })
                    .collect();
                keys_for_action.sort_by_key(|seq| seq.len());
                if let Some(first) = keys_for_action.first() {
                    let key_text = fmt_sequence(first);
                    segments.push(format!("{}: {}", key_text, label));
                }
            }
        }

        // Join with double space for readability
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{}", seg);
        }
        out
    }

    /// Update a CSV option
    fn update_csv_option(&mut self, c: char) {
        match self.option_selected {
            0 => self.csv_options.delimiter = c,
            1 => self.csv_options.has_header = !self.csv_options.has_header,
            2 => {
                if c == ' ' {
                    self.csv_options.quote_char = None;
                } else {
                    self.csv_options.quote_char = Some(c);
                }
            }
            3 => {
                if c == ' ' {
                    self.csv_options.escape_char = None;
                } else {
                    self.csv_options.escape_char = Some(c);
                }
            }
            _ => {}
        }
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
            .title("CSV Import Options")
            .borders(Borders::ALL);

        // Create a layout with the file path input at the top
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // File path input
                Constraint::Min(0),    // Options content
            ])
            .split(area);

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
        
        // Create individual option lines with highlighting
        let option_lines = [
            format!("Delimiter: '{}'", self.csv_options.delimiter),
            format!("Has Header: {}", self.csv_options.has_header),
            format!("Quote Char: {}", self.csv_options.quote_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string())),
            format!("Escape Char: {}", self.csv_options.escape_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string())),
        ];

        // Render each option line with highlighting
        for (i, line) in option_lines.iter().enumerate() {
            let y = options_area.y + (i + 1) as u16;  // +1 to account for the space between the text and the border
            let style = if i == self.option_selected && !self.finish_button_selected && !self.file_path_focused && !self.browse_button_selected {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default()
            };
            buf.set_string(options_area.x + 1, y, line, style);
        }

        // Render the options block border
        let options_block = Block::default()
            .borders(Borders::ALL)
            .title("CSV Options");

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
        
        // Render instructions at the bottom left
        let instructions = self.build_instructions_from_config();
        if !instructions.is_empty() {
            let instruction_x = area.x + 1;
            let instruction_y = area.y + area.height.saturating_sub(2);
            buf.set_string(instruction_x, instruction_y, instructions, Style::default().fg(Color::Yellow));
        }
        
        options_block.render(options_area, buf);
    }
}

impl Component for CsvOptionsDialog {
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
                    FileBrowserAction::Selected(path) => {
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
                    FileBrowserAction::Cancelled => {
                        // Cancel file browser
                        self.file_browser_mode = false;
                        self.file_browser = None;
                        return Ok(None);
                    }
                }
            }
            return Ok(None);
        }

        // First, honor config-driven actions (Global + CsvOptions)
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Ok(Some(Action::CloseCsvOptionsDialog));
                }
                Action::Enter => {
                    // Enter key: if browse button is focused, open file browser
                    if self.browse_button_selected {
                        let mut browser = FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["csv", "tsv"]),
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
                    } else {
                        return Ok(None);
                    }
                }
                Action::Up => {
                    if self.finish_button_selected {
                        // If finish button is selected, up arrow goes to browse button
                        self.finish_button_selected = false;
                        self.browse_button_selected = true;
                        self.file_path_focused = false;
                    } else if self.browse_button_selected {
                        // If browse button is selected, up arrow goes back to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    } else if self.file_path_focused {
                        // When file path is focused, up arrow moves to the last CSV option
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                        self.option_selected = 3; // Start at last CSV option (Escape Char)
                    } else if self.option_selected == 0 {
                        // If we're at the first CSV option, up arrow goes back to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                    } else {
                        // Navigate CSV options when not focused on file path
                        if self.option_selected > 0 {
                            self.option_selected = self.option_selected.saturating_sub(1);
                        }
                    }
                    return Ok(None);
                }
                Action::Down => {
                    if self.file_path_focused {
                        // When file path is focused, down arrow moves to CSV options
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                        self.option_selected = 0; // Start at first CSV option
                    } else if self.browse_button_selected {
                        // If browse button is selected, down arrow moves to finish button
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                        self.option_selected = 0; // Reset CSV option selection
                    } else if self.option_selected == 3 {
                        // If we're at the last CSV option, down arrow moves to finish button
                        self.finish_button_selected = true;
                        self.option_selected = 0; // Reset CSV option selection
                    } else {
                        // Navigate CSV options when not focused on file path
                        if self.option_selected < 3 { // 4 options: 0-3
                            self.option_selected = self.option_selected.saturating_add(1);
                        }
                    }
                    return Ok(None);
                }
                Action::Left => {
                    if self.finish_button_selected {
                        // Move from finish button back to CSV options
                        self.finish_button_selected = false;
                    } else if self.browse_button_selected {
                        // Move from browse button to file path
                        self.file_path_focused = true;
                        self.browse_button_selected = false;
                        self.option_selected = 0; // Reset CSV option selection
                    } else if self.file_path_focused {
                        // Let the TextArea handle the left arrow normally
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                    }
                    return Ok(None);
                }
                Action::Right => {
                    if self.file_path_focused {
                        // Check if cursor is at the end of the text
                        let lines = self.file_path_input.lines();
                        let cursor_pos = self.file_path_input.cursor();
                        
                        // If we're on the last line and at the end of that line
                        if cursor_pos.0 == lines.len().saturating_sub(1) && 
                           cursor_pos.1 >= lines.last().unwrap_or(&String::new()).len() {
                            // Cursor is at the end, move to browse button
                            self.file_path_focused = false;
                            self.browse_button_selected = true;
                            self.option_selected = 0; // Reset CSV option selection
                        } else {
                            // Let the TextArea handle the right arrow normally
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.update_file_path(self.file_path_input.lines().join("\n"));
                        }
                    } else if !self.file_path_focused && !self.browse_button_selected && !self.finish_button_selected {
                        // If a CSV option is selected, right arrow moves to finish button
                        self.finish_button_selected = true;
                        self.file_path_focused = false;
                        self.browse_button_selected = false;
                    }
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
                _ => {}
            }
        }

        if let Some(csv_action) = self.config.action_for_key(crate::config::Mode::CsvOptions, key) {
            match csv_action {
                Action::Tab => {
                    // Tab only moves between file path and browse button
                    if self.file_path_focused {
                        self.file_path_focused = false; // Move to browse button
                        self.browse_button_selected = true;
                        self.option_selected = 0; // Reset CSV option selection
                    } else {
                        self.file_path_focused = true; // Move back to file path
                        self.browse_button_selected = false;
                        self.option_selected = 0; // Reset CSV option selection
                    }
                    return Ok(None);
                }
                Action::OpenFileBrowser => {
                    // Ctrl+B: Open file browser
                    let mut browser = FileBrowserDialog::new(
                        Some(self.file_browser_path.clone()),
                        Some(vec!["csv", "tsv"]),
                        false,
                        FileBrowserMode::Load
                    );
                    browser.register_config_handler(self.config.clone());
                    self.file_browser = Some(browser);
                    self.file_browser_mode = true;
                    return Ok(None);
                }
                Action::Paste => {
                    // Ctrl+P: Paste clipboard text into the File Path when focused
                    if self.file_path_focused
                        && let Ok(mut clipboard) = Clipboard::new()
                        && let Ok(text) = clipboard.get_text() {
                        let first_line = text.lines().next().unwrap_or("").to_string();
                        self.set_file_path(first_line);
                    }
                    return Ok(None);
                }
                Action::CloseCsvOptionsDialog => { return Ok(Some(Action::CloseCsvOptionsDialog)); }
                _ => { /* ignore others for now */ }
            }
        }

        let result = if key.kind == crossterm::event::KeyEventKind::Press {
            match key.code {
                KeyCode::Char(c) => {
                    if self.file_path_focused {
                        // Handle text input for file path
                        use tui_textarea::Input as TuiInput;
                        let input: TuiInput = key.into();
                        self.file_path_input.input(input);
                        self.update_file_path(self.file_path_input.lines().join("\n"));
                        None
                    } else if !self.browse_button_selected && !self.file_path_focused && !self.finish_button_selected {
                        // Handle CSV option editing
                        self.update_csv_option(c);
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