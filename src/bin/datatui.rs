use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use datatui::{core::CsvImportOptions, tui::App};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<()> {
    // Setup color_eyre for better error messages
    color_eyre::install()?;

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Get workspace path (current directory for now)
    let workspace_path = std::env::current_dir()?;

    // Create app
    let mut app = App::new(&workspace_path)?;

    // If a file path is provided, import it
    if args.len() > 1 {
        let file_path = PathBuf::from(&args[1]);

        if file_path.extension().and_then(|s| s.to_str()) == Some("csv") {
            // Import CSV file
            let options = CsvImportOptions::default();
            let dataset_id = app.data_service().import_csv(file_path, options)?;
            app.load_dataset(&dataset_id)?;
        } else {
            eprintln!("Unsupported file type. Only CSV files are supported for now.");
            return Ok(());
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
