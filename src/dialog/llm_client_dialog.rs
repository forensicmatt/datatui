//! LlmClientDialog: Main dialog for selecting and configuring LLM providers
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType, List, ListItem, ListState};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use textwrap::wrap;
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;
use serde::{Deserialize, Serialize};
use strum::Display;
use rig::client::builder::{DynClientBuilder, BoxCompletionModel};
use rig::embeddings::embedding::EmbeddingModelDyn;
use crate::dialog::llm::{
    AzureOpenAiConfigDialog, OpenAiConfigDialog,
    OllamaConfigDialog, AzureOpenAiConfig, OpenAIConfig, OllamaConfig
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
pub enum LlmProvider {
    Azure,
    OpenAI,
    Ollama,
}

impl LlmProvider {
    pub fn display_name(&self) -> &'static str {
        match self {
            LlmProvider::Azure => "Azure OpenAI",
            LlmProvider::OpenAI => "OpenAI",
            LlmProvider::Ollama => "Ollama",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmClientDialogMode {
    ProviderSelection,
    AzureConfiguration,
    OpenAIConfiguration,
    OllamaConfiguration,
    Error(String),
}

#[derive(Debug)]
pub struct LlmClientDialog {
    pub mode: LlmClientDialogMode,
    pub selected_provider: LlmProvider,
    pub provider_list_state: ListState,
    pub error_active: bool,
    pub show_instructions: bool,
    pub config: Config,
    pub llm_config: LlmConfig,
    // Individual provider dialogs
    pub azure_dialog: AzureOpenAiConfigDialog,
    pub openai_dialog: OpenAiConfigDialog,
    pub ollama_dialog: OllamaConfigDialog,
}

#[derive(Serialize, Deserialize)]
pub struct LlmConfig {
    pub azure: Option<AzureOpenAiConfig>,
    pub openai: Option<OpenAIConfig>,
    pub ollama: Option<OllamaConfig>,
    #[serde(skip)]
    builders: HashMap<LlmProvider, DynClientBuilder>,
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig")
            .field("azure", &self.azure)
            .field("openai", &self.openai)
            .field("ollama", &self.ollama)
            .finish()
    }
}

impl Clone for LlmConfig {
    fn clone(&self) -> Self {
        Self {
            azure: self.azure.clone(),
            openai: self.openai.clone(),
            ollama: self.ollama.clone(),
            builders: HashMap::new(),
        }
    }
}

impl PartialEq for LlmConfig {
    fn eq(&self, other: &Self) -> bool {
        self.azure == other.azure && self.openai == other.openai && self.ollama == other.ollama
    }
}

impl Eq for LlmConfig {}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            azure: None,
            openai: None,
            ollama: None,
            builders: HashMap::new(),
        }
    }
}

impl LlmConfig {
    /// Returns a list of configured providers
    /// If no providers are configured, return None.
    pub fn configured_list(&self) -> Option<Vec<LlmProvider>> {
        let mut providers = Vec::new();

        if let Some(cfg) = &self.azure {
            if crate::dialog::llm::LlmConfig::is_configured(cfg) {
                providers.push(LlmProvider::Azure);
            }
        }

        if let Some(cfg) = &self.openai {
            if crate::dialog::llm::LlmConfig::is_configured(cfg) {
                providers.push(LlmProvider::OpenAI);
            }
        }

        if let Some(cfg) = &self.ollama {
            if crate::dialog::llm::LlmConfig::is_configured(cfg) {
                providers.push(LlmProvider::Ollama);
            }
        }

        if providers.is_empty() {
            None
        } else {
            Some(providers)
        }
    }

    /// Store a dynamic client builder for a given provider
    pub fn set_builder(&mut self, provider: LlmProvider, builder: DynClientBuilder) {
        self.builders.insert(provider, builder);
    }

    /// Retrieve a stored dynamic client builder for a given provider, if available
    pub fn get_builder(&self, provider: &LlmProvider) -> Option<&DynClientBuilder> {
        self.builders.get(provider)
    }

    fn provider_key(provider: &LlmProvider) -> &'static str {
        match provider {
            LlmProvider::OpenAI => "openai",
            LlmProvider::Azure => "azure",
            LlmProvider::Ollama => "ollama",
        }
    }

    pub fn default_embedding_model_for(&self, provider: &LlmProvider) -> &'static str {
        match provider {
            LlmProvider::OpenAI => "text-embedding-3-small",
            LlmProvider::Azure => "text-embedding-3-small",
            LlmProvider::Ollama => "nomic-embed-text",
        }
    }

    pub fn default_completion_model_for(&self, provider: &LlmProvider) -> &'static str {
        match provider {
            LlmProvider::OpenAI => "gpt-4o-mini",
            LlmProvider::Azure => "gpt-4o-mini",
            LlmProvider::Ollama => "llama3.1",
        }
    }

    /// Construct an EmbeddingModelDyn using the stored builder and default model for the provider
    pub fn get_embedding_model_dyn<'a>(&'a self, provider: LlmProvider) -> Result<Box<dyn EmbeddingModelDyn + 'a>> {
        let model = self.default_embedding_model_for(&provider);
        self.get_embedding_model_dyn_with(provider, model)
    }

    /// Construct an EmbeddingModelDyn using the stored builder and an explicit model name
    pub fn get_embedding_model_dyn_with<'a>(&'a self, provider: LlmProvider, model: &str) -> Result<Box<dyn EmbeddingModelDyn + 'a>> {
        let builder = self.builders.get(&provider)
            .ok_or_else(|| color_eyre::eyre::eyre!("No client builder registered for provider: {}", provider.display_name()))?;
        let provider_key = Self::provider_key(&provider);
        let model = builder
            .embeddings(provider_key, model)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build embedding model for {}: {}", provider.display_name(), e))?;
        Ok(model)
    }

    /// Construct a BoxCompletionModel using the stored builder and default model for the provider
    pub fn get_completion_model_box<'a>(&'a self, provider: LlmProvider) -> Result<BoxCompletionModel<'a>> {
        let model = self.default_completion_model_for(&provider);
        self.get_completion_model_box_with(provider, model)
    }

    /// Construct a BoxCompletionModel using the stored builder and an explicit model name
    pub fn get_completion_model_box_with<'a>(&'a self, provider: LlmProvider, model: &str) -> Result<BoxCompletionModel<'a>> {
        let builder = self.builders.get(&provider)
            .ok_or_else(|| color_eyre::eyre::eyre!("No client builder registered for provider: {}", provider.display_name()))?;
        let provider_key = Self::provider_key(&provider);
        let model = builder
            .completion(provider_key, model)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build completion model for {}: {}", provider.display_name(), e))?;
        Ok(model)
    }

    /// Get or create Azure config
    pub fn get_or_create_azure(&mut self) -> &mut AzureOpenAiConfig {
        if self.azure.is_none() {
            self.azure = Some(AzureOpenAiConfig::default());
        }
        self.azure.as_mut().unwrap()
    }

    /// Get or create OpenAI config
    pub fn get_or_create_openai(&mut self) -> &mut OpenAIConfig {
        if self.openai.is_none() {
            self.openai = Some(OpenAIConfig::default());
        }
        self.openai.as_mut().unwrap()
    }

    /// Get or create Ollama config
    pub fn get_or_create_ollama(&mut self) -> &mut OllamaConfig {
        if self.ollama.is_none() {
            self.ollama = Some(OllamaConfig::default());
        }
        self.ollama.as_mut().unwrap()
    }

    /// Fetch embeddings using the selected provider and model
    pub fn fetch_embeddings_via_provider(
        &self,
        provider: LlmProvider,
        model_name: &str,
        inputs: &Vec<String>,
        dims_opt: Option<usize>,
    ) -> color_eyre::Result<Vec<Vec<f32>>> {
        match provider {
            LlmProvider::OpenAI => self.fetch_openai_embeddings(model_name, inputs, dims_opt),
            LlmProvider::Azure => self.fetch_azure_embeddings(model_name, inputs, dims_opt),
            LlmProvider::Ollama => self.fetch_ollama_embeddings(model_name, inputs),
        }
    }

    fn fetch_openai_embeddings(
        &self,
        model_name: &str,
        inputs: &Vec<String>,
        dims_opt: Option<usize>,
    ) -> color_eyre::Result<Vec<Vec<f32>>> {
        let cfg = self.openai.as_ref().ok_or_else(|| color_eyre::eyre::eyre!("OpenAI config is not set"))?;
        let url = format!("{}/embeddings", cfg.base_url.trim_end_matches('/'));
        let client = reqwest::blocking::Client::new();
        #[derive(serde::Serialize)]
        struct OpenAIEmbReq<'a> { model: &'a str, input: &'a Vec<String>, #[serde(skip_serializing_if="Option::is_none")] dimensions: Option<usize> }
        #[derive(serde::Deserialize)]
        struct OpenAIEmbRes { data: Vec<OpenAIEmbDatum> }
        #[derive(serde::Deserialize)]
        struct OpenAIEmbDatum { embedding: Vec<f32> }
        let req = OpenAIEmbReq { model: model_name, input: inputs, dimensions: dims_opt };
        let res = client.post(url)
            .bearer_auth(&cfg.api_key)
            .json(&req)
            .send()
            .map_err(|e| color_eyre::eyre::eyre!("OpenAI embeddings request failed: {e}"))?;
        if !res.status().is_success() { return Err(color_eyre::eyre::eyre!("OpenAI embeddings HTTP error: {}", res.status())); }
        let parsed: OpenAIEmbRes = res.json().map_err(|e| color_eyre::eyre::eyre!("OpenAI embeddings parse failed: {e}"))?;
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }

    fn fetch_azure_embeddings(
        &self,
        model_name: &str,
        inputs: &Vec<String>,
        dims_opt: Option<usize>,
    ) -> color_eyre::Result<Vec<Vec<f32>>> {
        let cfg = self.azure.as_ref().ok_or_else(|| color_eyre::eyre::eyre!("Azure OpenAI config is not set"))?;
        // Expect base_url to point at deployment root, e.g., https://.../openai/deployments/<deployment>
        let url = format!("{}/embeddings?api-version={}", cfg.base_url.trim_end_matches('/'), cfg.api_version);
        let client = reqwest::blocking::Client::new();
        #[derive(serde::Serialize)]
        struct AzureEmbReq<'a> { input: &'a Vec<String>, #[serde(skip_serializing_if="Option::is_none")] dimensions: Option<usize>, model: &'a str }
        #[derive(serde::Deserialize)]
        struct AzureEmbRes { data: Vec<AzureEmbDatum> }
        #[derive(serde::Deserialize)]
        struct AzureEmbDatum { embedding: Vec<f32> }
        let req = AzureEmbReq { input: inputs, dimensions: dims_opt, model: model_name };
        let res = client.post(url)
            .header("api-key", &cfg.api_key)
            .json(&req)
            .send()
            .map_err(|e| color_eyre::eyre::eyre!("Azure embeddings request failed: {e}"))?;
        if !res.status().is_success() { return Err(color_eyre::eyre::eyre!("Azure embeddings HTTP error: {}", res.status())); }
        let parsed: AzureEmbRes = res.json().map_err(|e| color_eyre::eyre::eyre!("Azure embeddings parse failed: {e}"))?;
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }

    fn fetch_ollama_embeddings(
        &self,
        model_name: &str,
        inputs: &Vec<String>,
    ) -> color_eyre::Result<Vec<Vec<f32>>> {
        let cfg = self.ollama.as_ref().ok_or_else(|| color_eyre::eyre::eyre!("Ollama config is not set"))?;
        let url = format!("{}/api/embeddings", cfg.host.trim_end_matches('/'));
        let client = reqwest::blocking::Client::new();
        #[derive(serde::Serialize)]
        struct OllamaEmbReq<'a> { model: &'a str, prompt: &'a str }
        #[derive(serde::Deserialize)]
        struct OllamaEmbRes { embedding: Vec<f32> }
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(inputs.len());
        for inp in inputs.iter() {
            let req = OllamaEmbReq { model: model_name, prompt: inp };
            let res = client.post(&url)
                .json(&req)
                .send()
                .map_err(|e| color_eyre::eyre::eyre!("Ollama embeddings request failed: {e}"))?;
            if !res.status().is_success() { return Err(color_eyre::eyre::eyre!("Ollama embeddings HTTP error: {}", res.status())); }
            let parsed: OllamaEmbRes = res.json().map_err(|e| color_eyre::eyre::eyre!("Ollama embeddings parse failed: {e}"))?;
            out.push(parsed.embedding);
        }
        Ok(out)
    }
}

impl Default for LlmClientDialog {
    fn default() -> Self { Self::new() }
}

impl LlmClientDialog {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        
        Self {
            mode: LlmClientDialogMode::ProviderSelection,
            selected_provider: LlmProvider::OpenAI,
            provider_list_state: list_state,
            error_active: false,
            show_instructions: true,
            config: Config::default(),
            llm_config: LlmConfig::default(),
            azure_dialog: AzureOpenAiConfigDialog::new(),
            openai_dialog: OpenAiConfigDialog::new(),
            ollama_dialog: OllamaConfigDialog::new(),
        }
    }

    pub fn new_with_config(config: Config, llm_config: LlmConfig) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        
        // Initialize provider dialogs with existing configs
        let azure_dialog = if let Some(azure_config) = &llm_config.azure {
            AzureOpenAiConfigDialog::new_with_config(config.clone(), azure_config.clone())
        } else {
            AzureOpenAiConfigDialog::new_with_config(config.clone(), AzureOpenAiConfig::default())
        };
        
        let openai_dialog = if let Some(openai_config) = &llm_config.openai {
            OpenAiConfigDialog::new_with_config(config.clone(), openai_config.clone())
        } else {
            OpenAiConfigDialog::new_with_config(config.clone(), OpenAIConfig::default())
        };
        
        let ollama_dialog = if let Some(ollama_config) = &llm_config.ollama {
            OllamaConfigDialog::new_with_config(config.clone(), ollama_config.clone())
        } else {
            OllamaConfigDialog::new_with_config(config.clone(), OllamaConfig::default())
        };
        
        Self {
            mode: LlmClientDialogMode::ProviderSelection,
            selected_provider: LlmProvider::OpenAI,
            provider_list_state: list_state,
            error_active: false,
            show_instructions: true,
            config,
            llm_config,
            azure_dialog,
            openai_dialog,
            ollama_dialog,
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter)
        ])
    }

    fn get_provider_list_items() -> Vec<ListItem<'static>> {
        vec![
            ListItem::new("Azure OpenAI"),
            ListItem::new("OpenAI"),
            ListItem::new("Ollama"),
        ]
    }

    fn update_selected_provider(&mut self) {
        if let Some(selected) = self.provider_list_state.selected() {
            self.selected_provider = match selected {
                0 => LlmProvider::Azure,
                1 => LlmProvider::OpenAI,
                2 => LlmProvider::Ollama,
                _ => LlmProvider::OpenAI,
            };
        }
    }

    fn enter_configuration_mode(&mut self) {
        self.update_selected_provider();
        self.mode = match self.selected_provider {
            LlmProvider::Azure => LlmClientDialogMode::AzureConfiguration,
            LlmProvider::OpenAI => LlmClientDialogMode::OpenAIConfiguration,
            LlmProvider::Ollama => LlmClientDialogMode::OllamaConfiguration,
        };
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        
        // Outer container with double border
        let outer_block = Block::default()
            .title("LLM Client Configuration")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let wrap_width = content_area.width.saturating_sub(2) as usize;

        match &self.mode {
            LlmClientDialogMode::ProviderSelection => {
                let block = Block::default()
                    .title("Select Provider")
                    .borders(Borders::ALL);
                let list_area = block.inner(content_area);
                block.render(content_area, buf);

                let items = Self::get_provider_list_items();
                let list = List::new(items)
                    .highlight_style(Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White))
                    .highlight_symbol("> ");

                ratatui::prelude::StatefulWidget::render(
                    list, list_area, buf, 
                    &mut self.provider_list_state
                );
            }
            LlmClientDialogMode::AzureConfiguration => {
                self.azure_dialog.render(area, buf);
            }
            LlmClientDialogMode::OpenAIConfiguration => {
                self.openai_dialog.render(area, buf);
            }
            LlmClientDialogMode::OllamaConfiguration => {
                self.ollama_dialog.render(area, buf);
            }
            LlmClientDialogMode::Error(msg) => {
                let y = content_area.y;
                buf.set_string(content_area.x, y, "Error:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                let error_lines = wrap(msg, wrap_width);
                for (i, line) in error_lines.iter().enumerate() {
                    buf.set_string(content_area.x, y + 1 + i as u16, line, Style::default().fg(Color::Red));
                }
                buf.set_string(content_area.x, y + 1 + error_lines.len() as u16, "Press Esc or Enter to close error", Style::default().fg(Color::Yellow));
            }
        }

        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Get all configured actions once at the start
        let optional_global_action = self.config.action_for_key(
            crate::config::Mode::Global, key);
        let llm_dialog_action = self.config.action_for_key(
            crate::config::Mode::LlmClientDialog, key);

        // Handle global actions that work in all modes
        if let Some(global_action) = &optional_global_action
            && global_action == &Action::ToggleInstructions {
                self.show_instructions = !self.show_instructions;
                return None;
            }
        
        match &mut self.mode {
            LlmClientDialogMode::ProviderSelection => {
                // First, check Global actions
                if let Some(global_action) = &optional_global_action {
                    match global_action {
                        Action::Escape => {
                            return Some(Action::DialogClose);
                        }
                        _ => {}
                    }
                }

                // Next, check LlmClientDialog-specific actions
                if let Some(dialog_action) = &llm_dialog_action {
                    match dialog_action {
                        Action::Enter => {
                            self.enter_configuration_mode();
                            return None;
                        }
                        Action::Up => {
                            if let Some(selected) = self.provider_list_state.selected() {
                                let new_selection = if selected == 0 { 2 } else { selected - 1 };
                                self.provider_list_state.select(Some(new_selection));
                            }
                            return None;
                        }
                        Action::Down => {
                            if let Some(selected) = self.provider_list_state.selected() {
                                let new_selection = if selected == 2 { 0 } else { selected + 1 };
                                self.provider_list_state.select(Some(new_selection));
                            }
                            return None;
                        }
                        _ => {}
                    }
                }

                // Fallback for hardcoded keys
                match key.code {
                    KeyCode::Esc => {
                        return Some(Action::DialogClose);
                    }
                    KeyCode::Enter => {
                        self.enter_configuration_mode();
                        return None;
                    }
                    KeyCode::Up => {
                        if let Some(selected) = self.provider_list_state.selected() {
                            let new_selection = if selected == 0 { 2 } else { selected - 1 };
                            self.provider_list_state.select(Some(new_selection));
                        }
                        return None;
                    }
                    KeyCode::Down => {
                        if let Some(selected) = self.provider_list_state.selected() {
                            let new_selection = if selected == 2 { 0 } else { selected + 1 };
                            self.provider_list_state.select(Some(new_selection));
                        }
                        return None;
                    }
                    _ => {}
                }
            }
            LlmClientDialogMode::AzureConfiguration => {
                // Delegate to Azure dialog
                if let Some(action) = self.azure_dialog.handle_key_event(key) {
                    match action {
                        Action::LlmClientDialogApplied(config) => {
                            // Update our llm_config with the new Azure config
                            self.llm_config.azure = config.azure;
                            return Some(Action::LlmClientDialogApplied(self.llm_config.clone()));
                        }
                        Action::DialogClose => {
                            self.mode = LlmClientDialogMode::ProviderSelection;
                            return None;
                        }
                        _ => return Some(action),
                    }
                }
            }
            LlmClientDialogMode::OpenAIConfiguration => {
                // Delegate to OpenAI dialog
                if let Some(action) = self.openai_dialog.handle_key_event(key) {
                    match action {
                        Action::LlmClientDialogApplied(config) => {
                            // Update our llm_config with the new OpenAI config
                            self.llm_config.openai = config.openai;
                            return Some(Action::LlmClientDialogApplied(self.llm_config.clone()));
                        }
                        Action::DialogClose => {
                            self.mode = LlmClientDialogMode::ProviderSelection;
                            return None;
                        }
                        _ => return Some(action),
                    }
                }
            }
            LlmClientDialogMode::OllamaConfiguration => {
                // Delegate to Ollama dialog
                if let Some(action) = self.ollama_dialog.handle_key_event(key) {
                    match action {
                        Action::LlmClientDialogApplied(config) => {
                            // Update our llm_config with the new Ollama config
                            self.llm_config.ollama = config.ollama;
                            return Some(Action::LlmClientDialogApplied(self.llm_config.clone()));
                        }
                        Action::DialogClose => {
                            self.mode = LlmClientDialogMode::ProviderSelection;
                            return None;
                        }
                        _ => return Some(action),
                    }
                }
            }
            LlmClientDialogMode::Error(_) => {
                // Only close error on Esc or Enter
                if let Some(Action::Escape | Action::Enter) = &optional_global_action {
                    self.error_active = false;
                    self.mode = LlmClientDialogMode::ProviderSelection;
                    return None;
                }
                // Fallback for hardcoded keys
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.error_active = false;
                        self.mode = LlmClientDialogMode::ProviderSelection;
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Set error message and switch to error mode
    pub fn set_error(&mut self, msg: String) {
        self.mode = LlmClientDialogMode::Error(msg);
        self.error_active = true;
    }
}

impl Component for LlmClientDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> {
        // Register with individual dialogs
        self.azure_dialog.register_action_handler(_tx.clone())?;
        self.openai_dialog.register_action_handler(_tx.clone())?;
        self.ollama_dialog.register_action_handler(_tx)?;
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.config = _config.clone();
        // Register with individual dialogs
        self.azure_dialog.register_config_handler(_config.clone())?;
        self.openai_dialog.register_config_handler(_config.clone())?;
        self.ollama_dialog.register_config_handler(_config)?;
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        // Initialize individual dialogs
        self.azure_dialog.init(_area)?;
        self.openai_dialog.init(_area)?;
        self.ollama_dialog.init(_area)?;
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        if let Some(action) = self.handle_key_event(_key) {
            return Ok(Some(action));
        }
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}
