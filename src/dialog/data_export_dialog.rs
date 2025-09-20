//! DataExportDialog: Export current dataset to Text, Excel, JSONL, or Parquet

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use crate::components::dialog_layout::split_dialog_area;
use crate::components::Component;
use crate::action::Action;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode, KeyModifiers, MouseEvent};
use tui_textarea::TextArea;
use arboard::Clipboard;
use std::fs::{File, create_dir_all};
use std::io::{Write, BufWriter};
use std::path::PathBuf;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use polars::prelude::*;

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

#[derive(Debug)]
pub struct DataExportDialog {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>, // rows as strings, already formatted for text/CSV-like outputs
    pub mode: DataExportMode,
    pub file_path: String,
    pub file_path_input: TextArea<'static>,
    pub file_path_focused: bool,
    pub browse_button_selected: bool,
    pub export_button_selected: bool,
    pub format_index: usize, // 0 Text, 1 Excel, 2 JSONL, 3 Parquet
    pub file_browser: Option<FileBrowserDialog>,
    pub show_instructions: bool,
}

impl DataExportDialog {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>, suggested_path: Option<String>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .title("Output File Path")
                .borders(Borders::ALL)
        );
        if let Some(path) = &suggested_path { textarea.insert_str(path); }
        Self {
            headers,
            rows,
            mode: DataExportMode::Input,
            file_path: suggested_path.unwrap_or_else(|| String::from("export.csv")),
            file_path_input: textarea,
            file_path_focused: true,
            browse_button_selected: false,
            export_button_selected: false,
            format_index: 0,
            file_browser: None,
            show_instructions: true,
        }
    }

    fn current_format(&self) -> DataExportFormat {
        match self.format_index { 0 => DataExportFormat::Text, 1 => DataExportFormat::Excel, 2 => DataExportFormat::Jsonl, _ => DataExportFormat::Parquet }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = "Tab: Switch  ←/→: Move  Ctrl+p:Paste  Ctrl+c:Copy  Ctrl+b:Browse  Enter:[Browse/Export]  Esc:Close  Ctrl+f:Format";
        let layout = split_dialog_area(area, self.show_instructions, Some(instructions));
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        match &self.mode {
            DataExportMode::FileBrowser => {
                if let Some(browser) = &self.file_browser { browser.render(area, buf); }
                return;
            }
            DataExportMode::Error(msg) => {
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

        // Toggle instructions
        if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.show_instructions = !self.show_instructions;
            return None;
        }

        match &mut self.mode {
            DataExportMode::FileBrowser => {
                if let Some(browser) = &mut self.file_browser && let Some(action) = browser.handle_key_event(key) {
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
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        return Some(Action::DialogClose);
                    }
                    _ => {}
                }
                None
            }
            DataExportMode::Input => {
                match key.code {
                    KeyCode::Tab => {
                        if self.file_path_focused { self.file_path_focused = false; self.browse_button_selected = true; self.export_button_selected = false; }
                        else if self.browse_button_selected { self.browse_button_selected = false; self.export_button_selected = true; }
                        else { self.export_button_selected = false; self.file_path_focused = true; }
                        None
                    }
                    KeyCode::Right => {
                        if self.file_path_focused {
                            use tui_textarea::Input as TuiInput; let input: TuiInput = key.into();
                            self.file_path_input.input(input); self.sync_file_path_from_input();
                        } else if self.browse_button_selected { self.browse_button_selected = false; self.export_button_selected = true; }
                        None
                    }
                    KeyCode::Left => {
                        if self.export_button_selected { self.export_button_selected = false; self.browse_button_selected = true; }
                        else if self.browse_button_selected { self.browse_button_selected = false; self.file_path_focused = true; }
                        else if self.file_path_focused { use tui_textarea::Input as TuiInput; let input: TuiInput = key.into(); self.file_path_input.input(input); self.sync_file_path_from_input(); }
                        None
                    }
                    KeyCode::Up => { if self.export_button_selected { self.export_button_selected = false; self.browse_button_selected = true; } else if self.browse_button_selected { self.browse_button_selected = false; self.file_path_focused = true; } None }
                    KeyCode::Down => { if self.file_path_focused { self.file_path_focused = false; self.browse_button_selected = true; } else if self.browse_button_selected { self.browse_button_selected = false; self.export_button_selected = true; } None }
                    KeyCode::Enter => {
                        if self.browse_button_selected {
                            self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), false, FileBrowserMode::Save));
                            self.mode = DataExportMode::FileBrowser; return None;
                        }
                        if self.export_button_selected || self.file_path_focused {
                            match self.export_current() {
                                Ok(msg) => { self.mode = DataExportMode::Success(msg); }
                                Err(e) => { self.mode = DataExportMode::Error(format!("Failed to export: {e}")); }
                            }
                        }
                        None
                    }
                    KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), false, FileBrowserMode::Save));
                        self.mode = DataExportMode::FileBrowser; None
                    }
                    KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.format_index = (self.format_index + 1) % 4; None
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if self.file_path_focused && let Ok(mut clipboard) = Clipboard::new() && let Ok(text) = clipboard.get_text() { let first_line = text.lines().next().unwrap_or("").to_string(); self.set_file_path(first_line); }
                        None
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => { if let Ok(mut clipboard) = Clipboard::new() { let _ = clipboard.set_text(self.file_path.clone()); } None }
                    KeyCode::Backspace => { if self.file_path_focused { use tui_textarea::Input as TuiInput; let input: TuiInput = key.into(); self.file_path_input.input(input); self.sync_file_path_from_input(); } None }
                    KeyCode::Char(_c) => { if self.file_path_focused { use tui_textarea::Input as TuiInput; let input: TuiInput = key.into(); self.file_path_input.input(input); self.sync_file_path_from_input(); } None }
                    KeyCode::Esc => { Some(Action::DialogClose) }
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

    fn export_current(&self) -> color_eyre::Result<String> {
        match self.current_format() {
            DataExportFormat::Text => self.export_text(),
            DataExportFormat::Excel => self.export_excel(),
            DataExportFormat::Jsonl => self.export_jsonl(),
            DataExportFormat::Parquet => self.export_parquet(),
        }
    }

    fn ensure_parent(&self, path: &std::path::Path) -> color_eyre::Result<()> { if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() { let _ = create_dir_all(parent); } Ok(()) }

    fn export_text(&self) -> color_eyre::Result<String> {
        let path = PathBuf::from(&self.file_path);
        self.ensure_parent(path.as_path())?;
        let mut file = BufWriter::new(File::create(&path)?);
        // write header
        writeln!(file, "{}", self.headers.join(","))?;
        for row in &self.rows { writeln!(file, "{}", row.join(","))?; }
        Ok(format!("Exported Text to {}", path.display()))
    }

    fn export_jsonl(&self) -> color_eyre::Result<String> {
        use serde_json::json;
        let path = PathBuf::from(&self.file_path);
        self.ensure_parent(path.as_path())?;
        let mut file = BufWriter::new(File::create(&path)?);
        for row in &self.rows {
            let mut obj = serde_json::Map::new();
            for (h, v) in self.headers.iter().zip(row.iter()) { obj.insert(h.clone(), json!(v)); }
            let line = serde_json::to_string(&obj)?; writeln!(file, "{line}")?;
        }
        Ok(format!("Exported JSONL to {}", path.display()))
    }

    fn export_parquet(&self) -> color_eyre::Result<String> {
        // Build a string-typed DataFrame and write parquet
        let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(self.headers.len());
        for (col_idx, name) in self.headers.iter().enumerate() {
            let mut col_vals: Vec<String> = Vec::with_capacity(self.rows.len());
            for row in &self.rows { col_vals.push(row.get(col_idx).cloned().unwrap_or_default()); }
            let s = polars::prelude::Series::new(name.as_str().into(), col_vals);
            cols.push(s.into());
        }
        let df = polars::prelude::DataFrame::new(cols)?;
        let path = PathBuf::from(&self.file_path);
        self.ensure_parent(path.as_path())?;
        let file = File::create(&path)?;
        let writer = polars::prelude::ParquetWriter::new(file);
        let mut df_copy = df;
        let _ = writer.finish(&mut df_copy)?;
        Ok(format!("Exported Parquet to {}", path.display()))
    }

    fn export_excel(&self) -> color_eyre::Result<String> {
        // Fallback: write CSV with .xlsx suggestion not supported; use .csv for now (minimal)
        // Could integrate xlsx-writer crate later. For now, write CSV next to desired path with .csv
        let mut out = PathBuf::from(&self.file_path);
        if out.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase() == "xlsx" {
            out.set_extension("csv");
        }
        let cloned = self.clone_for_text(out.display().to_string());
        cloned.export_text()
    }

    fn clone_for_text(&self, new_path: String) -> DataExportDialog {
        let mut d = DataExportDialog::new(self.headers.clone(), self.rows.clone(), Some(new_path));
        d.format_index = 0; d
    }
}

impl Component for DataExportDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { Ok(()) }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> { self.render(area, frame.buffer_mut()); Ok(()) }
}


