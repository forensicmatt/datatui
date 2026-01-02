use color_eyre::Result;
use tracing_error::ErrorLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

lazy_static::lazy_static! {
    pub static ref LOG_FILE: String = format!("{}.log", env!("CARGO_PKG_NAME"));
}

/// Initialize logging with default settings (WARN level)
pub fn init() -> Result<()> {
    init_with(None, None)
}

/// Initialize logging with custom path and/or level
pub fn init_with(
    custom_log_path: Option<std::path::PathBuf>,
    level: Option<tracing::Level>,
) -> Result<()> {
    // Determine log file path
    let log_path = if let Some(path) = custom_log_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        path
    } else {
        // Default: log to current working directory
        let cwd = std::env::current_dir()?;
        cwd.join(LOG_FILE.clone())
    };

    // Configure filter. CLI level overrides env; otherwise use INFO as default
    let env_filter = if let Some(lvl) = level {
        EnvFilter::builder()
            .with_default_directive(lvl.into())
            .from_env_lossy()
    } else {
        EnvFilter::builder()
            .with_default_directive(tracing::Level::WARN.into())
            .from_env_lossy()
    };

    let writer_path = log_path.clone();
    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(move || {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&writer_path)
                .expect("failed to open log file")
        })
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);

    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .try_init()?;

    Ok(())
}
