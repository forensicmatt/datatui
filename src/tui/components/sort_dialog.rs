//! SortDialog: Modal dialog for configuring multi-column sorting

use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Margin, Rect},
    style::Color,
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};
use serde::{Deserialize, Serialize};

/// Represents a single sort column with direction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortColumn {
    pub name: String,
    pub ascending: bool,
}

/// Dialog mode: main list or add column
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortDialogMode {
    List,
    AddColumn,
}

/// Result from the sort dialog
#[derive(Debug, Clone)]
pub enum DialogResult {
    ApplySort(Vec<SortColumn>),
    Close,
}

/// SortDialog: UI for configuring sort columns and order
pub struct SortDialog {
    /// All available columns
    columns: Vec<String>,

    /// Current sort configuration
    sort_columns: Vec<SortColumn>,

    /// Selected index in list mode
    active_index: usize,

    /// Current dialog mode
    mode: SortDialogMode,

    /// Selected index in add column mode
    add_column_index: usize,

    /// Scroll offset for list mode
    scroll_offset: usize,

    /// Scroll offset for add column mode
    add_column_scroll_offset: usize,

    /// Hint for which column to highlight in add mode
    current_column: Option<String>,

    /// Show instructions block
    show_instructions: bool,

    /// Pending result ready for pickup
    pending_result: Option<DialogResult>,

    /// Cached max visible rows (calculated during render)
    cached_max_rows: usize,

    /// Whether the dialog has focus
    focused: bool,
}

impl SortDialog {
    /// Create a new SortDialog
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            sort_columns: Vec::new(),
            active_index: 0,
            mode: SortDialogMode::List,
            add_column_index: 0,
            scroll_offset: 0,
            add_column_scroll_offset: 0,
            current_column: None,
            show_instructions: true,
            pending_result: None,
            cached_max_rows: 10,
            focused: true,
        }
    }

    /// Set the current column hint for better UX when opening
    pub fn set_current_column(&mut self, column: Option<String>) {
        self.current_column = column;
    }

    /// Set existing sort columns (when reopening dialog)
    pub fn set_sort_columns(&mut self, columns: Vec<SortColumn>) {
        self.sort_columns = columns;
        if self.active_index >= self.sort_columns.len() && !self.sort_columns.is_empty() {
            self.active_index = self.sort_columns.len() - 1;
        }
    }

    /// Get columns available to add (not already in sort_columns)
    fn available_columns(&self) -> Vec<&String> {
        self.columns
            .iter()
            .filter(|c| !self.sort_columns.iter().any(|sc| &sc.name == *c))
            .collect()
    }

    /// Take the pending result
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }

    /// Adjust scroll position to ensure active item is visible
    fn adjust_scroll(&mut self, max_rows: usize) {
        match self.mode {
            SortDialogMode::List => {
                if !self.sort_columns.is_empty() {
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
                    if self.add_column_index < self.add_column_scroll_offset {
                        self.add_column_scroll_offset = self.add_column_index;
                    } else if self.add_column_index >= self.add_column_scroll_offset + max_rows {
                        self.add_column_scroll_offset = self.add_column_index + 1 - max_rows;
                    }
                }
            }
        }
    }
}

impl Component for SortDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Use cached max rows from last render
        let max_rows = self.cached_max_rows;

        match action {
            Action::Escape => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // Close dialog
            }

            Action::Cancel => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // Close dialog
            }

            Action::Confirm => {
                match self.mode {
                    SortDialogMode::List => {
                        // Close and apply current sort
                        self.pending_result =
                            Some(DialogResult::ApplySort(self.sort_columns.clone()));
                        Ok(false) // Close dialog
                    }
                    SortDialogMode::AddColumn => {
                        // Add selected column and return to list mode
                        let available = self.available_columns();
                        if !available.is_empty() && self.add_column_index < available.len() {
                            let col_name = available[self.add_column_index].clone();
                            self.sort_columns.push(SortColumn {
                                name: col_name,
                                ascending: true,
                            });
                            self.mode = SortDialogMode::List;
                            self.active_index = self.sort_columns.len() - 1;
                            self.adjust_scroll(max_rows);

                            // Apply sort immediately
                            self.pending_result =
                                Some(DialogResult::ApplySort(self.sort_columns.clone()));
                        }
                        Ok(true) // Stay open
                    }
                }
            }

            Action::MoveUp => {
                match self.mode {
                    SortDialogMode::List => {
                        if !self.sort_columns.is_empty() {
                            if self.active_index == 0 {
                                self.active_index = self.sort_columns.len() - 1;
                            } else {
                                self.active_index -= 1;
                            }
                            self.adjust_scroll(max_rows);
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
                            self.adjust_scroll(max_rows);
                        }
                    }
                }
                Ok(true) // Stay open
            }

            Action::MoveDown => {
                match self.mode {
                    SortDialogMode::List => {
                        if !self.sort_columns.is_empty() {
                            self.active_index = (self.active_index + 1) % self.sort_columns.len();
                            self.adjust_scroll(max_rows);
                        }
                    }
                    SortDialogMode::AddColumn => {
                        let available = self.available_columns();
                        if !available.is_empty() {
                            self.add_column_index = (self.add_column_index + 1) % available.len();
                            self.adjust_scroll(max_rows);
                        }
                    }
                }
                Ok(true) // Stay open
            }

            Action::ToggleHelp => {
                self.show_instructions = !self.show_instructions;
                Ok(true) // Stay open
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
                Ok(true) // Stay open
            }

            Action::RemoveSortColumn => {
                if self.mode == SortDialogMode::List
                    && !self.sort_columns.is_empty()
                    && self.active_index < self.sort_columns.len()
                {
                    self.sort_columns.remove(self.active_index);

                    if self.sort_columns.is_empty() {
                        self.active_index = 0;
                        self.scroll_offset = 0;
                    } else if self.active_index >= self.sort_columns.len() {
                        self.active_index = self.sort_columns.len() - 1;
                    }

                    self.adjust_scroll(max_rows);

                    // Apply sort immediately
                    self.pending_result = Some(DialogResult::ApplySort(self.sort_columns.clone()));
                }
                Ok(true) // Stay open
            }

            Action::ToggleSortDirection => {
                if self.mode == SortDialogMode::List && self.active_index < self.sort_columns.len()
                {
                    if let Some(col) = self.sort_columns.get_mut(self.active_index) {
                        col.ascending = !col.ascending;

                        // Apply sort immediately
                        self.pending_result =
                            Some(DialogResult::ApplySort(self.sort_columns.clone()));
                    }
                }
                Ok(true) // Stay open
            }

            _ => Ok(true), // Ignore other actions, stay open
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Clear the background
        frame.render_widget(Clear, area);

        // Outer block with title
        let outer_block = Block::default()
            .title("Sort")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            });
        let outer_inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Calculate layout for content and instructions
        let (content_area, instructions_area) = if self.show_instructions {
            let instruction_height = 6;
            let content_height = outer_inner.height.saturating_sub(instruction_height + 1);

            (
                Rect {
                    y: outer_inner.y,
                    height: content_height,
                    ..outer_inner
                },
                Some(Rect {
                    y: outer_inner.y + content_height,
                    height: instruction_height,
                    ..outer_inner
                }),
            )
        } else {
            (outer_inner, None)
        };

        // Content block
        let content_block = Block::default()
            .title(match self.mode {
                SortDialogMode::List => "Sort Columns",
                SortDialogMode::AddColumn => "Add Column",
            })
            .borders(Borders::ALL);
        let inner = content_block.inner(content_area);
        frame.render_widget(content_block, content_area);

        // Calculate max visible rows and cache it for scroll adjustments
        let max_rows = inner.height.saturating_sub(0) as usize;
        self.cached_max_rows = max_rows;
        let list_inner = inner.inner(Margin {
            vertical: 0,
            horizontal: 1,
        });

        match self.mode {
            SortDialogMode::List => {
                if self.sort_columns.is_empty() {
                    let empty_msg =
                        Paragraph::new("No sort columns selected.\nPress 'a' to add a column.")
                            .style(theme.normal_style().fg(Color::DarkGray));
                    frame.render_widget(empty_msg, list_inner);
                } else {
                    let end = (self.scroll_offset + max_rows).min(self.sort_columns.len());

                    for (vis_idx, i) in (self.scroll_offset..end).enumerate() {
                        let col = &self.sort_columns[i];
                        let selected = i == self.active_index;
                        let zebra = i % 2 == 0;

                        let dir = if col.ascending { "↑" } else { "↓" };
                        let text = if selected {
                            format!("> {}  {}", col.name, dir)
                        } else {
                            format!("  {}  {}", col.name, dir)
                        };

                        let style = if selected {
                            theme.selected_style()
                        } else if zebra {
                            theme.alt_row_style()
                        } else {
                            theme.normal_style()
                        };

                        let y = list_inner.y + vis_idx as u16;
                        let para = Paragraph::new(text).style(style);
                        frame.render_widget(
                            para,
                            Rect {
                                x: list_inner.x,
                                y,
                                width: list_inner.width,
                                height: 1,
                            },
                        );
                    }

                    // Render scrollbar if needed
                    if self.sort_columns.len() > max_rows {
                        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                            .style(theme.focused_border_style());
                        let mut state = ScrollbarState::new(self.sort_columns.len())
                            .position(self.scroll_offset)
                            .viewport_content_length(max_rows);
                        frame.render_stateful_widget(scrollbar, list_inner, &mut state);
                    }
                }
            }
            SortDialogMode::AddColumn => {
                let available = self.available_columns();

                if available.is_empty() {
                    let empty_msg = Paragraph::new("No columns available to add.")
                        .style(theme.normal_style().fg(Color::DarkGray));
                    frame.render_widget(empty_msg, list_inner);
                } else {
                    let end = (self.add_column_scroll_offset + max_rows).min(available.len());

                    for (vis_idx, i) in (self.add_column_scroll_offset..end).enumerate() {
                        let col = available[i];
                        let selected = i == self.add_column_index;
                        let zebra = i % 2 == 0;

                        let style = if selected {
                            theme.selected_style()
                        } else if zebra {
                            theme.alt_row_style()
                        } else {
                            theme.normal_style()
                        };

                        let y = list_inner.y + vis_idx as u16;
                        let para = Paragraph::new(col.as_str()).style(style);
                        frame.render_widget(
                            para,
                            Rect {
                                x: list_inner.x,
                                y,
                                width: list_inner.width,
                                height: 1,
                            },
                        );
                    }

                    // Render scrollbar if needed
                    if available.len() > max_rows {
                        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                            .style(theme.focused_border_style());
                        let mut state = ScrollbarState::new(available.len())
                            .position(self.add_column_scroll_offset)
                            .viewport_content_length(max_rows);
                        frame.render_stateful_widget(scrollbar, list_inner, &mut state);
                    }
                }
            }
        }

        // Render instructions if enabled
        if self.show_instructions {
            if let Some(inst_area) = instructions_area {
                let instructions_text = match self.mode {
                    SortDialogMode::List => vec![
                        "  • Enter: Close and apply sort",
                        "  • a: Add column to sort",
                        "  • d: Remove selected column",
                        "  • t: Toggle sort direction (asc/desc)",
                        "  • Ctrl+i: Toggle this help",
                    ],
                    SortDialogMode::AddColumn => vec![
                        "  • Enter: Add selected column",
                        "  • Esc: Return to sort list",
                        "  • ↑/↓: Navigate columns",
                        "  • Ctrl+i: Toggle this help",
                    ],
                };

                let instructions = Paragraph::new(instructions_text.join("\n"))
                    .block(
                        Block::default()
                            .borders(Borders::TOP)
                            .title("Instructions (Ctrl+i to hide)"),
                    )
                    .style(theme.warning_style())
                    .wrap(Wrap { trim: true });

                frame.render_widget(instructions, inst_area);
            }
        } else {
            // Show minimal hint at bottom
            let hint_area = Rect {
                x: outer_inner.x,
                y: outer_inner.bottom().saturating_sub(1),
                width: outer_inner.width,
                height: 1,
            };
            let hint = Paragraph::new("Press Ctrl+i for help")
                .style(theme.normal_style().fg(Color::DarkGray));
            frame.render_widget(hint, hint_area);
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Escape,
            Action::Cancel,
            Action::Confirm,
            Action::MoveUp,
            Action::MoveDown,
            Action::ToggleHelp,
            Action::AddSortColumn,
            Action::RemoveSortColumn,
            Action::ToggleSortDirection,
        ]
    }

    fn name(&self) -> &str {
        "SortDialog"
    }
}


impl Focusable for SortDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
