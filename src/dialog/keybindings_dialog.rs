//! KeybindingsDialog: Configure keybindings per grouping (mode) with dropdown, list, and capture overlay
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};

use crate::action::Action;
use crate::components::Component;
use crate::config::{Config, Mode};
use crate::style::StyleConfig;
use crate::components::dialog_layout::split_dialog_area;

// No explicit focus enum for now; dropdown and list are navigated via configured keys

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeybindingEntry {
    pub action: Action,
    pub key_display: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureState {
    Inactive,
    Active { action_index: usize, pressed_display: String, pressed_keys: Vec<KeyEvent> },
}

impl Default for CaptureState {
    fn default() -> Self { CaptureState::Inactive }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeybindingsDialog {
    pub show_instructions: bool,
    #[serde(skip)]
    pub config: Config,
    pub styles: StyleConfig,
    pub selected_grouping: usize,
    pub selected_index: usize,
    pub scroll_offset: usize,
    #[serde(skip)]
    capture_state: CaptureState,
}

impl Default for KeybindingsDialog {
    fn default() -> Self { Self::new() }
}

impl KeybindingsDialog {
    pub fn new() -> Self {
        Self {
            show_instructions: true,
            config: Config::default(),
            styles: StyleConfig::default(),
            selected_grouping: 0,
            selected_index: 0,
            scroll_offset: 0,
            capture_state: CaptureState::Inactive,
        }
    }

    pub fn get_config(&self) -> Config {
        self.config.clone()
    }

    fn groupings(&self) -> Vec<Mode> {
        vec![
            Mode::DataTabManager,
            Mode::Global,
            Mode::DataTableContainer,
            Mode::DataManagement,
            Mode::DataImport,
            Mode::CsvOptions,
            Mode::Sort,
            Mode::Filter,
            Mode::Find,
            Mode::FindAllResults,
            Mode::JmesPath,
            Mode::SqlDialog,
            Mode::XlsxOptionsDialog,
            Mode::ParquetOptionsDialog,
            Mode::SqliteOptionsDialog,
            Mode::FileBrowser,
            Mode::ColumnWidthDialog,
            Mode::JsonOptionsDialog,
            Mode::AliasEdit,
            Mode::ColumnOperationOptions,
            Mode::ColumnOperations,
            Mode::DataFrameDetails,
            Mode::ProjectSettings,
            Mode::TableExport,
            Mode::KeybindingsDialog,
        ]
    }

    fn current_mode(&self) -> Mode { self.groupings()[self.selected_grouping] }

    fn entries_for_mode(&self, mode: Mode) -> Vec<KeybindingEntry> {
        let mut entries: Vec<KeybindingEntry> = vec![];
        if let Some(map) = self.config.keybindings.0.get(&mode) {
            for (_seq, action) in map.iter() {
                let key_display = self.config.key_for_action(mode, action).unwrap_or_default();
                entries.push(KeybindingEntry { action: action.clone(), key_display });
            }
        }
        entries.sort_by(|a, b| format!("{}", a.action).cmp(&format!("{}", b.action)));
        entries
    }

    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (Mode::Global, Action::Enter),
            (Mode::Global, Action::Escape),
            (Mode::KeybindingsDialog, Action::OpenGroupingDropdown),
            (Mode::KeybindingsDialog, Action::StartRebinding),
            (Mode::KeybindingsDialog, Action::SaveKeybindings),
        ])
    }

    fn render_dropdown(&self, area: Rect, buf: &mut Buffer, focused: bool) {
        let modes = self.groupings();
        let title = "Select Grouping";
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        block.render(area, buf);
        let mut x = inner.x;
        for (i, m) in modes.iter().enumerate() {
            let name = format!("{}", serde_json::to_string(m).unwrap_or_else(|_| format!("{:?}", m)));
            let mut style = Style::default();
            if i == self.selected_grouping {
                style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
            }
            if focused { style = style.add_modifier(Modifier::UNDERLINED); }
            buf.set_string(x, inner.y, name.trim_matches('"'), style);
            x = x.saturating_add(name.len() as u16 + 2);
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        let entries = self.entries_for_mode(self.current_mode());
        let total_items = entries.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;
        let content_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        // Header
        let header = Line::from(vec![
            Span::styled("Key", self.styles.table_header),
            Span::raw("  "),
            Span::styled("Action", self.styles.table_header),
        ]);
        buf.set_line(area.x, area.y, &header, content_width);

        // Rows
        for (vis_idx, i) in (start_idx..end_idx).enumerate() {
            let y = area.y + 1 + vis_idx as u16;
            let is_selected = i == self.selected_index;
            let zebra = i % 2 == 0;
            let base = if zebra { self.styles.table_row_even } else { self.styles.table_row_odd };
            let style = if is_selected { self.styles.selected_row } else { base };
            let e = &entries[i];
            let text = format!("{:<20}  {}", e.key_display, self.config.action_to_friendly_name(&e.action));
            buf.set_string(area.x, y, text, style);
        }

        // Scrollbar
        if show_scroll_bar {
            let viewport = max_rows;
            let position_for_bar = if self.scroll_offset == 0 { 0 } else {
                self.scroll_offset
                    .saturating_add(viewport.saturating_sub(1))
                    .min(total_items.saturating_sub(1))
            };
            let scrollbar = ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Cyan));
            let mut state = ratatui::widgets::ScrollbarState::new(total_items)
                .position(position_for_bar)
                .viewport_content_length(viewport);
            scrollbar.render(area, buf, &mut state);
        }
    }

    fn render_capture_overlay(&self, area: Rect, buf: &mut Buffer) {
        if let CaptureState::Active { action_index: _, pressed_display, .. } = &self.capture_state {
            let overlay_block = Block::default()
                .title("Press new keybinding, Enter to apply, Esc to cancel")
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .style(self.styles.dialog);
            let inner = overlay_block.inner(area);
            overlay_block.render(area, buf);
            let p = Paragraph::new(pressed_display.as_str()).wrap(Wrap { trim: true });
            p.render(inner, buf);
        }
    }

    fn commit_capture(&mut self) {
        let mode = self.current_mode();
        let (action_index_opt, pressed_keys_opt) = match &self.capture_state {
            CaptureState::Active { action_index, pressed_keys, .. } => (Some(*action_index), Some(pressed_keys.clone())),
            _ => (None, None),
        };
        let entries_vec = self.entries_for_mode(mode);
        let action_to_set = action_index_opt
            .and_then(|idx| entries_vec.get(idx).map(|e| e.action.clone()));
        if let (Some(pressed_keys), Some(action)) = (pressed_keys_opt, action_to_set) {
            if let Some(entries) = self.config.keybindings.0.get_mut(&mode) {
                let to_remove: Vec<Vec<KeyEvent>> = entries.iter()
                    .filter_map(|(keys, act)| if act == &action { Some(keys.clone()) } else { None })
                    .collect();
                for k in to_remove { entries.remove(&k); }
                entries.insert(pressed_keys, action);
            }
        }
        self.capture_state = CaptureState::Inactive;
    }
}

impl Component for KeybindingsDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind != KeyEventKind::Press { return Ok(None); }

        // Global first
        if let Some(a) = self.config.action_for_key(Mode::Global, key) {
            match a {
                Action::Escape => {
                    if matches!(self.capture_state, CaptureState::Active { .. }) {
                        self.capture_state = CaptureState::Inactive;
                        return Ok(None);
                    }
                    return Ok(Some(Action::DialogClose));
                }
                Action::Enter => {
                    if let CaptureState::Active { .. } = &self.capture_state {
                        // Confirm via dialog action below
                    }
                }
                Action::Up => {
                    if self.selected_index > 0 { self.selected_index -= 1; }
                    if self.selected_index < self.scroll_offset { self.scroll_offset = self.selected_index; }
                }
                Action::Down => {
                    self.selected_index = self.selected_index.saturating_add(1);
                }
                _ => {}
            }
        }

        // Dialog-specific
        if let Some(a) = self.config.action_for_key(Mode::KeybindingsDialog, key) {
            match a {
                Action::OpenGroupingDropdown | Action::SelectNextGrouping | Action::SelectPrevGrouping => {
                    match a {
                        Action::SelectNextGrouping => {
                            let max = self.groupings().len();
                            if self.selected_grouping + 1 < max { self.selected_grouping += 1; }
                            self.selected_index = 0; self.scroll_offset = 0;
                        }
                        Action::SelectPrevGrouping => {
                            if self.selected_grouping > 0 { self.selected_grouping -= 1; }
                            self.selected_index = 0; self.scroll_offset = 0;
                        }
                        _ => {}
                    }
                    return Ok(None);
                }
                Action::StartRebinding => {
                    self.capture_state = CaptureState::Active { action_index: self.selected_index, pressed_display: String::new(), pressed_keys: vec![] };
                    return Ok(None);
                }
                Action::ConfirmRebinding => { self.commit_capture(); return Ok(None); }
                Action::CancelRebinding => { self.capture_state = CaptureState::Inactive; return Ok(None); }
                Action::ClearBinding => {
                    let mode = self.current_mode();
                    let entries_vec = self.entries_for_mode(mode);
                    let action_to_clear = entries_vec
                        .get(self.selected_index)
                        .map(|e| e.action.clone());
                    if let Some(action) = action_to_clear {
                        if let Some(entries) = self.config.keybindings.0.get_mut(&mode) {
                            let to_remove: Vec<Vec<KeyEvent>> = entries.iter()
                                .filter_map(|(keys, act)| if act == &action { Some(keys.clone()) } else { None })
                                .collect();
                            for k in to_remove { entries.remove(&k); }
                        }
                    }
                    return Ok(None);
                }
                Action::SaveKeybindings => {
                    // Persist to config file path
                    // For now, signal caller to save workspace (reuse existing action)
                    return Ok(Some(Action::SaveWorkspaceState));
                }
                _ => {}
            }
        }

        // If capturing, record key sequence and update display
        if let CaptureState::Active { pressed_display, pressed_keys, .. } = &mut self.capture_state {
            pressed_keys.push(key);
            let key_strs: Vec<String> = pressed_keys.iter().map(crate::config::key_event_to_string).collect();
            *pressed_display = key_strs.join(" ");
            if let Some(global) = self.config.action_for_key(Mode::Global, key) {
                if matches!(global, Action::Enter) { self.commit_capture(); return Ok(None); }
                if matches!(global, Action::Escape) { self.capture_state = CaptureState::Inactive; return Ok(None); }
            }
            return Ok(None);
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        let instructions = self.build_instructions_from_config();
        // Clamp selection/scroll based on current entries
        let total_entries = self.entries_for_mode(self.current_mode()).len();
        if total_entries == 0 { self.selected_index = 0; self.scroll_offset = 0; }
        else if self.selected_index >= total_entries { self.selected_index = total_entries - 1; }
        let block: Block<'_> = Block::default()
            .title("Keybindings")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);

        let inner = block.inner(area);
        Clear.render(area, frame.buffer_mut());
        block.render(area, frame.buffer_mut());

        let inner_layout = split_dialog_area(inner, self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content = inner_layout.content_area;

        let [dropdown_area, list_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(4)])
            .areas(content);

        self.render_dropdown(dropdown_area, frame.buffer_mut(), true);

        let max_rows = list_area.height.saturating_sub(1) as usize;
        if total_entries > 0 {
            if self.selected_index < self.scroll_offset { self.scroll_offset = self.selected_index; }
            let bottom = self.scroll_offset.saturating_add(max_rows.saturating_sub(1));
            if self.selected_index > bottom { self.scroll_offset = self.selected_index.saturating_sub(max_rows.saturating_sub(1)); }
        }
        self.render_list(list_area, frame.buffer_mut(), max_rows);

        if self.show_instructions {
            if let Some(instr_area) = inner_layout.instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, frame.buffer_mut());
            }
        }

        // Capture overlay takes over entire content when active
        if matches!(self.capture_state, CaptureState::Active { .. }) {
            self.render_capture_overlay(content, frame.buffer_mut());
        }

        Ok(())
    }
}


