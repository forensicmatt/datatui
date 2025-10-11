//! DataExportDialog: Export selected datasets to Text, Excel, JSONL, or Parquet

use ratatui::prelude::*;
use tracing::error;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use crate::components::dialog_layout::split_dialog_area;
use crate::components::Component;
use crate::config::Config;
use crate::action::Action;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, KeyCode};
use tui_textarea::TextArea;
use arboard::Clipboard;
use std::fs::create_dir_all;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataExportMode {
    Input,
    FileBrowser,
    Error(String),
    Success(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataExportFormat {
    Text,
    Excel,
    Jsonl,
    Parquet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
}

#[derive(Debug)]
pub struct DataExportDialog {
    // Datasets available for selection: (id, display name, selected)
    pub datasets: Vec<DatasetChoice>,
    pub selecting_datasets: bool,
    pub selected_dataset_index: usize,
    pub mode: DataExportMode,
    pub file_path: String,
    pub file_path_input: TextArea<'static>,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub export_button_selected: bool,
    pub format_index: usize, // 0 Text, 1 Excel, 2 JSONL, 3 Parquet
    pub file_browser: Option<FileBrowserDialog>,
    pub show_instructions: bool,
    pub config: Config,
    // CSV options for Text export
    pub csv_delimiter: char,
    pub csv_quote_char: Option<char>,
    pub csv_escape_char: Option<char>,
    pub csv_include_header: bool,
    pub csv_encoding: CsvEncoding,
    // Options navigation state
    pub options_active: bool,
    pub option_selected: usize,
}

#[derive(Debug, Clone)]
pub struct DatasetChoice {
    pub id: String,
    pub name: String,
    pub selected: bool,
}

impl DataExportDialog {
    /// Create a new dialog from dataset entries (id, display name)
    pub fn new(dataset_entries: Vec<(String, String)>, suggested_path: Option<String>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .title("Output File Path")
                .borders(Borders::ALL)
        );
        if let Some(path) = &suggested_path { textarea.insert_str(path); }
        let datasets: Vec<DatasetChoice> = dataset_entries
            .into_iter()
            .map(|(id, name)| DatasetChoice { id, name, selected: false })
            .collect();
        Self {
            datasets,
            selecting_datasets: true,
            selected_dataset_index: 0,
            mode: DataExportMode::Input,
            file_path: suggested_path.unwrap_or_else(|| String::from("export.csv")),
            file_path_input: textarea,
            file_path_focused: true,
            browse_button_selected: false,
            export_button_selected: false,
            format_index: 0,
            file_browser: None,
            show_instructions: true,
            config: Config::default(),
            csv_delimiter: ',',
            csv_quote_char: Some('"'),
            csv_escape_char: None,
            csv_include_header: true,
            csv_encoding: CsvEncoding::Utf8,
            options_active: false,
            option_selected: 0,
        }
    }

    fn current_format(&self) -> DataExportFormat {
        match self.format_index { 0 => DataExportFormat::Text, 1 => DataExportFormat::Excel, 2 => DataExportFormat::Jsonl, _ => DataExportFormat::Parquet }
    }

    fn build_instructions_from_config(&self) -> String {
        let mut s = self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Tab),
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::TableExport, crate::action::Action::OpenFileBrowser),
            (crate::config::Mode::TableExport, crate::action::Action::Paste),
            (crate::config::Mode::TableExport, crate::action::Action::CopyFilePath),
            (crate::config::Mode::TableExport, crate::action::Action::ExportTable),
        ]);
        if !s.is_empty() { s.push_str("  "); }
        s.push_str("Space: Toggle dataset  Up/Down: Navigate datasets");
        s
    }

    fn adjust_option(&mut self, delta: i32) {
        if !matches!(self.current_format(), DataExportFormat::Text) { return; }
        match self.option_selected {
            0 => { // delimiter
                let seq = [',', '\t', ';', '|'];
                let pos = seq.iter().position(|&c| c == self.csv_delimiter).unwrap_or(0) as i32;
                let new = (pos + delta).rem_euclid(seq.len() as i32) as usize;
                self.csv_delimiter = seq[new];
            }
            1 => { // quote char
                // cycle: '"' -> '\'' -> None -> '"'
                let state = self.csv_quote_char;
                self.csv_quote_char = match (state, delta.signum()) {
                    (Some('"'), 1) | (Some('"'), -1) => Some('\''),
                    (Some('\''), 1) | (Some('\''), -1) => None,
                    (None, 1) | (None, -1) => Some('"'),
                    _ => Some('"'),
                };
            }
            2 => { // escape char
                // set/remove with char; arrows toggle between None and '\\'
                if self.csv_escape_char.is_some() { self.csv_escape_char = None; } else { self.csv_escape_char = Some('\\'); }
            }
            3 => { // header
                self.csv_include_header = !self.csv_include_header;
            }
            4 => { // encoding
                self.csv_encoding = match (self.csv_encoding, delta.signum()) {
                    (CsvEncoding::Utf8, 1) | (CsvEncoding::Utf16Le, -1) => CsvEncoding::Utf8Bom,
                    (CsvEncoding::Utf8Bom, 1) | (CsvEncoding::Utf8, -1) => CsvEncoding::Utf16Le,
                    (CsvEncoding::Utf16Le, 1) | (CsvEncoding::Utf8Bom, -1) => CsvEncoding::Utf8,
                    (other, _) => other,
                };
            }
            _ => {}
        }
    }

    fn set_option_char(&mut self, c: Option<char>) {
        if !matches!(self.current_format(), DataExportFormat::Text) { return; }
        match self.option_selected {
            0 => { if let Some(ch) = c { self.csv_delimiter = ch; } }
            1 => { self.csv_quote_char = c; }
            2 => { self.csv_escape_char = c; }
            _ => {}
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(area, self.show_instructions, Some(instructions.as_str()));
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        match &self.mode {
            DataExportMode::FileBrowser => {
                if let Some(browser) = &self.file_browser {
                    browser.render(area, buf);
                }
                return;
            }
            DataExportMode::Error(msg) => {
                error!("{msg}");
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
            DataExportMode::Success(msg) => {
                let block = Block::default()
                    .title("Export Succeeded")
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Double);
                let inner = block.inner(content_area);
                block.render(content_area, buf);
                let paragraph = Paragraph::new(msg.clone())
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green));
                paragraph.render(inner, buf);
            }
            DataExportMode::Input => {
                let block = Block::default()
                    .title("Export Dataset")
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

                // Format selection line
                let fmt_y = inner.y + 4;
                let formats = ["Text", "Excel", "JSONL", "Parquet"]; let fmt = formats[self.format_index.min(formats.len()-1)];
                buf.set_string(inner.x + 1, fmt_y, format!("Format: {fmt} (Ctrl+F to cycle)"), Style::default());

                // Dataset selection list
                let mut list_y = fmt_y + 2;
                buf.set_string(inner.x + 1, list_y, "Datasets:", Style::default().add_modifier(Modifier::UNDERLINED));
                list_y = list_y.saturating_add(1);
                for (i, ds) in self.datasets.iter().enumerate() {
                    let checkbox = if ds.selected { "[x]" } else { "[ ]" };
                    let text = format!("{checkbox} {}", ds.name);
                    let style = if self.selecting_datasets && self.selected_dataset_index == i { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default() };
                    buf.set_string(inner.x + 2, list_y + i as u16, text.as_str(), style);
                }

                // CSV options (only when Text format is selected)
                if matches!(self.current_format(), DataExportFormat::Text) {
                    let base_y = list_y + self.datasets.len() as u16 + 1;
                    let enc = match self.csv_encoding { CsvEncoding::Utf8 => "UTF-8", CsvEncoding::Utf8Bom => "UTF-8 BOM", CsvEncoding::Utf16Le => "UTF-16LE" };
                    let quote_str = self.csv_quote_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string());
                    let escape_str = self.csv_escape_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string());
                    let header_str = if self.csv_include_header { "Yes" } else { "No" };

                    let options = [
                        format!("Delimiter: '{}'", self.csv_delimiter),
                        format!("Quote Char: {}", quote_str),
                        format!("Escape Char: {}", escape_str),
                        format!("Has Header: {}", header_str),
                        format!("Encoding: {}", enc),
                    ];
                    for (i, line) in options.iter().enumerate() {
                        let style = if self.options_active && self.option_selected == i { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default() };
                        buf.set_string(inner.x + 1, base_y + i as u16, line, style);
                    }
                }

                // Export button bottom-right
                let export_text = "[Export]";
                let export_x = content_area.x + content_area.width.saturating_sub(export_text.len() as u16 + 2);
                let export_y = content_area.y + content_area.height.saturating_sub(2);
                let export_style = if self.export_button_selected { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default().fg(Color::Gray) };
                buf.set_string(export_x, export_y, export_text, export_style);
            }
        }

        if self.show_instructions
            && let Some(inst_area) = instructions_area {
            let p = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            p.render(inst_area, buf);
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press { return None; }

        // Resolve actions from config
        let optional_global_action = self.config.action_for_key(crate::config::Mode::Global, key);
        let export_action = self.config.action_for_key(crate::config::Mode::TableExport, key);

        match &mut self.mode {
            DataExportMode::FileBrowser => {
                if let Some(browser) = &mut self.file_browser && let Some(action) = browser.handle_key_event(key) {
                    browser.register_config_handler(self.config.clone());
                    match action {
                        FileBrowserAction::Selected(path) => {
                            self.set_file_path(path.to_string_lossy().to_string());
                            self.mode = DataExportMode::Input;
                            self.file_browser = None;
                        }
                        FileBrowserAction::Cancelled => {
                            self.mode = DataExportMode::Input;
                            self.file_browser = None;
                        }
                    }
                }
                None
            }
            DataExportMode::Error(_) | DataExportMode::Success(_) => {
                if let Some(Action::Escape | Action::Enter) = &optional_global_action {
                    return Some(Action::DialogClose);
                }
                None
            }
            DataExportMode::Input => {
                // Space toggles dataset selection when focused
                if key.code == KeyCode::Char(' ') && self.selecting_datasets && !self.datasets.is_empty() {
                    let idx = self.selected_dataset_index.min(self.datasets.len()-1);
                    let cur = self.datasets[idx].selected;
                    self.datasets[idx].selected = !cur;
                    return None;
                }

                // Global navigation/actions first
                if let Some(action) = optional_global_action {
                    match action {
                        Action::Tab => {
                        if self.file_path_focused { self.file_path_focused = false; self.selecting_datasets = true; self.browse_button_selected = false; self.export_button_selected = false; }
                        else if self.selecting_datasets { self.selecting_datasets = false; self.browse_button_selected = true; self.export_button_selected = false; }
                        else if self.browse_button_selected { self.browse_button_selected = false; self.export_button_selected = true; }
                        else { self.export_button_selected = false; self.file_path_focused = true; }
                        None
                        }
                        Action::Right => {
                            if self.options_active {
                                self.adjust_option(1);
                                None
                            } else if self.file_path_focused {
                                use tui_textarea::Input as TuiInput; let input: TuiInput = key.into();
                                self.file_path_input.input(input); self.sync_file_path_from_input();
                                None
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false; self.export_button_selected = true; None
                            } else { None }
                        }
                        Action::Left => {
                            if self.options_active {
                                self.adjust_option(-1);
                                None
                            } else if self.export_button_selected {
                                self.export_button_selected = false; self.browse_button_selected = true; None
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false; self.file_path_focused = true; None
                            } else if self.file_path_focused {
                                use tui_textarea::Input as TuiInput; let input: TuiInput = key.into(); self.file_path_input.input(input); self.sync_file_path_from_input(); None
                            } else { None }
                        }
                        Action::Up => {
                        if self.selecting_datasets {
                            if self.selected_dataset_index > 0 { self.selected_dataset_index -= 1; }
                        } else if self.export_button_selected {
                            // Move up into options if Text format; otherwise to browse button
                            if matches!(self.current_format(), DataExportFormat::Text) {
                                self.export_button_selected = false;
                                self.options_active = true;
                                self.option_selected = 4; // last option
                            } else {
                                self.export_button_selected = false;
                                self.browse_button_selected = true;
                            }
                        } else if self.options_active {
                            if self.option_selected == 0 {
                                self.options_active = false;
                                self.file_path_focused = true;
                            } else {
                                self.option_selected = self.option_selected.saturating_sub(1);
                            }
                        } else if self.browse_button_selected {
                            self.browse_button_selected = false;
                            self.file_path_focused = true;
                        }
                        None
                        }
                        Action::Down => {
                        if self.file_path_focused {
                            // From file path, go into options if Text format; otherwise to browse
                            if matches!(self.current_format(), DataExportFormat::Text) {
                                self.file_path_focused = false;
                                self.selecting_datasets = true;
                            } else {
                                self.file_path_focused = false;
                                self.browse_button_selected = true;
                            }
                        } else if self.options_active {
                            if self.option_selected >= 4 {
                                self.options_active = false;
                                self.export_button_selected = true;
                            } else {
                                self.option_selected = self.option_selected.saturating_add(1).min(4);
                            }
                        } else if self.selecting_datasets {
                            if self.selected_dataset_index + 1 < self.datasets.len() { self.selected_dataset_index += 1; }
                        } else if self.browse_button_selected {
                            self.browse_button_selected = false;
                            self.export_button_selected = true;
                        }
                        None
                        }
                        Action::Enter => {
                        if self.browse_button_selected {
                            let mut dialog_file_browser = FileBrowserDialog::new(
                                None, 
                                Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), 
                                false, FileBrowserMode::Save
                            );
                            dialog_file_browser.register_config_handler(self.config.clone());
                            self.file_browser = Some(dialog_file_browser);
                            self.mode = DataExportMode::FileBrowser; return None;
                        }
                        if self.export_button_selected || self.file_path_focused {
                            let ids: Vec<String> = self.datasets.iter().filter(|d| d.selected).map(|d| d.id.clone()).collect();
                            return Some(Action::DataExportRequestedMulti {
                                dataset_ids: ids,
                                file_path: self.file_path.clone(),
                                format_index: self.format_index,
                            });
                        }
                        None
                        }
                        Action::Backspace => { if self.file_path_focused { use tui_textarea::Input as TuiInput; let input: TuiInput = key.into(); self.file_path_input.input(input); self.sync_file_path_from_input(); } else if self.options_active { self.set_option_char(None); } None }
                        Action::Paste => { if self.file_path_focused && let Ok(mut clipboard) = Clipboard::new() && let Ok(text) = clipboard.get_text() { let first_line = text.lines().next().unwrap_or("").to_string(); self.set_file_path(first_line); } None }
                        Action::CopyFilePath => { if let Ok(mut clipboard) = Clipboard::new() { let _ = clipboard.set_text(self.file_path.clone()); } None }
                        Action::Escape => { return Some(Action::DialogClose); }
                        _ => None,
                    }
                } else if let Some(action) = export_action {
                    match action {
                        Action::OpenFileBrowser => { self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), false, FileBrowserMode::Save)); self.mode = DataExportMode::FileBrowser; None }
                        Action::ExportTable => {
                            let ids: Vec<String> = self.datasets.iter().filter(|d| d.selected).map(|d| d.id.clone()).collect();
                            return Some(Action::DataExportRequestedMulti {
                                dataset_ids: ids,
                                file_path: self.file_path.clone(),
                                format_index: self.format_index,
                            });
                        }
                        _ => None,
                    }
                } else {
                    None
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

    fn ensure_parent(&self, path: &std::path::Path) -> color_eyre::Result<()> { if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() { let _ = create_dir_all(parent); } Ok(()) }
}

impl Component for DataExportDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.config = _config;
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}


