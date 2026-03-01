use serde::{Deserialize, Serialize};

/// LLM provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LlmProvider {
    OpenAI,
    Azure,
    Ollama,
}

impl LlmProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Azure => "Azure OpenAI",
            Self::Ollama => "Ollama",
        }
    }

    pub fn from_display_name(name: &str) -> Option<Self> {
        match name {
            "OpenAI" => Some(Self::OpenAI),
            "Azure OpenAI" => Some(Self::Azure),
            "Ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub fn all() -> Vec<LlmProvider> {
        vec![LlmProvider::OpenAI, LlmProvider::Azure, LlmProvider::Ollama]
    }
}

/// OpenAI configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        let base_url = if !api_key.is_empty() {
            "https://api.openai.com/v1".to_string()
        } else {
            String::new()
        };

        Self { api_key, base_url }
    }
}

impl OpenAiConfig {}

/// Azure OpenAI configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzureOpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub api_version: String,
}

impl Default for AzureOpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: String::new(),
            api_version: "2024-02-15-preview".to_string(),
        }
    }
}

impl AzureOpenAiConfig {}

/// Ollama configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub host: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        let host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());

        Self { host }
    }
}

impl OllamaConfig {}

/// Complete LLM settings configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmSettings {
    #[serde(default)]
    pub default_provider: Option<LlmProvider>,
    #[serde(default)]
    pub openai: Option<OpenAiConfig>,
    #[serde(default)]
    pub azure: Option<AzureOpenAiConfig>,
    #[serde(default)]
    pub ollama: Option<OllamaConfig>,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            default_provider: None,
            openai: None,
            azure: None,
            ollama: None,
        }
    }
}

impl LlmSettings {
    /// Get list of configured providers
    pub fn configured_providers(&self) -> Vec<LlmProvider> {
        let mut providers = Vec::new();

        if self.openai.is_some() {
            providers.push(LlmProvider::OpenAI);
        }

        if self.azure.is_some() {
            providers.push(LlmProvider::Azure);
        }

        if self.ollama.is_some() {
            providers.push(LlmProvider::Ollama);
        }

        providers
    }

    /// Check if a provider is configured
    pub fn is_provider_configured(&self, provider: LlmProvider) -> bool {
        match provider {
            LlmProvider::OpenAI => self.openai.is_some(),
            LlmProvider::Azure => self.azure.is_some(),
            LlmProvider::Ollama => self.ollama.is_some(),
        }
    }

    /// Get or create OpenAI config
    pub fn get_or_create_openai(&mut self) -> &mut OpenAiConfig {
        if self.openai.is_none() {
            self.openai = Some(OpenAiConfig::default());
        }
        self.openai.as_mut().unwrap()
    }

    /// Get or create Azure config
    pub fn get_or_create_azure(&mut self) -> &mut AzureOpenAiConfig {
        if self.azure.is_none() {
            self.azure = Some(AzureOpenAiConfig::default());
        }
        self.azure.as_mut().unwrap()
    }

    /// Get or create Ollama config
    pub fn get_or_create_ollama(&mut self) -> &mut OllamaConfig {
        if self.ollama.is_none() {
            self.ollama = Some(OllamaConfig::default());
        }
        self.ollama.as_mut().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config_default() {
        let config = OpenAiConfig::default();
        // Should have base_url set if api_key from env, otherwise empty
        assert!(!config.base_url.is_empty() || config.api_key.is_empty());
    }

    #[test]
    fn test_azure_config_default() {
        let config = AzureOpenAiConfig::default();
        assert_eq!(config.api_version, "2024-02-15-preview");
    }

    #[test]
    fn test_ollama_config_default() {
        let config = OllamaConfig::default();
        assert!(!config.host.is_empty());
    }

    #[test]
    fn test_llm_settings_configured_providers() {
        let mut settings = LlmSettings::default();

        // None should be configured by default
        let configured = settings.configured_providers();
        assert!(configured.is_empty());

        // Configure OpenAI
        settings.openai = Some(OpenAiConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
        });

        let configured = settings.configured_providers();
        assert!(configured.contains(&LlmProvider::OpenAI));
    }

    #[test]
    fn test_provider_display_names() {
        assert_eq!(LlmProvider::OpenAI.display_name(), "OpenAI");
        assert_eq!(LlmProvider::Azure.display_name(), "Azure OpenAI");
        assert_eq!(LlmProvider::Ollama.display_name(), "Ollama");
    }
}
