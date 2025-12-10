//! StyleSetBrowserDialog: Dialog for browsing and importing style sets
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::style::StyleConfig;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set_manager::StyleSetManager;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};

/// StyleSetBrowserDialog: UI for browsing folders and importing style sets
#[derive(Debug)]
pub struct StyleSetBrowserDialog {
    pub style_set_manager: StyleSetManager,
    pub file_browser: Option<FileBrowserDialog>,
    pub show_instructions: bool,
    pub config: Config,
    pub style: StyleConfig,
}

impl StyleSetBrowserDialog {
    /// Create a new StyleSetBrowserDialog
    pub fn new(style_set_manager: StyleSetManager) -> Self {
        Self {
            style_set_manager,
            file_browser: None,
            show_instructions: true,
            config: Config::default(),
            style: StyleConfig::default(),
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (Mode::Global, Action::Escape),
            (Mode::Global, Action::Enter),
            (Mode::Global, Action::ToggleInstructions),
        ])
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Set Browser")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        if let Some(ref browser) = self.file_browser {
            browser.render(content_area, buf);
        } else {
            let block = Block::default()
                .title("Browse for Style Set Folders")
                .borders(Borders::ALL);
            block.render(content_area, buf);
            
            let message = "Use the file browser to select a folder containing YAML style set files.";
            let p = Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            p.render(content_area, buf);
        }

        // Render instructions
        if self.show_instructions {
            if let Some(instr_area) = instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, buf);
            }
        }
    }

    /// Handle a key event
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        // Initialize file browser if needed
        if self.file_browser.is_none() {
            let mut browser = FileBrowserDialog::new(
                None,
                Some(vec!["yaml", "yml"]),
                true, // folder_only
                FileBrowserMode::Load,
            );
            browser.register_config_handler(self.config.clone());
            self.file_browser = Some(browser);
        }

        if let Some(ref mut browser) = self.file_browser {
            if let Some(action) = browser.handle_key_event(key) {
                match action {
                    FileBrowserAction::Selected(path) => {
                        // Load style sets from the selected folder
                        if path.is_dir() {
                            if let Err(e) = self.style_set_manager.load_from_folder(&path) {
                                tracing::error!("Failed to load style sets from folder: {}", e);
                            } else {
                                return Some(Action::StyleSetBrowserDialogApplied(
                                    self.style_set_manager.get_enabled_identifiers()
                                ));
                            }
                        }
                        return Some(Action::CloseStyleSetBrowserDialog);
                    }
                    FileBrowserAction::Cancelled => {
                        return Some(Action::CloseStyleSetBrowserDialog);
                    }
                }
            }
        }

        if key.kind == KeyEventKind::Press {
            // Check Global actions
            if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
                match global_action {
                    Action::Escape => {
                        return Some(Action::CloseStyleSetBrowserDialog);
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return None;
                    }
                    _ => {}
                }
            }
        }

        None
    }
}

impl Component for StyleSetBrowserDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        let config_clone = config.clone();
        self.config = config_clone.clone();
        if let Some(ref mut browser) = self.file_browser {
            browser.register_config_handler(config_clone);
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        Ok(self.handle_key_event_pub(key))
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}

