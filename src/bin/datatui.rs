use clap::{Parser, ValueEnum};
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use datatui::{
    core::{CsvImportOptions, JsonImportOptions},
    logging,
    tui::App,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

/// DataTUI - Interactive TUI for data exploration
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable file logging at the given level (overrides RUST_LOG)
    #[arg(long = "logging", value_enum)]
    logging: Option<LogLevel>,

    /// For JSON files: specify which attribute contains the records array
    /// Examples: "Records", "data", "response.items"
    #[arg(long = "json-records", default_value = "@")]
    json_records: String,

    /// File or glob pattern to load on startup (e.g., data.json, data/*.csv)
    file_path: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

fn main() -> Result<()> {
    // Setup color_eyre for better error messages
    color_eyre::install()?;

    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging to current working directory
    let cwd = std::env::current_dir()?;
    let log_path = cwd.join("datatui.log");
    let level = match args.logging {
        Some(LogLevel::Error) => Some(tracing::Level::ERROR),
        Some(LogLevel::Warn) => Some(tracing::Level::WARN),
        Some(LogLevel::Info) => Some(tracing::Level::INFO),
        Some(LogLevel::Debug) => Some(tracing::Level::DEBUG),
        Some(LogLevel::Trace) => Some(tracing::Level::TRACE),
        None => Some(tracing::Level::WARN),
    };
    logging::init_with(Some(log_path), level)?;

    // Get workspace path (current directory for now)
    let workspace_path = std::env::current_dir()?;

    // Create app
    let mut app = App::new(&workspace_path)?;

    // If a file path is provided, import it
    if let Some(file_path) = args.file_path {
        let extension = file_path.extension().and_then(|s| s.to_str());
        match extension {
            Some("csv") => {
                // Import CSV file
                let options = CsvImportOptions::default();
                let dataset_id = app.data_service().import_csv(file_path, options)?;
                app.load_dataset(&dataset_id)?;
            }
            Some("json") => {
                // Import JSON file with custom records path
                let options = if args.json_records == "@" {
                    JsonImportOptions::default()
                } else {
                    JsonImportOptions::with_records_expr(&args.json_records)
                };
                let dataset_id = app.data_service().import_json(file_path, options)?;
                app.load_dataset(&dataset_id)?;
            }
            Some("ndjson") | Some("jsonl") => {
                // Import NDJSON file
                let options = JsonImportOptions::ndjson();
                let dataset_id = app.data_service().import_json(file_path, options)?;
                app.load_dataset(&dataset_id)?;
            }
            Some("parquet") => {
                // Import Parquet file
                let dataset_id = app.data_service().import_parquet(file_path)?;
                app.load_dataset(&dataset_id)?;
            }
            _ => {
                eprintln!(
                    "Unsupported file type. Supported formats: CSV, JSON, NDJSON/JSONL, Parquet"
                );
                return Ok(());
            }
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Print any errors that occurred
    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        // Render
        terminal.draw(|f| app.render(f))?;

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Handle Ctrl+C specially to ensure clean exit
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                app.handle_key_event(key)?;

                // Check if we should quit
                if app.should_quit() {
                    break;
                }
            }
        }

        // Update app state
        app.update()?;
    }

    Ok(())
}
