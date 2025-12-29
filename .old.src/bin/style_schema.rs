#![cfg(feature = "json_schema")]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use datatui::dialog::styling::style_set::StyleSet;
use jsonschema::{Draft, JSONSchema};
use schemars::schema_for;
use std::{fs, path::PathBuf};

/// Generate the JSON Schema for StyleSet or validate a YAML rule-set against it.
#[derive(Parser, Debug)]
#[command(name = "style-schema", about = "StyleSet schema generator and validator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the StyleSet JSON schema (or write it to a file)
    Schema {
        /// Optional output path for the schema JSON
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Validate a YAML rule-set file against the StyleSet schema
    Validate {
        /// Path to the YAML file to validate
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Schema { output } => {
            let schema = schema_for!(StyleSet);
            let json = serde_json::to_string_pretty(&schema)?;

            if let Some(path) = output {
                fs::write(&path, json)?;
                eprintln!("Wrote schema to {}", path.display());
            } else {
                println!("{json}");
            }
        }
        Command::Validate { file } => {
            let schema = schema_for!(StyleSet);
            // jsonschema keeps a reference to the schema; leak a small boxed value to satisfy 'static.
            let schema_json = serde_json::to_value(schema)?;
            let schema_ref: &'static serde_json::Value = Box::leak(Box::new(schema_json));
            let compiled = JSONSchema::options()
                .with_draft(Draft::Draft7)
                .compile(schema_ref)
                .context("failed to compile StyleSet schema")?;

            let yaml_text = fs::read_to_string(&file).context("failed to read YAML file")?;
            // First, deserialize into the Rust type so YAML tags (!Conditional, etc.)
            // are resolved to real enum variants.
            let styleset: StyleSet = serde_yaml::from_str(&yaml_text)
                .context("failed to deserialize YAML into StyleSet")?;
            // Re-serialize to JSON Value for schema validation
            let json_value = serde_json::to_value(styleset)?;

            if let Err(errors) = compiled.validate(&json_value) {
                eprintln!("Validation errors for {}:", file.display());
                for err in errors {
                    eprintln!("- {} at {}", err, err.instance_path);
                }
                std::process::exit(1);
            } else {
                println!("{} is a valid StyleSet YAML", file.display());
            }
        }
    }

    Ok(())
}
