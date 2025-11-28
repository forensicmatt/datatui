use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::io;
use std::io::Read;
use std::fs;
use crossterm::event::{self, Event as CEvent, EnableMouseCapture, DisableMouseCapture};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui::prelude::{Rect, Widget};
use std::time::Duration;
use datatui::dialog::{DataTabManagerDialog, KeybindingsDialog, MessageDialog};
use datatui::style::StyleConfig;
use datatui::config::Config;
use datatui::components::Component;
use datatui::tui::Event as TuiEvent;
use datatui::action::Action;
use datatui::update_check;
use datatui::data_import_types::DataImportConfig;
use std::thread;
use datatui::dialog::csv_options_dialog::CsvImportOptions;
use datatui::dialog::xlsx_options_dialog::XlsxImportOptions;
use datatui::dialog::sqlite_options_dialog::SqliteImportOptions;
use datatui::dialog::parquet_options_dialog::ParquetImportOptions;
use datatui::dialog::json_options_dialog::JsonImportOptions;
use datatui::excel_operations::ExcelOperations;
use color_eyre::Result;
use tracing::error;
use uuid::Uuid;
use glob::glob;

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
    /// Load one or more datasets on startup. Repeat per dataset. Syntax: kind:path;key=value;...
    /// Examples: --load 'text:C:\\data\\a.csv;delim=comma;header=true'
    ///           --load 'xlsx:C:\\data\\book.xlsx;all_sheets=true'
    ///           --load 'sqlite:C:\\db\\app.sqlite;table=users'
    ///           --load 'json:STDIN' (reads from stdin into a temp file)
    #[arg(long = "load", value_name = "SPEC")]
    load: Vec<String>,
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

    // Set up terminal (render first before heavy work so users see UI immediately)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App loop (defer --load processing until after the first frame renders)
    let res = run_app(&mut terminal, &mut tab_manager, args.load);

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
    initial_load_specs: Vec<String>,
) -> color_eyre::Result<()> {
    let mut pending_load_specs: Option<Vec<String>> = if initial_load_specs.is_empty() { None } else { Some(initial_load_specs) };
    // Optional global Keybindings dialog overlay, opened via a global shortcut
    let mut keybindings_dialog: Option<KeybindingsDialog> = None;
    // Optional update check message dialog
    let mut update_message_dialog: Option<MessageDialog> = None;
    
    // Spawn update check in background thread if enabled (or if it's the first run and we just enabled it)
    let mut update_check_receiver: Option<std::sync::mpsc::Receiver<Result<Option<update_check::UpdateInfo>, color_eyre::Report>>> = None;
    if update_check::should_check_for_updates(tab_manager.config.next_update_check) {
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        update_check_receiver = Some(rx);
        
        // Spawn blocking update check in background thread
        thread::spawn(move || {
            let result = update_check::check_for_update(&current_version);
            let _ = tx.send(result);
        });
    }
    
    loop {
        terminal.draw(|f| {
            let size = f.area();
            tab_manager.draw(f, size).unwrap();
            // When open, render the keybindings dialog on top
            if let Some(dialog) = &mut keybindings_dialog {
                let _ = dialog.draw(f, size);
            }
            // When open, render the update message dialog on top (centered modal)
            if let Some(dialog) = &mut update_message_dialog {
                use ratatui::widgets::Clear;
                Clear.render(size, f.buffer_mut());
                // Center the dialog
                let dialog_width = (size.width as f32 * 0.6).min(80.0) as u16;
                let dialog_height = (size.height as f32 * 0.4).min(15.0) as u16;
                let dialog_x = size.x + (size.width.saturating_sub(dialog_width)) / 2;
                let dialog_y = size.y + (size.height.saturating_sub(dialog_height)) / 2;
                let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);
                let _ = dialog.draw(f, dialog_area);
            }
        })?;
        // After drawing, process queued Render work (overlay is now visible)
        let _ = tab_manager.update(Action::Render);

        // If we have deferred --load specs, process them now that UI has rendered at least once
        if let Some(specs) = pending_load_specs.take() {
            // Show an immediate status that we're detecting input formats before heavy work
            tab_manager.show_data_management = true;
            {
                let dmd = &mut tab_manager.data_management_dialog;
                dmd.busy_active = true;
                dmd.busy_message = "Detecting input formats...".to_string();
                dmd.busy_progress = 0.0;
                dmd.queue_total = 0;
                dmd.queue_done = 0;
                dmd.current_loading = None;
            }
            // Draw once to show the detection message before we start processing
            terminal.draw(|f| {
                let size = f.area();
                tab_manager.draw(f, size).unwrap();
            })?;
            let _ = tab_manager.update(Action::Render);

            match materialize_and_add_loads(&specs, tab_manager) {
                Ok(added) => {
                    if added > 0 {
                        // Show Data Management so progress is visible while imports run
                        tab_manager.show_data_management = true;
                        let _ = tab_manager.data_management_dialog.begin_queued_import();
                    } else {
                        // Nothing to import; clear busy overlay
                        let dmd = &mut tab_manager.data_management_dialog;
                        dmd.busy_active = false;
                        dmd.busy_message.clear();
                        dmd.busy_progress = 0.0;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to process --load specs: {e}");
                    return Err(e);
                }
            }
        }
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
        
        // Check if update check has completed (non-blocking)
        if let Some(ref receiver) = update_check_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    update_check_receiver = None;
                    let current_version = env!("CARGO_PKG_VERSION");
                    match result {
                        Ok(Some(update_info)) => {
                            // Store update status in DataManagementDialog (Some(Some(...)) = update available)
                            tab_manager.data_management_dialog.update_status = Some(Some(update_info.clone()));
                            
                            let message = format!(
                                "A new version of DataTUI is available!\n\nCurrent version: v{}\nLatest version: {}\n\nDownload: {}",
                                current_version,
                                update_info.latest_version,
                                update_info.download_url
                            );
                            let mut dialog = MessageDialog::with_title(message, "Update Available");
                            let _ = dialog.register_config_handler(tab_manager.config.clone());
                            update_message_dialog = Some(dialog);
                            
                            // Update next check date (1 day from now)
                            tab_manager.config.next_update_check = Some(update_check::calculate_next_check_date());
                            let _ = tab_manager.config.save();
                        }
                        Ok(None) => {
                            // No update available - set to Some(None) to indicate check completed with no update
                            tab_manager.data_management_dialog.update_status = Some(None);
                            
                            // Update next check date (1 day from now)
                            tab_manager.config.next_update_check = Some(update_check::calculate_next_check_date());
                            let _ = tab_manager.config.save();
                        }
                        Err(e) => {
                            error!("Update check failed: {}", e);
                            // Don't set update_status on error - leave it as is
                            // Still update the check date to avoid checking again immediately
                            tab_manager.config.next_update_check = Some(update_check::calculate_next_check_date());
                            let _ = tab_manager.config.save();
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still waiting for result, continue
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Sender dropped, check failed
                    update_check_receiver = None;
                }
            }
        }
        
        // Poll for events
        if event::poll(Duration::from_millis(100))?
            && let CEvent::Key(key_event) = event::read()? {
                if let Some(global_action) = tab_manager.config.action_for_key(datatui::config::Mode::Global, key_event){
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

            // If update message dialog is open, it consumes events first
            if let Some(dialog) = &mut update_message_dialog {
                match dialog.handle_events(Some(TuiEvent::Key(key_event))) {
                    Ok(Some(Action::DialogClose)) => {
                        update_message_dialog = None;
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(e) => error!("Error handling UpdateMessageDialog event: {e}"),
                }
                continue;
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



// Ensure only known option keys are present for a given kind; otherwise return an error
fn ensure_only_allowed_keys(kind: &str, kv: &std::collections::HashMap<String, String>, allowed: &[&str]) -> color_eyre::Result<()> {
    let mut unknown: Vec<String> = Vec::new();
    for k in kv.keys() {
        if !allowed.contains(&k.as_str()) {
            unknown.push(k.clone());
        }
    }
    if !unknown.is_empty() {
        return Err(color_eyre::eyre::eyre!(format!(
            "Unknown option(s) for {kind}: {}",
            unknown.join(", ")
        )));
    }
    Ok(())
}

// Parse and add --load specs to the DataTabManagerDialog.
fn materialize_and_add_loads(specs: &[String], tab_manager: &mut DataTabManagerDialog) -> color_eyre::Result<usize> {
    let mut added = 0usize;
    for s in specs {
        match parse_load_spec(s) {
            Ok(cfgs) => {
                for cfg in cfgs {
                    tab_manager.data_management_dialog.add_data_source(cfg);
                    added = added.saturating_add(1);
                }
            }
            Err(e) => {
                error!("Invalid --load spec '{s}': {e}");
                panic!("Invalid --load spec '{s}': {e}");
            }
        }
    }
    Ok(added)
}

// Returns one or multiple DataImportConfig values for a single spec (e.g., xlsx sheets can expand).
fn parse_load_spec(spec: &str) -> color_eyre::Result<Vec<DataImportConfig>> {
    // Split on the first ':' into kind and the rest
    let (kind_raw, rest) = spec
        .split_once(':')
        .ok_or_else(|| color_eyre::eyre::eyre!("Expected 'kind:path[;key=value...]'"))?;
    let kind = kind_raw.trim().to_ascii_lowercase();

    // Path is first segment before ';', options follow as key=value pairs separated by ';'
    let mut parts = rest.split(';');
    let mut path = parts.next().unwrap_or("").trim().to_string();
    let mut kv: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for seg in parts {
        if seg.trim().is_empty() { continue; }
        if let Some((k, v)) = seg.split_once('=') {
            kv.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        } else {
            // bare flags like 'all_sheets' => true
            kv.insert(seg.trim().to_ascii_lowercase(), "true".to_string());
        }
    }

    // Handle STDIN magic path for supported kinds
    if path.eq_ignore_ascii_case("stdin") || path == "-" {
        path = write_stdin_to_temp_file(&kind, &kv)?;
    }

    // Expand wildcards (glob). If none, returns the original path.
    let paths = expand_glob_paths(&path)?;

    match kind.as_str() {
        // Text/CSV-like
        "text" | "csv" | "tsv" | "psv" => {
            ensure_only_allowed_keys(
                &kind,
                &kv,
                &[
                    "delim",
                    "delimiter",
                    "header",
                    "has_header",
                    "quote",
                    "quote_char",
                    "escape",
                    "escape_char",
                    "merge",
                ],
            )?;
            let mut out = Vec::new();
            let merge = kv.get("merge").map(|v| parse_bool(v)).unwrap_or(false);
            if !paths.is_empty() {
                // First path becomes primary; rest go into additional_paths and merge flag.
                let mut primary: Option<DataImportConfig> = None;
                let mut extras: Vec<PathBuf> = Vec::new();
                for (idx, pb) in paths.iter().enumerate() {
                    let mut opts = CsvImportOptions::default();
                    // Kind shortcuts for delimiter
                    if kind == "tsv" { opts.delimiter = '\t'; }
                    if kind == "psv" { opts.delimiter = '|'; }
                    if kind == "csv" { opts.delimiter = ','; }
                    // Guess from extension if not overridden
                    if let Some(ext) = pb.extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_ascii_lowercase()
                    )
                        && opts.delimiter == ',' {
                            if ext == "tsv" { 
                                opts.delimiter = '\t';
                            } else if ext == "psv" { 
                                opts.delimiter = '|'; 
                            }
                        }
                    if let Some(v) = kv.get("delim").or_else(|| kv.get("delimiter")) {
                        opts.delimiter = parse_delimiter(v)?;
                    }
                    if let Some(v) = kv.get("header").or_else(|| kv.get("has_header")) {
                        opts.has_header = parse_bool(v);
                    }
                    if let Some(v) = kv.get("quote").or_else(|| kv.get("quote_char")) {
                        opts.quote_char = parse_char_opt(v);
                    }
                    if let Some(v) = kv.get("escape").or_else(|| kv.get("escape_char")) {
                        opts.escape_char = parse_char_opt(v);
                    }
                    if idx == 0 {
                        let mut cfg = DataImportConfig::text(pb.clone(), opts);
                        if let DataImportConfig::Text(ref mut t) = cfg {
                            t.merge = merge;
                        }
                        primary = Some(cfg);
                    } else {
                        extras.push(pb.clone());
                    }
                }
                if let Some(mut cfg) = primary {
                    if let DataImportConfig::Text(ref mut t) = cfg {
                        t.additional_paths = extras;
                    }
                    out.push(cfg);
                }
            }
            Ok(out)
        }
        // Excel
        "xlsx" | "xls" => {
            ensure_only_allowed_keys(&kind, &kv, &["all_sheets", "sheets", "sheet"]) ?;
            let mut out = Vec::new();
            for pb in paths {
                // Default: load worksheet info and mark all load=true
                let mut worksheets = ExcelOperations::read_worksheet_info(&pb).unwrap_or_default();
                // Filters
                let all = kv.get("all_sheets").map(|v| parse_bool(v)).unwrap_or(true);
                if !all
                    && let Some(names) = kv.get("sheets").or_else(|| kv.get("sheet")) {
                        let set: std::collections::HashSet<String> = names.split(',').map(|s| s.trim().to_string()).collect();
                        for ws in &mut worksheets { ws.load = set.contains(&ws.name); }
                        worksheets.retain(|w| w.load);
                    }
                let opts = XlsxImportOptions { worksheets };
                out.push(DataImportConfig::excel(pb, opts));
            }
            Ok(out)
        }
        // SQLite
        "sqlite" | "db" => {
            ensure_only_allowed_keys(&kind, &kv, &["import_all_tables", "table", "tables"]) ?;
            let mut out = Vec::new();
            for pb in paths {
                let mut opts = SqliteImportOptions::default();
                if let Some(v) = kv.get("import_all_tables") { opts.import_all_tables = parse_bool(v); }
                if let Some(t) = kv.get("table") { opts.import_all_tables = false; opts.selected_tables = vec![t.to_string()]; }
                if let Some(ts) = kv.get("tables") { opts.import_all_tables = false; opts.selected_tables = ts.split(',').map(|s| s.trim().to_string()).collect(); }
                out.push(DataImportConfig::sqlite(pb, opts));
            }
            Ok(out)
        }
        // Parquet
        "parquet" => {
            ensure_only_allowed_keys(&kind, &kv, &[])?;
            let mut out = Vec::new();
            for pb in paths {
                let opts = ParquetImportOptions::default();
                out.push(DataImportConfig::parquet(pb, opts));
            }
            Ok(out)
        }
        // JSON / NDJSON
        "json" | "jsonl" | "ndjson" => {
            ensure_only_allowed_keys(&kind, &kv, &["ndjson", "records", "merge"]) ?;
            let mut out = Vec::new();
            let merge = kv.get("merge").map(|v| parse_bool(v)).unwrap_or(false);
            if !paths.is_empty() {
                let mut primary: Option<DataImportConfig> = None;
                let mut extras: Vec<PathBuf> = Vec::new();
                for (idx, pb) in paths.iter().enumerate() {
                    let mut opts = JsonImportOptions::default();
                    if kind == "jsonl" || kind == "ndjson" { opts.ndjson = true; }
                    if let Some(v) = kv.get("ndjson") { opts.ndjson = parse_bool(v); }
                    if let Some(expr) = kv.get("records") { opts.records_expr = expr.to_string(); }
                    if idx == 0 {
                        let mut cfg = DataImportConfig::json(pb.clone(), opts);
                        if let DataImportConfig::Json(ref mut j) = cfg {
                            j.merge = merge;
                        }
                        primary = Some(cfg);
                    } else {
                        extras.push(pb.clone());
                    }
                }
                if let Some(mut cfg) = primary {
                    if let DataImportConfig::Json(ref mut j) = cfg {
                        j.additional_paths = extras;
                    }
                    out.push(cfg);
                }
            }
            Ok(out)
        }
        other => Err(color_eyre::eyre::eyre!(format!("Unknown load kind '{other}'")))
    }
}

fn parse_bool(v: &str) -> bool {
    matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn parse_char_opt(v: &str) -> Option<char> {
    if v.eq_ignore_ascii_case("none") || v.is_empty() { return None; }
    if v.starts_with("char:") {
        return v.chars().nth(5);
    }
    let unquoted = v.trim_matches('\'').trim_matches('"');
    unquoted.chars().next()
}

fn parse_delimiter(v: &str) -> color_eyre::Result<char> {
    let s = v.to_ascii_lowercase();
    Ok(match s.as_str() {
        "," | "comma" => ',',
        "\t" | "tab" => '\t',
        "|" | "pipe" | "psv" => '|',
        "space" => ' ',
        _ if s.starts_with("char:") => s.chars().nth(5).ok_or_else(|| color_eyre::eyre::eyre!("Missing char after 'char:'"))?,
        _ => s.chars().next().ok_or_else(|| color_eyre::eyre::eyre!("Invalid delimiter"))?,
    })
}

// If path is STDIN/-: read stdin bytes and write to a temp file with an extension based on kind.
fn write_stdin_to_temp_file(kind: &str, kv: &std::collections::HashMap<String, String>) -> color_eyre::Result<String> {
    let mut stdin = std::io::stdin();
    // If attached to terminal, we will still block waiting for input; it's the user's responsibility.
    let mut buf: Vec<u8> = Vec::with_capacity(1024 * 64);
    stdin.read_to_end(&mut buf)?;
    let ext = match kind {
        "text" | "csv" | "tsv" | "psv" => {
            match kv.get("delim").map(|s| s.as_str()) {
                Some("tab") => "tsv",
                Some("psv") | Some("pipe") => "psv",
                _ => "csv",
            }
        }
        "json" | "jsonl" | "ndjson" => {
            if kv.get("ndjson").map(|v| parse_bool(v)).unwrap_or(kind == "jsonl" || kind == "ndjson") { "jsonl" } else { "json" }
        }
        "parquet" => "parquet",
        "xlsx" | "xls" => "xlsx",
        "sqlite" | "db" => "sqlite",
        _ => "dat",
    };
    let tmp = std::env::temp_dir().join(format!("datatui_stdin_{}.{}", Uuid::new_v4(), ext));
    fs::write(&tmp, buf)?;
    Ok(tmp.to_string_lossy().to_string())
}

// Expand a potential glob into concrete paths. If no wildcard is present or expansion
// yields no matches, fall back to the original path as a single entry.
fn expand_glob_paths(input: &str) -> color_eyre::Result<Vec<PathBuf>> {
    let has_wildcards = input.contains('*') || input.contains('?') || input.contains('[');
    if !has_wildcards {
        return Ok(vec![PathBuf::from(input)]);
    }
    let mut out = Vec::new();
    for entry in glob(input).map_err(|e| color_eyre::eyre::eyre!(format!("Invalid glob pattern '{input}': {e}")))? {
        match entry {
            Ok(p) => out.push(p),
            Err(e) => error!("Glob error on '{input}': {e}"),
        }
    }
    if out.is_empty() {
        // If nothing matched, treat as literal to avoid surprising drops
        Ok(vec![PathBuf::from(input)])
    } else {
        Ok(out)
    }
}
