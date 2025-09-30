use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Table, Row, Cell, Clear, Paragraph, Wrap, BorderType};
use crate::components::dialog_layout::split_dialog_area;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::action::Action;
use crate::config::Config;

#[derive(Debug)]
pub struct FindAllResult {
    pub row: usize,
    pub column: String,
    pub context: String,
}

#[derive(Debug)]
pub struct FindAllResultsDialog {
    pub results: Vec<FindAllResult>,
    pub selected: usize,
    pub show_instructions: bool,
    pub instructions: String,
    pub scroll_offset: usize,
    pub search_pattern: String, // Store the search pattern for 
    pub visable_rows: usize,
    pub config: Config,
}

impl FindAllResultsDialog {
    pub fn new(results: Vec<FindAllResult>, instructions: String, search_pattern: String) -> Self {
        Self {
            results,
            selected: 0,
            show_instructions: true,
            instructions,
            scroll_offset: 0,
            search_pattern,
            visable_rows: 5,
            config: Config::default(),
        }
    }

    /// Register config handler 
    pub fn register_config_handler(&mut self, _config: Config) -> color_eyre::Result<()> { 
        self.config = _config; 
        Ok(()) 
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

        // Handle Global actions (Escape, Enter, Up, Down)
        if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
            for (key_seq, action) in global_bindings {
                match action {
                    crate::action::Action::Escape => {
                        segments.push(format!("{}: Close", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Enter => {
                        segments.push(format!("{}: Go to Result", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Up => {
                        segments.push(format!("{}: Navigate Up", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::Down => {
                        segments.push(format!("{}: Navigate Down", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::ToggleInstructions => {
                        segments.push(format!("{}: Toggle Instructions", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Handle FindAllResults-specific actions  
        if let Some(dialog_bindings) = self.config.keybindings.0.get(&crate::config::Mode::FindAllResults) {
            for (key_seq, action) in dialog_bindings {
                match action {
                    crate::action::Action::GoToFirst => {
                        segments.push(format!("{}: Go to First", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::GoToLast => {
                        segments.push(format!("{}: Go to Last", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::PageUp => {
                        segments.push(format!("{}: Page Up", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::PageDown => {
                        segments.push(format!("{}: Page Down", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Join segments
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{seg}");
        }
        out
    }

    /// Render the dialog with a scrollable table of results
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Clear the entire area
        Clear.render(area, buf);

        // Outer container with double border
        let outer_block = Block::default()
            .title("Find")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        // Render main content block
        let block = Block::default()
            .title("All Results")
            .borders(Borders::ALL);
        let all_results_area = block.inner(content_area);
        block.render(content_area, buf);
        
        if self.results.is_empty() {
            // Show "No results found" message
            let no_results = Paragraph::new("No matches found")
                .style(Style::default().fg(Color::Yellow));
            no_results.render(all_results_area, buf);
        } else {
            // Render the results table with scroll bar
            self.render_results_table(all_results_area, buf);
        }
        
        // Render instructions area if available
        if let Some(instructions_area) = instructions_area {
            let instructions_paragraph = Paragraph::new(instructions.as_str())
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true });
            instructions_paragraph.render(instructions_area, buf);
        }
    }

    /// Ensure the selected row is within the visible viewport by adjusting the scroll offset
    fn update_scroll_offset(&mut self, visible_rows: usize) {
        let selected = self.selected;
        let total_items = self.results.len();

        if selected < self.scroll_offset {
            self.scroll_offset = selected;
        } else if total_items > visible_rows && selected >= self.scroll_offset + visible_rows {
            // Scroll so that the selected item becomes the last visible row
            self.scroll_offset = selected + 1 - visible_rows;
        }
    }

    /// Render the scrollable results table with vertical scroll bar
    fn render_results_table(&mut self, area: Rect, buf: &mut Buffer) {
        // Define column widths (adjust as needed)
        let col_widths = [
            Constraint::Length(8),  // Row
            Constraint::Length(15), // Column
            Constraint::Min(20),    // Context (flexible)
        ];

        let max_rows = area.height.saturating_sub(1) as usize;
        self.visable_rows = max_rows;
        
        // Calculate visible range
        let start_idx = self.scroll_offset;
        let end_idx = (start_idx + max_rows).min(self.results.len());

        // Draw scroll bar on the right side if needed
        let show_scroll_bar = self.results.len() > max_rows;
        let table_width = if show_scroll_bar {
            area.width.saturating_sub(1) // Leave space for scroll bar
        } else {
            area.width
        };
        
        // Draw scroll bar if needed
        if show_scroll_bar {
            let scroll_bar_x = area.x + area.width.saturating_sub(1);
            let scroll_bar_height = max_rows;
            let scroll_bar_y_start = area.y;
            
            // Calculate thumb position and size
            let total_items = self.results.len();
            let visible_items = max_rows;
            let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
            let thumb_position = if total_items > visible_items {
                (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
            } else {
                0
            };
            
            // Draw scroll bar track
            for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
            }
            
            // Draw scroll bar thumb
            let thumb_start = scroll_bar_y_start + thumb_position as u16;
            let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
            for y in thumb_start..thumb_end {
                buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
            }
        }
        
        // Create table rows from visible results
        let rows: Vec<Row> = self.results[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let row_idx = start_idx + i;
                let is_selected = row_idx == self.selected;
                let is_zebra = row_idx % 2 == 0; // Zebra striping

                // Remove newlines and carriage returns from the context
                // because in a Span they are treated as separate lines and it causes issues
                // with rendering the context in the dialog.
                let context_str = result.context
                    .replace("\n", "")
                    .replace("\r", "");
                let highlighted_context = self.highlight_search_hit(&context_str);
                
                let mut style = Style::default();
                if is_selected {
                    style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
                } else if is_zebra {
                    style = style.bg(Color::Rgb(30, 30, 30)); // Dark gray for zebra rows
                }
                
                Row::new(vec![
                    Cell::from(format!("{}", result.row)).style(style),
                    Cell::from(result.column.clone()).style(style),
                    Cell::from(highlighted_context).style(style),
                ])
            })
            .collect();
        
        // Create and render the table with yellow header
        let table = Table::new(rows, col_widths)
            .header(Row::new(vec![
                Cell::from("Row").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Cell::from("Column").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Cell::from("Context").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]))
            .column_spacing(1);
        
        // Render table in the adjusted area
        let table_area = Rect {
            x: area.x,
            y: area.y,
            width: table_width,
            height: area.height,
        };
        
        ratatui::prelude::Widget::render(table, table_area, buf);
    }

    /// Highlight the search hit in the context string with yellow background
    fn highlight_search_hit(&self, context: &str) -> Line<'static> {
        if self.search_pattern.is_empty() {
            return Line::from(context.to_string());
        }
        
        let mut spans = Vec::new();
        let mut last_end = 0;
        
        // Find all occurrences of the search pattern (case-insensitive)
        let pattern_lower = self.search_pattern.to_lowercase();
        let context_lower = context.to_lowercase();
        
        // Simple substring matching for highlighting
        let mut pos = 0;
        while let Some(start) = context_lower[pos..].find(&pattern_lower) {
            let actual_start = pos + start;
            let actual_end = actual_start + self.search_pattern.len();
            
            // Add text before the match
            if actual_start > last_end {
                spans.push(Span::raw(context[last_end..actual_start].to_string()));
            }
            
            // Add highlighted match
            let matched_style = Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD);
            spans.push(Span::styled(
                context[actual_start..actual_end].to_string(),
                matched_style
            ));
            
            last_end = actual_end;
            pos = actual_end;
        }
        
        // Add remaining text after the last match
        if last_end < context.len() {
            spans.push(Span::raw(context[last_end..].to_string()));
        }
        
        // If no matches found, return the original context
        if spans.is_empty() {
            Line::from(context.to_string())
        } else {
            Line::from(spans)
        }
    }

    /// Handle keyboard events for navigation and actions
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        let max_rows = self.visable_rows;
        
        if key.kind != KeyEventKind::Press {
            return None;
        }
        
        // First, honor config-driven Global actions
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => return Some(Action::DialogClose),
                Action::Enter => {
                    // Go to the selected result in the main DataTable
                    return self.results.get(self.selected).map(|result| Action::GoToResult {
                        row: result.row,
                        column: result.column.clone(),
                    });
                }
                Action::Up => {
                    // Move selection up
                    if self.selected > 0 {
                        self.selected -= 1;
                        // Adjust scroll if needed
                        self.update_scroll_offset(max_rows);
                    }
                    return None;
                }
                Action::Down => {
                    // Move selection down
                    if self.selected < self.results.len().saturating_sub(1) {
                        self.selected += 1;
                        // Adjust scroll if needed
                        self.update_scroll_offset(max_rows);
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

        // Next, check for FindAllResults-specific actions
        if let Some(dialog_action) = self.config.action_for_key(crate::config::Mode::FindAllResults, key) {
            match dialog_action {
                Action::GoToFirst => {
                    // Go to first result
                    self.selected = 0;
                    self.scroll_offset = 0;
                    return None;
                }
                Action::GoToLast => {
                    // Go to last result
                    self.selected = self.results.len().saturating_sub(1);
                    // Adjust scroll to show the last result
                    self.update_scroll_offset(max_rows);
                    return None;
                }
                Action::PageUp => {
                    // Page up navigation
                    let page_size = max_rows.saturating_sub(1);
                    if self.selected >= page_size {
                        self.selected -= page_size;
                        self.update_scroll_offset(max_rows);
                    } else {
                        self.selected = 0;
                        self.scroll_offset = 0;
                    }
                    return None;
                }
                Action::PageDown => {
                    // Page down navigation
                    let page_size = max_rows.saturating_sub(1);
                    let max_idx = self.results.len().saturating_sub(1);
                    if self.selected + page_size <= max_idx {
                        self.selected += page_size;
                    } else {
                        self.selected = max_idx;
                    }
                    // Adjust scroll if needed
                    self.update_scroll_offset(max_rows);
                    return None;
                }
                _ => {}
            }
        }

        None
    }

    /// Get the currently selected result
    pub fn get_selected_result(&self) -> Option<&FindAllResult> {
        self.results.get(self.selected)
    }

    /// Update the results (for persistence/reopening)
    pub fn update_results(&mut self, results: Vec<FindAllResult>) {
        self.results = results;
        self.selected = 0;
        self.scroll_offset = 0;
    }
} 