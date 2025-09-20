use clap::{Parser, ValueEnum};
use std::io;
use crossterm::event::{self, Event as CEvent, KeyCode, KeyModifiers, EnableMouseCapture, DisableMouseCapture};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::time::Duration;
use datatui::dialog::DataTabManagerDialog;
use datatui::style::StyleConfig;
use datatui::components::Component;
use datatui::tui::Event as TuiEvent;
use datatui::action::Action;
use color_eyre::Result;
use tracing::error;

/// Simple CLI for DataTabManagerDialog demo
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable file logging at the given level (overrides RUST_LOG)
    #[arg(long = "logging", value_enum)]
    logging: Option<LogLevel>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LogLevel { Error, Warn, Info, Debug, Trace }

fn main() -> Result<()> {
    // Parse CLI args
    let args = Args::parse();
    // Initialize logging to file in current working directory
    let cwd = std::env::current_dir()?;
    let log_path = cwd.join("datatui.log");
    let level = match args.logging {
        Some(LogLevel::Error) => Some(tracing::Level::ERROR),
        Some(LogLevel::Warn)  => Some(tracing::Level::WARN),
        Some(LogLevel::Info)  => Some(tracing::Level::INFO),
        Some(LogLevel::Debug) => Some(tracing::Level::DEBUG),
        Some(LogLevel::Trace) => Some(tracing::Level::TRACE),
        None => Some(tracing::Level::WARN),
    };
    datatui::logging::init_with(Some(log_path), level)?;
    
    // Create DataTabManagerDialog
    let style = StyleConfig::default();
    let mut tab_manager = DataTabManagerDialog::new(style);
    
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App loop
    let res = run_app(&mut terminal, &mut tab_manager);

    // Restore terminal
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    if let Err(e) = res {
        error!("Error: {e}");
    }
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    tab_manager: &mut DataTabManagerDialog,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| {
            let size = f.area();
            tab_manager.draw(f, size).unwrap();
        })?;
        // After drawing, process queued Render work (overlay is now visible)
        let _ = tab_manager.update(Action::Render);
        
        // Poll for events
        if event::poll(Duration::from_millis(100))?
            && let CEvent::Key(key_event) = event::read()? {
                if let KeyCode::Char('z') = key_event.code && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                // Convert to TuiEvent and pass to handle_events
                let tui_event = TuiEvent::Key(key_event);
                match tab_manager.handle_events(Some(tui_event)) {
                    Ok(Some(action)) => {
                        if let Err(e) = tab_manager.update(action) {
                            error!("Error updating after action: {e}");
                        }
                    }
                    Ok(None) => {}
                    Err(e) => error!("Error handling TuiEvent: {e}"),
                }
        }
        // Tick update (animate progress, etc.)
        let _ = tab_manager.update(Action::Tick);
    }
    // On exit, attempt to save workspace state if path is valid
    if tab_manager.project_settings_dialog.config.workspace_path.as_ref().is_some_and(|p| p.is_dir()) {
        let _ = tab_manager.save_workspace_state();
    }
    Ok(())
}


