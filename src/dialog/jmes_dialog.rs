use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Wrap, Tabs, Table, Row, Cell};
use tui_textarea::TextArea;
use serde::{Deserialize, Serialize};
use crate::action::Action;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;
use crate::dialog::TransformScope;
use crate::dialog::error_dialog::{ErrorDialog, render_error_dialog};
use crate::style::StyleConfig;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusArea {
    Scope,
    Tabs,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JmesPathKeyValuePair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddPairFocus {
    Name,
    Value,
    Button,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JmesDialogMode {
    InputTransform,
    InputAddColumns,
    Error(String),
}

#[derive(Debug)]
pub struct JmesPathDialog {
    pub mode: JmesDialogMode,
    pub scope: TransformScope,
    pub textarea: TextArea<'static>,
    pub show_instructions: bool,
    pub styles: StyleConfig,
    pub add_columns: Vec<JmesPathKeyValuePair>,
    pub selected_add_col: usize,
    focus: FocusArea,
    selected_option: usize,
    add_pair_open: bool,
    // None => creating new pair; Some(index) => editing existing pair at index
    add_pair_edit_index: Option<usize>,
    add_pair_focus: AddPairFocus,
    add_pair_name: TextArea<'static>,
    add_pair_value: TextArea<'static>,
    pub config: Config,
}

impl Default for JmesPathDialog {
    fn default() -> Self { Self::new() }
}

impl JmesPathDialog {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_line_number_style(Style::default().bg(Color::DarkGray));
        Self {
            mode: JmesDialogMode::InputTransform,
            scope: TransformScope::Current,
            textarea,
            show_instructions: true,
            styles: StyleConfig::default(),
            add_columns: Vec::new(),
            selected_add_col: 0,
            focus: FocusArea::Scope,
            selected_option: 0,
            add_pair_open: false,
            add_pair_edit_index: None,
            add_pair_focus: AddPairFocus::Name,
            add_pair_name: TextArea::default(),
            add_pair_value: TextArea::default(),
            config: Config::default(),
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.mode = JmesDialogMode::Error(msg);
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        use std::fmt::Write as _;
        fn fmt_key_event(key: &crossterm::event::KeyEvent) -> String {
            use crossterm::event::{KeyCode, KeyModifiers};
            let mut parts: Vec<&'static str> = Vec::with_capacity(3);
            if key.modifiers.contains(KeyModifiers::CONTROL) { parts.push("Ctrl"); }
            if key.modifiers.contains(KeyModifiers::ALT) { parts.push("Alt"); }
            if key.modifiers.contains(KeyModifiers::SHIFT) { parts.push("Shift"); }
            let key_part = match key.code {
                KeyCode::Char(' ') => "Space".to_string(),
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) { c.to_ascii_uppercase().to_string() } else { c.to_string() }
                }
                KeyCode::Left => "Left".to_string(),
                KeyCode::Right => "Right".to_string(),
                KeyCode::Up => "Up".to_string(),
                KeyCode::Down => "Down".to_string(),
                KeyCode::Enter => "Enter".to_string(),
                KeyCode::Esc => "Esc".to_string(),
                KeyCode::Tab => "Tab".to_string(),
                KeyCode::BackTab => "BackTab".to_string(),
                KeyCode::Delete => "Delete".to_string(),
                KeyCode::Insert => "Insert".to_string(),
                KeyCode::Home => "Home".to_string(),
                KeyCode::End => "End".to_string(),
                KeyCode::PageUp => "PageUp".to_string(),
                KeyCode::PageDown => "PageDown".to_string(),
                KeyCode::F(n) => format!("F{n}"),
                _ => "?".to_string(),
            };
            if parts.is_empty() { key_part } else { format!("{}+{}", parts.join("+"), key_part) }
        }
        
        fn fmt_sequence(seq: &[crossterm::event::KeyEvent]) -> String {
            let parts: Vec<String> = seq.iter().map(fmt_key_event).collect();
            parts.join(", ")
        }

        let mut segments: Vec<String> = Vec::new();

        match self.mode {
            JmesDialogMode::InputTransform => {
                segments.push("Enter JMESPath expression.".to_string());
                
                // Add JMESPath-specific actions
                if let Some(jmes_bindings) = self.config.keybindings.0.get(&crate::config::Mode::JmesPath) {
                    for (keys, action) in jmes_bindings.iter() {
                        if action == &Action::ApplyTransform {
                            segments.push(format!("{}:Apply", fmt_sequence(keys)));
                        }
                    }
                }
                
                // Add Global actions
                if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
                    for (keys, action) in global_bindings.iter() {
                        match action {
                            Action::Up => segments.push(format!("{}:Move Focus", fmt_sequence(keys))),
                            Action::Down => segments.push(format!("{}:Move Focus", fmt_sequence(keys))),
                            Action::Left => segments.push(format!("{}:on Tabs", fmt_sequence(keys))),
                            Action::Right => segments.push(format!("{}:on Tabs", fmt_sequence(keys))),
                            Action::Escape => segments.push(format!("{}:Close", fmt_sequence(keys))),
                            Action::ToggleInstructions => segments.push(format!("{}:Toggle Instructions", fmt_sequence(keys))),
                            _ => {}
                        }
                    }
                }
                
                segments.push("Space:Toggle Option".to_string());
            }
            JmesDialogMode::InputAddColumns => {
                segments.push("Add column entries as key/value pairs.".to_string());
                
                // Add JMESPath-specific actions
                if let Some(jmes_bindings) = self.config.keybindings.0.get(&crate::config::Mode::JmesPath) {
                    for (keys, action) in jmes_bindings.iter() {
                        match action {
                            Action::AddColumn => segments.push(format!("{}:Add", fmt_sequence(keys))),
                            Action::EditColumn => segments.push(format!("{}:Edit", fmt_sequence(keys))),
                            Action::DeleteColumn => segments.push(format!("{}:Delete", fmt_sequence(keys))),
                            Action::ApplyTransform => segments.push(format!("{}:Apply", fmt_sequence(keys))),
                            _ => {}
                        }
                    }
                }
                
                // Add Global actions
                if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
                    for (keys, action) in global_bindings.iter() {
                        match action {
                            Action::Up => segments.push(format!("{}:Select/Move Focus", fmt_sequence(keys))),
                            Action::Down => segments.push(format!("{}:Select/Move Focus", fmt_sequence(keys))),
                            Action::Left => segments.push(format!("{}:on Tabs", fmt_sequence(keys))),
                            Action::Right => segments.push(format!("{}:on Tabs", fmt_sequence(keys))),
                            Action::Escape => segments.push(format!("{}:Close", fmt_sequence(keys))),
                            Action::ToggleInstructions => segments.push(format!("{}:Toggle Instructions", fmt_sequence(keys))),
                            _ => {}
                        }
                    }
                }
                
                segments.push("Space:Toggle Option (when selected)".to_string());
            }
            JmesDialogMode::Error(_) => return String::new(),
        }

        // Join segments
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{seg}");
        }
        
        if out.is_empty() {
            match self.mode {
                JmesDialogMode::InputTransform =>
                    "Enter JMESPath expression. Ctrl+Enter:Apply  Up/Down:Select/Move Focus  Left/Right:on Tabs  Space:Toggle Option  Ctrl+i:Toggle Instructions  Esc:Close".to_string(),
                JmesDialogMode::InputAddColumns =>
                    "Add column entries as key/value pairs. Ctrl+A:Add  Ctrl+E:Edit  Ctrl+D:Delete  Up/Down:Select/Move Focus  Left/Right:on Tabs  Space:Toggle Option (when selected)  Ctrl+Enter:Apply  Ctrl+i:Toggle Instructions  Esc:Close".to_string(),
                JmesDialogMode::Error(_) => String::new(),
            }
        } else {
            out
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        // Instructions text per mode; toggled by self.show_instructions
        let instructions = self.build_instructions_from_config();

        // Outer double-bordered block wrapping the entire dialog area
        let outer_block: Block<'_> = Block::default()
            .title("JMESPath Operations")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);
        let inner_total_area = outer_block.inner(area);
        outer_block.render(area, buf);

        // Split the inner area into content and optional instructions (inside the double border)
        let inner_layout = split_dialog_area(inner_total_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = inner_layout.content_area;

        // Split content into scope selector, tabs header, and body areas
        let [scope_area, tabs_area, body_area] =
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(3)])
                .areas(content_area);

        // Render options selector (checkboxes)
        let is_current = matches!(self.scope, TransformScope::Current);
        let checkbox = if is_current { "[✓]" } else { "[ ]" };
        let scope_line = Line::from(vec![
            Span::styled("Current Data Set ", Style::default()),
            {
                let mut style = Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD);
                if self.focus == FocusArea::Scope && self.selected_option == 0 {
                    style = style.bg(Color::Gray).fg(Color::Black);
                }
                Span::styled(checkbox, style)
            },
        ]);
        let scope_block = if self.focus == FocusArea::Scope {
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title("Options")
        } else {
            Block::default().borders(Borders::ALL).title("Options")
        };
        let scope_paragraph = Paragraph::new(scope_line).block(scope_block);
        ratatui::widgets::Widget::render(&scope_paragraph, scope_area, buf);

        // Render tabs
        let titles = vec![
            Line::from(vec![Span::raw(" "), Span::styled("Transform", Style::default())]),
            Line::from(vec![Span::raw(" "), Span::styled("Add Columns", Style::default())]),
        ];
        let selected_index = match self.mode {
            JmesDialogMode::InputTransform => 0,
            JmesDialogMode::InputAddColumns => 1,
            JmesDialogMode::Error(_) => 0,
        };
        let tabs_block = if self.focus == FocusArea::Tabs {
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title("JMESPath")
        } else {
            Block::default().borders(Borders::ALL).title("JMESPath")
        };
        let tabs = Tabs::new(titles)
            .block(tabs_block)
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .select(selected_index);
        ratatui::widgets::Widget::render(&tabs, tabs_area, buf);
        // Render instructions inside the outer block when toggled on
        if self.show_instructions && let Some(instructions_area) = inner_layout.instructions_area {
            let instructions_paragraph = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }

        match &self.mode {
            JmesDialogMode::InputTransform => {
                self.textarea.set_block(
                    Block::default()
                        .title("JMESPath Transform".to_string())
                        .borders(Borders::ALL)
                );
                self.textarea.set_line_number_style(Style::default().bg(Color::DarkGray));
                ratatui::widgets::Widget::render(&self.textarea, body_area, buf);
            }
            JmesDialogMode::InputAddColumns => {
                let block = Block::default()
                    .title("JMESPath Add Columns".to_string())
                    .borders(Borders::ALL)
                    .border_style(self.styles.table_border);
                let inner = block.inner(body_area);
                block.render(body_area, buf);

                // Build table rows with dynamic height to accommodate multi-line values
                let header = Row::new(vec![
                    Cell::from(Span::styled("Name", self.styles.table_header)),
                    Cell::from(Span::styled("Value", self.styles.table_header)),
                ]).style(self.styles.table_header);
                let rows: Vec<Row> = self
                    .add_columns
                    .iter()
                    .enumerate()
                    .map(|(i, pair)| {
                        // Zebra background for readability via shared styles
                        let base_style = if i.is_multiple_of(2) { self.styles.table_row_even } else { self.styles.table_row_odd };
                        // Selected row style overrides base
                        let style = if i == self.selected_add_col { self.styles.selected_row } else { base_style };
                        let name_lines = if pair.name.is_empty() { 1 } else { pair.name.lines().count() as u16 };
                        let value_lines = if pair.value.is_empty() { 1 } else { pair.value.lines().count() as u16 };
                        let row_height = name_lines.max(value_lines).max(1);
                        Row::new(vec![
                            Cell::from(pair.name.clone()).style(self.styles.table_cell),
                            Cell::from(pair.value.clone()).style(self.styles.table_cell),
                        ])
                        .height(row_height)
                        .style(style)
                    })
                    .collect();

                let widths = [Constraint::Percentage(20), Constraint::Percentage(80)];
                let table = Table::new(rows, widths)
                    .header(header)
                    .block(Block::default().borders(Borders::NONE));
                ratatui::widgets::Widget::render(&table, inner, buf);

                // Render Add Pair dialog overlay if open
                if self.add_pair_open {
                    let overlay_w = area.width.saturating_sub(area.width / 3).max(30);
                    // Dynamic height: at least 1 line; otherwise actual number of lines
                    let value_line_count = self.add_pair_value.lines().len().max(1) as u16;
                    let name_row_h = 3u16; // fixed height for Name field
                    let value_row_h = value_line_count.saturating_add(2).max(10); // include borders for Value
                    let row3_h = 2u16; // footer with button
                    let mut row2_h_desired = name_row_h + value_row_h; // total inputs area (Name + Value)
                    let mut overlay_h_desired = row2_h_desired + row3_h; // include overlay block borders
                    
                    // Clamp to available space
                    let max_overlay_h = area.height;
                    if overlay_h_desired > max_overlay_h {
                        overlay_h_desired = max_overlay_h;
                        row2_h_desired = overlay_h_desired
                            .saturating_sub(row3_h)
                            .max(name_row_h);
                    }
                    let overlay_h = overlay_h_desired;
                    let overlay_x = area.x + (area.width.saturating_sub(overlay_w)) / 2;
                    let overlay_y = area.y + (area.height.saturating_sub(overlay_h)) / 2;
                    let overlay = Rect {
                        x: overlay_x, y: overlay_y,
                        width: overlay_w, height: overlay_h
                    };

                    Clear.render(overlay, buf);
                    let overlay_title = if self.add_pair_edit_index.is_some() { " Edit Column " } else { " Add Column " };
                    let block = Block::default()
                        .title(overlay_title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .border_style(Style::default().fg(Color::White));
                    let inner_ov = block.inner(overlay);
                    block.render(overlay, buf);

                    // Layout inside overlay: two inputs row and footer row
                    let [row2, row3] = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(row2_h_desired), Constraint::Length(row3_h)])
                        .areas(inner_ov);

                    // Split row2 into two columns for fields
                    // Within row2, make sub-rows for Name and Value, allowing Value to grow
                    let [name_row, value_row] = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(name_row_h),
                            Constraint::Length(row2_h_desired.saturating_sub(name_row_h).max(3)),
                        ])
                        .areas(row2);

                    let name_area = name_row;
                    let value_area = value_row;

                    let mut name_input = self.add_pair_name.clone();
                    let mut value_input = self.add_pair_value.clone();

                    let name_block = if self.add_pair_focus == AddPairFocus::Name {
                        Block::default().borders(Borders::ALL).title("Name")
                            .border_style(Style::default().fg(Color::Yellow))
                    } else {
                        Block::default().borders(Borders::ALL).title("Name")
                    };
                    let value_block = if self.add_pair_focus == AddPairFocus::Value {
                        Block::default().borders(Borders::ALL).title("Value")
                            .border_style(Style::default().fg(Color::Yellow))
                    } else {
                        Block::default().borders(Borders::ALL).title("Value")
                    };

                    // Only show a cursor in the focused field: render textarea when focused, paragraph otherwise
                    if self.add_pair_focus == AddPairFocus::Name {
                        name_input.set_block(name_block);
                        ratatui::widgets::Widget::render(&name_input, name_area, buf);

                        let value_text = self.add_pair_value.lines().join("\n");
                        let value_paragraph = Paragraph::new(value_text).block(value_block);
                        value_paragraph.render(value_area, buf);
                    } else {
                        let name_text = self.add_pair_name.lines().join("\n");
                        let name_paragraph = Paragraph::new(name_text).block(name_block);
                        name_paragraph.render(name_area, buf);

                        value_input.set_block(value_block);
                        ratatui::widgets::Widget::render(&value_input, value_area, buf);
                    }

                    // Bottom row: right-aligned [ Add ] button (focusable)
                    let button_label = if self.add_pair_edit_index.is_some() { " [ Save ] " } else { " [ Add ] " };
                    let button_width = button_label.len() as u16;
                    let [_, button_area] = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Min(0), Constraint::Length(button_width)])
                        .areas(row3);
                    let is_button_focused = self.add_pair_focus == AddPairFocus::Button;
                    let button_style = if is_button_focused {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let button = Paragraph::new(button_label).style(button_style);
                    button.render(button_area, buf);
                }
            }
            JmesDialogMode::Error(msg) => {
                // Use shared ErrorDialog overlay within the dialog content area
                let err = ErrorDialog::with_title(msg.clone(), "Error");
                // Render into the inner area so it appears within the double frame
                render_error_dialog(&err, inner_total_area, buf);
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind == KeyEventKind::Press {
            // Handle Global actions first
            if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                match global_action {
                    Action::Escape => return Some(Action::DialogClose),
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return None;
                    }
                    Action::Up => {
                        match self.focus {
                            FocusArea::Tabs => { self.focus = FocusArea::Scope; return None; }
                            FocusArea::Scope => { return None; }
                            FocusArea::Body => { /* handled per-mode below */ }
                        }
                    }
                    Action::Down => {
                        match self.focus {
                            FocusArea::Scope => { self.focus = FocusArea::Tabs; return None; }
                            FocusArea::Tabs => { self.focus = FocusArea::Body; return None; }
                            FocusArea::Body => { /* handled per-mode below */ }
                        }
                    }
                    Action::Left => {
                        if self.focus == FocusArea::Tabs {
                            self.mode = JmesDialogMode::InputTransform;
                            return None;
                        }
                    }
                    Action::Right => {
                        if self.focus == FocusArea::Tabs {
                            self.mode = JmesDialogMode::InputAddColumns;
                            return None;
                        }
                    }
                    Action::Enter => {
                        // Handle per-mode below
                    }
                    _ => {}
                }
            }

            // Handle JmesPath-specific actions
            if let Some(jmes_action) = self.config.action_for_key(crate::config::Mode::JmesPath, key) {
                match jmes_action {
                    Action::ApplyTransform => {
                        match self.mode {
                            JmesDialogMode::InputTransform => {
                                let query = self.textarea.lines().join("\n");
                                return Some(Action::JmesTransformDataset((query, self.scope.clone())));
                            }
                            JmesDialogMode::InputAddColumns => {
                                return Some(Action::JmesTransformAddColumns(
                                    self.add_columns.clone(),
                                    self.scope.clone()
                                ));
                            }
                            _ => {}
                        }
                    }
                    Action::AddColumn => {
                        if matches!(self.mode, JmesDialogMode::InputAddColumns) {
                            self.add_pair_open = true;
                            self.add_pair_edit_index = None;
                            self.add_pair_focus = AddPairFocus::Name;
                            self.add_pair_name = TextArea::default();
                            self.add_pair_value = TextArea::default();
                            return None;
                        }
                    }
                    Action::EditColumn => {
                        if matches!(self.mode, JmesDialogMode::InputAddColumns) 
                            && !self.add_columns.is_empty() 
                            && self.selected_add_col < self.add_columns.len() {
                            let pair = self.add_columns[self.selected_add_col].clone();
                            self.add_pair_open = true;
                            self.add_pair_edit_index = Some(self.selected_add_col);
                            self.add_pair_focus = AddPairFocus::Name;
                            self.add_pair_name = TextArea::from(vec![pair.name]);
                            let value_lines: Vec<String> = if pair.value.is_empty() {
                                Vec::new()
                            } else {
                                pair.value.lines().map(|l| l.to_string()).collect()
                            };
                            self.add_pair_value = TextArea::from(value_lines);
                            return None;
                        }
                    }
                    Action::DeleteColumn => {
                        if matches!(self.mode, JmesDialogMode::InputAddColumns) 
                            && !self.add_columns.is_empty() 
                            && self.selected_add_col < self.add_columns.len() {
                            self.add_columns.remove(self.selected_add_col);
                            if self.selected_add_col > 0 {
                                self.selected_add_col -= 1;
                            }
                            return None;
                        }
                    }
                    _ => {}
                }
            }

            // Handle Space for toggle
            if key.code == KeyCode::Char(' ') && self.focus == FocusArea::Scope && self.selected_option == 0 {
                self.scope = match self.scope { 
                    TransformScope::Current => TransformScope::Original,
                    TransformScope::Original => TransformScope::Current
                };
                return None;
            }
            match &mut self.mode {
                JmesDialogMode::InputTransform => {
                    // Handle Global Enter action for textarea input
                    if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                        match global_action {
                            Action::Enter => {
                                let input: tui_textarea::Input = key.into();
                                if self.focus == FocusArea::Body { self.textarea.input(input); }
                                return None;
                            }
                            Action::Up => {
                                if self.focus == FocusArea::Body {
                                    let (row, _col) = self.textarea.cursor();
                                    if row == 0 {
                                        self.focus = FocusArea::Tabs;
                                        return None;
                                    }
                                    // otherwise let textarea handle it
                                    let input: tui_textarea::Input = key.into();
                                    self.textarea.input(input);
                                }
                                return None;
                            }
                            _ => {}
                        }
                    }

                    // Handle other input for textarea
                    let input: tui_textarea::Input = key.into();
                    if self.focus == FocusArea::Body { self.textarea.input(input); }
                    return None;
                }
                JmesDialogMode::InputAddColumns => {
                    // Handle Global Enter action for adding columns when not in pair dialog
                    if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                        match global_action {
                            Action::Enter if !self.add_pair_open => {
                                // Treat pressing Enter while Add Columns tab is focused (Body) as clicking the Add button
                                if self.focus == FocusArea::Body {
                                    self.add_pair_open = true;
                                    self.add_pair_edit_index = None;
                                    self.add_pair_focus = AddPairFocus::Name;
                                    self.add_pair_name = TextArea::default();
                                    self.add_pair_value = TextArea::default();
                                    return None;
                                }
                            }
                            Action::Up if !self.add_pair_open => {
                                if self.focus == FocusArea::Body && self.selected_add_col == 0 {
                                    self.focus = FocusArea::Tabs;
                                    return None;
                                }
                                if self.selected_add_col > 0 { self.selected_add_col -= 1; }
                                return None;
                            }
                            Action::Down if !self.add_pair_open => {
                                if self.selected_add_col + 1 < self.add_columns.len() { self.selected_add_col += 1; }
                                return None;
                            }
                            _ => {}
                        }
                    }

                    match key.code {
                        KeyCode::Enter if self.add_pair_open => {
                            match self.add_pair_focus {
                                AddPairFocus::Button => {
                                    let name = self.add_pair_name.lines().join("");
                                    let value = self.add_pair_value.lines().join("\n");

                                    if let Some(idx) = self.add_pair_edit_index {
                                        if let Some(pair) = self.add_columns.get_mut(idx) {
                                            pair.name = name;
                                            pair.value = value;
                                        }
                                        self.selected_add_col = idx;
                                    } else {
                                        self.add_columns.push(JmesPathKeyValuePair { name, value });
                                        self.selected_add_col = self.add_columns.len().saturating_sub(1);
                                    }
                                    self.add_pair_open = false;
                                    self.add_pair_edit_index = None;
                                }
                                AddPairFocus::Value => {
                                    // Insert newline into Value
                                    let input: tui_textarea::Input = key.into();
                                    self.add_pair_value.input(input);
                                }
                                AddPairFocus::Name => {
                                    self.add_pair_focus = AddPairFocus::Name;
                                }
                            }
                            return None;
                        }
                        KeyCode::Esc if self.add_pair_open => {
                            // Cancel add dialog
                            self.add_pair_open = false;
                            self.add_pair_edit_index = None;
                            return None;
                        }
                        KeyCode::Right if self.add_pair_open && self.add_pair_focus == AddPairFocus::Name => {
                            // If cursor is at the very end of the Name content, move focus to Value
                            let (row, col) = self.add_pair_name.cursor();
                            let lines = self.add_pair_name.lines();
                            let last_row = lines.len().saturating_sub(1);
                            let at_end = if lines.is_empty() {
                                true
                            } else {
                                let current_line_len = lines.get(row).map(|s| s.len()).unwrap_or(0);
                                row == last_row && col >= current_line_len
                            };
                            if at_end {
                                self.add_pair_focus = AddPairFocus::Value;
                                return None;
                            }
                            // otherwise let textarea handle it
                            let input: tui_textarea::Input = key.into();
                            self.add_pair_name.input(input);
                            return None;
                        }
                        KeyCode::Right if self.add_pair_open && self.add_pair_focus == AddPairFocus::Value => {
                            // If cursor is at the very end of the Value content, move focus to Add button
                            let (row, col) = self.add_pair_value.cursor();
                            let lines = self.add_pair_value.lines();
                            let last_row = lines.len().saturating_sub(1);
                            let at_end = if lines.is_empty() {
                                true
                            } else {
                                let current_line_len = lines.get(row).map(|s| s.len()).unwrap_or(0);
                                row == last_row && col >= current_line_len
                            };
                            if at_end {
                                self.add_pair_focus = AddPairFocus::Button;
                                return None;
                            }
                            // otherwise let textarea handle it
                            let input: tui_textarea::Input = key.into();
                            self.add_pair_value.input(input);
                            return None;
                        }
                        KeyCode::Left if self.add_pair_open && self.add_pair_focus == AddPairFocus::Value => {
                            // If cursor is at the very start of the Value content, move focus to Name
                            let (row, col) = self.add_pair_value.cursor();
                            if row == 0 && col == 0 {
                                self.add_pair_focus = AddPairFocus::Name;
                                return None;
                            }
                            // otherwise let textarea handle it
                            let input: tui_textarea::Input = key.into();
                            self.add_pair_value.input(input);
                            return None;
                        }
                        KeyCode::Down if self.add_pair_open => {
                            match self.add_pair_focus {
                                AddPairFocus::Name => {
                                    let (row, _col) = self.add_pair_name.cursor();
                                    let last_row = self.add_pair_name.lines().len().saturating_sub(1);
                                    if row >= last_row { self.add_pair_focus = AddPairFocus::Value; return None; }
                                    let input: tui_textarea::Input = key.into();
                                    self.add_pair_name.input(input);
                                    return None;
                                }
                                AddPairFocus::Value => {
                                    let (row, _col) = self.add_pair_value.cursor();
                                    let last_row = self.add_pair_value.lines().len().saturating_sub(1);
                                    if row >= last_row { self.add_pair_focus = AddPairFocus::Button; return None; }
                                    let input: tui_textarea::Input = key.into();
                                    self.add_pair_value.input(input);
                                    return None;
                                }
                                AddPairFocus::Button => { return None; }
                            }
                        }
                        KeyCode::Up if self.add_pair_open => {
                            match self.add_pair_focus {
                                AddPairFocus::Button => { self.add_pair_focus = AddPairFocus::Value; return None; }
                                AddPairFocus::Value => {
                                    let (row, _col) = self.add_pair_value.cursor();
                                    if row == 0 { self.add_pair_focus = AddPairFocus::Name; return None; }
                                    let input: tui_textarea::Input = key.into();
                                    self.add_pair_value.input(input);
                                    return None;
                                }
                                AddPairFocus::Name => {
                                    // At top of Name, stay; otherwise, propagate to textarea
                                    let (row, _col) = self.add_pair_name.cursor();
                                    if row > 0 {
                                        let input: tui_textarea::Input = key.into();
                                        self.add_pair_name.input(input);
                                    }
                                    return None;
                                }
                            }
                        }
                        _ if self.add_pair_open => {
                            // Route input to the focused add-pair field
                            let input: tui_textarea::Input = key.into();
                            match self.add_pair_focus {
                                AddPairFocus::Name => { self.add_pair_name.input(input); }
                                AddPairFocus::Value => { self.add_pair_value.input(input); }
                                AddPairFocus::Button => {}
                            };
                            return None;
                        }
                        _ => return None,
                    }
                }
                JmesDialogMode::Error(_) => {
                    // Handle Global actions in Error mode
                    if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
                        match global_action {
                            Action::Enter | Action::Escape => {
                                self.mode = JmesDialogMode::InputTransform;
                                return None;
                            }
                            _ => return None,
                        }
                    }
                    return None;
                }
            }
        }
        None
    }
}

impl Component for JmesPathDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { 
        self.config = _config; 
        Ok(()) 
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}


