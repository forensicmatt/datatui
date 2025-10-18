use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::io;
use crossterm::event::{self, Event as CEvent, EnableMouseCapture, DisableMouseCapture};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::time::Duration;
use datatui::dialog::{DataTabManagerDialog, KeybindingsDialog};
use datatui::style::StyleConfig;
use datatui::config::Config;
use datatui::components::Component;
use datatui::tui::Event as TuiEvent;
use datatui::action::Action;
use color_eyre::Result;
use tracing::{error, debug};

/// Simple CLI for DataTabManagerDialog demo
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable file logging at the given level (overrides RUST_LOG)
    #[arg(long = "logging", value_enum)]
    logging: Option<LogLevel>,
    /// Path to a config file (overrides default config discovery)
    #[arg(long = "config", value_name = "PATH")]
    config: Option<PathBuf>,
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
    
    // Load Config and create DataTabManagerDialog
    let style = StyleConfig::default();
    let mut tab_manager = DataTabManagerDialog::new(style);
    if let Ok(cfg) = Config::from_path(args.config.as_ref()) {
        let _ = tab_manager.register_config_handler(cfg);
    }
    
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
    // Optional global Keybindings dialog overlay, opened via a global shortcut
    let mut keybindings_dialog: Option<KeybindingsDialog> = None;
    loop {
        terminal.draw(|f| {
            let size = f.area();
            tab_manager.draw(f, size).unwrap();
            // When open, render the keybindings dialog on top
            if let Some(dialog) = &mut keybindings_dialog {
                let _ = dialog.draw(f, size);
            }
        })?;
        // After drawing, process queued Render work (overlay is now visible)
        let _ = tab_manager.update(Action::Render);
        // If Data Management is visible and busy, pump a few extra render/update cycles to show progress
        if tab_manager.show_data_management && tab_manager.data_management_dialog.busy_active {
            for _ in 0..3 {
                terminal.draw(|f| {
                    let size = f.area();
                    tab_manager.draw(f, size).unwrap();
                })?;
                let _ = tab_manager.update(Action::Render);
            }
        }
        
        // Poll for events
        if event::poll(Duration::from_millis(100))?
            && let CEvent::Key(key_event) = event::read()? {
                if let Some(global_action) = tab_manager.config.action_for_key(datatui::config::Mode::Global, key_event){
                    debug!("Global action: {global_action}");
                    match global_action {
                        Action::Quit => {
                            break;
                        }
                        Action::OpenKeybindings => {
                            if keybindings_dialog.is_some() {
                                keybindings_dialog = None;
                            } else {
                                let mut dlg = KeybindingsDialog::new();
                                if let Err(err) = dlg.register_config_handler(tab_manager.config.clone()){
                                    error!("Error registering config handler for KeybindingsDialog: {err}");
                                }
                                keybindings_dialog = Some(dlg);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }

            // If keybindings dialog is open, it consumes events first
            if let Some(dialog) = &mut keybindings_dialog {
                match dialog.handle_events(Some(TuiEvent::Key(key_event))) {
                    Ok(Some(Action::DialogClose)) => {
                        keybindings_dialog = None;
                    }
                    Ok(Some(Action::SaveKeybindings)) => {
                        let _ = tab_manager.register_config_handler(dialog.get_config());
                        keybindings_dialog = None;
                    }
                    Ok(Some(Action::SaveWorkspaceState)) => {
                        let _ = tab_manager.save_workspace_state();
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(e) => error!("Error handling KeybindingsDialog event: {e}"),
                }
                continue;
            }
            // Otherwise pass to tab manager
            // Convert to TuiEvent and pass to handle_events
            let tui_event = TuiEvent::Key(key_event);
            match tab_manager.handle_events(Some(tui_event)) {
                Ok(Some(action)) => {
                    // Handle global quit/suspend
                    match action {
                        Action::Quit | Action::Suspend => break,
                        other => {
                            if let Err(e) = tab_manager.update(other) {
                                error!("Error updating after action: {e}");
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => error!("Error handling TuiEvent: {e}"),
            }
        }
        // Tick update (animate progress, etc.)
        if let Ok(Some(a)) = tab_manager.update(Action::Tick)
            && matches!(a, Action::Quit | Action::Suspend) { break; }
    }
    // On exit, attempt to save workspace state if path is valid
    if tab_manager.project_settings_dialog.config.workspace_path.as_ref().is_some_and(|p| p.is_dir()) {
        let _ = tab_manager.save_workspace_state();
    }
    Ok(())
}


