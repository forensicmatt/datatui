//! Map Viewer Dialog Component
//!
//! Interactive viewer for Map/Dictionary data structures with:
//! - Collapsible key-value pairs
//! - Selection and navigation
//! - Copy functionality
//! - Scrolling support

use crate::tui::{Action, Component, Focusable, Theme};
use arboard::Clipboard;
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};
use std::collections::HashMap;

/// Represents a key-value pair with collapse state
#[derive(Debug, Clone)]
struct MapEntry {
    key: String,
    value: String,
    collapsed: bool,
    data_type: DataType,
}

/// Data type indicator for values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataType {
    String,
    Number,
    Boolean,
    Null,
    Object,
    Array,
}

impl DataType {
    fn from_value(value: &str) -> Self {
        // Simple heuristic detection
        if value == "null" || value == "NULL" {
            DataType::Null
        } else if value == "true" || value == "false" {
            DataType::Boolean
        } else if value.parse::<f64>().is_ok() {
            DataType::Number
        } else if (value.starts_with('{') && value.ends_with('}'))
            || (value.starts_with('[') && value.ends_with(']'))
        {
            if value.starts_with('{') {
                DataType::Object
            } else {
                DataType::Array
            }
        } else {
            DataType::String
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            DataType::String => "\"\"",
            DataType::Number => "123",
            DataType::Boolean => "T/F",
            DataType::Null => "∅",
            DataType::Object => "{}",
            DataType::Array => "[]",
        }
    }

    fn color(&self, theme: &Theme) -> Color {
        match self {
            DataType::String => theme.success,
            DataType::Number => theme.border_focused,
            DataType::Boolean => Color::Magenta,
            DataType::Null => Color::DarkGray,
            DataType::Object => theme.warning,
            DataType::Array => theme.info,
        }
    }
}

/// Copy mode for the selected entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyMode {
    Key,
    Value,
    Both,
}

/// Map Viewer Dialog
pub struct MapViewerDialog {
    /// Title for the dialog
    title: String,

    /// Map entries (ordered)
    entries: Vec<MapEntry>,

    /// Currently selected entry index
    selected_idx: usize,

    /// Scroll offset for visible entries
    scroll_offset: usize,

    /// Whether the component has focus
    focused: bool,

    /// Search filter
    search_filter: String,

    /// Whether search mode is active
    search_active: bool,

    /// Copy feedback message
    copy_message: Option<String>,

    /// Message display counter
    message_timer: u8,

    /// Whether to show instructions
    show_instructions: bool,
}

impl MapViewerDialog {
    /// Create a new map viewer from a HashMap
    pub fn new(title: String, map: HashMap<String, String>) -> Self {
        let mut entries: Vec<MapEntry> = map
            .into_iter()
            .map(|(key, value)| {
                let data_type = DataType::from_value(&value);
                MapEntry {
                    key,
                    value,
                    collapsed: false, // Start expanded
                    data_type,
                }
            })
            .collect();

        // Sort by key
        entries.sort_by(|a, b| a.key.cmp(&b.key));

        Self {
            title,
            entries,
            selected_idx: 0,
            scroll_offset: 0,
            focused: true,
            search_filter: String::new(),
            search_active: false,
            copy_message: None,
            message_timer: 0,
            show_instructions: false,
        }
    }

    /// Create from a vector of key-value tuples
    pub fn from_pairs(title: String, pairs: Vec<(String, String)>) -> Self {
        let mut entries: Vec<MapEntry> = pairs
            .into_iter()
            .map(|(key, value)| {
                let data_type = DataType::from_value(&value);
                MapEntry {
                    key,
                    value,
                    collapsed: false,
                    data_type,
                }
            })
            .collect();

        // Sort by key
        // entries.sort_by(|a, b| a.key.cmp(&b.key));

        Self {
            title,
            entries,
            selected_idx: 0,
            scroll_offset: 0,
            focused: true,
            search_filter: String::new(),
            search_active: false,
            copy_message: None,
            message_timer: 0,
            show_instructions: false,
        }
    }

    /// Toggle instructions visibility
    fn toggle_instructions(&mut self) {
        self.show_instructions = !self.show_instructions;
    }

    /// Toggle collapse state of selected entry
    fn toggle_collapse(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected_idx) {
            entry.collapsed = !entry.collapsed;
        }
    }

    /// Collapse all entries
    fn collapse_all(&mut self) {
        for entry in &mut self.entries {
            entry.collapsed = true;
        }
    }

    /// Expand all entries
    fn expand_all(&mut self) {
        for entry in &mut self.entries {
            entry.collapsed = false;
        }
    }

    /// Copy selected entry
    fn copy_selected(&mut self, mode: CopyMode) {
        if let Some(entry) = self.entries.get(self.selected_idx) {
            let content = match mode {
                CopyMode::Key => entry.key.clone(),
                CopyMode::Value => entry.value.clone(),
                CopyMode::Both => format!("{}: {}", entry.key, entry.value),
            };

            // Copy to clipboard
            match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&content)) {
                Ok(_) => {
                    self.copy_message = Some(format!(
                        "Copied {} to clipboard",
                        match mode {
                            CopyMode::Key => "key",
                            CopyMode::Value => "value",
                            CopyMode::Both => "key:value",
                        }
                    ));
                }
                Err(e) => {
                    self.copy_message = Some(format!("Failed to copy to clipboard: {}", e));
                }
            }
            self.message_timer = 30; // Show for ~30 frames
        }
    }

    /// Copy all entries as JSON
    fn copy_all(&mut self) {
        // Create a JSON object from entries
        let map: serde_json::Map<String, serde_json::Value> = self
            .entries
            .iter()
            .map(|e| (e.key.clone(), serde_json::Value::String(e.value.clone())))
            .collect();

        let json_value = serde_json::Value::Object(map);

        // Serialize to pretty-printed JSON
        let content_str = match serde_json::to_string_pretty(&json_value) {
            Ok(json) => json,
            Err(e) => {
                self.copy_message = Some(format!("Failed to serialize JSON: {}", e));
                self.message_timer = 30;
                return;
            }
        };

        // Copy to clipboard
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&content_str)) {
            Ok(_) => {
                self.copy_message = Some(format!(
                    "Copied {} entries as JSON to clipboard",
                    self.entries.len()
                ));
            }
            Err(e) => {
                self.copy_message = Some(format!("Failed to copy to clipboard: {}", e));
            }
        }
        self.message_timer = 30;
    }

    /// Calculate the height needed for an entry
    fn entry_height(&self, entry: &MapEntry, available_width: u16) -> u16 {
        if entry.collapsed {
            2 // Key line + collapsed indicator
        } else {
            // Calculate wrapped lines for value
            let value_width = available_width.saturating_sub(4) as usize; // Account for borders and padding
            let value_lines = if entry.value.is_empty() {
                1
            } else {
                ((entry.value.len() + value_width - 1) / value_width).max(1)
            };
            2 + value_lines as u16 // Key + value lines
        }
    }

    /// Render the dialog
    fn render_dialog(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // let theme = Theme::default(); // Removed

        // Decrement message timer
        if self.message_timer > 0 {
            self.message_timer -= 1;
            if self.message_timer == 0 {
                self.copy_message = None;
            }
        }

        // Clear background
        frame.render_widget(Clear, area);

        // Main block
        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let title = if self.search_active {
            format!(" {} [Filter: {}] ", self.title, self.search_filter)
        } else {
            format!(" {} ({} entries) ", self.title, self.entries.len())
        };

        let block_border = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Double)
            .border_style(border_style)
            .style(theme.normal_style());

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.normal_style());

        // Render outer double border first
        let outer_inner = block_border.inner(area);
        frame.render_widget(block_border, area);

        // Render inner block inside the outer border
        let inner = block.inner(outer_inner);
        frame.render_widget(block, outer_inner);

        if self.entries.is_empty() {
            let empty_msg = Paragraph::new("No entries to display")
                .style(theme.normal_style().fg(Color::DarkGray))
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_msg, inner);
            return;
        }

        // Split area for content and optional instructions
        let chunks = if self.show_instructions {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(7)])
                .split(inner)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(0)])
                .split(inner)
        };

        let content_area = chunks[0];

        // Calculate visible entries
        let viewport_height = content_area.height as usize;
        let mut visible_entries = Vec::new();
        let mut current_height = 0;
        let mut start_idx = self.scroll_offset;

        // Adjust scroll to keep selection visible
        if self.selected_idx < self.scroll_offset {
            self.scroll_offset = self.selected_idx;
        }

        // Build visible list
        for (idx, entry) in self.entries.iter().enumerate().skip(self.scroll_offset) {
            let entry_h = self.entry_height(entry, content_area.width);
            if current_height + entry_h as usize <= viewport_height {
                visible_entries.push((idx, entry, entry_h));
                current_height += entry_h as usize;
            } else if idx == self.selected_idx {
                // Ensure selected is visible
                self.scroll_offset = idx;
                break;
            } else {
                break;
            }
        }

        // Render entries
        let mut y_offset = 0;
        for (idx, entry, entry_h) in visible_entries {
            let entry_area = Rect {
                x: content_area.x,
                y: content_area.y + y_offset,
                width: content_area.width,
                height: entry_h,
            };

            self.render_entry(frame, entry_area, entry, idx == self.selected_idx, &theme);
            y_offset += entry_h;
        }

        // Render scrollbar
        if self.entries.len() > 5 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(self.entries.len()).position(self.selected_idx);

            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }

        // Render instructions if enabled
        if self.show_instructions {
            let instructions_text = vec![
                "• ↑/↓: Navigate entries",
                "• Space/+/-: Collapse/Expand entry",
                "• Shift+Space: Collapse/Expand all",
                "• Enter: Copy value to clipboard",
                "• c: Copy key to clipboard",
                "• a: Copy all as JSON",
                "• Esc: Close dialog",
            ];

            let instructions = Paragraph::new(instructions_text.join("\n"))
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .title("Instructions (Ctrl+i to hide)"),
                )
                .style(theme.warning_style())
                .wrap(Wrap { trim: true });

            frame.render_widget(instructions, chunks[1]);
        } else if let Some(ref msg) = self.copy_message {
            // Show copy message at bottom if instructions are hidden
            let msg_para = Paragraph::new(format!(" {} ", msg))
                .style(theme.success_style())
                .block(Block::default().borders(Borders::TOP));
            frame.render_widget(
                msg_para,
                Rect {
                    x: inner.x,
                    y: inner.y + inner.height.saturating_sub(1),
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }

    /// Render a single entry
    fn render_entry(
        &self,
        frame: &mut Frame,
        area: Rect,
        entry: &MapEntry,
        is_selected: bool,
        theme: &Theme,
    ) {
        let bg_style = if is_selected {
            theme.alt_row_style()
        } else {
            Style::default()
        };

        // Split into key and value areas
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(if entry.collapsed {
                vec![Constraint::Length(1), Constraint::Length(1)]
            } else {
                vec![Constraint::Length(1), Constraint::Min(1)]
            })
            .split(area);

        // Render key line with type indicator and collapse icon
        let collapse_icon = if entry.collapsed { "+" } else { "-" };
        let type_icon = entry.data_type.icon();
        let type_color = entry.data_type.color(theme);

        let key_spans = vec![
            Span::styled(
                format!(" {} ", collapse_icon),
                theme.warning_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("[{}] ", type_icon), Style::default().fg(type_color)),
            Span::styled(
                &entry.key,
                theme.focused_border_style().add_modifier(Modifier::BOLD),
            ),
        ];

        let key_line = Line::from(key_spans);
        let key_para = Paragraph::new(key_line).style(bg_style);
        frame.render_widget(key_para, chunks[0]);

        // Render value
        if entry.collapsed {
            let collapsed_text = Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    "(collapsed)",
                    theme
                        .normal_style()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]);
            let value_para = Paragraph::new(collapsed_text).style(bg_style);
            frame.render_widget(value_para, chunks[1]);
        } else {
            let value_text = format!("   {}", entry.value);
            let value_para = Paragraph::new(value_text)
                .style(bg_style.fg(theme.foreground))
                .wrap(Wrap { trim: false });
            frame.render_widget(value_para, chunks[1]);
        }
    }
}

impl Component for MapViewerDialog {
    fn name(&self) -> &str {
        "MapViewerDialog"
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::MoveUp => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                }
                Ok(true)
            }
            Action::MoveDown => {
                if self.selected_idx < self.entries.len().saturating_sub(1) {
                    self.selected_idx += 1;
                }
                Ok(true)
            }
            Action::PageUp => {
                self.selected_idx = self.selected_idx.saturating_sub(10);
                Ok(true)
            }
            Action::PageDown => {
                let max_idx = self.entries.len().saturating_sub(1);
                self.selected_idx = (self.selected_idx + 10).min(max_idx);
                Ok(true)
            }
            Action::Home => {
                self.selected_idx = 0;
                Ok(true)
            }
            Action::End => {
                self.selected_idx = self.entries.len().saturating_sub(1);
                Ok(true)
            }
            Action::Confirm => {
                // Enter = Copy value
                self.copy_selected(CopyMode::Value);
                Ok(true)
            }
            Action::Copy => {
                // Ctrl+C = Copy key
                self.copy_selected(CopyMode::Key);
                Ok(true)
            }
            Action::CopyWithHeaders => {
                // Shift+C = Copy both
                self.copy_selected(CopyMode::Both);
                Ok(true)
            }
            Action::CopyAll => {
                // a = Copy all
                self.copy_all();
                Ok(true)
            }
            Action::ToggleCollapseAll => {
                let any_expanded = self.entries.iter().any(|e| !e.collapsed);
                if any_expanded {
                    self.collapse_all();
                } else {
                    self.expand_all();
                }
                Ok(true)
            }
            Action::ToggleVisibility => {
                // Space = Toggle collapse
                self.toggle_collapse();
                Ok(true)
            }
            Action::ToggleInstructions => {
                self.toggle_instructions();
                Ok(true)
            }
            Action::Cancel => Ok(false), // Close dialog
            _ => Ok(true),               // Consume other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.render_dialog(frame, area, theme);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::MoveUp,
            Action::MoveDown,
            Action::PageUp,
            Action::PageDown,
            Action::Home,
            Action::End,
            Action::Confirm,
            Action::Copy,
            Action::CopyWithHeaders,
            Action::CopyAll,
            Action::ToggleVisibility,
            Action::ToggleCollapseAll,
            Action::ToggleInstructions,
            Action::Cancel,
        ]
    }
}

impl Focusable for MapViewerDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
