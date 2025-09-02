//! FileBrowserDialog: Popup dialog for selecting files/directories in a TUI
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use std::fs;
use std::path::{PathBuf};
use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::style::Color;
use crate::components::dialog_layout::split_dialog_area;

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
    pub prompt: FileBrowserPrompt, // New: prompt state
    pub show_instructions: bool, // new: show instructions area (default true)
    pub open_button_selected: bool, // Load mode: footer [Open] button selection
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
            prompt: FileBrowserPrompt::None,
            show_instructions: true,
            open_button_selected: false,
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
            entries = entries.into_iter().filter(|e| {
                e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
            }).collect();
        } else if let Some(exts) = filter_ext {
            entries = entries.into_iter().filter(|e| {
                if let Ok(ft) = e.file_type() {
                    if ft.is_dir() {
                        return true;
                    } else {
                        return e.path().extension()
                            .and_then(|x| x.to_str())
                            .map(|x| exts.iter().any(|ext| x == ext))
                            .unwrap_or(false);
                    }
                }
                false
            }).collect();
        }
        entries
    }

    fn at_root(&self) -> bool {
        self.current_dir.parent().is_none()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let instructions = match self.mode {
            FileBrowserMode::Load => "Up/Down: Navigate  Ctrl+Up/Down: Page  Enter: Open/Select  Esc: Cancel  Backspace: Up  Tab: Open Button",
            FileBrowserMode::Save => "Up/Down: Navigate  Ctrl+Up/Down: Page  Enter: Open Folder/Save  Esc: Cancel  Backspace: Up  Tab: Filename",
        };
        let layout = split_dialog_area(area, self.show_instructions, Some(instructions));
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
            let display = format!("{} {}", icon, name);
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
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title("File Name");
            block.render(filename_area, buf);
            buf.set_string(filename_area.x + 1, filename_area.y + 1, format!("{}", input), style);
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
        if self.show_instructions {
            if let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<FileBrowserAction> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
        // Handle Ctrl+I to toggle instructions
        if key.code == KeyCode::Char('i') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.show_instructions = !self.show_instructions;
            return None;
        }
        
        // Handle prompt first
        if let FileBrowserPrompt::OverwriteConfirm(ref path) = self.prompt {
            if key.kind == KeyEventKind::Press {
                let path_clone = path.clone();
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.prompt = FileBrowserPrompt::None;
                        return Some(FileBrowserAction::Selected(path_clone));
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.prompt = FileBrowserPrompt::None;
                        return None;
                    }
                    _ => {}
                }
            }
            return None;
        }
        let entries_offset = if self.at_root() { 0 } else { 1 };
        let total_items = self.entries.len() + entries_offset;
        if self.mode == FileBrowserMode::Save && self.filename_active {
            // Handle filename input
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Tab => {
                        self.filename_active = false;
                        return None;
                    }
                    KeyCode::Enter => {
                        if !self.filename_input.is_empty() {
                            return Some(FileBrowserAction::Selected(self.current_dir.join(&self.filename_input)));
                        }
                    }
                    KeyCode::Backspace => {
                        self.filename_input.pop();
                    }
                    KeyCode::Char(c) => {
                        self.filename_input.push(c);
                    }
                    KeyCode::Esc => {
                        return Some(FileBrowserAction::Cancelled);
                    }
                    _ => {}
                }
            }
            return None;
        }
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Tab => {
                    match self.mode {
                        FileBrowserMode::Save => {
                            self.filename_active = true;
                            return None;
                        }
                        FileBrowserMode::Load => {
                            // Toggle footer [Open] button selection
                            self.open_button_selected = !self.open_button_selected;
                            return None;
                        }
                    }
                }
                KeyCode::Up => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        // Ctrl+Up: Page up - move by visible rows minus 1 (to show overlap)
                        let page_size = self.calculate_visible_rows().saturating_sub(1);
                        if self.selected > page_size {
                            self.selected = self.selected.saturating_sub(page_size);
                        } else {
                            self.selected = 0;
                        }
                        self.update_scroll_offset(self.calculate_visible_rows());
                    } else {
                        // Regular Up: Move one item up
                        if self.selected > 0 {
                            self.selected -= 1;
                            // Update scroll offset with calculated visible rows
                            self.update_scroll_offset(self.calculate_visible_rows());
                        }
                    }
                }
                KeyCode::Down => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        // Ctrl+Down: Page down - move by visible rows minus 1 (to show overlap)
                        let page_size = self.calculate_visible_rows().saturating_sub(1);
                        if self.selected + page_size < total_items {
                            self.selected += page_size;
                        } else {
                            self.selected = total_items.saturating_sub(1);
                        }
                        self.update_scroll_offset(self.calculate_visible_rows());
                    } else {
                        // Regular Down: Move one item down
                        if self.selected + 1 < total_items {
                            self.selected += 1;
                            // Update scroll offset with calculated visible rows
                            self.update_scroll_offset(self.calculate_visible_rows());
                        }
                    }
                }
                KeyCode::Enter => {
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
                    if let Some(entry) = self.entries.get(entry_idx) {
                        if let Ok(ft) = entry.file_type() {
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
                    }
                }
                KeyCode::Esc => {
                    // If footer button is selected, unselect first; otherwise cancel
                    if self.mode == FileBrowserMode::Load && self.open_button_selected {
                        self.open_button_selected = false;
                        return None;
                    }
                    return Some(FileBrowserAction::Cancelled);
                }
                KeyCode::Backspace => {
                    if !self.at_root() {
                        if let Some(parent) = self.current_dir.parent() {
                            self.current_dir = parent.to_path_buf();
                            self.entries = Self::read_dir(&self.current_dir, self.filter_ext.as_ref(), self.folder_only);
                            self.selected = 0;
                            self.scroll_offset = 0; // Reset scroll when changing directories
                        }
                    }
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