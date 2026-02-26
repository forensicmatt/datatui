//! LLM Management Dialog
//!
//! Main entry point for managing LLM provider configurations and default selection.

use crate::core::llm_config::LlmProvider;
use crate::services::llm_service::LlmService;
use crate::tui::components::{AzureOpenAiConfigDialog, OllamaConfigDialog, OpenAiConfigDialog};
use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

/// Active state of the management dialog
pub enum ActiveDialog {
    None,
    OpenAi(OpenAiConfigDialog),
    Azure(AzureOpenAiConfigDialog),
    Ollama(OllamaConfigDialog),
}

/// LLM management dialog component
pub struct LlmManagementDialog {
    pub service: LlmService,
    pub active_dialog: ActiveDialog,
    pub list_state: ListState,
    pub providers: Vec<LlmProvider>,
    pub focused: bool,
    pub show_instructions: bool,
    pub closed: bool,
}

impl LlmManagementDialog {
    pub fn new(service: LlmService) -> Self {
        let providers = LlmProvider::all();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            service,
            active_dialog: ActiveDialog::None,
            list_state,
            providers,
            focused: true,
            show_instructions: true,
            closed: false,
        }
    }

    fn select_next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.providers.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_prev(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.providers.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn open_selected_config(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let provider = self.providers[i];
            match provider {
                LlmProvider::OpenAI => {
                    let config = self.service.get_openai_config();
                    self.active_dialog = ActiveDialog::OpenAi(OpenAiConfigDialog::new(config));
                }
                LlmProvider::Azure => {
                    let config = self.service.get_azure_config();
                    self.active_dialog = ActiveDialog::Azure(AzureOpenAiConfigDialog::new(config));
                }
                LlmProvider::Ollama => {
                    let config = self.service.get_ollama_config();
                    self.active_dialog = ActiveDialog::Ollama(OllamaConfigDialog::new(config));
                }
            }
        }
    }

    fn set_default_provider(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let provider = self.providers[i];
            // Only allow setting default if it's configured
            if self.service.is_provider_configured(provider) {
                self.service.set_default_provider(Some(provider));
                let _ = self.service.save(); // Best effort save
            }
        }
    }
}

impl Component for LlmManagementDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        match &mut self.active_dialog {
            ActiveDialog::OpenAi(dialog) => dialog.handle_key_event(key),
            ActiveDialog::Azure(dialog) => dialog.handle_key_event(key),
            ActiveDialog::Ollama(dialog) => dialog.handle_key_event(key),
            ActiveDialog::None => Ok(KeyEventResult::Ignored),
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        let was_sub_active = !matches!(self.active_dialog, ActiveDialog::None);

        // Delegate to active sub-dialog if any
        let sub_dialog_handled = match &mut self.active_dialog {
            ActiveDialog::OpenAi(dialog) => {
                let handled = dialog.handle_action(action)?;
                if dialog.closed {
                    if let Some(config) = dialog.result.take() {
                        self.service.set_openai_config(config);
                        let _ = self.service.save();
                    }
                    self.active_dialog = ActiveDialog::None;
                }
                handled
            }
            ActiveDialog::Azure(dialog) => {
                let handled = dialog.handle_action(action)?;
                if dialog.closed {
                    if let Some(config) = dialog.result.take() {
                        self.service.set_azure_config(config);
                        let _ = self.service.save();
                    }
                    self.active_dialog = ActiveDialog::None;
                }
                handled
            }
            ActiveDialog::Ollama(dialog) => {
                let handled = dialog.handle_action(action)?;
                if dialog.closed {
                    if let Some(config) = dialog.result.take() {
                        self.service.set_ollama_config(config);
                        let _ = self.service.save();
                    }
                    self.active_dialog = ActiveDialog::None;
                }
                handled
            }
            ActiveDialog::None => false,
        };

        if was_sub_active {
            return Ok(true);
        }

        if sub_dialog_handled {
            return Ok(true);
        }

        // Handle management dialog actions
        match action {
            Action::Cancel | Action::Escape => {
                self.closed = true;
                Ok(false)
            }
            Action::MoveUp => {
                self.select_prev();
                Ok(true)
            }
            Action::MoveDown => {
                self.select_next();
                Ok(true)
            }
            Action::Confirm => {
                self.open_selected_config();
                Ok(true)
            }
            Action::ToggleInstructions => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            // Use a custom key for setting default (e.g., 'd')
            // Since we don't have a specific action for it yet, we could use a char key in handle_key_event
            // but let's see if we can use an existing action or just add it to handle_key_event later.
            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // If a sub-dialog is active, render it instead
        match &mut self.active_dialog {
            ActiveDialog::OpenAi(dialog) => {
                let dialog_area = Rect {
                    x: area.x + (area.width.saturating_sub(60)) / 2,
                    y: area.y + (area.height.saturating_sub(20)) / 2,
                    width: 60.min(area.width),
                    height: 20.min(area.height),
                };
                dialog.render(frame, dialog_area, theme);
                return;
            }
            ActiveDialog::Azure(dialog) => {
                let dialog_area = Rect {
                    x: area.x + (area.width.saturating_sub(60)) / 2,
                    y: area.y + (area.height.saturating_sub(23)) / 2,
                    width: 60.min(area.width),
                    height: 23.min(area.height),
                };
                dialog.render(frame, dialog_area, theme);
                return;
            }
            ActiveDialog::Ollama(dialog) => {
                let dialog_area = Rect {
                    x: area.x + (area.width.saturating_sub(60)) / 2,
                    y: area.y + (area.height.saturating_sub(15)) / 2,
                    width: 60.min(area.width),
                    height: 15.min(area.height),
                };
                dialog.render(frame, dialog_area, theme);
                return;
            }
            ActiveDialog::None => {}
        }

        frame.render_widget(Clear, area);

        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let block = Block::default()
            .title(" LLM Management ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style);

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let (list_area, instr_area) = if self.show_instructions {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(6)])
                .split(inner_area);
            (chunks[0], Some(chunks[1]))
        } else {
            (inner_area, None)
        };

        let default_provider = self.service.get_default_provider();

        let items: Vec<ListItem> = self
            .providers
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let is_configured = self.service.is_provider_configured(*p);
                let is_default = default_provider == Some(*p);

                let status = if is_configured {
                    if is_default {
                        " (Default) "
                    } else {
                        " [Configured] "
                    }
                } else {
                    " [Not Configured] "
                };

                let content = format!("{:<20} {}", p.display_name(), status);
                let style = if Some(i) == self.list_state.selected() {
                    theme.selected_style()
                } else if is_configured {
                    theme.success_style()
                } else {
                    theme.normal_style()
                };

                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(theme.selected_style())
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, list_area, &mut self.list_state);

        if let Some(instr_area) = instr_area {
            self.render_instructions(frame, instr_area, theme);
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Escape,
            Action::MoveUp,
            Action::MoveDown,
            Action::Confirm,
            Action::ToggleInstructions,
        ]
    }

    fn name(&self) -> &str {
        "LlmManagementDialog"
    }
}

impl LlmManagementDialog {
    fn render_instructions(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Instructions (Ctrl+i to hide) ")
            .borders(Borders::TOP)
            .border_style(theme.border_style());

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let instructions = vec![
            "• Up/Down: Navigate providers",
            "• Enter: Configure selected provider",
            "• 'd': Set as default provider (if configured)",
            "• Esc: Close Management",
        ];

        let text = Paragraph::new(instructions.join("\n")).style(theme.warning_style());
        frame.render_widget(text, inner_area);
    }

    /// We need to handle 'd' key for setting default
    pub fn handle_raw_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        // If sub-dialog is active, it already handled it in handle_key_event delegate
        if !matches!(self.active_dialog, ActiveDialog::None) {
            return Ok(KeyEventResult::Ignored);
        }

        if let crossterm::event::KeyCode::Char('d') = key.code {
            self.set_default_provider();
            return Ok(KeyEventResult::Consumed);
        }

        Ok(KeyEventResult::Ignored)
    }
}

impl Focusable for LlmManagementDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
