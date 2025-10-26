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
use crate::dialog::llm::{
    AzureOpenAiConfigDialog, OpenAiConfigDialog,
    OllamaConfigDialog, AzureOpenAiConfig, OpenAIConfig, OllamaConfig
};

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
            selected_provider: llm_config.selected_provider.clone(),
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
            (crate::config::Mode::LlmClientDialog, crate::action::Action::Enter),
            (crate::config::Mode::LlmClientDialog, crate::action::Action::LlmClientDialogApplied(LlmConfig::default())),
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
            self.llm_config.selected_provider = self.selected_provider.clone();
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
                            self.llm_config.selected_provider = config.selected_provider;
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
                            self.llm_config.selected_provider = config.selected_provider;
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
                            self.llm_config.selected_provider = config.selected_provider;
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
