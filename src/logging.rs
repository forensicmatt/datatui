use color_eyre::Result;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::config;

lazy_static::lazy_static! {
    pub static ref LOG_ENV: String = format!("{}_LOG_LEVEL", config::PROJECT_NAME.clone());
    pub static ref LOG_FILE: String = format!("{}.log", env!("CARGO_PKG_NAME"));
}

pub fn init() -> Result<()> { init_with(None, None) }

pub fn init_with(custom_log_path: Option<std::path::PathBuf>, level: Option<tracing::Level>) -> Result<()> {
    // Determine log file path
    let log_path = if let Some(path) = custom_log_path {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        path
    } else {
        let directory = config::get_data_dir();
        std::fs::create_dir_all(directory.clone())?;
        directory.join(LOG_FILE.clone())
    };

    // Configure filter. CLI level overrides env; otherwise try env, else default INFO
    let env_filter = if let Some(lvl) = level {
        EnvFilter::builder()
            .with_default_directive(lvl.into())
            .from_env_lossy()
    } else {
        let builder = EnvFilter::builder().with_default_directive(tracing::Level::INFO.into());
        builder
            .try_from_env()
            .or_else(|_| builder.with_env_var(LOG_ENV.clone()).from_env())?
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
