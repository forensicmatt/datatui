//! FileBrowserDialog: Popup dialog for selecting files/directories in a TUI
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::fs;
use std::path::{PathBuf};
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::style::Color;
use crate::components::dialog_layout::split_dialog_area;
use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserMode {
    Load,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserAction {
    Selected(PathBuf),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserPrompt {
    None,
    OverwriteConfirm(PathBuf),
}

#[derive(Debug)]
pub struct FileBrowserDialog {
    pub current_dir: PathBuf,
    pub entries: Vec<fs::DirEntry>,
    pub selected: usize,
    pub filter_ext: Option<Vec<String>>,
    pub folder_only: bool,
    pub error: Option<String>,
    pub scroll_offset: usize, // <-- add scroll offset
    pub mode: FileBrowserMode,
    pub filename_input: String, // Only used in Save mode
    pub filename_active: bool,  // Whether filename input is focused
    pub filename_cursor: usize, // Cursor position in filename input
    pub prompt: FileBrowserPrompt, // New: prompt state
    pub show_instructions: bool, // new: show instructions area (default true)
    pub open_button_selected: bool, // Load mode: footer [Open] button selection
    pub config: Config, // Configuration for keybindings
}

impl FileBrowserDialog {
    pub fn new(start_path: Option<PathBuf>, filter_ext: Option<Vec<&str>>, folder_only: bool, mode: FileBrowserMode) -> Self {
        let dir = start_path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let ext_vec = filter_ext.map(|v| v.into_iter().map(|s| s.to_string()).collect());
        let entries = Self::read_dir(&dir, ext_vec.as_ref(), folder_only);
        let filename_active = false; // Always start with filename input not active
        Self {
            current_dir: dir,
            entries,
            selected: 0,
            filter_ext: ext_vec,
            folder_only,
            error: None,
            scroll_offset: 0,
            mode: mode.clone(),
            filename_input: String::new(),
            filename_active,
            filename_cursor: 0,
            prompt: FileBrowserPrompt::None,
            show_instructions: true,
            open_button_selected: false,
            config: Config::default(),
        }
    }

    fn read_dir(dir: &PathBuf, filter_ext: Option<&Vec<String>>, folder_only: bool) -> Vec<fs::DirEntry> {
        let mut entries: Vec<fs::DirEntry> = match fs::read_dir(dir) {
            Ok(read_dir) => read_dir.filter_map(|e| e.ok())
                .filter(|_e| {
                    true
                })
                .collect(),
            Err(_) => vec![],
        };
        entries.sort_by_key(|e| e.file_name());
        if folder_only {
            entries.retain(|e| {
                e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
            });
        } else if let Some(exts) = filter_ext {
            entries.retain(|e| {
                if let Ok(ft) = e.file_type() {
                    if ft.is_dir() {
                        true
                    } else {
                        e.path().extension()
                            .and_then(|x| x.to_str())
                            .map(|x| exts.iter().any(|ext| x == ext))
                            .unwrap_or(false)
                    }
                } else { false }
            });
        }
        entries
    }

    fn at_root(&self) -> bool {
        self.current_dir.parent().is_none()
    }

    /// Register config handler
    pub fn register_config_handler(&mut self, config: Config) {
        self.config = config;
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
                KeyCode::Backspace => "Backspace".to_string(),
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

        // Handle Global actions
        if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
            for (key_seq, action) in global_bindings {
                match action {
                    crate::action::Action::Tab => {
                        segments.push(format!("{}: Tab", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Handle FileBrowser-specific actions  
        if let Some(dialog_bindings) = self.config.keybindings.0.get(&crate::config::Mode::FileBrowser) {
            for (key_seq, action) in dialog_bindings {
                match action {
                    crate::action::Action::FileBrowserPageUp => {
                        segments.push(format!("{}: Page Up", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::FileBrowserPageDown => {
                        segments.push(format!("{}: Page Down", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::NavigateToParent => {
                        segments.push(format!("{}: Up Directory", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::ConfirmOverwrite => {
                        segments.push(format!("{}: Confirm Overwrite", fmt_sequence(key_seq)));
                    }
                    crate::action::Action::DenyOverwrite => {
                        segments.push(format!("{}: Deny Overwrite", fmt_sequence(key_seq)));
                    }
                    _ => {}
                }
            }
        }

        // Join segments
        let mut out = String::new();
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 { let _ = write!(out, "  "); }
            let _ = write!(out, "{}", seg);
        }
        out
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let mut content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        let mut filename_area = None;
        let mut footer_area = None;
        // Reserve space for footer [Open] button in Load mode
        if self.mode == FileBrowserMode::Load {
            let footer_height = 1;
            if content_area.height > footer_height {
                content_area.height = content_area.height.saturating_sub(footer_height);
                footer_area = Some(Rect {
                    x: content_area.x,
                    y: content_area.y + content_area.height,
                    width: content_area.width,
                    height: footer_height,
                });
            }
        }
        if self.mode == FileBrowserMode::Save {
            // Reserve 3 lines for filename input
            let filename_height = 3;
            content_area.height = content_area.height.saturating_sub(filename_height);
            filename_area = Some(Rect {
                x: content_area.x,
                y: content_area.y + content_area.height,
                width: content_area.width,
                height: filename_height,
            });
        }
        let block = Block::default()
            .title(format!("File Browser: {}", self.current_dir.display()))
            .borders(Borders::ALL);
        block.render(content_area, buf);
        let inner = content_area.inner(Margin { vertical: 1, horizontal: 2 });
        let _wrap_width = inner.width.saturating_sub(4).max(10) as usize;
        let max_rows = inner.height as usize;
        // Show .. for parent directory
        let show_parent = !self.at_root();
        let entries_offset = if show_parent { 1 } else { 0 };
        let total_items = self.entries.len() + entries_offset;
        let visible_rows = max_rows;
        // Calculate scroll offset - only scroll when necessary
        // This prevents premature scrolling when there are fewer items than visible rows
        let selected = self.selected;
        let mut scroll_offset = self.scroll_offset;
        
        // If selected item is above the visible area, scroll up
        if selected < scroll_offset {
            scroll_offset = selected;
        }
        // If selected item is below the visible area, scroll down
        // Only scroll if there are more items than can fit in the visible area
        else if total_items > visible_rows && selected >= scroll_offset + visible_rows {
            scroll_offset = selected + 1 - visible_rows;
        }
        // If we have fewer items than visible rows, reset scroll to 0
        // This prevents unnecessary scrolling for small directories
        else if total_items <= visible_rows {
            scroll_offset = 0;
        }
        // Draw file list with scrolling
        let mut display_row = 0;
        if show_parent && scroll_offset == 0 {
            let style = if selected == 0 && !(self.mode == FileBrowserMode::Load && self.open_button_selected) {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            buf.set_string(inner.x, inner.y, "📁 ..", style.fg(Color::White));
            display_row += 1;
        }
        let entry_start = if show_parent { scroll_offset.saturating_sub(1) } else { scroll_offset };
        let _entry_end = (entry_start + visible_rows - display_row).min(self.entries.len());
        for (i, entry) in self.entries.iter().enumerate().skip(entry_start).take(visible_rows - display_row) {
            let y = inner.y + display_row as u16;
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let style = if selected == i + entries_offset && !(self.mode == FileBrowserMode::Load && self.open_button_selected) {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let (icon, color) = if is_dir {
                ("📁", Color::White)
            } else {
                ("📄", Color::White)
            };
            let display = format!("{icon} {name}");
            buf.set_string(inner.x, y, display, style.fg(color));
            display_row += 1;
        }
        // Draw scroll bar if needed - only when there are more items than visible rows
        if total_items > visible_rows {
            let bar_height = inner.height; // Use actual content height
            let bar_x = inner.right().saturating_sub(1);
            
            // Calculate thumb size (minimum 1, proportional to visible content)
            let thumb_size = std::cmp::max(1, (bar_height as usize * visible_rows) / total_items);
            
            // Calculate thumb position based on scroll offset
            let max_scroll = total_items.saturating_sub(visible_rows);
            let thumb_pos = if max_scroll > 0 {
                (scroll_offset * (bar_height as usize - thumb_size)) / max_scroll
            } else {
                0
            };
            
            // Draw scroll bar track
            for i in 0..bar_height {
                buf.set_string(bar_x, inner.y + i, "│", Style::default().fg(Color::DarkGray));
            }
            
            // Draw scroll bar thumb
            for i in 0..thumb_size {
                let y_pos = inner.y + thumb_pos as u16 + i as u16;
                if y_pos < inner.y + bar_height {
                    buf.set_string(bar_x, y_pos, "█", Style::default().fg(Color::Cyan));
                }
            }
        }
        if let Some(err) = &self.error {
            buf.set_string(
                inner.x, 
                inner.y + inner.height.saturating_sub(1),
                err,
                Style::default().fg(Color::Red)
            );
        }
        // Render filename input if in Save mode
        if let Some(filename_area) = filename_area {
            let input = &self.filename_input;
            let style = if self.filename_active {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(30, 30, 30))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                  .fg(Color::White)
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title("File Name");
            block.render(filename_area, buf);
            
            // Render the input text
            buf.set_string(filename_area.x + 1, filename_area.y + 1, input, style);
            
            // Render cursor if filename input is active
            if self.filename_active {
                let cursor_x = filename_area.x + 1 + self.filename_cursor as u16;
                let cursor_y = filename_area.y + 1;
                // Ensure cursor is within the visible area
                if cursor_x < filename_area.x + filename_area.width - 1 {
                    // Use a different background color for the cursor
                    let cursor_style = Style::default()
                        .fg(Color::Black)
                        .bg(Color::White);
                    
                    // Get the character at cursor position, or use space if at end
                    let cursor_char = if self.filename_cursor < input.len() {
                        input.chars().nth(self.filename_cursor).unwrap_or(' ')
                    } else {
                        ' '
                    };
                    
                    buf.set_string(cursor_x, cursor_y, cursor_char.to_string(), cursor_style);
                }
            }
        }
        // Render footer with [Open] button in Load mode
        if let Some(footer) = footer_area {
            let open_text = "[Open]";
            let open_x = footer.x + footer.width.saturating_sub(open_text.len() as u16 + 1);
            let open_style = if self.open_button_selected {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };
            buf.set_string(open_x, footer.y, open_text, open_style);
        }
        // Render overwrite confirmation prompt if needed
        if let FileBrowserPrompt::OverwriteConfirm(ref path) = self.prompt {
            let msg = format!("Overwrite file {}? (Y/N)", path.display());
            let prompt_width = area.width / 2;
            let wrap_width = prompt_width.saturating_sub(4).max(10) as usize;
            let wrapped_lines = textwrap::wrap(&msg, wrap_width);
            let prompt_height = (wrapped_lines.len() as u16).max(1) + 2; // 2 for borders
            let prompt_area = Rect {
                x: area.x + area.width / 4,
                y: area.y + area.height / 2 - prompt_height / 2,
                width: prompt_width,
                height: prompt_height,
            };
            let block = Block::default().borders(Borders::ALL).title("Confirm Overwrite");
            block.render(prompt_area, buf);
            for (i, line) in wrapped_lines.iter().enumerate() {
                buf.set_string(prompt_area.x + 2, prompt_area.y + 1 + i as u16, line, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
            }
        }
        // Render instructions at the bottom
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<FileBrowserAction> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Get config-driven actions once
        let global_action = self.config.action_for_key(crate::config::Mode::Global, key);
        let filebrowser_action = self.config.action_for_key(crate::config::Mode::FileBrowser, key);
        
        // Handle Global ToggleInstructions first
        if let Some(crate::action::Action::ToggleInstructions) = &global_action {
            self.show_instructions = !self.show_instructions;
            return None;
        }
        
        // Handle prompt first
        if let FileBrowserPrompt::OverwriteConfirm(ref path) = self.prompt {
            let path_clone = path.clone();
            
            // Check for confirm/deny actions
            if let Some(filebrowser_action) = &filebrowser_action {
                match filebrowser_action {
                    crate::action::Action::ConfirmOverwrite => {
                        self.prompt = FileBrowserPrompt::None;
                        return Some(FileBrowserAction::Selected(path_clone));
                    }
                    crate::action::Action::DenyOverwrite => {
                        self.prompt = FileBrowserPrompt::None;
                        return None;
                    }
                    _ => {}
                }
            }
            
            // Also check for global Escape
            if let Some(crate::action::Action::Escape) = &global_action {
                self.prompt = FileBrowserPrompt::None;
                return None;
            }
            
            return None;
        }
        let entries_offset = if self.at_root() { 0 } else { 1 };
        let total_items = self.entries.len() + entries_offset;
        
        if self.mode == FileBrowserMode::Save && self.filename_active {
            // Handle filename input
            // Check config-driven actions for filename input mode
            if let Some(global_action) = &global_action {
                match global_action {
                    crate::action::Action::Tab => {
                        self.filename_active = false;
                        return None;
                    }
                    crate::action::Action::Enter => {
                        if !self.filename_input.is_empty() {
                            return Some(FileBrowserAction::Selected(self.current_dir.join(&self.filename_input)));
                        }
                        return None;
                    }
                    crate::action::Action::Backspace => {
                        if self.filename_cursor > 0 {
                            self.filename_cursor -= 1;
                            self.filename_input.remove(self.filename_cursor);
                        }
                        return None;
                    }
                    crate::action::Action::Escape => {
                        return Some(FileBrowserAction::Cancelled);
                    }
                    crate::action::Action::Left => {
                        if self.filename_cursor > 0 {
                            self.filename_cursor -= 1;
                        }
                        return None;
                    }
                    crate::action::Action::Right => {
                        if self.filename_cursor < self.filename_input.len() {
                            self.filename_cursor += 1;
                        }
                        return None;
                    }
                    _ => {}
                }
            }
            
            // Fallback for character input
            use crossterm::event::KeyCode;
            if let KeyCode::Char(c) = key.code {
                self.filename_input.insert(self.filename_cursor, c);
                self.filename_cursor += 1;
            }
            return None;
        }
        // Handle Global actions
        if let Some(global_action) = &global_action {
            match global_action {
                crate::action::Action::Tab => {
                    match self.mode {
                        FileBrowserMode::Save => {
                            self.filename_active = true;
                            self.filename_cursor = self.filename_input.len(); // Position cursor at end
                            return None;
                        }
                        FileBrowserMode::Load => {
                            // Toggle footer [Open] button selection
                            self.open_button_selected = !self.open_button_selected;
                            return None;
                        }
                    }
                }
                crate::action::Action::Up => {
                    // Regular Up: Move one item up
                    if self.selected > 0 {
                        self.selected -= 1;
                        // Update scroll offset with calculated visible rows
                        self.update_scroll_offset(self.calculate_visible_rows());
                    }
                    return None;
                }
                crate::action::Action::Down => {
                    // Regular Down: Move one item down
                    if self.selected + 1 < total_items {
                        self.selected += 1;
                        // Update scroll offset with calculated visible rows
                        self.update_scroll_offset(self.calculate_visible_rows());
                    }
                    return None;
                }
                crate::action::Action::Enter => {
                    // If footer [Open] is selected in Load mode, select the current directory only
                    if self.mode == FileBrowserMode::Load && self.open_button_selected {
                        return Some(FileBrowserAction::Selected(self.current_dir.clone()));
                    }
                    if !self.at_root() && self.selected == 0 {
                        // '..' selected
                        if let Some(parent) = self.current_dir.parent() {
                            self.current_dir = parent.to_path_buf();
                            self.entries = Self::read_dir(&self.current_dir, self.filter_ext.as_ref(), self.folder_only);
                            self.selected = 0;
                            self.scroll_offset = 0; // Reset scroll when changing directories
                        }
                        return None;
                    }
                    let entry_idx = self.selected - entries_offset;
                    if let Some(entry) = self.entries.get(entry_idx)
                        && let Ok(ft) = entry.file_type() {
                            if ft.is_dir() {
                                self.current_dir = entry.path();
                                self.entries = Self::read_dir(&self.current_dir, self.filter_ext.as_ref(), self.folder_only);
                                self.selected = 0;
                                self.scroll_offset = 0; // Reset scroll when changing directories
                                return None;
                            } else if self.mode == FileBrowserMode::Load {
                                return Some(FileBrowserAction::Selected(entry.path()));
                            } else if self.mode == FileBrowserMode::Save {
                                // Prompt for overwrite if file exists
                                self.prompt = FileBrowserPrompt::OverwriteConfirm(entry.path());
                                return None;
                            }
                        }
                    return None;
                }
                crate::action::Action::Escape => {
                    // If footer button is selected, unselect first; otherwise cancel
                    if self.mode == FileBrowserMode::Load && self.open_button_selected {
                        self.open_button_selected = false;
                        return None;
                    }
                    return Some(FileBrowserAction::Cancelled);
                }
                _ => {}
            }
        }

        // Handle FileBrowser-specific actions
        if let Some(filebrowser_action) = &filebrowser_action {
            match filebrowser_action {
                crate::action::Action::FileBrowserPageUp => {
                    // Ctrl+Up: Page up - move by visible rows minus 1 (to show overlap)
                    let page_size = self.calculate_visible_rows().saturating_sub(1);
                    if self.selected > page_size {
                        self.selected = self.selected.saturating_sub(page_size);
                    } else {
                        self.selected = 0;
                    }
                    self.update_scroll_offset(self.calculate_visible_rows());
                    return None;
                }
                crate::action::Action::FileBrowserPageDown => {
                    // Ctrl+Down: Page down - move by visible rows minus 1 (to show overlap)
                    let page_size = self.calculate_visible_rows().saturating_sub(1);
                    if self.selected + page_size < total_items {
                        self.selected += page_size;
                    } else {
                        self.selected = total_items.saturating_sub(1);
                    }
                    self.update_scroll_offset(self.calculate_visible_rows());
                    return None;
                }
                crate::action::Action::NavigateToParent => {
                    if !self.at_root()
                        && let Some(parent) = self.current_dir.parent() {
                            self.current_dir = parent.to_path_buf();
                            self.entries = Self::read_dir(&self.current_dir, self.filter_ext.as_ref(), self.folder_only);
                            self.selected = 0;
                            self.scroll_offset = 0; // Reset scroll when changing directories
                        }
                    return None;
                }
                _ => {}
            }
        }
        None
    }

    pub fn get_selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).map(|e| e.path())
    }

    fn update_scroll_offset(&mut self, visible_rows: usize) {
        let entries_offset = if self.at_root() { 0 } else { 1 };
        let total_items = self.entries.len() + entries_offset;
        let selected = self.selected;
        
        // If selected item is above the visible area, scroll up
        if selected < self.scroll_offset {
            self.scroll_offset = selected;
        }
        // If selected item is below the visible area, scroll down
        // Only scroll if there are more items than can fit in the visible area
        else if total_items > visible_rows && selected >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected + 1 - visible_rows;
        }
        // If we have fewer items than visible rows, reset scroll to 0
        else if total_items <= visible_rows {
            self.scroll_offset = 0;
        }
    }

    fn calculate_visible_rows(&self) -> usize {
        // Estimate visible rows based on typical terminal size and dialog layout
        // Account for dialog borders, instructions area, and filename input area
        let base_rows: usize = 25; // Typical terminal height
        let dialog_margins: usize = 4; // Top/bottom margins for dialog
        let instructions_height: usize = if self.show_instructions { 3 } else { 0 };
        let filename_height: usize = if self.mode == FileBrowserMode::Save { 3 } else { 0 };
        
        base_rows.saturating_sub(dialog_margins + instructions_height + filename_height)
    }
} 