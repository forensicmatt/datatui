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
use crate::providers::openai::Client as OpenAIClient;
use crate::sql::set_openai_embeddings_provider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSettingsDialogMode {
    Input,
    Error(String),
    Save,
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

// Derive Default instead of manual impl

// Derive Default instead of manual impl

#[derive(Debug)]
pub struct ProjectSettingsDialog {
    pub config: ProjectSettingsConfig,
    pub mode: ProjectSettingsDialogMode,
    pub error_active: bool,
    pub show_instructions: bool,
    pub openai_key_input: String,
    pub workspace_path_input: String,
    pub current_field: usize, // 0 = workspace_path, 1 = openai_key
    pub browse_button_selected: bool,
    pub connect_button_selected: bool,
    pub finish_button_selected: bool,
    pub file_browser_mode: bool,
    pub file_browser_path: PathBuf,
    pub file_browser: Option<FileBrowserDialog>,
    pub openai_client: Option<OpenAIClient>,
    pub message_dialog_mode: bool,
    pub message_dialog: Option<MessageDialog>,
    pub data_viewer_selected: bool,
    pub keybindings_config: crate::config::Config,
}

impl ProjectSettingsDialog {
    pub fn new(config: ProjectSettingsConfig) -> Self {
        let mut openai_key_input = config.openai_key.clone().unwrap_or_default();
        if openai_key_input.trim().is_empty()
            && let Ok(env_key) = std::env::var("OPENAI_API_KEY")
            && !env_key.trim().is_empty() {
            openai_key_input = env_key;
        }
        let workspace_path_input = String::new();
        
        Self {
            config,
            mode: ProjectSettingsDialogMode::Input,
            error_active: false,
            show_instructions: true,
            openai_key_input,
            workspace_path_input,
            current_field: 0,
            browse_button_selected: false,
            connect_button_selected: false,
            finish_button_selected: false,
            file_browser_mode: false,
            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_browser: None,
            openai_client: None,
            message_dialog_mode: false,
            message_dialog: None,
            data_viewer_selected: false,
            keybindings_config: crate::config::Config::default(),
        }
    }

    fn in_buttons_zone(&self) -> bool {
        self.browse_button_selected || self.connect_button_selected || self.finish_button_selected
    }

    fn select_settings_index(&mut self, idx: usize) {
        // Clear buttons
        self.browse_button_selected = false;
        self.connect_button_selected = false;
        self.finish_button_selected = false;
        // Select setting
        match idx % 3 {
            0 => { self.current_field = 0; self.data_viewer_selected = false; }
            1 => { self.current_field = 1; self.data_viewer_selected = false; }
            2 => { self.data_viewer_selected = true; self.current_field = 2; }
            _ => {}
        }
    }

    fn select_button_index(&mut self, idx: usize) {
        // Clear settings
        self.data_viewer_selected = false;
        // Select button
        let i = idx % 3;
        self.browse_button_selected = i == 0;
        self.connect_button_selected = i == 1;
        self.finish_button_selected = i == 2;
    }

    fn current_settings_index(&self) -> usize {
        if self.data_viewer_selected { 2 } else if self.current_field == 1 { 1 } else { 0 }
    }

    fn current_button_index(&self) -> usize {
        if self.browse_button_selected { 0 } else if self.connect_button_selected { 1 } else if self.finish_button_selected { 2 } else { 0 }
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

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let wrap_width = content_area.width.saturating_sub(2) as usize;

        // Render file browser dialog overlay if active
        if self.file_browser_mode {
            if let Some(browser) = &self.file_browser {
                browser.render(area, buf);
            }
            return 1;
        }

        match &self.mode {
            ProjectSettingsDialogMode::Input => {
                let block = Block::default()
                    .title("Project Settings")
                    .borders(Borders::ALL);
                let input_area = block.inner(content_area);
                block.render(content_area, buf);

                // Inline fields: "Label: value" with selection highlighting
                let line0_y = input_area.y; // Workspace first
                let line1_y = input_area.y + 1; // OpenAI second
                let line2_y = input_area.y + 3; // Data Viewer section top

                // Workspace Path line + [Browse]
                let label_wp = "Workspace Path: ";
                let style_wp = if self.current_field == 0 && !self.browse_button_selected && !self.finish_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                let wp_text = format!("{}{}", label_wp, self.workspace_path_input);
                buf.set_string(input_area.x, line0_y, wp_text, style_wp);
                // [Browse] button on same line, right-aligned
                let browse_text = "[Browse]";
                let browse_x = input_area
                    .x
                    + input_area.width.saturating_sub(browse_text.len() as u16 + 1);
                let browse_style = if self.browse_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(browse_x, line0_y, browse_text, browse_style);
                // Cursor on workspace input when focused and not on a button
                if self.current_field == 0 && !self.browse_button_selected && !self.finish_button_selected {
                    let cursor_x = input_area.x + (label_wp.len() + self.workspace_path_input.len()) as u16;
                    buf.set_string(cursor_x, line0_y, "_", Style::default().fg(Color::Yellow));
                }

                // OpenAI API Key line + [Connect]
                let label_key = "OpenAI API Key: ";
                let style_key = if self.current_field == 1 && !self.browse_button_selected && !self.finish_button_selected && !self.connect_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                let key_text = format!("{}{}", label_key, self.openai_key_input);
                buf.set_string(input_area.x, line1_y, key_text, style_key);
                if self.current_field == 1 && !self.browse_button_selected && !self.finish_button_selected && !self.connect_button_selected {
                    let cursor_x = input_area.x + (label_key.len() + self.openai_key_input.len()) as u16;
                    buf.set_string(cursor_x, line1_y, "_", Style::default().fg(Color::Yellow));
                }
                // [Connect] button on same line, right-aligned
                let connect_text = "[Connect]";
                let connect_x = input_area
                    .x
                    + input_area.width.saturating_sub(connect_text.len() as u16 + 1);
                let connect_style = if self.connect_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                };
                buf.set_string(connect_x, line1_y, connect_text, connect_style);

                // Data Viewer section with bordered block
                let dv_block_area = Rect {
                    x: input_area.x,
                    y: line2_y,
                    width: input_area.width,
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
                let dv_style = if self.data_viewer_selected && !self.finish_button_selected && !self.browse_button_selected && !self.connect_button_selected {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White)
                };
                buf.set_string(dv_inner.x, dv_inner.y, format!("{dv_label}{dv_value}"), dv_style);

                // [Save] button at bottom-right of content area
                let save_text = "[Save]";
                let save_x = content_area.x + content_area.width.saturating_sub(save_text.len() as u16 + 2);
                let save_y = content_area.y + content_area.height.saturating_sub(2);
                let save_style = if self.finish_button_selected {
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
        }

        if self.show_instructions && let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
        // Finally, render message dialog overlay if active
        if self.message_dialog_mode && let Some(msg) = &self.message_dialog {
            msg.render(area, buf);
        }
        1
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
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
                    if self.data_viewer_selected {
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
                                self.workspace_path_input = path.to_string_lossy().to_string();
                                self.file_browser_mode = false;
                                self.file_browser = None;
                                self.browse_button_selected = false;
                            }
                            FileBrowserAction::Cancelled => {
                                self.file_browser_mode = false;
                                self.file_browser = None;
                                self.browse_button_selected = false;
                            }
                        }
                    }
                    return None;
                }

                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Tab => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                // Reverse cycle: Save -> Connect -> OpenAI -> Browse -> Workspace
                                if self.finish_button_selected {
                                    self.finish_button_selected = false;
                                    self.current_field = 1;
                                    self.connect_button_selected = true;
                                    self.browse_button_selected = false;
                                    // ensure only [Connect] is selected
                                } else if self.current_field == 1 {
                                    if self.connect_button_selected {
                                        self.connect_button_selected = false;
                                        // stay on OpenAI input
                                    } else {
                                        self.current_field = 0;
                                        self.browse_button_selected = true;
                                        // unselect [Connect] when [Browse] is selected
                                        self.connect_button_selected = false;
                                    }
                                } else if self.current_field == 0 && self.browse_button_selected {
                                    self.browse_button_selected = false;
                                    self.current_field = 0;
                                    // unselect [Connect] when leaving [Browse]
                                    self.connect_button_selected = false;
                                } else {
                                    self.finish_button_selected = true;
                                    // unselect [Connect] when [Save] is selected
                                    self.connect_button_selected = false;
                                }
                            } else {
                                // Cycle: Workspace -> Browse -> OpenAI -> Connect -> Save
                                if self.current_field == 0 && !self.browse_button_selected {
                                    self.browse_button_selected = true;
                                    // unselect [Connect] when [Browse] is selected
                                    self.connect_button_selected = false;
                                } else if self.current_field == 0 && self.browse_button_selected {
                                    self.browse_button_selected = false;
                                    self.current_field = 1;
                                    // entering OpenAI input, ensure [Connect] not selected
                                    self.connect_button_selected = false;
                                } else if self.current_field == 1 {
                                    if !self.connect_button_selected {
                                        self.connect_button_selected = true;
                                    } else {
                                        self.connect_button_selected = false;
                                        self.finish_button_selected = true;
                                    }
                                } else if self.finish_button_selected {
                                    self.finish_button_selected = false;
                                    self.current_field = 0;
                                    // leaving [Save], ensure [Connect] not selected
                                    self.connect_button_selected = false;
                                }
                            }
                        }
                        KeyCode::Up => {
                            if self.in_buttons_zone() {
                                // Cycle buttons: Browse -> Connect -> Save -> Browse ... (reverse on Up)
                                let idx = self.current_button_index();
                                let next = (idx + 2) % 3; // -1 mod 3
                                self.select_button_index(next);
                            } else {
                                // Cycle settings: Workspace -> OpenAI -> DataViewer -> Workspace ... (reverse on Up)
                                let idx = self.current_settings_index();
                                let next = (idx + 2) % 3; // -1 mod 3
                                self.select_settings_index(next);
                            }
                        }
                        KeyCode::Down => {
                            if self.in_buttons_zone() {
                                // Cycle buttons forward
                                let idx = self.current_button_index();
                                let next = (idx + 1) % 3;
                                self.select_button_index(next);
                            } else {
                                // Cycle settings forward
                                let idx = self.current_settings_index();
                                let next = (idx + 1) % 3;
                                self.select_settings_index(next);
                            }
                        }
                        KeyCode::Enter => {
                            if self.finish_button_selected {
                                // Save settings
                                if self.save_settings().is_ok() {
                                    return Some(Action::ProjectSettingsApplied(self.config.clone()));
                                } else {
                                    return None;
                                }
                            }
                            if self.connect_button_selected {
                                // Create OpenAI client using input or env var
                                let key = if !self.openai_key_input.trim().is_empty() {
                                    Some(self.openai_key_input.trim().to_string())
                                } else {
                                    std::env::var("OPENAI_API_KEY").ok()
                                };
                                match key {
                                    Some(k) if !k.is_empty() => {
                                        let client = OpenAIClient::new(k);
                                        // Wire up global embeddings provider using this client
                                        set_openai_embeddings_provider(client.clone(), None);
                                        self.openai_client = Some(client);
                                        // Show confirmation message dialog
                                        self.message_dialog = Some(MessageDialog::with_title(
                                            "OpenAI API key has been set.",
                                            "Info",
                                        ));
                                        self.message_dialog_mode = true;
                                        return None;
                                    }
                                    _ => {
                                        self.set_error("OpenAI API key not set. Enter a key or set OPENAI_API_KEY.".to_string());
                                        return None;
                                    }
                                }
                            }
                            if self.browse_button_selected {
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
                            // If on fields, Enter also saves
                            if self.save_settings().is_ok() {
                                return Some(Action::ProjectSettingsApplied(self.config.clone()));
                            } else {
                                return None;
                            }
                        }
                        KeyCode::Esc => {
                            return Some(Action::DialogClose);
                        }
                        KeyCode::Backspace => {
                            // Handle backspace for current field
                            if !self.browse_button_selected && !self.finish_button_selected {
                                match self.current_field {
                                    0 => { self.workspace_path_input.pop(); }
                                    1 => { self.openai_key_input.pop(); }
                                    _ => {}
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            // Handle character input for current field
                            if !self.browse_button_selected && !self.finish_button_selected {
                                match self.current_field {
                                    0 => { self.workspace_path_input.push(c); }
                                    1 => { self.openai_key_input.push(c); }
                                    _ => {}
                                }
                            }
                        }
                        KeyCode::Right => {
                            // Switch to buttons zone, keep index aligned: 0->Browse, 1->Connect, 2->Save
                            if !self.in_buttons_zone() {
                                let idx = self.current_settings_index();
                                self.select_button_index(idx);
                            }
                        }
                        KeyCode::Left => {
                            // Switch to settings zone, keep index aligned: 0->Workspace, 1->OpenAI, 2->DataViewer
                            if self.in_buttons_zone() {
                                let idx = self.current_button_index();
                                self.select_settings_index(idx);
                            }
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
        }
        None
    }

    /// Save the current input values to the config and persist to JSON if applicable
    fn save_settings(&mut self) -> Result<()> {
        self.config.openai_key = if self.openai_key_input.trim().is_empty() {
            None
        } else {
            Some(self.openai_key_input.trim().to_string())
        };
        
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

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
        if key.kind == KeyEventKind::Press {
            // Handle Ctrl+I for instructions toggle if applicable
            if let Some(global_action) = self.keybindings_config.action_for_key(crate::config::Mode::Global, key) {
                if global_action == Action::ToggleInstructions {
                    self.show_instructions = !self.show_instructions;
                    return Ok(None);
                }
            }

            // Check for ProjectSettings-specific actions
            if let Some(dialog_action) = self.keybindings_config.action_for_key(crate::config::Mode::ProjectSettings, key) {
                if dialog_action == Action::ToggleDataViewerOption {
                    // Toggle boolean when data viewer option is selected
                    if self.data_viewer_selected {
                        self.config.data_viewer.auto_exapand_value_display = !self.config.data_viewer.auto_exapand_value_display;
                    }
                    return Ok(None);
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
                    return Ok(None);
                }

                // If file browser mode is active, forward events to it
                if self.file_browser_mode {
                    if let Some(browser) = &mut self.file_browser
                        && let Some(action) = browser.handle_key_event(key) {
                        match action {
                            FileBrowserAction::Selected(path) => {
                                self.workspace_path_input = path.to_string_lossy().to_string();
                                self.file_browser_mode = false;
                                self.file_browser = None;
                                self.browse_button_selected = false;
                            }
                            FileBrowserAction::Cancelled => {
                                self.file_browser_mode = false;
                                self.file_browser = None;
                                self.browse_button_selected = false;
                            }
                        }
                    }
                    return Ok(None);
                }

                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Tab => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                // Reverse cycle: Save -> OpenAI -> Browse -> Workspace
                                if self.finish_button_selected {
                                    self.finish_button_selected = false;
                                    self.current_field = 1;
                                    self.browse_button_selected = false;
                                } else if self.current_field == 1 {
                                    self.current_field = 0;
                                    self.browse_button_selected = true;
                                } else if self.current_field == 0 && self.browse_button_selected {
                                    self.browse_button_selected = false;
                                    self.current_field = 0;
                                } else {
                                    self.finish_button_selected = true;
                                }
                            } else {
                                // Cycle: Workspace -> Browse -> OpenAI -> Save
                                if self.current_field == 0 && !self.browse_button_selected {
                                    self.browse_button_selected = true;
                                } else if self.current_field == 0 && self.browse_button_selected {
                                    self.browse_button_selected = false;
                                    self.current_field = 1;
                                } else if self.current_field == 1 {
                                    self.finish_button_selected = true;
                                } else if self.finish_button_selected {
                                    self.finish_button_selected = false;
                                    self.current_field = 0;
                                }
                            }
                        }
                        KeyCode::Up => {
                            if self.finish_button_selected {
                                self.finish_button_selected = false;
                                self.browse_button_selected = true;
                                self.current_field = 0;
                            } else if self.data_viewer_selected {
                                // Move from data viewer option up to OpenAI [Connect]
                                self.data_viewer_selected = false;
                                self.current_field = 1;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.current_field = 0;
                            } else if self.current_field == 1 {
                                self.current_field = 0;
                            }
                        }
                        KeyCode::Down => {
                            if self.current_field == 0 && !self.browse_button_selected {
                                self.current_field = 1;
                            } else if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.finish_button_selected = true;
                            } else if self.current_field == 1 {
                                // Move from OpenAI to Data Viewer option
                                self.data_viewer_selected = true;
                            } else if self.data_viewer_selected {
                                // Move from Data Viewer to Save
                                self.data_viewer_selected = false;
                                self.finish_button_selected = true;
                            }
                        }
                        KeyCode::Enter => {
                            if self.finish_button_selected {
                                // Save settings
                                if self.save_settings().is_ok() {
                                    return Ok(Some(Action::ProjectSettingsApplied(self.config.clone())));
                                } else {
                                    return Ok(None);
                                }
                            }
                            if self.browse_button_selected {
                                // Open file browser
                                self.file_browser = Some(FileBrowserDialog::new(
                                    Some(self.file_browser_path.clone()),
                                    None,
                                    false,
                                    FileBrowserMode::Load,
                                ));
                                self.file_browser_mode = true;
                                return Ok(None);
                            }
                            // If on fields, Enter also saves
                            if self.save_settings().is_ok() {
                                return Ok(Some(Action::ProjectSettingsApplied(self.config.clone())));
                            } else {
                                return Ok(None);
                            }
                        }
                        KeyCode::Esc => {
                            return Ok(Some(Action::DialogClose));
                        }
                        KeyCode::Backspace => {
                            // Handle backspace for current field
                            if !self.browse_button_selected && !self.finish_button_selected {
                                match self.current_field {
                                    0 => { self.workspace_path_input.pop(); }
                                    1 => { self.openai_key_input.pop(); }
                                    _ => {}
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            // Handle character input for current field
                            if !self.browse_button_selected && !self.finish_button_selected {
                                match self.current_field {
                                    0 => { self.workspace_path_input.push(c); }
                                    1 => { self.openai_key_input.push(c); }
                                    _ => {}
                                }
                            }
                        }
                        KeyCode::Right => {
                            if self.current_field == 0 && !self.browse_button_selected {
                                self.browse_button_selected = true;
                            } else if !self.finish_button_selected && self.current_field == 1 {
                                self.finish_button_selected = true;
                            }
                        }
                        KeyCode::Left => {
                            if self.browse_button_selected {
                                self.browse_button_selected = false;
                                self.current_field = 0;
                            } else if self.finish_button_selected {
                                self.finish_button_selected = false;
                                self.current_field = 1;
                            }
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
                            return Ok(Some(Action::DialogClose));
                        }
                        _ => {}
                    }
                }
            }
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
