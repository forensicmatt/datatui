use color_eyre::Result;
use std::fs;
use std::path::PathBuf;

use crate::core::llm_config::{
    AzureOpenAiConfig, LlmProvider, LlmSettings, OllamaConfig, OpenAiConfig,
};
use crate::services::embedding_service::{
    embed_azure, embed_ollama, embed_openai, EmbeddingRequest, ProgressCallback,
};

use std::sync::{Arc, RwLock};

/// Service for managing LLM configuration
#[derive(Debug, Clone)]
pub struct LlmService {
    inner: Arc<RwLock<LlmServiceInner>>,
}

#[derive(Debug)]
struct LlmServiceInner {
    settings: LlmSettings,
    config_path: PathBuf,
}

impl LlmService {
    /// Create a new LLM service with the given config path
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        let config_path = config_dir.join(".datatui-llm-settings.toml");
        let settings = Self::load_settings(&config_path)?;

        Ok(Self {
            inner: Arc::new(RwLock::new(LlmServiceInner {
                settings,
                config_path,
            })),
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
        let inner = self.inner.read().unwrap();
        // Ensure parent directory exists
        if let Some(parent) = inner.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_content = toml::to_string_pretty(&inner.settings)?;
        fs::write(&inner.config_path, toml_content)?;

        Ok(())
    }

    /// Get the current settings (cloned for thread safety)
    pub fn settings(&self) -> LlmSettings {
        self.inner.read().unwrap().settings.clone()
    }

    // The `settings_mut` method is removed as it's not compatible with the Arc<RwLock> pattern for external mutable access.
    // Instead, direct setters are provided for specific fields.

    /// Set the default provider
    pub fn set_default_provider(&self, provider: Option<LlmProvider>) {
        self.inner.write().unwrap().settings.default_provider = provider;
    }

    /// Get the default provider
    pub fn get_default_provider(&self) -> Option<LlmProvider> {
        self.inner.read().unwrap().settings.default_provider
    }

    /// Update OpenAI configuration
    pub fn set_openai_config(&self, config: OpenAiConfig) {
        self.inner.write().unwrap().settings.openai = Some(config);
    }

    /// Get OpenAI configuration
    pub fn get_openai_config(&self) -> Option<OpenAiConfig> {
        self.inner.read().unwrap().settings.openai.clone()
    }

    /// Get mutable OpenAI configuration (creates default if not exists)
    pub fn get_or_create_openai_config(&self) -> OpenAiConfig {
        self.inner
            .write()
            .unwrap()
            .settings
            .get_or_create_openai()
            .clone()
    }

    /// Update Azure configuration
    pub fn set_azure_config(&self, config: AzureOpenAiConfig) {
        self.inner.write().unwrap().settings.azure = Some(config);
    }

    /// Get Azure configuration
    pub fn get_azure_config(&self) -> Option<AzureOpenAiConfig> {
        self.inner.read().unwrap().settings.azure.clone()
    }

    /// Get mutable Azure configuration (creates default if not exists)
    pub fn get_or_create_azure_config(&self) -> AzureOpenAiConfig {
        self.inner
            .write()
            .unwrap()
            .settings
            .get_or_create_azure()
            .clone()
    }

    /// Update Ollama configuration
    pub fn set_ollama_config(&self, config: OllamaConfig) {
        self.inner.write().unwrap().settings.ollama = Some(config);
    }

    /// Get Ollama configuration
    pub fn get_ollama_config(&self) -> Option<OllamaConfig> {
        self.inner.read().unwrap().settings.ollama.clone()
    }

    /// Get mutable Ollama configuration (creates default if not exists)
    pub fn get_or_create_ollama_config(&self) -> OllamaConfig {
        self.inner
            .write()
            .unwrap()
            .settings
            .get_or_create_ollama()
            .clone()
    }

    /// Check if a provider is configured
    pub fn is_provider_configured(&self, provider: LlmProvider) -> bool {
        self.inner
            .read()
            .unwrap()
            .settings
            .is_provider_configured(provider)
    }

    /// Get list of configured providers
    pub fn configured_providers(&self) -> Vec<LlmProvider> {
        self.inner.read().unwrap().settings.configured_providers()
    }

    /// Get the config file path
    pub fn config_path(&self) -> PathBuf {
        self.inner.read().unwrap().config_path.clone()
    }

    /// Generate embeddings for the given texts using the specified provider.
    ///
    /// If `provider` is `None`, the configured default provider is used.
    /// Returns one `Vec<f32>` per input text, in order.
    ///
    /// This call is **blocking** — run it on a background thread for UI use.
    pub fn generate_embeddings(
        &self,
        texts: Vec<String>,
        provider: Option<LlmProvider>,
        model: impl Into<String>,
        dimensions: Option<usize>,
        batch_size: Option<usize>,
        progress: Option<ProgressCallback>,
    ) -> Result<Vec<Vec<f32>>> {
        let provider = provider
            .or_else(|| self.get_default_provider())
            .ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "No LLM provider specified and no default provider configured."
                )
            })?;

        let mut req = EmbeddingRequest::new(texts, model);
        if let Some(d) = dimensions {
            req = req.with_dimensions(d);
        }
        if let Some(b) = batch_size {
            req = req.with_batch_size(b);
        }

        let progress_ref = progress.as_ref();

        match provider {
            LlmProvider::OpenAI => {
                let config = self.get_openai_config().ok_or_else(|| {
                    color_eyre::eyre::eyre!("OpenAI is not configured. Open the LLM Management dialog (l) to configure it.")
                })?;
                embed_openai(&config, &req, progress_ref)
            }
            LlmProvider::Azure => {
                let config = self.get_azure_config().ok_or_else(|| {
                    color_eyre::eyre::eyre!("Azure OpenAI is not configured. Open the LLM Management dialog (l) to configure it.")
                })?;
                embed_azure(&config, &req, progress_ref)
            }
            LlmProvider::Ollama => {
                let config = self.get_ollama_config().ok_or_else(|| {
                    color_eyre::eyre::eyre!("Ollama is not configured. Open the LLM Management dialog (l) to configure it.")
                })?;
                embed_ollama(&config, &req, progress_ref)
            }
        }
    }

    /// Generate a single embedding for a query prompt.
    pub fn generate_query_embedding(
        &self,
        text: String,
        provider: Option<LlmProvider>,
        model: impl Into<String>,
        dimensions: Option<usize>,
    ) -> Result<Vec<f32>> {
        let results =
            self.generate_embeddings(vec![text], provider, model, dimensions, None, None)?;
        Ok(results.into_iter().next().unwrap_or_default())
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
        let service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

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
        let service = LlmService::new(temp_dir.path().to_path_buf()).unwrap();

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
