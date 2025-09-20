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
        }
    }

    /// Set the textarea content from anything that can be referenced as a str (e.g., String, &str, etc.)
    pub fn set_textarea_content<S: AsRef<str>>(&mut self, content: S) {
        let text = content.as_ref();
        self.textarea = TextArea::default();
        self.textarea.insert_str(text);
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        let instructions = "Ctrl+Enter:Run  Ctrl+Shift+Enter:New Dataset  Ctrl+a:SelectAll  Ctrl+c:Copy  Ctrl+l:Clear  Ctrl+r:Restore  Ctrl+o:OpenFile  Ctrl+p:Paste  Esc:Cancel";
        // Outer container with double border
        let outer_block = Block::default()
            .title("SQL")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, Some(instructions));
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
                buf.set_string(input_area.x, input_area.y + 1, &self.dataset_name_input, Style::default().fg(Color::White));
                
                // Render cursor
                let cursor_x = input_area.x + self.dataset_name_input.len() as u16;
                buf.set_string(cursor_x, input_area.y + 1, "_", Style::default().fg(Color::Yellow));
                
                buf.set_string(input_area.x, input_area.y + 3, "Enter: Create Dataset  Esc: Cancel", Style::default().fg(Color::Gray));
            }
        }
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
        }
        1
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        use tui_textarea::{Input as TuiInput};
        
        // Handle Ctrl+I to toggle instructions
        if key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.show_instructions = !self.show_instructions;
            return None;
        }
        
        match &mut self.mode {
            SqlDialogMode::Input => {
                if self.error_active {
                    // Only allow Esc or Enter to clear error
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter => {
                                self.error_active = false;
                                self.mode = SqlDialogMode::Input;
                            }
                            _ => {}
                        }
                    }
                    return None;
                }
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+a: select all text in the textarea
                            self.textarea.select_all();
                            return None;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+c: copy full textarea to clipboard
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let text = self.textarea.lines().join("\n");
                                let _ = clipboard.set_text(text);
                            }
                            return None;
                        }
                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT) => {
                            // Ctrl+Shift+Enter: create new dataset
                            self.mode = SqlDialogMode::NewDatasetInput;
                            return None;
                        }
                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+Enter: run query
                            return Some(Action::SqlDialogApplied(self.textarea.lines().join("\n")));
                        }
                        KeyCode::Enter => {
                            // Enter: insert newline in textarea
                            let input: TuiInput = key.into();
                            self.textarea.input(input);
                        }
                        KeyCode::Esc => {
                            return Some(Action::DialogClose);
                        }
                        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Some(Action::SqlDialogRestore);
                        }
                        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["sql"]), false, FileBrowserMode::Load));
                            self.mode = SqlDialogMode::FileBrowser;
                            return None;
                        }
                        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.textarea = TextArea::default();
                            let style = Style::default().bg(Color::DarkGray);
                            self.textarea.set_line_number_style(style);
                            return None;
                        }
                        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Ok(mut clipboard) = Clipboard::new()
                                && let Ok(text) = clipboard.get_text() {
                                self.textarea.insert_str(&text);
                            }
                            return None;
                        }
                        _ => {
                            // Forward to textarea
                            let input: TuiInput = key.into();
                            self.textarea.input(input);
                        }
                    }
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
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Enter => {
                            // Create new dataset if name is not empty
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
                            // Cancel new dataset creation
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
            }
            SqlDialogMode::Error(_) => {
                // Only close error on Esc or Enter
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            self.error_active = false;
                            self.mode = SqlDialogMode::Input;
                        }
                        _ => {}
                    }
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
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
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