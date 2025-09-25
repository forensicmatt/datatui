//! DataImportDialog: Wizard dialog for importing different types of data files

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap, BorderType};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
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
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::{
    csv_options_dialog::{CsvOptionsDialog, CsvImportOptions},
    xlsx_options_dialog::{XlsxOptionsDialog, XlsxImportOptions},
    sqlite_options_dialog::{SqliteOptionsDialog, SqliteImportOptions},
    parquet_options_dialog::{ParquetOptionsDialog, ParquetImportOptions},
    json_options_dialog::{JsonOptionsDialog, JsonImportOptions},
};

/// Supported file types for import
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    Csv,
    Tsv,
    Xlsx,
    Sqlite,
    Parquet,
    Unknown,
}

/// Data source types for import
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataSourceType {
    Text,
    Excel,
    Sqlite,
    Parquet,
    Json,
}

impl Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::Csv => write!(f, "CSV"),
            FileType::Tsv => write!(f, "TSV"),
            FileType::Xlsx => write!(f, "Excel (XLSX)"),
            FileType::Sqlite => write!(f, "SQLite Database"),
            FileType::Parquet => write!(f, "Parquet"),
            FileType::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Display for DataSourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSourceType::Text => write!(f, "Text Files (CSV, TSV, etc.)"),
            DataSourceType::Excel => write!(f, "Excel Files (XLSX, XLS)"),
            DataSourceType::Sqlite => write!(f, "SQLite Database"),
            DataSourceType::Parquet => write!(f, "Parquet Files (.parquet)"),
            DataSourceType::Json => write!(f, "JSON Files (.json, .ndjson)"),
        }
    }
}

impl FileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "csv" => FileType::Csv,
            "tsv" => FileType::Tsv,
            "xlsx" | "xls" => FileType::Xlsx,
            "db" | "sqlite" | "sqlite3" => FileType::Sqlite,
            "parquet" => FileType::Parquet,
            _ => FileType::Unknown,
        }
    }
}

/// Dialog mode for the import wizard
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataImportDialogMode {
    DataSourceSelection,
    CsvOptions,
    XlsxOptions,
    SqliteOptions,
    ParquetOptions,
    JsonOptions,
    Confirmation,
}

/// DataImportDialog: Wizard for importing different types of data files
#[derive(Debug, Serialize, Deserialize)]
pub struct DataImportDialog {
    pub mode: DataImportDialogMode,
    pub show_instructions: bool,
    pub file_browser_path: PathBuf,
    pub selected_data_source: Option<DataSourceType>,
    pub data_source_selection_index: usize,
    pub current_file_path: String,
    #[serde(skip)]
    pub csv_options_dialog: Option<CsvOptionsDialog>,
    #[serde(skip)]
    pub xlsx_options_dialog: Option<XlsxOptionsDialog>,
    #[serde(skip)]
    pub sqlite_options_dialog: Option<SqliteOptionsDialog>,
    #[serde(skip)]
    pub parquet_options_dialog: Option<ParquetOptionsDialog>,
    #[serde(skip)]
    pub json_options_dialog: Option<JsonOptionsDialog>,
    #[serde(skip)]
    pub config: Config,
}

impl Default for DataImportDialog {
    fn default() -> Self { Self::new() }
}

impl DataImportDialog {
    /// Create a new DataImportDialog
    pub fn new() -> Self {
        Self {
            mode: DataImportDialogMode::DataSourceSelection,
            show_instructions: true,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected_data_source: None,
            data_source_selection_index: 0,
            current_file_path: String::new(),
            csv_options_dialog: None,
            xlsx_options_dialog: None,
            sqlite_options_dialog: None,
            parquet_options_dialog: None,
            json_options_dialog: None,
            config: Config::default(),
        }
    }

    /// Set whether to show the instructions area
    pub fn set_show_instructions(&mut self, show: bool) {
        self.show_instructions = show;
    }

    /// Render the dialog (UI skeleton)
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        // Clear the background for the popup
        Clear.render(area, buf);
        
        // Dialog instructions per mode
        let instructions = match self.mode {
            DataImportDialogMode::DataSourceSelection => "Up/Down: Navigate  Enter: Select  Esc: Close",
            DataImportDialogMode::CsvOptions => "Use CSV options dialog controls",
            DataImportDialogMode::XlsxOptions => "Use Excel options dialog controls",
            DataImportDialogMode::SqliteOptions => "Use SQLite options dialog controls",
            DataImportDialogMode::ParquetOptions => "Use Parquet options dialog controls",
            DataImportDialogMode::JsonOptions => "Use JSON options dialog controls",
            DataImportDialogMode::Confirmation => "y: Confirm Import  n: Back  Esc: Cancel",
        };

        // Outer container with double border and title "Data Import Wizard"
        let outer_block = Block::default()
            .title("Data Import Wizard")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(
            inner_area,
            self.show_instructions, 
            Some(instructions)
        );
        let content_area = layout.content_area;
        let mut no_instructions = false;
        let instructions_area = layout.instructions_area;

        // Check if we should render any of the option dialogs
        if let Some(ref csv_dialog) = self.csv_options_dialog {
            csv_dialog.render(inner_area, buf);
            no_instructions = true;
        } else if let Some(ref xlsx_dialog) = self.xlsx_options_dialog {
            xlsx_dialog.render(inner_area, buf);
            no_instructions = true;
        } else if let Some(ref sqlite_dialog) = self.sqlite_options_dialog {
            sqlite_dialog.render(inner_area, buf);
            no_instructions = true;
        } else if let Some(ref parquet_dialog) = self.parquet_options_dialog {
            parquet_dialog.render(inner_area, buf);
            no_instructions = true;
        } else if let Some(ref json_dialog) = self.json_options_dialog {
            json_dialog.render(inner_area, buf);
            no_instructions = true;
        } else {
            // Render content based on mode
            match self.mode {
                DataImportDialogMode::DataSourceSelection => {
                    self.render_data_source_selection_mode(content_area, buf);
                }
                DataImportDialogMode::CsvOptions => {
                    // This should not be reached as CSV options dialog should be active
                    self.render_csv_options_mode(content_area, buf);
                }
                DataImportDialogMode::XlsxOptions => {
                    // This should not be reached as XLSX options dialog should be active
                    self.render_xlsx_options_mode(content_area, buf);
                }
                DataImportDialogMode::SqliteOptions => {
                    // This should not be reached as SQLite options dialog should be active
                    self.render_sqlite_options_mode(content_area, buf);
                }
                DataImportDialogMode::ParquetOptions => {
                    // This should not be reached as Parquet options dialog should be active
                    // No fallback content needed beyond a simple note
                    self.render_parquet_options_mode(content_area, buf);
                }
                DataImportDialogMode::JsonOptions => {
                    self.render_json_options_mode(content_area, buf);
                }
                DataImportDialogMode::Confirmation => {
                    self.render_confirmation_mode(content_area, buf);
                }
            }
        }

        // Render instructions if enabled
        if !no_instructions && let Some(instructions_area) = instructions_area {
            self.render_instructions(instructions_area, buf, instructions);
        }

        // Return the number of lines rendered (for scrolling calculations)
        content_area.height as usize
    }

    /// Render the data source selection mode
    fn render_data_source_selection_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Select Data Source Type")
            .borders(Borders::ALL);

        let data_sources = [
            DataSourceType::Text,
            DataSourceType::Excel,
            DataSourceType::Sqlite,
            DataSourceType::Parquet,
            DataSourceType::Json,
        ];

        let list_items: Vec<ListItem> = data_sources
            .iter()
            .enumerate()
            .map(|(index, data_source)| {
                let style = if index == self.data_source_selection_index {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default()
                };
                
                let text = format!("{data_source}");
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(list_items)
            .block(block)
            .style(Style::default());

        Widget::render(list, area, buf);
    }

    /// Render the CSV options mode (fallback)
    fn render_csv_options_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("CSV Import Options")
            .borders(Borders::ALL);

        let content = "CSV options dialog should be active.";
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render the XLSX options mode (fallback)
    fn render_xlsx_options_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Excel Import Options")
            .borders(Borders::ALL);

        let content = "Excel options dialog should be active.";
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render the SQLite options mode (fallback)
    fn render_sqlite_options_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("SQLite Import Options")
            .borders(Borders::ALL);

        let content = "SQLite options dialog should be active.";
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render the Parquet options mode (fallback)
    fn render_parquet_options_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Parquet Import Options")
            .borders(Borders::ALL);

        let content = "Parquet options dialog should be active.";
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render the JSON options mode (fallback)
    fn render_json_options_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("JSON Import Options")
            .borders(Borders::ALL);

        let content = "JSON options dialog should be active.";
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render the confirmation mode
    fn render_confirmation_mode(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Confirm Import")
            .borders(Borders::ALL);

        let content = "Import configuration ready.\n\nPress 'y' to confirm or 'n' to go back.";

        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });
        paragraph.render(area, buf);
    }

    /// Render instructions area
    fn render_instructions(&self, area: Rect, buf: &mut Buffer, instructions: &str) {
        let block = Block::default()
            .title("Instructions")
            .style(Style::default().fg(Color::Yellow))
            .borders(Borders::ALL);
        
        let paragraph = Paragraph::new(instructions)
            .block(block)
            .wrap(Wrap { trim: true });
        
        paragraph.render(area, buf);
    }

    /// Move to the next mode in the wizard
    fn next_mode(&mut self) {
        self.mode = match self.mode {
            DataImportDialogMode::DataSourceSelection => {
                // Go directly to the appropriate options dialog based on selected data source
                match self.selected_data_source {
                    Some(DataSourceType::Text) => DataImportDialogMode::CsvOptions,
                    Some(DataSourceType::Excel) => DataImportDialogMode::XlsxOptions,
                    Some(DataSourceType::Sqlite) => DataImportDialogMode::SqliteOptions,
                    Some(DataSourceType::Parquet) => DataImportDialogMode::ParquetOptions,
                    Some(DataSourceType::Json) => DataImportDialogMode::JsonOptions,
                    None => DataImportDialogMode::DataSourceSelection,
                }
            }
            DataImportDialogMode::CsvOptions | DataImportDialogMode::XlsxOptions | DataImportDialogMode::SqliteOptions | DataImportDialogMode::ParquetOptions | DataImportDialogMode::JsonOptions => {
                DataImportDialogMode::Confirmation
            }
            DataImportDialogMode::Confirmation => {
                DataImportDialogMode::Confirmation
            }
        };
    }

    /// Move to the previous mode in the wizard
    fn previous_mode(&mut self) {
        self.mode = match self.mode {
            DataImportDialogMode::DataSourceSelection => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::CsvOptions => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::XlsxOptions => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::SqliteOptions => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::ParquetOptions => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::JsonOptions => {
                DataImportDialogMode::DataSourceSelection
            }
            DataImportDialogMode::Confirmation => {
                // Go back to the appropriate options mode based on data source
                match self.selected_data_source {
                    Some(DataSourceType::Text) => DataImportDialogMode::CsvOptions,
                    Some(DataSourceType::Excel) => DataImportDialogMode::XlsxOptions,
                    Some(DataSourceType::Sqlite) => DataImportDialogMode::SqliteOptions,
                    Some(DataSourceType::Parquet) => DataImportDialogMode::ParquetOptions,
                    Some(DataSourceType::Json) => DataImportDialogMode::JsonOptions,
                    None => DataImportDialogMode::DataSourceSelection,
                }
            }
        };
    }

    /// Create the appropriate options dialog based on the current data source
    fn create_options_dialog(&mut self) {
        let file_path = self.current_file_path.clone();
        
        match self.selected_data_source {
            Some(DataSourceType::Text) => {
                self.csv_options_dialog = Some(CsvOptionsDialog::new(
                    file_path,
                    CsvImportOptions::default()
                ));
                if let Some(ref mut d) = self.csv_options_dialog {
                    let _ = d.register_config_handler(self.config.clone());
                }
            }
            Some(DataSourceType::Excel) => {
                self.xlsx_options_dialog = Some(XlsxOptionsDialog::new(
                    file_path,
                    XlsxImportOptions::default()
                ));
                if let Some(ref mut d) = self.xlsx_options_dialog {
                    let _ = d.register_config_handler(self.config.clone());
                }
            }
            Some(DataSourceType::Sqlite) => {
                self.sqlite_options_dialog = Some(SqliteOptionsDialog::new(
                    file_path,
                    SqliteImportOptions::default()
                ));
                if let Some(ref mut d) = self.sqlite_options_dialog {
                    let _ = d.register_config_handler(self.config.clone());
                }
            }
            Some(DataSourceType::Parquet) => {
                self.parquet_options_dialog = Some(ParquetOptionsDialog::new(
                    file_path,
                    ParquetImportOptions::default()
                ));
                if let Some(ref mut d) = self.parquet_options_dialog {
                    let _ = d.register_config_handler(self.config.clone());
                }
            }
            Some(DataSourceType::Json) => {
                self.json_options_dialog = Some(JsonOptionsDialog::new(
                    file_path,
                    JsonImportOptions::default()
                ));
                if let Some(ref mut d) = self.json_options_dialog {
                    let _ = d.register_config_handler(self.config.clone());
                }
            }
            None => {}
        }
    }
}

impl Component for DataImportDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
        self.config = _config;
        // Propagate to child dialogs if they exist
        if let Some(ref mut d) = self.csv_options_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.xlsx_options_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.sqlite_options_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.parquet_options_dialog { let _ = d.register_config_handler(self.config.clone()); }
        if let Some(ref mut d) = self.json_options_dialog { let _ = d.register_config_handler(self.config.clone()); }
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
        // Handle option dialogs first
        if let Some(ref mut csv_dialog) = self.csv_options_dialog {
            if let Some(action) = csv_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseCsvOptionsDialog => {
                        self.csv_options_dialog = None;
                        self.previous_mode();
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config: _ } => {
                        // Add the data source from the import config
                        self.csv_options_dialog = None;
                        self.next_mode();
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        if let Some(ref mut xlsx_dialog) = self.xlsx_options_dialog {
            if let Some(action) = xlsx_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseXlsxOptionsDialog => {
                        self.xlsx_options_dialog = None;
                        self.previous_mode();
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config: _ } => {
                        // Add the data source from the import config
                        self.xlsx_options_dialog = None;
                        self.next_mode();
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        if let Some(ref mut sqlite_dialog) = self.sqlite_options_dialog {
            if let Some(action) = sqlite_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseSqliteOptionsDialog => {
                        self.sqlite_options_dialog = None;
                        self.previous_mode();
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config: _ } => {
                        // Add the data source from the import config
                        self.sqlite_options_dialog = None;
                        self.next_mode();
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        if let Some(ref mut parquet_dialog) = self.parquet_options_dialog {
            if let Some(action) = parquet_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseParquetOptionsDialog => {
                        self.parquet_options_dialog = None;
                        self.previous_mode();
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config: _ } => {
                        // Add the data source from the import config
                        self.parquet_options_dialog = None;
                        self.next_mode();
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        if let Some(ref mut json_dialog) = self.json_options_dialog {
            if let Some(action) = json_dialog.handle_key_event(key)? {
                match action {
                    Action::CloseJsonOptionsDialog => {
                        self.json_options_dialog = None;
                        self.previous_mode();
                        return Ok(None);
                    }
                    Action::AddDataImportConfig { config: _ } => {
                        // Add the data source from the import config
                        self.json_options_dialog = None;
                        self.next_mode();
                        return Ok(Some(action));
                    }
                    _ => {
                        return Ok(Some(action));
                    }
                }
            }
            return Ok(None);
        }

        // Config-driven: handle Global actions (navigation/escape)
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Ok(Some(Action::CloseDataImportDialog));
                }
                Action::Up => {
                    if self.mode == DataImportDialogMode::DataSourceSelection && self.data_source_selection_index > 0 {
                        self.data_source_selection_index = self.data_source_selection_index.saturating_sub(1);
                    }
                    return Ok(None);
                }
                Action::Down => {
                    if self.mode == DataImportDialogMode::DataSourceSelection && self.data_source_selection_index < 4 {
                        self.data_source_selection_index = self.data_source_selection_index.saturating_add(1);
                    }
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Config-driven: handle DataImport-specific actions
        if let Some(import_action) = self.config.action_for_key(crate::config::Mode::DataImport, key) {
            match import_action {
                Action::DataImportSelect => {
                    if self.mode == DataImportDialogMode::DataSourceSelection {
                        let data_sources = [
                            DataSourceType::Text,
                            DataSourceType::Excel,
                            DataSourceType::Sqlite,
                            DataSourceType::Parquet,
                            DataSourceType::Json,
                        ];
                        if let Some(selected) = data_sources.get(self.data_source_selection_index) {
                            self.selected_data_source = Some(selected.clone());
                            self.create_options_dialog();
                            self.next_mode();
                        }
                    }
                    return Ok(None);
                }
                Action::ConfirmDataImport => {
                    return Ok(Some(Action::ConfirmDataImport));
                }
                Action::DataImportBack => {
                    if self.mode == DataImportDialogMode::Confirmation {
                        self.previous_mode();
                        return Ok(None);
                    }
                    return Ok(None);
                }
                Action::CloseDataImportDialog => {
                    return Ok(Some(Action::CloseDataImportDialog));
                }
                _ => {
                    return Ok(Some(import_action));
                }
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