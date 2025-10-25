//! LlmClientDialog: Popup dialog for configuring LLM client providers
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
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
    Configuration,
    Error(String),
}

#[derive(Debug)]
pub struct LlmClientDialog {
    pub mode: LlmClientDialogMode,
    pub selected_provider: LlmProvider,
    pub provider_list_state: ListState,
    pub config_fields: LlmClientConfig,
    pub error_active: bool,
    pub show_instructions: bool,
    pub config: Config,
    pub llm_config: LlmConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzureOpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub deployment: String,
    pub api_version: String,
    pub model: String,
}

impl Default for AzureOpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://your-resource.openai.azure.com/".to_string(),
            deployment: String::new(),
            api_version: "2024-02-15-preview".to_string(),
            model: "gpt-4o".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub host: String,
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmConfig {
    pub azure: Option<AzureOpenAiConfig>,
    pub openai: Option<OpenAIConfig>,
    pub ollama: Option<OllamaConfig>,
    pub selected_provider: LlmProvider,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            azure: None,
            openai: None,
            ollama: None,
            selected_provider: LlmProvider::OpenAI,
        }
    }
}

impl LlmConfig {
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmClientConfig {
    pub provider: LlmProvider,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    // Azure-specific fields
    pub azure_deployment: String,
    pub azure_api_version: String,
    // Ollama-specific fields
    pub ollama_host: String,
}

impl Default for LlmClientConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::OpenAI,
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            azure_deployment: String::new(),
            azure_api_version: "2024-02-15-preview".to_string(),
            ollama_host: "http://localhost:11434".to_string(),
        }
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
            config_fields: LlmClientConfig::default(),
            error_active: false,
            show_instructions: true,
            config: Config::default(),
            llm_config: LlmConfig::default(),
        }
    }

    pub fn new_with_config(config: Config, llm_config: LlmConfig) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        
        Self {
            mode: LlmClientDialogMode::ProviderSelection,
            selected_provider: llm_config.selected_provider.clone(),
            provider_list_state: list_state,
            config_fields: LlmClientConfig::default(),
            error_active: false,
            show_instructions: true,
            config,
            llm_config,
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Up),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Down),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Tab),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Backspace),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::LlmClientDialogApplied(LlmClientConfig::default())),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::LlmClientDialogCancel),
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
            self.config_fields.provider = self.selected_provider.clone();
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
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
                    .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
                    .highlight_symbol("> ");
                ratatui::prelude::StatefulWidget::render(list, list_area, buf, &mut self.provider_list_state);
            }
            LlmClientDialogMode::Configuration => {
                self.render_configuration_form(content_area, buf, wrap_width);
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
        1
    }

    fn render_configuration_form(&self, area: Rect, buf: &mut Buffer, _wrap_width: usize) {
        let block = Block::default()
            .title(format!("Configure {}", self.selected_provider.display_name()))
            .borders(Borders::ALL);
        let form_area = block.inner(area);
        block.render(area, buf);

        let mut y = form_area.y;
        let x = form_area.x;

        // Provider info
        buf.set_string(x, y, format!("Provider: {}", self.selected_provider.display_name()), 
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        y += 2;

        // Get current config values based on selected provider
        let (api_key, base_url, model) = match self.selected_provider {
            LlmProvider::Azure => {
                if let Some(azure_config) = &self.llm_config.azure {
                    (
                        &azure_config.api_key,
                        &azure_config.base_url,
                        &azure_config.model,
                    )
                } else {
                    static EMPTY: String = String::new();
                    (&EMPTY, &EMPTY, &EMPTY)
                }
            },
            LlmProvider::OpenAI => {
                if let Some(openai_config) = &self.llm_config.openai {
                    (
                        &openai_config.api_key,
                        &openai_config.base_url,
                        &openai_config.model,
                    )
                } else {
                    static EMPTY: String = String::new();
                    (&EMPTY, &EMPTY, &EMPTY)
                }
            },
            LlmProvider::Ollama => {
                if let Some(ollama_config) = &self.llm_config.ollama {
                    (
                        &ollama_config.host, // Use host as the "api_key" for Ollama
                        &ollama_config.host,
                        &ollama_config.model,
                    )
                } else {
                    static EMPTY: String = String::new();
                    (&EMPTY, &EMPTY, &EMPTY)
                }
            },
        };

        // Common fields
        if self.selected_provider != LlmProvider::Ollama {
            buf.set_string(x, y, "API Key:", Style::default().fg(Color::Yellow));
            buf.set_string(x, y + 1, api_key, Style::default().fg(Color::White));
            y += 3;
        }

        buf.set_string(x, y, if self.selected_provider == LlmProvider::Ollama { "Host:" } else { "Base URL:" }, 
            Style::default().fg(Color::Yellow));
        buf.set_string(x, y + 1, base_url, Style::default().fg(Color::White));
        y += 3;

        buf.set_string(x, y, "Model:", Style::default().fg(Color::Yellow));
        buf.set_string(x, y + 1, model, Style::default().fg(Color::White));
        y += 3;

        // Provider-specific fields
        match self.selected_provider {
            LlmProvider::Azure => {
                if let Some(azure_config) = &self.llm_config.azure {
                    buf.set_string(x, y, "Azure Deployment:", Style::default().fg(Color::Yellow));
                    buf.set_string(x, y + 1, &azure_config.deployment, Style::default().fg(Color::White));
                    y += 3;

                    buf.set_string(x, y, "Azure API Version:", Style::default().fg(Color::Yellow));
                    buf.set_string(x, y + 1, &azure_config.api_version, Style::default().fg(Color::White));
                } else {
                    buf.set_string(x, y, "Azure Deployment:", Style::default().fg(Color::Yellow));
                    buf.set_string(x, y + 1, "", Style::default().fg(Color::White));
                    y += 3;

                    buf.set_string(x, y, "Azure API Version:", Style::default().fg(Color::Yellow));
                    buf.set_string(x, y + 1, "", Style::default().fg(Color::White));
                }
            }
            LlmProvider::Ollama => {
                // No additional fields for Ollama
            }
            LlmProvider::OpenAI => {
                // No additional fields for OpenAI
            }
        }

        y += 4;
        buf.set_string(x, y, "Enter: Apply  Esc: Cancel  Tab: Back to Provider Selection", 
            Style::default().fg(Color::Gray));
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Get all configured actions once at the start
        let optional_global_action = self.config.action_for_key(crate::config::Mode::Global, key);
        let llm_dialog_action = self.config.action_for_key(crate::config::Mode::LlmClientDialog, key);

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
                            self.update_selected_provider();
                            self.mode = LlmClientDialogMode::Configuration;
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
                        self.update_selected_provider();
                        self.mode = LlmClientDialogMode::Configuration;
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
            LlmClientDialogMode::Configuration => {
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
                            return Some(Action::LlmClientDialogApplied(self.config_fields.clone()));
                        }
                        Action::Tab => {
                            self.mode = LlmClientDialogMode::ProviderSelection;
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
                        return Some(Action::LlmClientDialogApplied(self.config_fields.clone()));
                    }
                    KeyCode::Tab => {
                        self.mode = LlmClientDialogMode::ProviderSelection;
                        return None;
                    }
                    _ => {}
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
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.config = _config;
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
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
