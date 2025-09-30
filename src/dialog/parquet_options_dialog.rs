//! ParquetOptionsDialog: Dialog for configuring Parquet import options (minimal)

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
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode};
use tui_textarea::TextArea;
use arboard::Clipboard;

/// Parquet import options (currently empty/minimal)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ParquetImportOptions {}

/// ParquetOptionsDialog: Dialog for selecting a Parquet file to import
#[derive(Debug, Serialize, Deserialize)]
pub struct ParquetOptionsDialog {
    pub file_path: String,
    pub parquet_options: ParquetImportOptions,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub finish_button_selected: bool,
    pub file_browser_mode: bool, // Whether the file browser is currently active
    pub file_browser_path: PathBuf,
    pub show_instructions: bool, // Whether to show instructions area
    #[serde(skip)]
    pub file_path_input: TextArea<'static>,
    #[serde(skip)]
    pub file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    pub config: Config,
}

impl ParquetOptionsDialog {
    /// Create a new ParquetOptionsDialog
    pub fn new(file_path: String, parquet_options: ParquetImportOptions) -> Self {
        let mut file_path_input = TextArea::default();
        file_path_input.set_block(
            Block::default()
                .title("File Path")
                .borders(Borders::ALL)
        );
        file_path_input.insert_str(&file_path);
        
        Self {
            file_path,
            parquet_options,
            file_path_focused: true,
            browse_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            show_instructions: true,
            file_path_input,
            file_browser: None,
            config: Config::default(),
        }
    }

    /// Create a DataImportConfig from the current dialog state
    pub fn create_import_config(&self) -> crate::data_import_types::DataImportConfig {
        use crate::data_import_types::DataImportConfig;
        let file_path = PathBuf::from(&self.file_path);
        DataImportConfig::parquet(file_path, self.parquet_options.clone())
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

        // Handle Global actions
        if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
            for (key_seq, action) in global_bindings {
                match action {
                    crate::action::Action::Escape => {
                        segments.push(format!("{}: Cancel", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Enter => {
                        segments.push(format!("{}: Confirm", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Tab => {
                        segments.push(format!("{}: Tab Fields", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Up => {
                        segments.push(format!("{}: Navigate Up", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Down => {
                        segments.push(format!("{}: Navigate Down", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Left => {
                        segments.push(format!("{}: Navigate Left", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Right => {
                        segments.push(format!("{}: Navigate Right", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::ToggleInstructions => {
                        segments.push(format!("{}: Toggle Instructions", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Handle ParquetOptionsDialog-specific actions  
        if let Some(dialog_bindings) = self.config.keybindings.0.get(&crate::config::Mode::ParquetOptionsDialog) {
            for (key_seq, action) in dialog_bindings {
                match action {
                    crate::action::Action::OpenParquetFileBrowser => {
                        segments.push(format!("{}: Browse Files", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::PasteParquetFilePath => {
                        segments.push(format!("{}: Paste Path", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Join segments
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{seg}");
        }
        out
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
            .title("Parquet Import Options")
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
                Constraint::Min(0),    // Options/content (empty currently)
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

impl Component for ParquetOptionsDialog {
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
        let parquet_dialog_action = self.config.action_for_key(crate::config::Mode::ParquetOptionsDialog, key);

        // First, honor config-driven Global actions
        if let Some(global_action) = &global_action {
            match global_action {
                Action::Escape => {
                    return Ok(Some(Action::CloseParquetOptionsDialog));
                }
                Action::ToggleInstructions => {
                    self.show_instructions = !self.show_instructions;
                    return Ok(None);
                }
                Action::Tab => {
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
                    return Ok(None);
                }
                Action::Left => {
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
                    return Ok(None);
                }
                Action::Up => {
                    if self.finish_button_selected {
                        self.finish_button_selected = false;
                        self.browse_button_selected = true;
                    } else if self.browse_button_selected {
                        self.browse_button_selected = false;
                        self.file_path_focused = true;
                    }
                    return Ok(None);
                }
                Action::Down => {
                    if self.file_path_focused {
                        self.file_path_focused = false;
                        self.browse_button_selected = true;
                    } else if self.browse_button_selected {
                        self.browse_button_selected = false;
                        self.finish_button_selected = true;
                    }
                    return Ok(None);
                }
                Action::Enter => {
                    if self.browse_button_selected {
                        // Open file browser
                        let mut browser = FileBrowserDialog::new(
                            Some(self.file_browser_path.clone()),
                            Some(vec!["parquet"]),
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
                    }
                    return Ok(None);
                }
                Action::Backspace => {
                    if self.file_path_focused {
                        // Handle backspace to delete characters in file path
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

        // Next, check for ParquetOptionsDialog-specific actions
        if let Some(dialog_action) = &parquet_dialog_action {
            match dialog_action {
                Action::OpenParquetFileBrowser => {
                    // Ctrl+B: Open file browser
                    let mut browser = FileBrowserDialog::new(
                        Some(self.file_browser_path.clone()),
                        Some(vec!["parquet"]),
                        false,
                        FileBrowserMode::Load
                    );
                    browser.register_config_handler(self.config.clone());
                    self.file_browser = Some(browser);
                    self.file_browser_mode = true;
                    return Ok(None);
                }
                Action::PasteParquetFilePath => {
                    // Ctrl+P: Paste clipboard text into the File Path when focused
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
                _ => {}
            }
        }

        // Fallback for character input or other unhandled keys
        if let KeyCode::Char(_c) = key.code {
            if self.file_path_focused {
                // Handle text input for file path
                use tui_textarea::Input as TuiInput;
                let input: TuiInput = key.into();
                self.file_path_input.input(input);
                self.update_file_path(self.file_path_input.lines().join("\n"));
                return Ok(None);
            }
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


