//! TableExportDialog: Dialog for exporting a table to a CSV file
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use crate::components::dialog_layout::split_dialog_area;
use crate::components::Component;
use crate::action::Action;
use crate::config::Config;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use tui_textarea::TextArea;
use arboard::Clipboard;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableExportMode {
    Input,
    FileBrowser,
    Error(String),
}

#[derive(Debug)]
pub struct TableExportDialog {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mode: TableExportMode,
    pub file_path: String,
    pub file_path_input: TextArea<'static>,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub export_button_selected: bool,
    pub file_browser: Option<FileBrowserDialog>,
    pub show_instructions: bool,
    pub config: Config,
}

impl TableExportDialog {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>, suggested_path: Option<String>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .title("Output File Path")
                .borders(Borders::ALL)
        );
        if let Some(path) = &suggested_path {
            textarea.insert_str(path);
        }
        Self {
            headers,
            rows,
            mode: TableExportMode::Input,
            file_path: suggested_path.unwrap_or_else(|| String::from("export.csv")),
            file_path_input: textarea,
            file_path_focused: true,
            browse_button_selected: false,
            export_button_selected: false,
            file_browser: None,
            show_instructions: true,
            config: Config::default(),
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Tab),
            (crate::config::Mode::Global, crate::action::Action::Up),
            (crate::config::Mode::Global, crate::action::Action::Down),
            (crate::config::Mode::Global, crate::action::Action::Left),
            (crate::config::Mode::Global, crate::action::Action::Right),
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::TableExport, crate::action::Action::OpenFileBrowser),
            (crate::config::Mode::TableExport, crate::action::Action::Paste),
            (crate::config::Mode::TableExport, crate::action::Action::CopyFilePath),
            (crate::config::Mode::TableExport, crate::action::Action::ExportTable),
        ])
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        match &self.mode {
            TableExportMode::FileBrowser => {
                if let Some(browser) = &self.file_browser { browser.render(area, buf); }
                return;
            }
            TableExportMode::Error(msg) => {
                let block = Block::default()
                    .title("Export Error")
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Double);
                let inner = block.inner(content_area);
                block.render(content_area, buf);
                let paragraph = Paragraph::new(msg.clone())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Red));
                paragraph.render(inner, buf);
            }
            TableExportMode::Input => {
                let block = Block::default()
                    .title("Export Table to CSV")
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Double);
                let inner = block.inner(content_area);
                block.render(content_area, buf);

                // File path line with [Browse]
                let file_path_area = Rect { x: inner.x, y: inner.y, width: inner.width, height: 3 };
                let outer = Block::default().title("Output File Path").borders(Borders::ALL);
                outer.render(file_path_area, buf);
                let inner_x = file_path_area.x.saturating_add(1);
                let inner_y = file_path_area.y.saturating_add(1);
                let inner_w = file_path_area.width.saturating_sub(2);
                let inner_h = file_path_area.height.saturating_sub(2);
                let browse_text = "[Browse]";
                let reserved = (browse_text.len() as u16).saturating_add(1);
                let input_w = inner_w.saturating_sub(reserved);
                let input_area = Rect { x: inner_x, y: inner_y, width: input_w, height: inner_h };
                let mut ta = self.file_path_input.clone();
                ta.set_block(Block::default());
                if !self.file_path_focused { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                ta.render(input_area, buf);
                let browse_x = inner_x.saturating_add(inner_w.saturating_sub(browse_text.len() as u16));
                let browse_style = if self.browse_button_selected { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default().fg(Color::Gray) };
                buf.set_string(browse_x, inner_y, browse_text, browse_style);

                // Export button bottom-right
                let export_text = "[Export]";
                let export_x = content_area.x + content_area.width.saturating_sub(export_text.len() as u16 + 2);
                let export_y = content_area.y + content_area.height.saturating_sub(2);
                let export_style = if self.export_button_selected { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default().fg(Color::Gray) };
                buf.set_string(export_x, export_y, export_text, export_style);
            }
        }

        if self.show_instructions && let Some(inst_area) = instructions_area {
            let p = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            p.render(inst_area, buf);
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        if key.kind != KeyEventKind::Press { return None; }

        // Handle Ctrl+I for instructions toggle if applicable
        if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.show_instructions = !self.show_instructions;
            return None;
        }

        match &mut self.mode {
            TableExportMode::FileBrowser => {
                if let Some(browser) = &mut self.file_browser && let Some(action) = browser.handle_key_event(key) {
                        match action {
                            FileBrowserAction::Selected(path) => {
                                self.set_file_path(path.to_string_lossy().to_string());
                                self.mode = TableExportMode::Input;
                                self.file_browser = None;
                            }
                            FileBrowserAction::Cancelled => {
                                self.mode = TableExportMode::Input;
                                self.file_browser = None;
                            }
                        }
                }
                None
            }
            TableExportMode::Error(_) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.mode = TableExportMode::Input;
                    }
                    _ => {}
                }
                None
            }
            TableExportMode::Input => {
                // First, honor config-driven Global actions
                if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                    match global_action {
                        Action::Escape => return Some(Action::DialogClose),
                        Action::Tab => {
                            if self.file_path_focused {
                                self.file_path_focused = false;
                                self.browse_button_selected = true;
                                self.export_button_selected = false;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.export_button_selected = true;
                            } else {
                                self.export_button_selected = false;
                                self.file_path_focused = true;
                            }
                            return None;
                        }
                        Action::Up => {
                            if self.export_button_selected {
                                self.export_button_selected = false;
                                self.browse_button_selected = true;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.file_path_focused = true;
                            }
                            return None;
                        }
                        Action::Down => {
                            if self.file_path_focused {
                                self.file_path_focused = false;
                                self.browse_button_selected = true;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.export_button_selected = true;
                            }
                            return None;
                        }
                        Action::Left => {
                            if self.export_button_selected {
                                self.export_button_selected = false;
                                self.browse_button_selected = true;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.file_path_focused = true;
                            } else if self.file_path_focused {
                                use tui_textarea::Input as TuiInput;
                                let input: TuiInput = key.into();
                                self.file_path_input.input(input);
                                self.sync_file_path_from_input();
                            }
                            return None;
                        }
                        Action::Right => {
                            if self.file_path_focused {
                                // forward to textarea
                                use tui_textarea::Input as TuiInput;
                                let input: TuiInput = key.into();
                                self.file_path_input.input(input);
                                self.sync_file_path_from_input();
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.export_button_selected = true;
                            }
                            return None;
                        }
                        Action::Enter => {
                            if self.browse_button_selected {
                                self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv"]), false, FileBrowserMode::Save));
                                self.mode = TableExportMode::FileBrowser;
                                return None;
                            }
                            if self.export_button_selected || self.file_path_focused {
                                match self.export_to_csv() {
                                    Ok(_) => { return Some(Action::DialogClose); }
                                    Err(e) => { self.mode = TableExportMode::Error(format!("Failed to export: {e}")); return None; }
                                }
                            }
                            return None;
                        }
                        _ => {}
                    }
                }

                // Next, check for dialog-specific actions
                if let Some(dialog_action) = self.config.action_for_key(crate::config::Mode::TableExport, key) {
                    match dialog_action {
                        Action::OpenFileBrowser => {
                            self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv"]), false, FileBrowserMode::Save));
                            self.mode = TableExportMode::FileBrowser;
                            return None;
                        }
                        Action::Paste => {
                            if self.file_path_focused && let Ok(mut clipboard) = Clipboard::new()
                                && let Ok(text) = clipboard.get_text() {
                                    let first_line = text.lines().next().unwrap_or("").to_string();
                                    self.set_file_path(first_line);
                                }
                            return None;
                        }
                        Action::CopyFilePath => {
                            if let Ok(mut clipboard) = Clipboard::new() { let _ = clipboard.set_text(self.file_path.clone()); }
                            return None;
                        }
                        Action::ExportTable => {
                            match self.export_to_csv() {
                                Ok(_) => { return Some(Action::DialogClose); }
                                Err(e) => { self.mode = TableExportMode::Error(format!("Failed to export: {e}")); return None; }
                            }
                        }
                        _ => {}
                    }
                }

                // Fallback for character input or other unhandled keys
                match key.code {
                    KeyCode::Backspace => {
                        if self.file_path_focused {
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.sync_file_path_from_input();
                        }
                        None
                    }
                    KeyCode::Char(_c) => {
                        if self.file_path_focused {
                            use tui_textarea::Input as TuiInput;
                            let input: TuiInput = key.into();
                            self.file_path_input.input(input);
                            self.sync_file_path_from_input();
                        }
                        None
                    }
                    _ => None,
                }
            }
        }
    }

    fn set_file_path(&mut self, path: String) {
        self.file_path = path.clone();
        self.file_path_input = TextArea::from(vec![path]);
        self.file_path_input.set_block(Block::default().title("Output File Path").borders(Borders::ALL));
    }

    fn sync_file_path_from_input(&mut self) {
        self.file_path = self.file_path_input.lines().join("\n");
    }

    fn export_to_csv(&self) -> Result<()> {
        let path = PathBuf::from(&self.file_path);
        if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() { let _ = create_dir_all(parent); }
        let mut file = File::create(&path)?;

        // write header
        writeln!(file, "{}", self.headers.iter().map(|h| csv_escape(h)).collect::<Vec<_>>().join(","))?;
        for row in &self.rows {
            writeln!(file, "{}", row.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(","))?;
        }
        Ok(())
    }
}

fn csv_escape(s: &str) -> String {
    let needs_quotes = s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');
    if needs_quotes {
        let mut out = String::from("\"");
        for ch in s.chars() { if ch == '"' { out.push('"'); } out.push(ch); }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

impl Component for TableExportDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { 
        self.config = _config; 
        Ok(()) 
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}


