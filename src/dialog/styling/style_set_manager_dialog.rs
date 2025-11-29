//! StyleSetManagerDialog: Dialog for managing style sets with tree/table split view
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::style::StyleConfig;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set_manager::StyleSetManager;
use crate::dialog::styling::style_set::StyleSet;
use crate::dialog::styling::style_set_editor_dialog::StyleSetEditorDialog;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use tracing::error;
use std::collections::{BTreeMap, BTreeSet};

/// Focus panel in the dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSetManagerFocus {
    CategoryTree,
    StyleSetTable,
    SearchInput,
}

/// Category tree node
#[derive(Debug, Clone)]
pub struct CategoryNode {
    pub name: String,
    pub full_path: String,
    pub children: Vec<CategoryNode>,
    pub is_expanded: bool,
}

/// Dialog mode
#[derive(Debug)]
pub enum StyleSetManagerDialogMode {
    List,
    FileBrowser(Box<FileBrowserDialog>),
    StyleSetEditor(Box<StyleSetEditorDialog>),
}

/// StyleSetManagerDialog: UI for managing style sets with tree/table split view
#[derive(Debug)]
pub struct StyleSetManagerDialog {
    pub style_set_manager: StyleSetManager,
    pub mode: StyleSetManagerDialogMode,
    pub focus: StyleSetManagerFocus,
    pub selected_table_index: usize,
    pub table_scroll_offset: usize,
    pub selected_category_path: Option<String>,
    pub category_tree: Vec<CategoryNode>,
    pub selected_tree_index: usize,
    pub tree_scroll_offset: usize,
    pub search_filter: String,
    pub search_cursor_position: usize,
    pub show_category_panel: bool,
    pub show_instructions: bool,
    pub config: Config,
    pub style: StyleConfig,
    pub export_selected_id: Option<String>,
    pub editor_style_set_id: Option<String>,
    pub columns: Vec<String>,
}

impl StyleSetManagerDialog {
    /// Create a new StyleSetManagerDialog
    pub fn new(style_set_manager: StyleSetManager) -> Self {
        let mut dialog = Self {
            style_set_manager,
            mode: StyleSetManagerDialogMode::List,
            focus: StyleSetManagerFocus::StyleSetTable,
            selected_table_index: 0,
            table_scroll_offset: 0,
            selected_category_path: None,
            category_tree: vec![],
            selected_tree_index: 0,
            tree_scroll_offset: 0,
            search_filter: String::new(),
            search_cursor_position: 0,
            show_category_panel: true,
            show_instructions: true,
            config: Config::default(),
            style: StyleConfig::default(),
            export_selected_id: None,
            editor_style_set_id: None,
            columns: vec![],
        };
        dialog.rebuild_category_tree();
        dialog
    }

    /// Set the available columns for filter expressions
    pub fn set_columns(&mut self, columns: Vec<String>) {
        self.columns = columns;
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
        self.rebuild_category_tree();
    }

    /// Rebuild the category tree from current style sets
    fn rebuild_category_tree(&mut self) {
        let mut categories: BTreeSet<String> = BTreeSet::new();
        
        // Collect all categories from all style sets
        for (_, style_set, _) in self.style_set_manager.get_all_sets() {
            if let Some(ref cats) = style_set.categories {
                for cat in cats {
                    categories.insert(cat.clone());
                    // Also add parent categories
                    let parts: Vec<&str> = cat.split('/').collect();
                    for i in 1..parts.len() {
                        categories.insert(parts[..i].join("/"));
                    }
                }
            }
        }

        // Build tree structure
        let mut root_nodes: BTreeMap<String, CategoryNode> = BTreeMap::new();
        
        for cat in &categories {
            let parts: Vec<&str> = cat.split('/').collect();
            if parts.len() == 1 {
                // Root level category
                if !root_nodes.contains_key(parts[0]) {
                    root_nodes.insert(parts[0].to_string(), CategoryNode {
                        name: parts[0].to_string(),
                        full_path: parts[0].to_string(),
                        children: vec![],
                        is_expanded: true,
                    });
                }
            }
        }

        // Add children to root nodes
        for cat in &categories {
            let parts: Vec<&str> = cat.split('/').collect();
            if parts.len() > 1 {
                if let Some(root) = root_nodes.get_mut(parts[0]) {
                    Self::add_child_to_node(root, &parts[1..], cat);
                }
            }
        }

        self.category_tree = root_nodes.into_values().collect();
    }

    /// Add a child category to a node
    fn add_child_to_node(node: &mut CategoryNode, remaining_parts: &[&str], full_path: &str) {
        if remaining_parts.is_empty() {
            return;
        }

        let child_name = remaining_parts[0];
        
        // Find or create child
        let child_idx = node.children.iter().position(|c| c.name == child_name);
        
        if let Some(idx) = child_idx {
            if remaining_parts.len() > 1 {
                Self::add_child_to_node(&mut node.children[idx], &remaining_parts[1..], full_path);
            }
        } else {
            let child_full_path = format!("{}/{}", node.full_path, child_name);
            let mut new_child = CategoryNode {
                name: child_name.to_string(),
                full_path: child_full_path,
                children: vec![],
                is_expanded: true,
            };
            if remaining_parts.len() > 1 {
                Self::add_child_to_node(&mut new_child, &remaining_parts[1..], full_path);
            }
            node.children.push(new_child);
        }
    }

    /// Get flattened list of visible tree nodes for rendering
    fn get_visible_tree_nodes(&self) -> Vec<(usize, &str, &str, bool)> {
        let mut result = vec![];
        result.push((0, "All", "", true)); // "All" node at the top
        
        fn collect_nodes<'a>(nodes: &'a [CategoryNode], depth: usize, result: &mut Vec<(usize, &'a str, &'a str, bool)>) {
            for node in nodes {
                result.push((depth, &node.name, &node.full_path, node.is_expanded));
                if node.is_expanded {
                    collect_nodes(&node.children, depth + 1, result);
                }
            }
        }
        
        collect_nodes(&self.category_tree, 1, &mut result);
        result
    }

    /// Get filtered style sets based on search and category selection
    fn get_filtered_sets(&self) -> Vec<(String, StyleSet, bool)> {
        let all_sets = self.style_set_manager.get_all_sets();
        
        all_sets.into_iter()
            .filter(|(id, set, _)| {
                // Filter by search
                if !self.search_filter.is_empty() {
                    let search_lower = self.search_filter.to_lowercase();
                    let matches_search = 
                        id.to_lowercase().contains(&search_lower) ||
                        set.name.to_lowercase().contains(&search_lower) ||
                        set.description.to_lowercase().contains(&search_lower) ||
                        set.tags.as_ref().map(|t| t.iter().any(|tag| tag.to_lowercase().contains(&search_lower))).unwrap_or(false);
                    if !matches_search {
                        return false;
                    }
                }
                
                // Filter by category
                if let Some(ref selected_cat) = self.selected_category_path {
                    if !selected_cat.is_empty() {
                        match &set.categories {
                            Some(cats) => {
                                let matches_cat = cats.iter().any(|cat| {
                                    cat == selected_cat || cat.starts_with(&format!("{}/", selected_cat))
                                });
                                if !matches_cat {
                                    return false;
                                }
                            }
                            None => return false,
                        }
                    }
                }
                
                true
            })
            .map(|(id, set, enabled)| (id.clone(), set.clone(), enabled))
            .collect()
    }

    /// Get stats string
    fn get_stats_string(&self) -> String {
        let all_sets = self.style_set_manager.get_all_sets();
        let enabled_count = all_sets.iter().filter(|(_, _, enabled)| *enabled).count();
        let total_count = all_sets.len();
        format!("{}/{} style sets enabled", enabled_count, total_count)
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match &self.mode {
            StyleSetManagerDialogMode::List => {
                let focus_hint = match self.focus {
                    StyleSetManagerFocus::CategoryTree => "Categories",
                    StyleSetManagerFocus::StyleSetTable => "Table",
                    StyleSetManagerFocus::SearchInput => "Search (type to filter)",
                };
                format!(
                    "[{}]  {}",
                    focus_hint,
                    self.config.actions_to_instructions(&[
                        (Mode::Global, Action::Escape),
                        (Mode::Global, Action::Enter),
                        (Mode::StyleSetManagerDialog, Action::EditStyleSet),
                        (Mode::StyleSetManagerDialog, Action::AddStyleSet),
                        (Mode::StyleSetManagerDialog, Action::RemoveStyleSet),
                        (Mode::StyleSetManagerDialog, Action::DisableStyleSet),
                        (Mode::StyleSetManagerDialog, Action::ImportStyleSet),
                        (Mode::StyleSetManagerDialog, Action::ExportStyleSet),
                        (Mode::StyleSetManagerDialog, Action::ToggleCategoryPanel),
                        (Mode::Global, Action::ToggleInstructions),
                    ])
                )
            }
            StyleSetManagerDialogMode::FileBrowser(_) => {
                "File Browser - Enter: Select  Esc: Cancel".to_string()
            }
            StyleSetManagerDialogMode::StyleSetEditor(_) => {
                "Style Set Editor".to_string()
            }
        }
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) -> usize {
        // Handle sub-dialog modes
        match &self.mode {
            StyleSetManagerDialogMode::FileBrowser(browser) => {
                browser.render(area, buf);
                return 0;
            }
            StyleSetManagerDialogMode::StyleSetEditor(editor) => {
                editor.render(area, buf);
                return 0;
            }
            StyleSetManagerDialogMode::List => {}
        }

        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Set Manager")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(
            inner_area,
            self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) },
        );
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        // Stats toolbar at top
        let stats = self.get_stats_string();
        let stats_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: content_area.width,
            height: 1,
        };
        buf.set_string(stats_area.x, stats_area.y, &stats, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        // Search bar
        let search_y = content_area.y + 1;
        let search_label = "Search: ";
        buf.set_string(content_area.x, search_y, search_label, Style::default().fg(Color::Gray));
        
        let search_input_x = content_area.x + search_label.len() as u16;
        let search_style = if self.focus == StyleSetManagerFocus::SearchInput {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        buf.set_string(search_input_x, search_y, &self.search_filter, search_style);
        
        // Render cursor if focused on search
        if self.focus == StyleSetManagerFocus::SearchInput {
            let cursor_x = search_input_x + self.search_cursor_position as u16;
            let cursor_char = self.search_filter.chars().nth(self.search_cursor_position).unwrap_or(' ');
            let cursor_style = self.config.style_config.cursor.block();
            buf.set_string(cursor_x, search_y, cursor_char.to_string(), cursor_style);
        }

        // Main content area starts after stats and search
        let main_y = content_area.y + 3;
        let main_height = content_area.height.saturating_sub(3);

        if self.show_category_panel {
            // Split view: category tree on left (30%), table on right (70%)
            let left_width = content_area.width * 30 / 100;
            let right_width = content_area.width.saturating_sub(left_width).saturating_sub(1);

            let left_area = Rect {
                x: content_area.x,
                y: main_y,
                width: left_width,
                height: main_height,
            };

            let right_area = Rect {
                x: content_area.x + left_width + 1,
                y: main_y,
                width: right_width,
                height: main_height,
            };

            self.render_category_tree(left_area, buf);
            self.render_style_set_table(right_area, buf);
        } else {
            // Full width table
            let table_area = Rect {
                x: content_area.x,
                y: main_y,
                width: content_area.width,
                height: main_height,
            };
            self.render_style_set_table(table_area, buf);
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

    /// Render the category tree
    fn render_category_tree(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.focus == StyleSetManagerFocus::CategoryTree;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title("Categories")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let visible_nodes = self.get_visible_tree_nodes();
        let max_visible = inner.height as usize;
        let end = (self.tree_scroll_offset + max_visible).min(visible_nodes.len());

        for (vis_idx, i) in (self.tree_scroll_offset..end).enumerate() {
            let (depth, name, full_path, is_expanded) = &visible_nodes[i];
            let y = inner.y + vis_idx as u16;
            
            let is_selected = if full_path.is_empty() {
                self.selected_category_path.is_none()
            } else {
                self.selected_category_path.as_ref() == Some(&full_path.to_string())
            };

            let is_tree_selected = is_focused && i == self.selected_tree_index;

            let style = if is_tree_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let indent = "  ".repeat(*depth);
            let expand_marker = if i == 0 {
                "" // "All" node has no expand marker
            } else if visible_nodes.iter().skip(i + 1).any(|(_, _, p, _)| p.starts_with(&format!("{}/", full_path))) {
                if *is_expanded { "▼ " } else { "► " }
            } else {
                "  "
            };

            let line = format!("{}{}{}", indent, expand_marker, name);
            buf.set_string(inner.x, y, &line, style);
        }
    }

    /// Render the style set table
    fn render_style_set_table(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.focus == StyleSetManagerFocus::StyleSetTable;
        let border_style = if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title("Style Sets")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        let filtered_sets = self.get_filtered_sets();
        let max_visible = inner.height as usize;

        if filtered_sets.is_empty() {
            buf.set_string(inner.x, inner.y, "No style sets found", Style::default().fg(Color::DarkGray));
            return;
        }

        // Calculate scroll offset
        let selected_idx = self.selected_table_index.min(filtered_sets.len().saturating_sub(1));
        let scroll_offset = if selected_idx < self.table_scroll_offset {
            selected_idx
        } else if selected_idx >= self.table_scroll_offset + max_visible {
            selected_idx.saturating_sub(max_visible - 1)
        } else {
            self.table_scroll_offset
        };

        let end = (scroll_offset + max_visible).min(filtered_sets.len());

        // Header
        let header_style = Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD);
        let header = format!("{:^3} {:20} {:30} {:15} {:6}", "", "Name", "Description", "Categories", "Rules");
        let header_truncated = if header.len() > inner.width as usize {
            header.chars().take(inner.width as usize).collect()
        } else {
            header
        };
        buf.set_string(inner.x, inner.y, &header_truncated, header_style);

        for (vis_idx, i) in (scroll_offset..end).enumerate() {
            let y = inner.y + 1 + vis_idx as u16;
            let (_id, set, enabled) = &filtered_sets[i];
            
            let is_selected = is_focused && i == selected_idx;

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if i % 2 == 0 {
                Style::default().bg(Color::Rgb(30, 30, 30))
            } else {
                Style::default()
            };

            let status = if *enabled { "[✓]" } else { "[ ]" };
            let cats = set.categories.as_ref()
                .map(|c| c.join(", "))
                .unwrap_or_default();
            let rules_count = set.rules.len();

            // Format row with truncation
            let name_width = 20;
            let desc_width = 30;
            let cat_width = 15;

            let name_display = if set.name.len() > name_width {
                format!("{}...", &set.name[..name_width - 3])
            } else {
                format!("{:width$}", set.name, width = name_width)
            };

            let desc_display = if set.description.len() > desc_width {
                format!("{}...", &set.description[..desc_width - 3])
            } else {
                format!("{:width$}", set.description, width = desc_width)
            };

            let cat_display = if cats.len() > cat_width {
                format!("{}...", &cats[..cat_width - 3])
            } else {
                format!("{:width$}", cats, width = cat_width)
            };

            let line = format!("{} {} {} {} {:6}", status, name_display, desc_display, cat_display, rules_count);
            let line_truncated = if line.len() > inner.width as usize {
                line.chars().take(inner.width as usize).collect()
            } else {
                line
            };
            
            buf.set_string(inner.x, y, &line_truncated, style);
        }
    }

    /// Handle a key event (public for external use)
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Handle StyleSetEditor mode first
        if let StyleSetManagerDialogMode::StyleSetEditor(editor) = &mut self.mode {
            if let Some(action) = editor.handle_key_event_pub(key) {
                match action {
                    Action::CloseStyleSetEditorDialog => {
                        self.mode = StyleSetManagerDialogMode::List;
                        self.editor_style_set_id = None;
                        return None;
                    }
                    Action::StyleSetEditorDialogApplied(style_set) => {
                        // Update or add the style set
                        if let Some(ref old_id) = self.editor_style_set_id {
                            let was_enabled = self.style_set_manager.is_enabled(old_id);
                            self.style_set_manager.remove_set(old_id);
                            let new_id = self.style_set_manager.add_set(style_set);
                            if was_enabled {
                                self.style_set_manager.enable_style_set(&new_id);
                            }
                        } else {
                            let new_id = self.style_set_manager.add_set(style_set);
                            self.style_set_manager.enable_style_set(&new_id);
                        }
                        self.rebuild_category_tree();
                        self.mode = StyleSetManagerDialogMode::List;
                        self.editor_style_set_id = None;
                        return None;
                    }
                    _ => {}
                }
            }
            return None;
        }

        // Handle FileBrowser mode
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
                                // Import style set from file/folder
                                if path.is_file() {
                                    if let Err(e) = self.style_set_manager.load_from_file(&path) {
                                        error!("Failed to import style set: {}", e);
                                    }
                                } else if path.is_dir() {
                                    if let Err(e) = self.style_set_manager.load_from_folder(&path) {
                                        error!("Failed to load style sets from folder: {}", e);
                                    }
                                }
                                self.rebuild_category_tree();
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

        // Handle Global actions first
        if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
            match global_action {
                Action::Escape => {
                    return Some(Action::CloseStyleSetManagerDialog);
                }
                Action::Enter => {
                    match self.focus {
                        StyleSetManagerFocus::CategoryTree => {
                            // Select category
                            let visible_nodes = self.get_visible_tree_nodes();
                            if let Some((_, _, full_path, _)) = visible_nodes.get(self.selected_tree_index) {
                                if full_path.is_empty() {
                                    self.selected_category_path = None;
                                } else {
                                    self.selected_category_path = Some(full_path.to_string());
                                }
                            }
                            self.selected_table_index = 0;
                            self.table_scroll_offset = 0;
                        }
                        StyleSetManagerFocus::StyleSetTable => {
                            // Toggle enabled state of selected style set
                            let filtered_sets = self.get_filtered_sets();
                            if let Some((id, _, enabled)) = filtered_sets.get(self.selected_table_index) {
                                if *enabled {
                                    self.style_set_manager.disable_style_set(id);
                                } else {
                                    self.style_set_manager.enable_style_set(id);
                                }
                            }
                        }
                        StyleSetManagerFocus::SearchInput => {
                            // Switch focus to table
                            self.focus = StyleSetManagerFocus::StyleSetTable;
                        }
                    }
                    return None;
                }
                Action::Up => {
                    match self.focus {
                        StyleSetManagerFocus::CategoryTree => {
                            if self.selected_tree_index > 0 {
                                self.selected_tree_index -= 1;
                                if self.selected_tree_index < self.tree_scroll_offset {
                                    self.tree_scroll_offset = self.selected_tree_index;
                                }
                            }
                        }
                        StyleSetManagerFocus::StyleSetTable => {
                            if self.selected_table_index > 0 {
                                self.selected_table_index -= 1;
                            }
                        }
                        StyleSetManagerFocus::SearchInput => {
                            // Do nothing for search
                        }
                    }
                    return None;
                }
                Action::Down => {
                    match self.focus {
                        StyleSetManagerFocus::CategoryTree => {
                            let visible_nodes = self.get_visible_tree_nodes();
                            if self.selected_tree_index < visible_nodes.len().saturating_sub(1) {
                                self.selected_tree_index += 1;
                            }
                        }
                        StyleSetManagerFocus::StyleSetTable => {
                            let filtered_sets = self.get_filtered_sets();
                            if self.selected_table_index < filtered_sets.len().saturating_sub(1) {
                                self.selected_table_index += 1;
                            }
                        }
                        StyleSetManagerFocus::SearchInput => {
                            // Do nothing for search
                        }
                    }
                    return None;
                }
                Action::Left => {
                    if self.focus == StyleSetManagerFocus::SearchInput && self.search_cursor_position > 0 {
                        self.search_cursor_position -= 1;
                    }
                    return None;
                }
                Action::Right => {
                    if self.focus == StyleSetManagerFocus::SearchInput {
                        if self.search_cursor_position < self.search_filter.chars().count() {
                            self.search_cursor_position += 1;
                        }
                    }
                    return None;
                }
                Action::Tab => {
                    // Cycle focus
                    self.focus = match self.focus {
                        StyleSetManagerFocus::SearchInput => {
                            if self.show_category_panel {
                                StyleSetManagerFocus::CategoryTree
                            } else {
                                StyleSetManagerFocus::StyleSetTable
                            }
                        }
                        StyleSetManagerFocus::CategoryTree => StyleSetManagerFocus::StyleSetTable,
                        StyleSetManagerFocus::StyleSetTable => StyleSetManagerFocus::SearchInput,
                    };
                    return None;
                }
                Action::Backspace => {
                    if self.focus == StyleSetManagerFocus::SearchInput && self.search_cursor_position > 0 {
                        let chars: Vec<char> = self.search_filter.chars().collect();
                        self.search_filter = chars[..self.search_cursor_position - 1]
                            .iter()
                            .chain(chars[self.search_cursor_position..].iter())
                            .collect();
                        self.search_cursor_position -= 1;
                        self.selected_table_index = 0;
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
                Action::ToggleCategoryPanel => {
                    self.show_category_panel = !self.show_category_panel;
                    if !self.show_category_panel && self.focus == StyleSetManagerFocus::CategoryTree {
                        self.focus = StyleSetManagerFocus::StyleSetTable;
                    }
                    return None;
                }
                Action::FocusCategoryTree => {
                    if self.show_category_panel {
                        self.focus = StyleSetManagerFocus::CategoryTree;
                    }
                    return None;
                }
                Action::FocusStyleSetTable => {
                    self.focus = StyleSetManagerFocus::StyleSetTable;
                    return None;
                }
                Action::EditStyleSet => {
                    let filtered_sets = self.get_filtered_sets();
                    if let Some((id, set, _)) = filtered_sets.get(self.selected_table_index) {
                        let mut editor = StyleSetEditorDialog::new(set.clone(), self.columns.clone());
                        let _ = editor.register_config_handler(self.config.clone());
                        self.editor_style_set_id = Some(id.clone());
                        self.mode = StyleSetManagerDialogMode::StyleSetEditor(Box::new(editor));
                    }
                    return None;
                }
                Action::AddStyleSet => {
                    let mut editor = StyleSetEditorDialog::new_empty(self.columns.clone());
                    let _ = editor.register_config_handler(self.config.clone());
                    self.editor_style_set_id = None; // New style set
                    self.mode = StyleSetManagerDialogMode::StyleSetEditor(Box::new(editor));
                    return None;
                }
                Action::RemoveStyleSet => {
                    let filtered_sets = self.get_filtered_sets();
                    if let Some((id, _, _)) = filtered_sets.get(self.selected_table_index) {
                        self.style_set_manager.remove_set(id);
                        self.rebuild_category_tree();
                        if self.selected_table_index >= filtered_sets.len().saturating_sub(1) && self.selected_table_index > 0 {
                            self.selected_table_index -= 1;
                        }
                    }
                    return None;
                }
                Action::DisableStyleSet => {
                    let filtered_sets = self.get_filtered_sets();
                    if let Some((id, _, enabled)) = filtered_sets.get(self.selected_table_index) {
                        if *enabled {
                            self.style_set_manager.disable_style_set(id);
                        } else {
                            self.style_set_manager.enable_style_set(id);
                        }
                    }
                    return None;
                }
                Action::ImportStyleSet => {
                    let mut browser = FileBrowserDialog::new(
                        None,
                        Some(vec!["yaml", "yml"]),
                        false,
                        FileBrowserMode::Load,
                    );
                    browser.register_config_handler(self.config.clone());
                    self.mode = StyleSetManagerDialogMode::FileBrowser(Box::new(browser));
                    return None;
                }
                Action::ExportStyleSet => {
                    let filtered_sets = self.get_filtered_sets();
                    if let Some((id, _, _)) = filtered_sets.get(self.selected_table_index) {
                        self.export_selected_id = Some(id.clone());
                        let mut browser = FileBrowserDialog::new(
                            None,
                            Some(vec!["yaml", "yml"]),
                            false,
                            FileBrowserMode::Save,
                        );
                        browser.register_config_handler(self.config.clone());
                        self.mode = StyleSetManagerDialogMode::FileBrowser(Box::new(browser));
                    }
                    return None;
                }
                _ => {}
            }
        }

        // Handle character input for search
        if self.focus == StyleSetManagerFocus::SearchInput {
            if let KeyCode::Char(c) = key.code {
                let chars: Vec<char> = self.search_filter.chars().collect();
                let before: String = chars[..self.search_cursor_position].iter().collect();
                let after: String = chars[self.search_cursor_position..].iter().collect();
                self.search_filter = format!("{}{}{}", before, c, after);
                self.search_cursor_position += 1;
                self.selected_table_index = 0;
                return None;
            }
            if key.code == KeyCode::Delete {
                let chars: Vec<char> = self.search_filter.chars().collect();
                if self.search_cursor_position < chars.len() {
                    self.search_filter = chars[..self.search_cursor_position]
                        .iter()
                        .chain(chars[self.search_cursor_position + 1..].iter())
                        .collect();
                    self.selected_table_index = 0;
                }
                return None;
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
