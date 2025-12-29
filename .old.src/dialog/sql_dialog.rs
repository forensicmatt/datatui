//! SqlDialog: Popup dialog for entering and running SQL queries on a DataFrame
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use textwrap::wrap;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use arboard::Clipboard;
use tui_textarea::TextArea;
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlDialogMode {
    Input,
    Error(String),
    FileBrowser,
    NewDatasetInput,
}

#[derive(Debug)]
pub struct SqlDialog {
    pub textarea: TextArea<'static>,
    pub mode: SqlDialogMode,
    pub error_active: bool,
    pub file_browser: Option<FileBrowserDialog>,
    pub show_instructions: bool, // new: show instructions area (default true)
    pub create_new_dataset: bool, // whether to create a new dataset from the query
    pub dataset_name_input: String, // input for new dataset name
    pub config: Config,
}

impl Default for SqlDialog {
    fn default() -> Self { Self::new() }
}

impl SqlDialog {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        let style = Style::default().bg(Color::DarkGray);
        textarea.set_line_number_style(style);
        Self {
            textarea,
            mode: SqlDialogMode::Input,
            error_active: false,
            file_browser: None,
            show_instructions: true,
            create_new_dataset: false,
            dataset_name_input: String::new(),
            config: Config::default(),
        }
    }

    /// Set the textarea content from anything that can be referenced as a str (e.g., String, &str, etc.)
    pub fn set_textarea_content<S: AsRef<str>>(&mut self, content: S) {
        let text = content.as_ref();
        self.textarea = TextArea::default();
        self.textarea.insert_str(text);
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::SqlDialog, crate::action::Action::RunQuery),
            // (crate::config::Mode::SqlDialog, crate::action::Action::CreateNewDataset),
            (crate::config::Mode::SqlDialog, crate::action::Action::SelectAllText),
            (crate::config::Mode::SqlDialog, crate::action::Action::CopyText),
            (crate::config::Mode::SqlDialog, crate::action::Action::ClearText),
            (crate::config::Mode::SqlDialog, crate::action::Action::RestoreDataFrame),
            (crate::config::Mode::SqlDialog, crate::action::Action::OpenSqlFileBrowser),
            (crate::config::Mode::SqlDialog, crate::action::Action::PasteText),
        ])
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        // Outer container with double border
        let outer_block = Block::default()
            .title("SQL")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let wrap_width = content_area.width.saturating_sub(2) as usize;
        match &self.mode {
            SqlDialogMode::Input => {
                self.textarea.set_block(
                    Block::default()
                        .title("DataFrame Query")
                        .borders(Borders::ALL)
                );
                self.textarea.set_line_number_style(Style::default().bg(Color::DarkGray));
                ratatui::widgets::Widget::render(&self.textarea, content_area, buf);
                if self.error_active
                    && let SqlDialogMode::Error(msg) = &self.mode {
                        let error_lines = wrap(msg, wrap_width);
                        let error_y = content_area.y + self.textarea.lines().len() as u16 + 1;
                        buf.set_string(content_area.x, error_y, "Error:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                        for (i, line) in error_lines.iter().enumerate() {
                            buf.set_string(content_area.x, error_y + 1 + i as u16, line, Style::default().fg(Color::Red));
                        }
                        buf.set_string(content_area.x, error_y + 1 + error_lines.len() as u16, "Press Esc or Enter to close error", Style::default().fg(Color::Yellow));
                    }
            }
            SqlDialogMode::Error(msg) => {
                let y = content_area.y;
                buf.set_string(content_area.x, y, "Error:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                let error_lines = wrap(msg, wrap_width);
                for (i, line) in error_lines.iter().enumerate() {
                    buf.set_string(content_area.x, y + 1 + i as u16, line, Style::default().fg(Color::Red));
                }
                buf.set_string(content_area.x, y + 1 + error_lines.len() as u16, "Press Esc or Enter to close error", Style::default().fg(Color::Yellow));
            }
            SqlDialogMode::FileBrowser => {
                if let Some(browser) = &self.file_browser {
                    browser.render(inner_area, buf);
                }
            }
            SqlDialogMode::NewDatasetInput => {
                let block = Block::default()
                    .title("Enter Dataset Name")
                    .borders(Borders::ALL);
                let input_area = block.inner(content_area);
                block.render(content_area, buf);
                
                // Render input prompt and field
                buf.set_string(input_area.x, input_area.y, "Dataset name:", Style::default().fg(Color::Yellow));
                
                // Draw the full text first
                buf.set_string(input_area.x, input_area.y + 1, &self.dataset_name_input, Style::default().fg(Color::White));
                
                // Overlay block cursor at the end
                let cursor_x = input_area.x + self.dataset_name_input.chars().map(|c| c.len_utf8()).sum::<usize>() as u16;
                buf.set_string(cursor_x, input_area.y + 1, " ", self.config.style_config.cursor.block());
                
                buf.set_string(input_area.x, input_area.y + 3, "Enter: Create Dataset  Esc: Cancel", Style::default().fg(Color::Gray));
            }
        }
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions.as_str())
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
        }
        1
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        use tui_textarea::{Input as TuiInput};
        
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Get all configured actions once at the start
        let optional_global_action = self.config.action_for_key(crate::config::Mode::Global, key);
        let sql_dialog_action = self.config.action_for_key(crate::config::Mode::SqlDialog, key);

        // Handle global actions that work in all modes
        if let Some(global_action) = &optional_global_action
            && global_action == &Action::ToggleInstructions {
                self.show_instructions = !self.show_instructions;
                return None;
            }
        
        match &mut self.mode {
            SqlDialogMode::Input => {
                if self.error_active {
                    // Only allow Esc or Enter to clear error
                    if let Some(Action::Escape | Action::Enter) = &optional_global_action {
                        self.error_active = false;
                        self.mode = SqlDialogMode::Input;
                        return None;
                    }
                    // Fallback for hardcoded Esc/Enter
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            self.error_active = false;
                            self.mode = SqlDialogMode::Input;
                        }
                        _ => {}
                    }
                    return None;
                }
                
                // First, check Global actions
                if let Some(global_action) = &optional_global_action {
                    match global_action {
                        Action::Escape => {
                            return Some(Action::DialogClose);
                        }
                        Action::Enter => {
                            // Enter: insert newline in textarea
                            let input: TuiInput = key.into();
                            self.textarea.input(input);
                            return None;
                        }
                        _ => {}
                    }
                }

                // Next, check SqlDialog-specific actions
                if let Some(dialog_action) = &sql_dialog_action {
                    match dialog_action {
                        Action::SelectAllText => {
                            // select all text in the textarea
                            self.textarea.select_all();
                            return None;
                        }
                        Action::CopyText => {
                            // copy full textarea to clipboard
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let text = self.textarea.lines().join("\n");
                                let _ = clipboard.set_text(text);
                            }
                            return None;
                        }
                        Action::CreateNewDataset => {
                            // create new dataset
                            self.mode = SqlDialogMode::NewDatasetInput;
                            return None;
                        }
                        Action::RunQuery => {
                            // run query
                            return Some(Action::SqlDialogApplied(self.textarea.lines().join("\n")));
                        }
                        Action::RestoreDataFrame => {
                            return Some(Action::SqlDialogRestore);
                        }
                        Action::OpenSqlFileBrowser => {
                            let mut browser = FileBrowserDialog::new(None, Some(vec!["sql"]), false, FileBrowserMode::Load);
                            browser.register_config_handler(self.config.clone());
                            self.file_browser = Some(browser);
                            self.mode = SqlDialogMode::FileBrowser;
                            return None;
                        }
                        Action::ClearText => {
                            self.textarea = TextArea::default();
                            let style = Style::default().bg(Color::DarkGray);
                            self.textarea.set_line_number_style(style);
                            return None;
                        }
                        Action::PasteText => {
                            if let Ok(mut clipboard) = Clipboard::new()
                                && let Ok(text) = clipboard.get_text() {
                                self.textarea.insert_str(&text);
                            }
                            return None;
                        }
                        _ => {}
                    }
                }

                // For any other character input, forward to textarea
                match key.code {
                    KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete | 
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down |
                    KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown |
                    KeyCode::Tab => {
                        let input: TuiInput = key.into();
                        self.textarea.input(input);
                    }
                    _ => {}
                }
            }
            SqlDialogMode::FileBrowser => {
                if let Some(browser) = &mut self.file_browser
                    && let Some(action) = browser.handle_key_event(key) {
                        match action {
                            FileBrowserAction::Selected(path) => {
                                match std::fs::read_to_string(&path) {
                                    Ok(contents) => {
                                        let lines: Vec<String> = contents.lines().map(|l| l.to_string()).collect();
                                        self.textarea = TextArea::from(lines);
                                        let style = Style::default().bg(Color::DarkGray);
                                        self.textarea.set_line_number_style(style);
                                        self.mode = SqlDialogMode::Input;
                                        self.file_browser = None;
                                    }
                                    Err(e) => {
                                        self.set_error(format!("Failed to load file: {e}"));
                                        self.mode = SqlDialogMode::Input;
                                        self.file_browser = None;
                                    }
                                }
                            }
                            FileBrowserAction::Cancelled => {
                                self.mode = SqlDialogMode::Input;
                                self.file_browser = None;
                            }
                        }
                    }
            }
            SqlDialogMode::NewDatasetInput => {
                // Check Global actions first
                if let Some(global_action) = &optional_global_action {
                    match global_action {
                        Action::Enter => {
                            // Create new dataset if name is not empty
                            if !self.dataset_name_input.trim().is_empty() {
                                let query = self.textarea.lines().join("\n");
                                let dataset_name = self.dataset_name_input.trim().to_string();
                                // Reset state
                                self.dataset_name_input.clear();
                                self.mode = SqlDialogMode::Input;
                                return Some(Action::SqlDialogApplied(format!("NEW_DATASET:{dataset_name}:{query}")));
                            }
                            return None;
                        }
                        Action::Escape => {
                            // Cancel new dataset creation
                            self.dataset_name_input.clear();
                            self.mode = SqlDialogMode::Input;
                            return None;
                        }
                        _ => {}
                    }
                }

                // Handle character input for dataset name
                match key.code {
                    KeyCode::Enter => {
                        // Fallback for Enter
                        if !self.dataset_name_input.trim().is_empty() {
                            let query = self.textarea.lines().join("\n");
                            let dataset_name = self.dataset_name_input.trim().to_string();
                            // Reset state
                            self.dataset_name_input.clear();
                            self.mode = SqlDialogMode::Input;
                            return Some(Action::SqlDialogApplied(format!("NEW_DATASET:{dataset_name}:{query}")));
                        }
                    }
                    KeyCode::Esc => {
                        // Fallback for Esc
                        self.dataset_name_input.clear();
                        self.mode = SqlDialogMode::Input;
                    }
                    KeyCode::Backspace => {
                        self.dataset_name_input.pop();
                    }
                    KeyCode::Char(c) => {
                        self.dataset_name_input.push(c);
                    }
                    _ => {}
                }
            }
            SqlDialogMode::Error(_) => {
                // Only close error on Esc or Enter
                // Check Global actions first
                if let Some(Action::Escape | Action::Enter) = &optional_global_action {
                    self.error_active = false;
                    self.mode = SqlDialogMode::Input;
                    return None;
                }
                // Fallback for hardcoded keys
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.error_active = false;
                        self.mode = SqlDialogMode::Input;
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Set error message and switch to error mode
    pub fn set_error(&mut self, msg: String) {
        self.mode = SqlDialogMode::Error(msg);
        self.error_active = true;
    }
}

impl Component for SqlDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.config = _config;
        // Propagate to FileBrowserDialog if it exists
        if let Some(ref mut browser) = self.file_browser {
            browser.register_config_handler(self.config.clone());
        }
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        if let Some(action) = self.handle_key_event(_key) {
            return Ok(Some(action));
        }
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
} 