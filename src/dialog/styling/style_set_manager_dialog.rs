//! StyleSetManagerDialog: Dialog for managing style sets
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
use crate::dialog::styling::style_set::StyleSet;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use tracing::error;

/// Dialog mode
#[derive(Debug)]
pub enum StyleSetManagerDialogMode {
    List,
    FileBrowser(Box<FileBrowserDialog>), // for loading/saving folders or files
}

/// StyleSetManagerDialog: UI for managing style sets
#[derive(Debug)]
pub struct StyleSetManagerDialog {
    pub style_set_manager: StyleSetManager,
    pub mode: StyleSetManagerDialogMode,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub show_instructions: bool,
    pub config: Config,
    pub style: StyleConfig,
    pub search_filter: String,
    pub export_selected_id: Option<String>, // ID of style set to export
}

impl StyleSetManagerDialog {
    /// Create a new StyleSetManagerDialog
    pub fn new(style_set_manager: StyleSetManager) -> Self {
        Self {
            style_set_manager,
            mode: StyleSetManagerDialogMode::List,
            selected_index: 0,
            scroll_offset: 0,
            show_instructions: true,
            config: Config::default(),
            style: StyleConfig::default(),
            search_filter: String::new(),
            export_selected_id: None,
        }
    }

    /// Get a reference to the style set manager
    pub fn get_manager(&self) -> &StyleSetManager {
        &self.style_set_manager
    }

    /// Get a mutable reference to the style set manager
    pub fn get_manager_mut(&mut self) -> &mut StyleSetManager {
        &mut self.style_set_manager
    }

    /// Update the style set manager (sync from external manager)
    pub fn sync_manager(&mut self, manager: &StyleSetManager) {
        self.style_set_manager = manager.clone();
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match &self.mode {
            StyleSetManagerDialogMode::List => {
                self.config.actions_to_instructions(&[
                    (Mode::Global, Action::Escape),
                    (Mode::Global, Action::Enter),
                    (Mode::Global, Action::Up),
                    (Mode::Global, Action::Down),
                    (Mode::StyleSetManagerDialog, Action::AddStyleSet),
                    (Mode::StyleSetManagerDialog, Action::RemoveStyleSet),
                    (Mode::StyleSetManagerDialog, Action::DisableStyleSet),
                    (Mode::StyleSetManagerDialog, Action::ImportStyleSet),
                    (Mode::StyleSetManagerDialog, Action::ExportStyleSet),
                    (Mode::StyleSetManagerDialog, Action::OpenStyleSetBrowserDialog),
                    (Mode::Global, Action::ToggleInstructions),
                ])
            }
            StyleSetManagerDialogMode::FileBrowser(_) => {
                "Enter: OK  Esc: Cancel".to_string()
            }
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Set Manager")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        match &self.mode {
            StyleSetManagerDialogMode::List => {
                let block = Block::default()
                    .title("Style Sets")
                    .borders(Borders::ALL);
                block.render(content_area, buf);

                let list_start_y = content_area.y + 1;
                let start_x = content_area.x + 1;
                let max_rows = (content_area.height.saturating_sub(2)) as usize;

                let all_sets = self.style_set_manager.get_all_sets();
                let filtered_sets: Vec<_> = if self.search_filter.is_empty() {
                    all_sets.iter().collect()
                } else {
                    all_sets.iter()
                        .filter(|(id, set, _)| {
                            id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                            set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                            set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                        })
                        .collect()
                };

                if filtered_sets.is_empty() {
                    buf.set_string(start_x, list_start_y, "No style sets loaded.", Style::default().fg(Color::DarkGray));
                } else {
                    // Adjust selected_index to be within bounds
                    let selected_idx = self.selected_index.min(filtered_sets.len().saturating_sub(1));
                    
                    // Calculate scroll offset to keep selected item visible
                    let scroll_offset = if selected_idx < self.scroll_offset {
                        selected_idx
                    } else if selected_idx >= self.scroll_offset + max_rows {
                        selected_idx + 1 - max_rows
                    } else {
                        self.scroll_offset
                    };

                    let end = (scroll_offset + max_rows).min(filtered_sets.len());
                    for (vis_idx, i) in (scroll_offset..end).enumerate() {
                        let y = list_start_y + vis_idx as u16;
                        let (id, set, enabled) = filtered_sets[i];
                        let mut style = Style::default();
                        if i == selected_idx {
                            style = style.fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD);
                        } else if i % 2 == 0 {
                            style = style.bg(Color::Rgb(30, 30, 30));
                        }
                        let status = if *enabled { "[✓]" } else { "[ ]" };
                        let line = format!("{} {} - {} ({})", status, set.name, set.description, id);
                        buf.set_string(start_x, y, line, style);
                    }
                }

                // Render search filter if active
                if !self.search_filter.is_empty() {
                    let search_y = content_area.y + content_area.height.saturating_sub(1);
                    buf.set_string(start_x, search_y, format!("Search: {}", self.search_filter), Style::default().fg(Color::Yellow));
                }
            }
            StyleSetManagerDialogMode::FileBrowser(browser) => {
                browser.render(content_area, buf);
                return 0;
            }
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

        0
    }

    /// Handle a key event (public for external use)
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {

        // Handle FileBrowser mode first
        if let StyleSetManagerDialogMode::FileBrowser(browser) = &mut self.mode {
            if let Some(action) = browser.handle_key_event(key) {
                match action {
                    FileBrowserAction::Selected(path) => {
                        match browser.mode {
                            FileBrowserMode::Save => {
                                // Export selected style set to file
                                if let Some(ref id) = self.export_selected_id {
                                    if let Some(style_set) = self.style_set_manager.get_set(id) {
                                        if let Err(e) = self.style_set_manager.save_to_file(style_set, &path) {
                                            error!("Failed to export style set: {}", e);
                                        }
                                    }
                                }
                                self.export_selected_id = None;
                                self.mode = StyleSetManagerDialogMode::List;
                            }
                            FileBrowserMode::Load => {
                                // Import style set from file
                                if path.is_file() {
                                    if let Err(e) = self.style_set_manager.load_from_file(&path) {
                                        error!("Failed to import style set: {}", e);
                                    }
                                } else if path.is_dir() {
                                    // Load all style sets from folder
                                    if let Err(e) = self.style_set_manager.load_from_folder(&path) {
                                        error!("Failed to load style sets from folder: {}", e);
                                    }
                                }
                                self.mode = StyleSetManagerDialogMode::List;
                            }
                        }
                    }
                    FileBrowserAction::Cancelled => {
                        self.export_selected_id = None;
                        self.mode = StyleSetManagerDialogMode::List;
                    }
                }
            }
            return None;
        }

        if key.kind == KeyEventKind::Press {
            // Check Global actions first
            if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
                match global_action {
                    Action::Escape => {
                        return Some(Action::CloseStyleSetManagerDialog);
                    }
                    Action::Enter => {
                        // Toggle enabled state of selected style set
                        let all_sets: Vec<(String, bool)> = {
                            let manager_sets = self.style_set_manager.get_all_sets();
                            manager_sets.into_iter()
                                .filter(|(id, set, _)| {
                                    self.search_filter.is_empty() ||
                                    id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                    set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                    set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                                })
                                .map(|(id, _, enabled)| (id.clone(), enabled))
                                .collect()
                        };
                        if let Some((id, enabled)) = all_sets.get(self.selected_index) {
                            if *enabled {
                                self.style_set_manager.disable_style_set(id);
                            } else {
                                self.style_set_manager.enable_style_set(id);
                            }
                        }
                        return None;
                    }
                    Action::Up => {
                        if self.selected_index > 0 {
                            self.selected_index -= 1;
                            // Update scroll_offset to keep selected item visible
                            // This will be properly calculated in render, but we can adjust here too
                        }
                        return None;
                    }
                    Action::Down => {
                        let all_sets = self.style_set_manager.get_all_sets();
                        let filtered_count = if self.search_filter.is_empty() {
                            all_sets.len()
                        } else {
                            all_sets.iter()
                                .filter(|(id, set, _)| {
                                    id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                    set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                    set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                                })
                                .count()
                        };
                        if self.selected_index < filtered_count.saturating_sub(1) {
                            self.selected_index += 1;
                        }
                        return None;
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return None;
                    }
                    _ => {}
                }
            }

            // Check dialog-specific actions
            if let Some(dialog_action) = self.config.action_for_key(Mode::StyleSetManagerDialog, key) {
                match dialog_action {
                    Action::AddStyleSet => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            // Create a new empty style set
                            let new_set = StyleSet {
                                name: format!("New Style Set {}", self.style_set_manager.get_all_sets().len() + 1),
                                description: String::new(),
                                rules: vec![],
                            };
                            let id = self.style_set_manager.add_set(new_set);
                            // Auto-enable the new set
                            self.style_set_manager.enable_style_set(&id);
                            // Adjust selected_index to point to the new set
                            let all_sets = self.style_set_manager.get_all_sets();
                            if let Some(pos) = all_sets.iter().position(|(set_id, _, _)| **set_id == id) {
                                self.selected_index = pos;
                            }
                        }
                        return None;
                    }
                    Action::RemoveStyleSet => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            let all_sets: Vec<String> = {
                                let manager_sets = self.style_set_manager.get_all_sets();
                                manager_sets.into_iter()
                                    .filter(|(id, set, _)| {
                                        self.search_filter.is_empty() ||
                                        id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                                    })
                                    .map(|(id, _, _)| id.clone())
                                    .collect()
                            };
                            if let Some(id) = all_sets.get(self.selected_index) {
                                self.style_set_manager.remove_set(id);
                                // Adjust selected_index if needed
                                if self.selected_index >= all_sets.len().saturating_sub(1) && self.selected_index > 0 {
                                    self.selected_index -= 1;
                                }
                            }
                        }
                        return None;
                    }
                    Action::DisableStyleSet => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            let all_sets: Vec<(String, bool)> = {
                                let manager_sets = self.style_set_manager.get_all_sets();
                                manager_sets.into_iter()
                                    .filter(|(id, set, _)| {
                                        self.search_filter.is_empty() ||
                                        id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                                    })
                                    .map(|(id, _, enabled)| (id.clone(), enabled))
                                    .collect()
                            };
                            if let Some((id, enabled)) = all_sets.get(self.selected_index) {
                                if *enabled {
                                    self.style_set_manager.disable_style_set(id);
                                } else {
                                    self.style_set_manager.enable_style_set(id);
                                }
                            }
                        }
                        return None;
                    }
                    Action::ImportStyleSet => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            // Open file browser for importing (can be file or folder)
                            let mut browser = FileBrowserDialog::new(
                                None,
                                Some(vec!["yaml", "yml"]),
                                false, // folder_only = false, allow files and folders
                                FileBrowserMode::Load,
                            );
                            browser.register_config_handler(self.config.clone());
                            self.mode = StyleSetManagerDialogMode::FileBrowser(Box::new(browser));
                        }
                        return None;
                    }
                    Action::ExportStyleSet => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            // Get selected style set ID
                            let all_sets: Vec<String> = {
                                let manager_sets = self.style_set_manager.get_all_sets();
                                manager_sets.into_iter()
                                    .filter(|(id, set, _)| {
                                        self.search_filter.is_empty() ||
                                        id.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.name.to_lowercase().contains(&self.search_filter.to_lowercase()) ||
                                        set.description.to_lowercase().contains(&self.search_filter.to_lowercase())
                                    })
                                    .map(|(id, _, _)| id.clone())
                                    .collect()
                            };
                            if let Some(id) = all_sets.get(self.selected_index) {
                                self.export_selected_id = Some(id.clone());
                                // Open file browser for saving
                                let mut browser = FileBrowserDialog::new(
                                    None,
                                    Some(vec!["yaml", "yml"]),
                                    false, // folder_only = false
                                    FileBrowserMode::Save,
                                );
                                browser.register_config_handler(self.config.clone());
                                self.mode = StyleSetManagerDialogMode::FileBrowser(Box::new(browser));
                            }
                        }
                        return None;
                    }
                    Action::OpenStyleSetBrowserDialog => {
                        if matches!(self.mode, StyleSetManagerDialogMode::List) {
                            // Open file browser for loading folders (folder_only = true)
                            let mut browser = FileBrowserDialog::new(
                                None,
                                Some(vec!["yaml", "yml"]),
                                true, // folder_only
                                FileBrowserMode::Load,
                            );
                            browser.register_config_handler(self.config.clone());
                            self.mode = StyleSetManagerDialogMode::FileBrowser(Box::new(browser));
                        }
                        return None;
                    }
                    _ => {}
                }
            }
        }

        None
    }
}

impl Component for StyleSetManagerDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
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

