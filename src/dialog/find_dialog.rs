//! FindDialog: Popup dialog for searching text in a DataFrame (Notepad++ style)

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Wrap, BorderType};
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use crate::components::dialog_layout::split_dialog_area;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindDialogMode {
    Main,
    Error(String),
    Count(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    Normal,
    Regex,
}

#[derive(Debug, Clone)]
pub struct FindDialog {
    pub search_pattern: String,
    pub search_pattern_cursor: usize, // New: cursor index for search pattern
    pub options: FindOptions,
    pub search_mode: SearchMode,
    pub active_field: FindDialogField,
    pub mode: FindDialogMode,
    pub show_instructions: bool,
    pub searching: bool,
    pub search_progress: f64,
    pub action_selected: FindActionSelected, // New field
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindOptions {
    pub backward: bool,
    pub whole_word: bool,
    pub match_case: bool,
    pub wrap_around: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindDialogField {
    Pattern,
    Backward,
    WholeWord,
    MatchCase,
    WrapAround,
    SearchMode,
    ActionsRow, // New: represents the row of actions
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindActionSelected {
    FindNext,
    Count,
    FindAll,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            backward: false,
            whole_word: false,
            match_case: false,
            wrap_around: true,
        }
    }
}

impl Default for FindDialog {
    fn default() -> Self {
        Self {
            search_pattern: String::new(),
            search_pattern_cursor: 0,
            options: FindOptions::default(),
            search_mode: SearchMode::Normal,
            active_field: FindDialogField::Pattern,
            mode: FindDialogMode::Main,
            show_instructions: true,
            searching: false,
            search_progress: 0.0,
            action_selected: FindActionSelected::FindNext, // New field
        }
    }
}

impl FindDialog {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_field(&self) -> FindDialogField {
        use FindDialogField::*;
        match self.active_field {
            Pattern => Backward,
            Backward => WholeWord,
            WholeWord => MatchCase,
            MatchCase => WrapAround,
            WrapAround => SearchMode,
            SearchMode => ActionsRow,
            ActionsRow => Pattern,
        }
    }
    fn prev_field(&self) -> FindDialogField {
        use FindDialogField::*;
        match self.active_field {
            Pattern => ActionsRow,
            Backward => Pattern,
            WholeWord => Backward,
            MatchCase => WholeWord,
            WrapAround => MatchCase,
            SearchMode => WrapAround,
            ActionsRow => SearchMode,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        use ratatui::widgets::Gauge;
        use ratatui::widgets::Paragraph;
        use ratatui::style::{Style, Color};
        Clear.render(area, buf);
        // Outer container with double border
        let outer_block = Block::default()
            .title("Find")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);
        if self.searching {
            let gauge = Gauge::default()
                .block(Block::default().title("Searching...").borders(Borders::ALL))
                .ratio(self.search_progress)
                .label("Searching...");
            gauge.render(inner_area, buf);
            return 1;
        }
        let instructions = "Enter search pattern. Tab: Next field  Space: Toggle  Enter: Action  Esc: Close";
        let layout = split_dialog_area(inner_area, self.show_instructions, Some(instructions));
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let block = Block::default()
            .borders(Borders::ALL);
        block.render(content_area, buf);
        let start_x = content_area.x + 2;
        let mut y = content_area.y + 1;
        // Search Pattern input
        let pattern_label = "Search Pattern:";
        if self.active_field == FindDialogField::Pattern {
            // Show cursor in the pattern with proper highlighting
            buf.set_string(start_x, y, pattern_label, Style::default().add_modifier(Modifier::BOLD));
            buf.set_string(start_x + 18, y, "> ", Style::default().fg(Color::Black).bg(Color::Cyan));
            
            // Draw the search pattern character by character, highlighting the cursor
            let mut x_pos = start_x + 20; // Start after "> "
            for (i, c) in self.search_pattern.chars().enumerate() {
                let style = if i == self.search_pattern_cursor {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                };
                buf.set_string(x_pos, y, &c.to_string(), style);
                x_pos += 1;
            }
            
            // If cursor is at the end, show a highlighted space
            if self.search_pattern_cursor == self.search_pattern.len() {
                buf.set_string(x_pos, y, " ", Style::default().fg(Color::Black).bg(Color::Yellow));
            }
        } else {
            buf.set_string(start_x, y, pattern_label, Style::default().add_modifier(Modifier::BOLD));
            buf.set_string(start_x + 18, y, &self.search_pattern, Style::default());
        }
        y += 1;
        // Options (checkboxes)
        let options = [
            ("Backward direction", FindDialogField::Backward, self.options.backward),
            ("Match whole word only", FindDialogField::WholeWord, self.options.whole_word),
            ("Match case", FindDialogField::MatchCase, self.options.match_case),
            ("Wrap around", FindDialogField::WrapAround, self.options.wrap_around),
        ];
        for (label, field, checked) in options.iter() {
            let check = if *checked { "[✓]" } else { "[ ]" };
            let style = if self.active_field == *field { Style::default().fg(Color::Black).bg(Color::Cyan) } else { Style::default() };
            buf.set_string(start_x, y, check, style);
            buf.set_string(start_x + 4, y, label, style);
            y += 1;
        }
        y += 1;
        // Search Mode (radio)
        let normal_style = if self.active_field == FindDialogField::SearchMode && self.search_mode == SearchMode::Normal {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else if self.search_mode == SearchMode::Normal {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };
        let regex_style = if self.active_field == FindDialogField::SearchMode && self.search_mode == SearchMode::Regex {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else if self.search_mode == SearchMode::Regex {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };
        buf.set_string(start_x, y, "Search Mode:", Style::default().add_modifier(Modifier::BOLD));
        buf.set_string(start_x + 14, y, if self.search_mode == SearchMode::Normal { "(o) Normal" } else { "( ) Normal" }, normal_style);
        buf.set_string(start_x + 28, y, if self.search_mode == SearchMode::Regex { "(o) Regular Expression" } else { "( ) Regular Expression" }, regex_style);
        y += 2;
        // Actions (buttons)
        let actions = [
            ("Find Next", FindActionSelected::FindNext),
            ("Count", FindActionSelected::Count),
            ("Find All", FindActionSelected::FindAll),
        ];
        let mut x = start_x;
        for (_i, (label, action)) in actions.iter().enumerate() {
            let style = if self.active_field == FindDialogField::ActionsRow && self.action_selected == *action {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if self.action_selected == *action {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            buf.set_string(x, y, *label, style);
            x += label.len() as u16 + 4;
        }
        // Overlay error block if in error mode
        if let FindDialogMode::Error(ref msg) = self.mode {
            let block_width = inner_area.width.saturating_sub(10).min(40).max(20);
            let block_height = 5;
            let block_x = inner_area.x + (inner_area.width.saturating_sub(block_width)) / 2;
            let block_y = inner_area.y + (inner_area.height.saturating_sub(block_height)) / 2;
            let error_area = ratatui::prelude::Rect {
                x: block_x,
                y: block_y,
                width: block_width,
                height: block_height,
            };
            // Fill the error area with black background (fully opaque, including border)
            for y in error_area.y..error_area.y + error_area.height {
                let line = " ".repeat(error_area.width as usize);
                buf.set_string(error_area.x, y, &line, Style::default().bg(Color::Black));
            }
            let error_block = Block::default()
                .title("Error")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .style(Style::default().bg(Color::Black));
            error_block.render(error_area, buf);
            let msg_style = Style::default().fg(Color::Red).bg(Color::Black);
            let msg_y = error_area.y + error_area.height / 2;
            let msg_x = error_area.x + 2;
            buf.set_string(msg_x, msg_y, msg, msg_style);
            // Optionally, add instructions to dismiss
            let hint = "Press Enter to dismiss";
            let hint_style = Style::default().fg(Color::DarkGray).bg(Color::Black);
            buf.set_string(msg_x, msg_y + 1, hint, hint_style);
        }
        // Overlay count block if in count mode
        if let FindDialogMode::Count(ref msg) = self.mode {
            let block_width = inner_area.width.saturating_sub(10).min(40).max(20);
            let block_height = 5;
            let block_x = inner_area.x + (inner_area.width.saturating_sub(block_width)) / 2;
            let block_y = inner_area.y + (inner_area.height.saturating_sub(block_height)) / 2;
            let count_area = ratatui::prelude::Rect {
                x: block_x,
                y: block_y,
                width: block_width,
                height: block_height,
            };
            // Fill the count area with white background (fully opaque, including border)
            for y in count_area.y..count_area.y + count_area.height {
                let line = " ".repeat(count_area.width as usize);
                buf.set_string(count_area.x, y, &line, Style::default().bg(Color::White));
            }
            let count_block = Block::default()
                .title("Count")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Black))
                .style(Style::default().bg(Color::White));
            count_block.render(count_area, buf);
            let msg_style = Style::default().fg(Color::Black).bg(Color::White);
            let msg_y = count_area.y + count_area.height / 2;
            let msg_x = count_area.x + 2;
            buf.set_string(msg_x, msg_y, msg, msg_style);
            // Optionally, add instructions to dismiss
            let hint = "Press Enter to dismiss";
            let hint_style = Style::default().fg(Color::DarkGray).bg(Color::White);
            buf.set_string(msg_x, msg_y + 1, hint, hint_style);
        }
        // Instructions area
        if self.show_instructions {
            if let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
            }
        }
        1
    }

    /// Call this on each tick to animate the search gauge
    pub fn tick_search_progress(&mut self) {
        if self.searching {
            self.search_progress += 0.05;
            if self.search_progress >= 1.0 {
                self.search_progress = 0.0;
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        use SearchMode::*;
        use crossterm::event::{KeyCode, KeyModifiers};
        if let FindDialogMode::Error(_) = self.mode {
            // Only allow Esc or Enter to clear error
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.mode = FindDialogMode::Main;
                    }
                    _ => {}
                }
            }
            return None;
        }
        if let FindDialogMode::Count(_) = self.mode {
            // Only allow Esc or Enter to clear count result
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.mode = FindDialogMode::Main;
                    }
                    _ => {}
                }
            }
            return None;
        }
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Down => {
                    self.active_field = self.next_field();
                }
                KeyCode::Up => {
                    self.active_field = self.prev_field();
                }
                KeyCode::Left => {
                    if self.active_field == FindDialogField::Pattern {
                        if self.search_pattern_cursor > 0 {
                            self.search_pattern_cursor -= 1;
                        }
                    } else if self.active_field == FindDialogField::ActionsRow {
                        use FindActionSelected::*;
                        self.action_selected = match self.action_selected {
                            FindNext => FindAll,
                            Count => FindNext,
                            FindAll => Count,
                        };
                    } else {
                        use FindDialogField::*;
                        match self.active_field {
                            Backward => self.options.backward = !self.options.backward,
                            WholeWord => self.options.whole_word = !self.options.whole_word,
                            MatchCase => self.options.match_case = !self.options.match_case,
                            WrapAround => self.options.wrap_around = !self.options.wrap_around,
                            SearchMode => {
                                self.search_mode = match self.search_mode {
                                    Normal => Regex,
                                    Regex => Normal,
                                }
                            }
                            _ => {}
                        }
                    }
                }
                KeyCode::Right => {
                    if self.active_field == FindDialogField::Pattern {
                        if self.search_pattern_cursor < self.search_pattern.len() {
                            self.search_pattern_cursor += 1;
                        }
                    } else if self.active_field == FindDialogField::ActionsRow {
                        use FindActionSelected::*;
                        self.action_selected = match self.action_selected {
                            FindNext => Count,
                            Count => FindAll,
                            FindAll => FindNext,
                        };
                    } else {
                        use FindDialogField::*;
                        match self.active_field {
                            Backward => self.options.backward = !self.options.backward,
                            WholeWord => self.options.whole_word = !self.options.whole_word,
                            MatchCase => self.options.match_case = !self.options.match_case,
                            WrapAround => self.options.wrap_around = !self.options.wrap_around,
                            SearchMode => {
                                self.search_mode = match self.search_mode {
                                    Normal => Regex,
                                    Regex => Normal,
                                }
                            }
                            _ => {}
                        }
                    }
                }
                KeyCode::Esc => {
                    return Some(Action::DialogClose);
                }
                KeyCode::Char(' ') => {
                    use FindDialogField::*;
                    match self.active_field {
                        Backward => self.options.backward = !self.options.backward,
                        WholeWord => self.options.whole_word = !self.options.whole_word,
                        MatchCase => self.options.match_case = !self.options.match_case,
                        WrapAround => self.options.wrap_around = !self.options.wrap_around,
                        SearchMode => {
                            self.search_mode = match self.search_mode {
                                Normal => Regex,
                                Regex => Normal,
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Enter => {
                    if self.active_field == FindDialogField::ActionsRow || self.active_field == FindDialogField::Pattern {
                        match self.action_selected {
                            FindActionSelected::FindNext => {
                                return Some(Action::FindNext {
                                    pattern: self.search_pattern.clone(),
                                    options: self.options.clone(),
                                    search_mode: self.search_mode.clone(),
                                });
                            }
                            FindActionSelected::Count => {
                                return Some(Action::FindCount {
                                    pattern: self.search_pattern.clone(),
                                    options: self.options.clone(),
                                    search_mode: self.search_mode.clone(),
                                });
                            }
                            FindActionSelected::FindAll => {
                                return Some(Action::FindAll {
                                    pattern: self.search_pattern.clone(),
                                    options: self.options.clone(),
                                    search_mode: self.search_mode.clone(),
                                });
                            }
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if self.active_field == FindDialogField::Pattern && !key.modifiers.contains(KeyModifiers::CONTROL) {
                        let cursor = self.search_pattern_cursor.min(self.search_pattern.len());
                        self.search_pattern.insert(cursor, c);
                        self.search_pattern_cursor = cursor + 1;
                    }
                    if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        self.show_instructions = !self.show_instructions;
                    }
                }
                KeyCode::Backspace => {
                    if self.active_field == FindDialogField::Pattern {
                        if self.search_pattern_cursor > 0 && !self.search_pattern.is_empty() {
                            let cursor = self.search_pattern_cursor;
                            let mut chars: Vec<char> = self.search_pattern.chars().collect();
                            chars.remove(cursor - 1);
                            self.search_pattern = chars.into_iter().collect();
                            self.search_pattern_cursor -= 1;
                        }
                    }
                }
                KeyCode::Delete => {
                    if self.active_field == FindDialogField::Pattern {
                        let cursor = self.search_pattern_cursor;
                        if cursor < self.search_pattern.len() && !self.search_pattern.is_empty() {
                            let mut chars: Vec<char> = self.search_pattern.chars().collect();
                            chars.remove(cursor);
                            self.search_pattern = chars.into_iter().collect();
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
}

impl Component for FindDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> {
        Ok(())
    }
    fn init(&mut self, _area: Size) -> Result<()> {
        Ok(())
    }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        Ok(None)
    }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> {
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