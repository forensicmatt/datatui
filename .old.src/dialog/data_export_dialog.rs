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
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use std::path::PathBuf;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedField {
    OutputFilePath,
    BrowseButton,
    Datasets,
    Format,
    Options,
    ExportButton,
}

#[derive(Debug)]
pub struct DataExportDialog {
    // Datasets available for selection: (id, display name, selected)
    pub datasets: Vec<DatasetChoice>,
    pub selected_dataset_index: usize,
    pub mode: DataExportMode,
    pub file_path: String,
    pub file_path_input: TextArea<'static>,
    pub selected_field: SelectedField,
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
            selected_dataset_index: 0,
            mode: DataExportMode::Input,
            file_path: suggested_path.unwrap_or_else(|| String::from("export.csv")),
            file_path_input: textarea,
            selected_field: SelectedField::OutputFilePath,
            format_index: 0,
            file_browser: None,
            show_instructions: true,
            config: Config::default(),
            csv_delimiter: ',',
            csv_quote_char: Some('"'),
            csv_escape_char: None,
            csv_include_header: true,
            csv_encoding: CsvEncoding::Utf8,
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
            (crate::config::Mode::TableExport, crate::action::Action::ToggleDataViewerOption),
            (crate::config::Mode::TableExport, crate::action::Action::ToggleFormat),
        ]);
        if !s.is_empty() { s.push_str("  "); }
        s.push_str("Space: Toggle dataset/option  Up/Down: Navigate datasets");
        s
    }

    fn update_output_path_for_format(&mut self) {
        let ext = match self.current_format() {
            DataExportFormat::Text => "csv",
            DataExportFormat::Excel => "xlsx",
            DataExportFormat::Jsonl => "jsonl",
            DataExportFormat::Parquet => "parquet",
        };
        let mut pb = PathBuf::from(&self.file_path);
        pb.set_extension(ext);
        if let Some(s) = pb.to_str() {
            self.set_file_path(s.to_string());
        }
    }

    fn adjust_option(&mut self, delta: i32) {
        if !matches!(self.current_format(), DataExportFormat::Text) { return; }
        match self.option_selected {
            0 => { // delimiter
                let seq = [',', '\t', ';', '|'];
                let pos = seq.iter().position(|&c| c == self.csv_delimiter).unwrap_or(0) as i32;
                let new: usize = (pos + delta).rem_euclid(seq.len() as i32) as usize;
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
                // Shrink inner content area by one row to avoid drawing on bottom border
                let inner = Rect { x: inner.x, y: inner.y, width: inner.width, height: inner.height.saturating_sub(1) };
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

                // Usable inner area (avoid bottom border)
                let usable = Rect { x: inner.x, y: inner.y, width: inner.width, height: inner.height.saturating_sub(1) };

                // TOP: Output File Path spanning full width
                let file_path_area = Rect { x: usable.x, y: usable.y, width: usable.width, height: 3 };
                let outer = Block::default().title("Output File Path").borders(Borders::ALL);
                outer.render(file_path_area, buf);
                let fp_inner_x = file_path_area.x.saturating_add(1);
                let fp_inner_y = file_path_area.y.saturating_add(1);
                let fp_inner_w = file_path_area.width.saturating_sub(2);
                let fp_inner_h = file_path_area.height.saturating_sub(2);
                let browse_text = "[Browse]";
                let reserved = (browse_text.len() as u16).saturating_add(1);
                let input_w = fp_inner_w.saturating_sub(reserved);
                let input_area = Rect { x: fp_inner_x, y: fp_inner_y, width: input_w, height: fp_inner_h };
                let mut ta = self.file_path_input.clone();
                ta.set_block(Block::default());
                if !matches!(self.selected_field, SelectedField::OutputFilePath) { ta.set_cursor_style(Style::default().fg(Color::Gray)); }
                ta.render(input_area, buf);
                let browse_x = fp_inner_x.saturating_add(fp_inner_w.saturating_sub(browse_text.len() as u16));
                let browse_style = if matches!(self.selected_field, SelectedField::BrowseButton) { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default().fg(Color::Gray) };
                buf.set_string(browse_x, fp_inner_y, browse_text, browse_style);

                // BELOW: split remaining height into left (datasets) and right (options)
                let below_area = Rect { x: usable.x, y: usable.y.saturating_add(3), width: usable.width, height: usable.height.saturating_sub(3) };
                let left_w = below_area.width / 2;
                let right_w = below_area.width.saturating_sub(left_w);
                let left_area = Rect { x: below_area.x, y: below_area.y, width: left_w, height: below_area.height };
                let right_area = Rect { x: below_area.x.saturating_add(left_w), y: below_area.y, width: right_w, height: below_area.height };

                // LEFT: Dataset selection list (scrollable)
                let mut left_y = left_area.y;
                buf.set_string(left_area.x + 1, left_y, "Datasets:", Style::default().add_modifier(Modifier::UNDERLINED));
                left_y = left_y.saturating_add(1);

                let total_datasets = self.datasets.len();
                let max_visible_left: u16 = left_area.height.saturating_sub(2).max(1);
                let max_visible_left_usize = max_visible_left as usize;
                let visible_count = total_datasets.min(max_visible_left_usize);
                let start_index = if total_datasets > max_visible_left_usize {
                    let half = max_visible_left_usize / 2;
                    if self.selected_dataset_index <= half { 0 }
                    else if self.selected_dataset_index + half >= total_datasets { total_datasets - max_visible_left_usize }
                    else { self.selected_dataset_index - half }
                } else { 0 };

                for (i, ds_index) in (start_index..start_index + visible_count).enumerate() {
                    let ds = &self.datasets[ds_index];
                    let checkbox = if ds.selected { "[x]" } else { "[ ]" };
                    let text = format!("{checkbox} {}", ds.name);
                    let style = if matches!(self.selected_field, SelectedField::Datasets) && self.selected_dataset_index == ds_index { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default() };
                    buf.set_string(left_area.x + 2, left_y + i as u16, text.as_str(), style);
                }

                // RIGHT: Format and CSV options
                let fmt_y = right_area.y;
                let formats = ["Text", "Excel", "JSONL", "Parquet"]; let fmt = formats[self.format_index.min(formats.len()-1)];
                let fmt_style = if matches!(self.selected_field, SelectedField::Format) { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default() };
                buf.set_string(right_area.x + 1, fmt_y, format!("Format: {fmt} (Ctrl+F to cycle)"), fmt_style);

                if matches!(self.current_format(), DataExportFormat::Text) {
                    let base_y = fmt_y + 2;
                    let enc = match self.csv_encoding { CsvEncoding::Utf8 => "UTF-8", CsvEncoding::Utf8Bom => "UTF-8 BOM", CsvEncoding::Utf16Le => "UTF-16LE" };
                    let quote_str = self.csv_quote_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string());
                    let escape_str = self.csv_escape_char.map(|c| format!("'{c}'")).unwrap_or_else(|| "None".to_string());
                    let header_str = if self.csv_include_header { "Yes" } else { "No" };
                    let delimiter_str = if self.csv_delimiter == '\t' { "\\t".to_string() } else { format!("'{}'", self.csv_delimiter) };

                    let options = [
                        format!("Delimiter: {delimiter_str}"),
                        format!("Quote Char: {quote_str}"),
                        format!("Escape Char: {escape_str}"),
                        format!("Has Header: {header_str}"),
                        format!("Encoding: {enc}"),
                    ];
                    for (i, line) in options.iter().enumerate() {
                        let style = if matches!(self.selected_field, SelectedField::Options) && self.option_selected == i { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default() };
                        buf.set_string(right_area.x + 1, base_y + i as u16, line, style);
                    }
                }

                // RIGHT: Export button bottom-right of right pane
                let export_text = "[Export]";
                let export_x = right_area.x + right_area.width.saturating_sub(export_text.len() as u16 + 2);
                let export_y = right_area.y + right_area.height.saturating_sub(1);
                let export_style = if matches!(self.selected_field, SelectedField::ExportButton) { Style::default().fg(Color::Black).bg(Color::White) } else { Style::default().fg(Color::Gray) };
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

    fn feed_text_input_key(&mut self, code: KeyCode) {
        use crossterm::event::{KeyEvent, KeyModifiers};
        let kev = KeyEvent::new(code, KeyModifiers::empty());
        let inp = tui_textarea::Input::from(kev);
        self.file_path_input.input(inp);
        self.sync_file_path_from_input();
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
                // Handle text input for OutputFilePath when it's selected
                if matches!(self.selected_field, SelectedField::OutputFilePath) {
                    match key.code {
                        KeyCode::Up => {
                            // If at top of text, move to Format or Datasets
                            let (row, _col) = self.file_path_input.cursor();
                            if row == 0 {
                                // Check if Format is available (it's always visible)
                                self.selected_field = SelectedField::Format;
                            } else {
                                self.feed_text_input_key(KeyCode::Up);
                            }
                            return None;
                        }
                        KeyCode::Down => {
                            // If at bottom of text, move to Datasets
                            let (row, _col) = self.file_path_input.cursor();
                            let last_idx = self.file_path_input.lines().len().saturating_sub(1);
                            if row >= last_idx {
                                self.selected_field = SelectedField::Datasets;
                            } else {
                                self.feed_text_input_key(KeyCode::Down);
                            }
                            return None;
                        }
                        KeyCode::Right => {
                            // If cursor is at far right, select Browse button
                            let (row, col) = self.file_path_input.cursor();
                            let lines = self.file_path_input.lines();
                            let current_line_len = lines.get(row).map(|s| s.len()).unwrap_or(0);
                            if col >= current_line_len {
                                self.selected_field = SelectedField::BrowseButton;
                            } else {
                                self.feed_text_input_key(KeyCode::Right);
                            }
                            return None;
                        }
                        KeyCode::Left
                        | KeyCode::Home
                        | KeyCode::End
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                        | KeyCode::Backspace
                        | KeyCode::Delete
                        | KeyCode::Enter
                        | KeyCode::Tab => {
                            self.feed_text_input_key(key.code);
                            return None;
                        }
                        KeyCode::Char(ch) => {
                            self.feed_text_input_key(KeyCode::Char(ch));
                            return None;
                        }
                        _ => {}
                    }
                }

                // Space toggles dataset selection or cycles option
                if key.code == KeyCode::Char(' ') {
                    if matches!(self.selected_field, SelectedField::Datasets) && !self.datasets.is_empty() {
                        let idx = self.selected_dataset_index.min(self.datasets.len()-1);
                        self.datasets[idx].selected = !self.datasets[idx].selected;
                        return None;
                    }
                    if matches!(self.selected_field, SelectedField::Options) {
                        self.adjust_option(1);
                        return None;
                    }
                }

                // Global navigation/actions
                if let Some(action) = optional_global_action {
                    match action {
                        Action::Tab => {
                            // Cycle through fields
                            self.selected_field = match self.selected_field {
                                SelectedField::OutputFilePath => SelectedField::Datasets,
                                SelectedField::Datasets => {
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        SelectedField::Options
                                    } else {
                                        SelectedField::BrowseButton
                                    }
                                }
                                SelectedField::Options => SelectedField::BrowseButton,
                                SelectedField::BrowseButton => SelectedField::ExportButton,
                                SelectedField::ExportButton => SelectedField::OutputFilePath,
                                SelectedField::Format => SelectedField::Datasets,
                            };
                            None
                        }
                        Action::Right => {
                            match self.selected_field {
                                SelectedField::OutputFilePath => {
                                    // Already handled above - should not reach here
                                    None
                                }
                                SelectedField::BrowseButton => {
                                    self.selected_field = SelectedField::ExportButton;
                                    None
                                }
                                SelectedField::Datasets => {
                                    // Move to right side: Options (if Text) or Browse
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        self.selected_field = SelectedField::Options;
                                        self.option_selected = 0;
                                    } else {
                                        self.selected_field = SelectedField::BrowseButton;
                                    }
                                    None
                                }
                                SelectedField::Options => {
                                    self.selected_field = SelectedField::ExportButton;
                                    None
                                }
                                SelectedField::Format => {
                                    // Format is on the right side, so Right moves to Options or Browse
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        self.selected_field = SelectedField::Options;
                                        self.option_selected = 0;
                                    } else {
                                        self.selected_field = SelectedField::BrowseButton;
                                    }
                                    None
                                }
                                SelectedField::ExportButton => None,
                            }
                        }
                        Action::Left => {
                            match self.selected_field {
                                SelectedField::OutputFilePath => {
                                    self.feed_text_input_key(KeyCode::Left);
                                    None
                                }
                                SelectedField::BrowseButton => {
                                    // Move left: if Text format, go to Options, else to Datasets
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        self.selected_field = SelectedField::Options;
                                        self.option_selected = 4.min(self.option_selected);
                                    } else {
                                        self.selected_field = SelectedField::Datasets;
                                    }
                                    None
                                }
                                SelectedField::Datasets => {
                                    // Move to OutputFilePath
                                    self.selected_field = SelectedField::OutputFilePath;
                                    None
                                }
                                SelectedField::Options => {
                                    // Move back to Datasets
                                    self.selected_field = SelectedField::Datasets;
                                    None
                                }
                                SelectedField::Format => {
                                    // Move to OutputFilePath
                                    self.selected_field = SelectedField::OutputFilePath;
                                    None
                                }
                                SelectedField::ExportButton => {
                                    // Move left: if Text format, go to Options, else to Browse
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        self.selected_field = SelectedField::Options;
                                        self.option_selected = 4.min(self.option_selected);
                                    } else {
                                        self.selected_field = SelectedField::BrowseButton;
                                    }
                                    None
                                }
                            }
                        }
                        Action::Up => {
                            match self.selected_field {
                                SelectedField::OutputFilePath => {
                                    // Already handled above - should not reach here
                                    None
                                }
                                SelectedField::Datasets => {
                                    // When at topmost dataset (index 0), move to OutputFilePath
                                    if self.selected_dataset_index == 0 {
                                        self.selected_field = SelectedField::OutputFilePath;
                                    } else {
                                        self.selected_dataset_index -= 1;
                                    }
                                    None
                                }
                                SelectedField::Format => {
                                    // When Format is selected, Up moves to OutputFilePath
                                    self.selected_field = SelectedField::OutputFilePath;
                                    None
                                }
                                SelectedField::Options => {
                                    // Move up within options, or to OutputFilePath if at top
                                    if self.option_selected == 0 {
                                        self.selected_field = SelectedField::OutputFilePath;
                                    } else {
                                        self.option_selected -= 1;
                                    }
                                    None
                                }
                                SelectedField::BrowseButton => {
                                    self.selected_field = SelectedField::OutputFilePath;
                                    None
                                }
                                SelectedField::ExportButton => {
                                    // Move to Browse button
                                    self.selected_field = SelectedField::BrowseButton;
                                    None
                                }
                            }
                        }
                        Action::Down => {
                            match self.selected_field {
                                SelectedField::OutputFilePath => {
                                    // Already handled above - should not reach here
                                    None
                                }
                                SelectedField::Datasets => {
                                    // Move down within datasets, or to Options/Browse if at bottom
                                    if self.selected_dataset_index + 1 < self.datasets.len() {
                                        self.selected_dataset_index += 1;
                                    } else {
                                        // At last dataset, move to right side
                                        if matches!(self.current_format(), DataExportFormat::Text) {
                                            self.selected_field = SelectedField::Options;
                                            self.option_selected = 0;
                                        } else {
                                            self.selected_field = SelectedField::BrowseButton;
                                        }
                                    }
                                    None
                                }
                                SelectedField::Format => {
                                    // Format is at top of right side, Down moves to Options or Browse
                                    if matches!(self.current_format(), DataExportFormat::Text) {
                                        self.selected_field = SelectedField::Options;
                                        self.option_selected = 0;
                                    } else {
                                        self.selected_field = SelectedField::BrowseButton;
                                    }
                                    None
                                }
                                SelectedField::Options => {
                                    // Move down within options, or to ExportButton if at bottom
                                    if self.option_selected >= 4 {
                                        self.selected_field = SelectedField::ExportButton;
                                    } else {
                                        self.option_selected += 1;
                                    }
                                    None
                                }
                                SelectedField::BrowseButton => {
                                    self.selected_field = SelectedField::ExportButton;
                                    None
                                }
                                SelectedField::ExportButton => None,
                            }
                        }
                        Action::Enter => {
                            if matches!(self.selected_field, SelectedField::BrowseButton) {
                                let mut dialog_file_browser = FileBrowserDialog::new(
                                    None, 
                                    Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), 
                                    false, FileBrowserMode::Save
                                );
                                dialog_file_browser.register_config_handler(self.config.clone());
                                self.file_browser = Some(dialog_file_browser);
                                self.mode = DataExportMode::FileBrowser;
                                return None;
                            }
                            if matches!(self.selected_field, SelectedField::ExportButton | SelectedField::OutputFilePath) {
                                let ids: Vec<String> = self.datasets.iter().filter(|d| d.selected).map(|d| d.id.clone()).collect();
                                return Some(Action::DataExportRequestedMulti {
                                    dataset_ids: ids,
                                    file_path: self.file_path.clone(),
                                    format_index: self.format_index,
                                });
                            }
                            None
                        }
                        Action::Backspace => {
                            if matches!(self.selected_field, SelectedField::OutputFilePath) {
                                self.feed_text_input_key(KeyCode::Backspace);
                            } else if matches!(self.selected_field, SelectedField::Options) {
                                self.set_option_char(None);
                            }
                            None
                        }
                        Action::ToggleDataViewerOption => {
                            if matches!(self.selected_field, SelectedField::Options) {
                                self.adjust_option(1);
                            }
                            None
                        }
                        Action::Paste => {
                            if matches!(self.selected_field, SelectedField::OutputFilePath) {
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    if let Ok(text) = clipboard.get_text() {
                                        let first_line = text.lines().next().unwrap_or("").to_string();
                                        self.set_file_path(first_line);
                                    }
                                }
                            }
                            None
                        }
                        Action::CopyFilePath => {
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let _ = clipboard.set_text(self.file_path.clone());
                            }
                            None
                        }
                        Action::Escape => {
                            Some(Action::DialogClose)
                        }
                        _ => None,
                    }
                } else if let Some(action) = export_action {
                    match action {
                        Action::OpenFileBrowser => {
                            self.file_browser = Some(FileBrowserDialog::new(None, Some(vec!["csv","xlsx","jsonl","ndjson","parquet"]), false, FileBrowserMode::Save));
                            self.mode = DataExportMode::FileBrowser;
                            None
                        }
                        Action::ToggleFormat => {
                            self.format_index = (self.format_index + 1) % 4;
                            self.update_output_path_for_format();
                            None
                        }
                        Action::ExportTable => {
                            let ids: Vec<String> = self.datasets.iter().filter(|d| d.selected).map(|d| d.id.clone()).collect();
                            Some(Action::DataExportRequestedMulti {
                                dataset_ids: ids,
                                file_path: self.file_path.clone(),
                                format_index: self.format_index,
                            })
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


