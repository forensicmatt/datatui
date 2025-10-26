//! ProjectSettingsDialog: Popup dialog for configuring project settings
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Wrap};
use crate::tui::Event;
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use std::fs::File;
use serde_json;
use textwrap::wrap;
use std::path::PathBuf;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserMode, FileBrowserAction};
use crate::dialog::message_dialog::MessageDialog;
use crate::dialog::llm_client_dialog::LlmClientDialog;
use crate::providers::openai::Client as OpenAIClient;
use crate::config::get_config_dir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSettingsDialogMode {
    Input,
    Error(String),
    Save,
    LlmClientDialog,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct ProjectSettingsConfig {
    pub openai_key: Option<String>,
    pub workspace_path: Option<PathBuf>,
    #[serde(default)]
    pub data_viewer: DataViewerOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct DataViewerOptions {
    #[serde(default)]
    pub auto_exapand_value_display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedOption {
    WorkspacePath,
    WorkspaceBrowse,
    LlmConfigPath,
    LlmConfigBrowse,
    ConfigureLlmClients,
    AutoExpandValueDisplay,
    Save,
}

// Derive Default instead of manual impl

// Derive Default instead of manual impl

#[derive(Debug)]
pub struct ProjectSettingsDialog {
    pub config: ProjectSettingsConfig,
    pub mode: ProjectSettingsDialogMode,
    pub error_active: bool,
    pub show_instructions: bool,
    pub workspace_path_input: String,
    pub config_path_input: String,
    pub selected_option: SelectedOption,
    pub file_browser_mode: bool,
    pub file_browser_path: PathBuf,
    pub file_browser: Option<FileBrowserDialog>,
    pub openai_client: Option<OpenAIClient>,
    pub message_dialog_mode: bool,
    pub message_dialog: Option<MessageDialog>,
    pub llm_client_dialog: Option<LlmClientDialog>,
    pub keybindings_config: crate::config::Config,
}

impl ProjectSettingsDialog {
    pub fn new(config: ProjectSettingsConfig) -> Self {
        let workspace_path_input = String::new();
        let config_path_input = get_config_dir().join("llm-settings.toml").to_string_lossy().to_string();
        
        Self {
            config,
            mode: ProjectSettingsDialogMode::Input,
            error_active: false,
            show_instructions: true,
            workspace_path_input,
            config_path_input,
            selected_option: SelectedOption::WorkspacePath,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_browser: None,
            openai_client: None,
            message_dialog_mode: false,
            message_dialog: None,
            llm_client_dialog: None,
            keybindings_config: crate::config::Config::default(),
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.keybindings_config.actions_to_instructions(&[
            (crate::config::Mode::Global, crate::action::Action::Up),
            (crate::config::Mode::Global, crate::action::Action::Down),
            (crate::config::Mode::Global, crate::action::Action::Left),
            (crate::config::Mode::Global, crate::action::Action::Right),
            (crate::config::Mode::Global, crate::action::Action::Tab),
            (crate::config::Mode::Global, crate::action::Action::Enter),
            (crate::config::Mode::Global, crate::action::Action::Escape),
            (crate::config::Mode::Global, crate::action::Action::Backspace),
            (crate::config::Mode::ProjectSettings, crate::action::Action::ToggleDataViewerOption),
        ])
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        let optional_instructions = if instructions.is_empty() { None } else { Some(instructions.as_str()) }; 
        
        // Create outer block that encompasses the entire area (content + instructions)
        let outer_block = Block::default()
            .title("Settings")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);

        // Get the inner area from the outer block (this will be the area inside the double border)
        let inner_area = outer_block.inner(area);
        
        // Render the outer block around the entire area
        outer_block.render(area, buf);
        
        // Now split the inner area for content and instructions
        let layout = split_dialog_area(
            inner_area, self.show_instructions,
            optional_instructions
        );
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let wrap_width = content_area.width.saturating_sub(2) as usize;

        // Render file browser dialog overlay if active
        if self.file_browser_mode {
            if let Some(browser) = &self.file_browser {
                browser.render(area, buf);
            }
            return;
        }

        match &self.mode {
            ProjectSettingsDialogMode::Input => {

                // Inline fields: "Label: value" with selection highlighting
                let line0_y = content_area.y + 1; // Workspace first
                let line1_y = content_area.y + 3; // LLM Config second
                let line2_y = content_area.y + 5; // Data Viewer section top

                // Workspace Path line + [Browse]
                let label_wp = "Workspace Path: ";
                let style_wp = if self.selected_option == SelectedOption::WorkspacePath {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                let wp_text = format!("{}{}", label_wp, self.workspace_path_input);
                buf.set_string(content_area.x, line0_y, wp_text, style_wp);
                // [Browse] button on same line, right-aligned
                let browse_text = "[Browse]";
                let browse_x = content_area
                    .x
                    + content_area.width.saturating_sub(browse_text.len() as u16 + 1);
                let browse_style = if self.selected_option == SelectedOption::WorkspaceBrowse {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(browse_x, line0_y, browse_text, browse_style);
                // Cursor on workspace input when focused
                if self.selected_option == SelectedOption::WorkspacePath {
                    let cursor_x = content_area.x + (label_wp.len() + self.workspace_path_input.len()) as u16;
                    buf.set_string(cursor_x, line0_y, "_", Style::default().fg(Color::Yellow));
                }

                // LLM Config Path line + [Browse]
                let label_config = "LLM Config Path: ";
                let style_config = if self.selected_option == SelectedOption::LlmConfigPath {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                let config_text = format!("{}{}", label_config, self.config_path_input);
                buf.set_string(content_area.x, line1_y, config_text, style_config);
                if self.selected_option == SelectedOption::LlmConfigPath {
                    let cursor_x = content_area.x + (label_config.len() + self.config_path_input.len()) as u16;
                    buf.set_string(cursor_x, line1_y, "_", Style::default().fg(Color::Yellow));
                }
                // [Browse] button on same line, right-aligned
                let browse_text = "[Browse]";
                let browse_x = content_area
                    .x
                    + content_area.width.saturating_sub(browse_text.len() as u16 + 1);
                let browse_style = if self.selected_option == SelectedOption::LlmConfigBrowse {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(browse_x, line1_y, browse_text, browse_style);

                // Configure LLM Clients button on a new line
                let llm_client_text = "[Configure LLM Clients]";
                let llm_client_x = content_area.x;
                let llm_client_y = line1_y + 2;
                let llm_client_style = if self.selected_option == SelectedOption::ConfigureLlmClients {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(llm_client_x, llm_client_y, llm_client_text, llm_client_style);

                // Data Viewer section with bordered block
                let dv_block_area = Rect {
                    x: content_area.x,
                    y: line2_y + 1, // Move down one line to accommodate LLM Client button
                    width: content_area.width,
                    height: 3,
                };
                let dv_block = Block::default()
                    .title("Data Viewer")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded);
                let dv_inner = dv_block.inner(dv_block_area);
                dv_block.render(dv_block_area, buf);

                let dv_label = "Auto Exapand Value Display: ";
                let dv_value = if self.config.data_viewer.auto_exapand_value_display { "true" } else { "false" };
                let dv_style = if self.selected_option == SelectedOption::AutoExpandValueDisplay {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                buf.set_string(dv_inner.x, dv_inner.y, format!("{dv_label}{dv_value}"), dv_style);

                // [Save] button at bottom-right of content area
                let save_text = "[Save]";
                let save_x = content_area.x + content_area.width.saturating_sub(save_text.len() as u16 + 2);
                let save_y = content_area.y + content_area.height.saturating_sub(2);
                let save_style = if self.selected_option == SelectedOption::Save {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(save_x, save_y, save_text, save_style);
            }
            ProjectSettingsDialogMode::Error(msg) => {
                let y = content_area.y;
                buf.set_string(content_area.x, y, "Error:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
                let error_lines = wrap(msg, wrap_width);
                for (i, line) in error_lines.iter().enumerate() {
                    buf.set_string(content_area.x, y + 1 + i as u16, line, Style::default().fg(Color::Red));
                }
                buf.set_string(content_area.x, y + 1 + error_lines.len() as u16, "Press Esc or Enter to close error", Style::default().fg(Color::Yellow));
            }
            ProjectSettingsDialogMode::Save => { }
            ProjectSettingsDialogMode::LlmClientDialog => {
                if let Some(dialog) = &mut self.llm_client_dialog {
                    dialog.render(area, buf);
                    return;
                }
            }
        }

        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default()
                    .title("Instructions")
                    .borders(Borders::ALL))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
        // Finally, render message dialog overlay if active
        if self.message_dialog_mode && let Some(msg) = &self.message_dialog {
            msg.render(area, buf);
        }
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        
        if key.kind == KeyEventKind::Press {
            // Handle Ctrl+I for instructions toggle if applicable
            if let Some(global_action) = self.keybindings_config.action_for_key(crate::config::Mode::Global, key) {
                if global_action == Action::ToggleInstructions {
                    self.show_instructions = !self.show_instructions;
                    return None;
                }
            }

            // Check for ProjectSettings-specific actions
            if let Some(dialog_action) = self.keybindings_config.action_for_key(crate::config::Mode::ProjectSettings, key) {
                if dialog_action == Action::ToggleDataViewerOption {
                    // Toggle boolean when data viewer option is selected
                    if self.selected_option == SelectedOption::AutoExpandValueDisplay {
                        self.config.data_viewer.auto_exapand_value_display = !self.config.data_viewer.auto_exapand_value_display;
                    }
                    return None;
                }
            }
        }
        
        match &mut self.mode {
            ProjectSettingsDialogMode::Input => {
                if self.error_active {
                    // Only allow Esc or Enter to clear error
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter => {
                                self.error_active = false;
                                self.mode = ProjectSettingsDialogMode::Input;
                            }
                            _ => {}
                        }
                    }
                    return None;
                }

                // If message dialog mode is active, forward events to it
                if self.message_dialog_mode {
                    if let Some(dialog) = &mut self.message_dialog
                        && let Ok(Some(action)) = Component::handle_key_event(dialog, key)
                        && action == Action::DialogClose {
                            self.message_dialog_mode = false;
                            self.message_dialog = None;
                        }
                    return None;
                }

                // If file browser mode is active, forward events to it
                if self.file_browser_mode {
                    if let Some(browser) = &mut self.file_browser
                        && let Some(action) = browser.handle_key_event(key) {
                        match action {
                            FileBrowserAction::Selected(path) => {
                                match self.selected_option {
                                    SelectedOption::WorkspaceBrowse => {
                                        self.workspace_path_input = path.to_string_lossy().to_string();
                                    }
                                    SelectedOption::LlmConfigBrowse => {
                                        self.config_path_input = path.to_string_lossy().to_string();
                                    }
                                    _ => {}
                                }
                                self.file_browser_mode = false;
                                self.file_browser = None;
                            }
                            FileBrowserAction::Cancelled => {
                                self.file_browser_mode = false;
                                self.file_browser = None;
                            }
                        }
                    }
                    return None;
                }

                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Up => {
                            // Up navigation:
                            // Left side: Auto Expand Value Display -> Configure LLM Clients -> LLM Config Path -> Workspace Path
                            // Right side: [Save] -> [Browse] (LLM config path) -> [Browse] (workspace path)
                            self.selected_option = match self.selected_option {
                                // Left side navigation
                                SelectedOption::AutoExpandValueDisplay => SelectedOption::ConfigureLlmClients,
                                SelectedOption::ConfigureLlmClients => SelectedOption::LlmConfigPath,
                                SelectedOption::LlmConfigPath => SelectedOption::WorkspacePath,
                                SelectedOption::WorkspacePath => SelectedOption::AutoExpandValueDisplay, // wrap around
                                
                                // Right side navigation
                                SelectedOption::Save => SelectedOption::LlmConfigBrowse,
                                SelectedOption::LlmConfigBrowse => SelectedOption::WorkspaceBrowse,
                                SelectedOption::WorkspaceBrowse => SelectedOption::Save, // wrap around
                            };
                        }
                        KeyCode::Down => {
                            // Down navigation:
                            // Left side: Workspace Path -> LLM Config Path -> Configure LLM Clients -> Auto Expand Value Display
                            // Right side: [Browse] (workspace path) -> [Browse] (LLM config path) -> [Save]
                            self.selected_option = match self.selected_option {
                                // Left side navigation
                                SelectedOption::WorkspacePath => SelectedOption::LlmConfigPath,
                                SelectedOption::LlmConfigPath => SelectedOption::ConfigureLlmClients,
                                SelectedOption::ConfigureLlmClients => SelectedOption::AutoExpandValueDisplay,
                                SelectedOption::AutoExpandValueDisplay => SelectedOption::WorkspacePath, // wrap around
                                
                                // Right side navigation
                                SelectedOption::WorkspaceBrowse => SelectedOption::LlmConfigBrowse,
                                SelectedOption::LlmConfigBrowse => SelectedOption::Save,
                                SelectedOption::Save => SelectedOption::WorkspaceBrowse, // wrap around
                            };
                        }
                        KeyCode::Enter => {
                            match self.selected_option {
                                SelectedOption::Save => {
                                    // Save settings
                                    if self.save_settings().is_ok() {
                                        return Some(Action::ProjectSettingsApplied(self.config.clone()));
                                    } else {
                                        return None;
                                    }
                                }
                                SelectedOption::ConfigureLlmClients => {
                                    // Open LLM Client dialog
                                    let llm_config = self.keybindings_config.llm_config.clone().unwrap_or_default();
                                    let mut llm_dialog = LlmClientDialog::new_with_config(
                                        self.keybindings_config.clone(),
                                        llm_config
                                    );
                                    let _ = llm_dialog.register_config_handler(self.keybindings_config.clone());
                                    self.llm_client_dialog = Some(llm_dialog);
                                    self.mode = ProjectSettingsDialogMode::LlmClientDialog;
                                }
                                SelectedOption::WorkspaceBrowse | SelectedOption::LlmConfigBrowse => {
                                    // Open file browser
                                    self.file_browser = Some(FileBrowserDialog::new(
                                        Some(self.file_browser_path.clone()),
                                        None,
                                        true,
                                        FileBrowserMode::Load,
                                    ));
                                    self.file_browser_mode = true;
                                    return None;
                                }
                                _ => {
                                    // If on fields, Enter also saves
                                    if self.save_settings().is_ok() {
                                        return Some(Action::ProjectSettingsApplied(self.config.clone()));
                                    } else {
                                        return None;
                                    }
                                }
                            }
                        }
                        KeyCode::Esc => {
                            return Some(Action::DialogClose);
                        }
                        KeyCode::Backspace => {
                            // Handle backspace for current field
                            match self.selected_option {
                                SelectedOption::WorkspacePath => { self.workspace_path_input.pop(); }
                                SelectedOption::LlmConfigPath => { self.config_path_input.pop(); }
                                _ => {}
                            }
                        }
                        KeyCode::Char(c) => {
                            // Handle character input for current field
                            match self.selected_option {
                                SelectedOption::WorkspacePath => { self.workspace_path_input.push(c); }
                                SelectedOption::LlmConfigPath => { self.config_path_input.push(c); }
                                _ => {}
                            }
                        }
                        KeyCode::Right => {
                            // Right navigation:
                            // Workspace Path -> [Browse] (workspace path)
                            // LLM Config Path -> [Browse] (LLM config path)
                            // [Configure LLM Clients] -> [Save]
                            // Auto Expand Value Display -> [Save]
                            self.selected_option = match self.selected_option {
                                SelectedOption::WorkspacePath => SelectedOption::WorkspaceBrowse,
                                SelectedOption::LlmConfigPath => SelectedOption::LlmConfigBrowse,
                                SelectedOption::ConfigureLlmClients => SelectedOption::Save,
                                SelectedOption::AutoExpandValueDisplay => SelectedOption::Save,
                                _ => SelectedOption::WorkspacePath, // default
                            };
                        }
                        KeyCode::Left => {
                            // Left navigation (inverse of Right):
                            // [Browse] (workspace path) -> Workspace Path
                            // [Browse] (LLM config path) -> LLM Config Path
                            // [Save] -> [Configure LLM Clients]
                            // [Save] -> Auto Expand Value Display
                            self.selected_option = match self.selected_option {
                                SelectedOption::WorkspaceBrowse => SelectedOption::WorkspacePath,
                                SelectedOption::LlmConfigBrowse => SelectedOption::LlmConfigPath,
                                SelectedOption::Save => SelectedOption::ConfigureLlmClients, // default choice
                                _ => SelectedOption::WorkspacePath, // default
                            };
                        }
                        _ => {}
                    }
                }
            }
            ProjectSettingsDialogMode::Error(_) => {
                // Only close error on Esc or Enter
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            self.error_active = false;
                            self.mode = ProjectSettingsDialogMode::Input;
                        }
                        _ => {}
                    }
                }
            }
            ProjectSettingsDialogMode::Save => {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            return Some(Action::DialogClose);
                        }
                        _ => {}
                    }
                }
            }
            ProjectSettingsDialogMode::LlmClientDialog => {
                if let Some(dialog) = &mut self.llm_client_dialog {
                    if let Some(action) = dialog.handle_key_event(key) {
                        match action {
                            Action::LlmClientDialogApplied(_config) => {
                                // Handle the applied LLM client configuration
                                // Save the LLM config if it was modified
                                if let Some(llm_dialog) = &self.llm_client_dialog {
                                    // Update the main config with the new LLM config
                                    self.keybindings_config.llm_config = Some(llm_dialog.llm_config.clone());
                                    // Save the config to file
                                    let _ = self.keybindings_config.save_llm_config();
                                }
                                self.llm_client_dialog = None;
                                self.mode = ProjectSettingsDialogMode::Input;
                                return None;
                            }
                            Action::DialogClose => {
                                self.llm_client_dialog = None;
                                self.mode = ProjectSettingsDialogMode::Input;
                                return None;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        None
    }

    /// Save the current input values to the config and persist to JSON if applicable
    fn save_settings(&mut self) -> Result<()> {
        // Store the config path for LLM settings
        // This could be used to load/save LLM config from a custom location
        // For now, we'll use the default location from get_config_dir()
        
        // Only update workspace path if provided; otherwise keep existing config value
        let workspace_input = self.workspace_path_input.trim();
        if !workspace_input.is_empty() {
            let path = PathBuf::from(workspace_input);
            if !path.is_dir() {
                self.set_error(format!("Workspace path is not a valid folder: {workspace_input}"));
                return Err(color_eyre::eyre::eyre!("invalid workspace path"));
            }
            self.config.workspace_path = Some(path.clone());
            // Persist settings JSON into the workspace folder
            let file_path = path.join("datatui_workspace_settings.json");
            let file = File::create(&file_path)
                .map_err(|e| color_eyre::eyre::eyre!("failed to create settings file {}: {}", file_path.display(), e))?;
            serde_json::to_writer_pretty(file, &self.config)
                .map_err(|e| color_eyre::eyre::eyre!("failed to write settings JSON: {}", e))?;
        }
        Ok(())
    }

    /// Set error message and switch to error mode
    pub fn set_error(&mut self, msg: String) {
        self.mode = ProjectSettingsDialogMode::Error(msg);
        self.error_active = true;
    }
}

impl Component for ProjectSettingsDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        self.keybindings_config = _config;
        Ok(())
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        if let Some(Event::Key(key)) = event {
            Ok(self.handle_key_event(key))
        } else {
            Ok(None)
        }
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
