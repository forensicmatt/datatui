//! LLM Provider Configuration Dialogs
//! 
//! This module contains individual dialogs for configuring each LLM provider.
//! Each dialog is responsible for editing the specific configuration for its provider type.

pub mod azure_openai;
pub mod openai;
pub mod ollama;

pub use azure_openai::{AzureOpenAiConfigDialog, AzureOpenAiConfig};
pub use openai::{OpenAiConfigDialog, OpenAIConfig};
pub use ollama::{OllamaConfigDialog, OllamaConfig};


pub trait LlmConfig {
    /// Returns true if the configuration is considered valid & complete, otherwise false.
    fn is_configured(&self) -> bool;
}
