//! SortDialog: Popup dialog for configuring multi-column sorting of a DataFrame

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, BorderType, Scrollbar, ScrollbarState, ScrollbarOrientation};
use serde::{Deserialize, Serialize};
use strum::Display as SDisplay;
use std::fmt::Display;
use crate::action::Action;

use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;


/// Represents a single sort column with direction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortColumn {
    pub name: String,
    pub ascending: bool,
}
impl Display for SortColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, if self.ascending { "asc" } else { "desc" })
    }
}

/// Dialog mode: main list or add column
#[derive(Debug, Clone, PartialEq, Eq, SDisplay, Serialize, Deserialize)]
pub enum SortDialogMode {
    List,
    AddColumn,
}

/// SortDialog: UI for configuring sort columns and order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortDialog {
    pub columns: Vec<String>,
    pub sort_columns: Vec<SortColumn>,
    pub active_index: usize,
    pub mode: SortDialogMode,
    // AddColumn mode state
    pub add_column_index: usize,
    // Scrolling state
    pub scroll_offset: usize,
    pub add_column_scroll_offset: usize,
    pub current_column: Option<String>,
    pub show_instructions: bool, // new: show instructions area (default true)
    #[serde(skip)]
    pub config: Config,
}

impl SortDialog {
    /// Create a new SortDialog
    pub fn new(columns: Vec<String>) -> Self {
        let sort_columns = Vec::new();
        Self {
            columns,
            sort_columns,
            active_index: 0,
            mode: SortDialogMode::List,
            add_column_index: 0,
            scroll_offset: 0,
            add_column_scroll_offset: 0,
            current_column: None,
            show_instructions: true,
            config: Config::default(),
        }
    }

    /// Set whether to show the instructions area
    pub fn set_show_instructions(&mut self, show: bool) {
        self.show_instructions = show;
    }

    /// Set the columns for the dialog and the current DataTable column
    pub fn set_columns(&mut self, columns: Vec<String>, current_index: usize) {
        self.columns = columns;
        self.current_column = self.columns.get(current_index).cloned();
    }

    /// Build instructions string from configured keybindings for Sort mode
    fn build_instructions_from_config(&self) -> String {
        match self.mode {
            SortDialogMode::List => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::Global, crate::action::Action::Up),
                    (crate::config::Mode::Global, crate::action::Action::Down),
                    (crate::config::Mode::Global, crate::action::Action::Enter),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                    (crate::config::Mode::Sort, crate::action::Action::ToggleSortDirection),
                    (crate::config::Mode::Sort, crate::action::Action::RemoveSortColumn),
                    (crate::config::Mode::Sort, crate::action::Action::AddSortColumn),
                ])
            }
            SortDialogMode::AddColumn => {
                // Special handling for AddColumn mode - use hardcoded strings for now
                "Up/Down: Select  Enter: Add  Esc: Cancel".to_string()
            }
        }
    }

    /// Get columns available to add (not already in sort_columns)
    fn available_columns(&self) -> Vec<&String> {
        self.columns.iter().filter(|c| !self.sort_columns.iter().any(|sc| &sc.name == *c)).collect()
    }

    /// Render the dialog (UI skeleton)
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        // Clear the background for the popup
        Clear.render(area, buf);
        // Outer container with double border and title "Sort"
        let outer_block = Block::default()
            .title("Sort")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let outer_inner_area = outer_block.inner(area);
        outer_block.render(area, buf);
        // Build dynamic instructions from config
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(outer_inner_area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        // Draw dialog frame
        let block = Block::default()
            .title("Columns")
            .borders(Borders::ALL);
        block.render(content_area, buf);
        let inner = content_area.inner(Margin { vertical: 1, horizontal: 2 });
        let max_rows = std::cmp::max(1, inner.height as usize);
        let mut content_len: usize = 0;
        let mut scroll_pos: usize = 0;
        match self.mode {
            SortDialogMode::List => {
                let list_start_y = inner.y;
                if self.sort_columns.is_empty() {
                    buf.set_string(
                        inner.x,
                        list_start_y, 
                        "No sort columns selected.", 
                        Style::default()
                            .fg(Color::DarkGray)
                    );
                } else {
                    let end = (self.scroll_offset + max_rows).min(self.sort_columns.len());
                    for (vis_idx, i) in (self.scroll_offset..end).enumerate() {
                        let y = list_start_y + vis_idx as u16;
                        let col = &self.sort_columns[i];
                        let selected = i == self.active_index;
                        let zebra = i % 2 == 0;
                        let dir = if col.ascending { "↑" } else { "↓" };
                        let text = if selected {
                            format!("> {}  {}", col.name, dir)
                        } else {
                            format!("  {}  {}", col.name, dir)
                        };
                        let mut style = Style::default();
                        if selected {
                            style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
                        } else if zebra {
                            style = style.bg(Color::Rgb(30,30,30));
                        }
                        buf.set_string(inner.x, y, text, style);
                    }
                    content_len = self.sort_columns.len();
                    scroll_pos = self.scroll_offset;
                }
            }
            SortDialogMode::AddColumn => {
                let available = self.available_columns();
                let list_start_y = inner.y;
                if available.is_empty() {
                    buf.set_string(
                        inner.x,
                        list_start_y, 
                        "No columns available to add.", 
                        Style::default()
                            .fg(Color::DarkGray)
                    );
                } else {
                    let end = (self.add_column_scroll_offset + max_rows).min(available.len());
                    for (vis_idx, i) in (self.add_column_scroll_offset..end).enumerate() {
                        let y: u16 = list_start_y + vis_idx as u16;
                        let col = available[i];
                        let selected = i == self.add_column_index;
                        let zebra = i % 2 == 0;
                        let mut style = Style::default();
                        if selected {
                            style = style.fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD);
                        } else if zebra {
                            style = style.bg(Color::Rgb(30,30,30));
                        }
                        buf.set_string(
                            inner.x,
                            y,
                            col.as_str(),
                            style
                        );
                    }
                    content_len = available.len();
                    scroll_pos = self.add_column_scroll_offset;
                }
            }
        }
        // Render vertical scrollbar on the right of the content area when needed
        if content_len > max_rows {
            let viewport = max_rows;
            let position_for_bar = if scroll_pos == 0 { 0 } else {
                scroll_pos
                    .saturating_add(viewport.saturating_sub(1))
                    .min(content_len.saturating_sub(1))
            };
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Cyan));
            let mut state = ScrollbarState::new(content_len)
                .position(position_for_bar)
                .viewport_content_length(viewport);
            scrollbar.render(inner, buf, &mut state);
        }
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                use ratatui::widgets::{Paragraph, Wrap};
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
            }
        max_rows
    }

    /// Handle a key event. Returns Some(Action) if the dialog should close and apply, None otherwise.
    pub fn handle_key_event(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        if key.kind == KeyEventKind::Press {
            // Handle Ctrl+I to toggle instructions (works in both modes)
            if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.show_instructions = !self.show_instructions;
                return None;
            }

            // First, honor config-driven actions (Global + Sort)
            if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                match global_action {
                    Action::Escape => {
                        return Some(Action::DialogClose);
                    }
                    Action::Enter => {
                        match self.mode {
                            SortDialogMode::List => {
                                return Some(Action::SortDialogApplied(self.sort_columns.clone()));
                            }
                            SortDialogMode::AddColumn => {
                                let available = self.available_columns();
                                if !available.is_empty() {
                                    let col_name = available[self.add_column_index].clone();
                                    self.sort_columns.push(SortColumn { name: col_name.clone(), ascending: true });
                                    self.mode = SortDialogMode::List;
                                    self.active_index = self.sort_columns.len() - 1;
                                    // Adjust scroll for main list
                                    if self.active_index < self.scroll_offset {
                                        self.scroll_offset = self.active_index;
                                    } else if self.active_index >= self.scroll_offset + max_rows {
                                        self.scroll_offset = self.active_index + 1 - max_rows;
                                    }
                                    // Clamp active_index and scroll_offset
                                    if self.active_index >= self.sort_columns.len() {
                                        self.active_index = self.sort_columns.len().saturating_sub(1);
                                    }
                                    if self.scroll_offset > self.active_index {
                                        self.scroll_offset = self.active_index;
                                    }
                                    if self.scroll_offset + max_rows > self.sort_columns.len() {
                                        self.scroll_offset = self.sort_columns.len().saturating_sub(max_rows);
                                    }
                                    // Reset AddColumn state
                                    self.add_column_index = 0;
                                    self.add_column_scroll_offset = 0;
                                }
                                return None;
                            }
                        }
                    }
                    Action::Up => {
                        match self.mode {
                            SortDialogMode::List => {
                                if !self.sort_columns.is_empty() {
                                    if self.active_index == 0 {
                                        self.active_index = self.sort_columns.len() - 1;
                                    } else {
                                        self.active_index -= 1;
                                    }
                                    if self.active_index < self.scroll_offset {
                                        self.scroll_offset = self.active_index;
                                    } else if self.active_index >= self.scroll_offset + max_rows {
                                        self.scroll_offset = self.active_index + 1 - max_rows;
                                    }
                                }
                            }
                            SortDialogMode::AddColumn => {
                                let available = self.available_columns();
                                if !available.is_empty() {
                                    if self.add_column_index == 0 {
                                        self.add_column_index = available.len() - 1;
                                    } else {
                                        self.add_column_index -= 1;
                                    }
                                    if self.add_column_index < self.add_column_scroll_offset {
                                        self.add_column_scroll_offset = self.add_column_index;
                                    } else if self.add_column_index >= self.add_column_scroll_offset + max_rows {
                                        self.add_column_scroll_offset = self.add_column_index + 1 - max_rows;
                                    }
                                }
                            }
                        }
                        return None;
                    }
                    Action::Down => {
                        match self.mode {
                            SortDialogMode::List => {
                                if !self.sort_columns.is_empty() {
                                    self.active_index = (self.active_index + 1) % self.sort_columns.len();
                                    if self.active_index < self.scroll_offset {
                                        self.scroll_offset = self.active_index;
                                    } else if self.active_index >= self.scroll_offset + max_rows {
                                        self.scroll_offset = self.active_index + 1 - max_rows;
                                    }
                                }
                            }
                            SortDialogMode::AddColumn => {
                                let available = self.available_columns();
                                if !available.is_empty() {
                                    self.add_column_index = (self.add_column_index + 1) % available.len();
                                    if self.add_column_index < self.add_column_scroll_offset {
                                        self.add_column_scroll_offset = self.add_column_index;
                                    } else if self.add_column_index >= self.add_column_scroll_offset + max_rows {
                                        self.add_column_scroll_offset = self.add_column_index + 1 - max_rows;
                                    }
                                }
                            }
                        }
                        return None;
                    }
                    _ => { /* ignore others for now */ }
                }
            }

            // Next, check for Sort mode specific actions
            if let Some(sort_action) = self.config.action_for_key(crate::config::Mode::Sort, key) {
                match sort_action {
                    Action::ToggleSortDirection => {
                        if self.mode == SortDialogMode::List
                            && let Some(col) = self.sort_columns.get_mut(self.active_index) {
                                col.ascending = !col.ascending;
                            }
                        return None;
                    }
                    Action::RemoveSortColumn => {
                        if self.mode == SortDialogMode::List
                            && !self.sort_columns.is_empty() && self.active_index < self.sort_columns.len() {
                                self.sort_columns.remove(self.active_index);
                                if self.sort_columns.is_empty() {
                                    self.active_index = 0;
                                    self.scroll_offset = 0;
                                } else {
                                    // Clamp active_index after removal
                                    if self.active_index >= self.sort_columns.len() {
                                        self.active_index = self.sort_columns.len() - 1;
                                    }
                                    // Adjust scroll if needed
                                    if self.active_index < self.scroll_offset {
                                        self.scroll_offset = self.active_index;
                                    } else if self.active_index >= self.scroll_offset + max_rows {
                                        self.scroll_offset = self.active_index.saturating_sub(max_rows - 1);
                                    }
                                    // Clamp scroll_offset
                                    if self.scroll_offset + max_rows > self.sort_columns.len() {
                                        self.scroll_offset = self.sort_columns.len().saturating_sub(max_rows);
                                    }
                                }
                            }
                        return None;
                    }
                    Action::AddSortColumn => {
                        if self.mode == SortDialogMode::List {
                            self.mode = SortDialogMode::AddColumn;
                            let available = self.available_columns();
                            // Highlight the current DataTable column if present
                            if let Some(ref col_name) = self.current_column {
                                if let Some(idx) = available.iter().position(|c| **c == *col_name) {
                                    self.add_column_index = idx;
                                    self.add_column_scroll_offset = idx.saturating_sub(max_rows / 2);
                                } else {
                                    self.add_column_index = 0;
                                    self.add_column_scroll_offset = 0;
                                }
                            } else {
                                self.add_column_index = 0;
                                self.add_column_scroll_offset = 0;
                            }
                        }
                        return None;
                    }
                    _ => { /* ignore others for now */ }
                }
            }
        }

        None
    }
}

impl Component for SortDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: Config) -> Result<()> { self.config = _config; Ok(()) }
    fn init(&mut self, _area: Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
} 