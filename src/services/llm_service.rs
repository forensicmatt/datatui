use color_eyre::Result;
use std::fs;
use std::path::PathBuf;

use crate::core::llm_config::{
    AzureOpenAiConfig, LlmProvider, LlmSettings, OllamaConfig, OpenAiConfig,
};

/// Service for managing LLM configuration
#[derive(Debug, Clone)]
pub struct LlmService {
    settings: LlmSettings,
    config_path: PathBuf,
}

impl LlmService {
    /// Create a new LLM service with the given config path
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        let config_path = config_dir.join(".datatui-llm-settings.toml");
        let settings = Self::load_settings(&config_path)?;

        Ok(Self {
            settings,
            config_path,
        })
    }

    /// Load settings from the config file, creating defaults if it doesn't exist
    fn load_settings(config_path: &PathBuf) -> Result<LlmSettings> {
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            let settings = toml::from_str(&content)?;
            Ok(settings)
        } else {
            // Return defaults which will be saved on first update
            Ok(LlmSettings::default())
        }
    }

    /// Save current settings to the config file
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_content = toml::to_string_pretty(&self.settings)?;
        fs::write(&self.config_path, toml_content)?;

        Ok(())
    }

    /// Get the current settings (read-only)
    pub fn settings(&self) -> &LlmSettings {
        &self.settings
    }

    /// Get mutable settings
    pub fn settings_mut(&mut self) -> &mut LlmSettings {
        &mut self.settings
    }

    /// Set the default provider
    pub fn set_default_provider(&mut self, provider: Option<LlmProvider>) {
        self.settings.default_provider = provider;
    }

    /// Get the default provider
    pub fn get_default_provider(&self) -> Option<LlmProvider> {
        self.settings.default_provider
    }

    /// Update OpenAI configuration
    pub fn set_openai_config(&mut self, config: OpenAiConfig) {
        self.settings.openai = Some(config);
    }

    /// Get OpenAI configuration
    pub fn get_openai_config(&self) -> Option<&OpenAiConfig> {
        self.settings.openai.as_ref()
    }

    /// Get mutable OpenAI configuration (creates default if not exists)
    pub fn get_or_create_openai_config(&mut self) -> &mut OpenAiConfig {
        self.settings.get_or_create_openai()
    }

    /// Update Azure configuration
    pub fn set_azure_config(&mut self, config: AzureOpenAiConfig) {
        self.settings.azure = Some(config);
    }

    /// Get Azure configuration
    pub fn get_azure_config(&self) -> Option<&AzureOpenAiConfig> {
        self.settings.azure.as_ref()
    }

    /// Get mutable Azure configuration (creates default if not exists)
    pub fn get_or_create_azure_config(&mut self) -> &mut AzureOpenAiConfig {
        self.settings.get_or_create_azure()
    }

    /// Update Ollama configuration
    pub fn set_ollama_config(&mut self, config: OllamaConfig) {
        self.settings.ollama = Some(config);
    }

    /// Get Ollama configuration
    pub fn get_ollama_config(&self) -> Option<&OllamaConfig> {
        self.settings.ollama.as_ref()
    }

    /// Get mutable Ollama configuration (creates default if not exists)
    pub fn get_or_create_ollama_config(&mut self) -> &mut OllamaConfig {
        self.settings.get_or_create_ollama()
    }

    /// Check if a provider is configured
    pub fn is_provider_configured(&self, provider: LlmProvider) -> bool {
        self.settings.is_provider_configured(provider)
    }

    /// Get list of configured providers
    pub fn configured_providers(&self) -> Vec<LlmProvider> {
        self.settings.configured_providers()
    }

    /// Get the config file path
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_service() {
        let temp_dir = TempDir::new().unwrap();
        let service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

        assert!(
            service.settings().configured_providers().is_empty()
                || service.settings().configured_providers().len() <= 2
        ); // Might have env defaults
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let mut service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

        // Set some configuration
        service.set_openai_config(OpenAiConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        });
        service.set_default_provider(Some(LlmProvider::OpenAI));

        // Save
        service.save().unwrap();

        // Load again
        let loaded_service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

        assert_eq!(
            loaded_service.get_default_provider(),
            Some(LlmProvider::OpenAI)
        );
        assert!(loaded_service.get_openai_config().is_some());
        assert_eq!(
            loaded_service.get_openai_config().unwrap().api_key,
            "test-key"
        );
    }

    #[test]
    fn test_provider_configuration() {
        let temp_dir = TempDir::new().unwrap();
        let mut service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

        // Initially not configured
        assert!(
            !service.is_provider_configured(LlmProvider::OpenAI)
                || std::env::var("OPENAI_API_KEY").is_ok()
        ); // Unless env var is set

        // Configure
        service.set_openai_config(OpenAiConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        });

        // Now configured
        assert!(service.is_provider_configured(LlmProvider::OpenAI));
    }
}
