//! Find All Results Dialog Component
//!
//! Displays all search results in a navigable table format with column aggregations.

use crate::services::search_service::FindAllResult;
use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table,
    },
    Frame,
};
use std::collections::HashMap;

/// Display mode for Find All Results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Display as a panel below the DataTable (default, like Notepad++)
    Panel,
    /// Display as a centered overlay dialog
    Overlay,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Panel
    }
}

/// Dialog showing all search results
pub struct FindAllResultsDialog {
    results: Vec<FindAllResult>,
    selected_index: usize,
    pattern: String,
    display_mode: DisplayMode,
    viewport_top: usize,                       // First visible result index
    viewport_height: usize,                    // Number of visible rows
    focused: bool,                             // Whether this component has focus
    elapsed_time: Option<std::time::Duration>, // Time taken for search
}

impl FindAllResultsDialog {
    /// Create a new Find All Results dialog with Panel display mode
    pub fn new(results: Vec<FindAllResult>, pattern: String) -> Self {
        Self::with_mode(results, pattern, DisplayMode::default())
    }

    /// Create a new Find All Results dialog with specified display mode
    pub fn with_mode(
        results: Vec<FindAllResult>,
        pattern: String,
        display_mode: DisplayMode,
    ) -> Self {
        Self {
            results,
            selected_index: 0,
            pattern,
            display_mode,
            viewport_top: 0,
            viewport_height: 10, // Default, will be updated during render
            focused: false,
            elapsed_time: None,
        }
    }

    /// Get the currently selected result
    pub fn get_selected(&self) -> Option<&FindAllResult> {
        self.results.get(self.selected_index)
    }

    /// Navigate selection up
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_selection_visible();
        }
    }

    /// Navigate selection down
    pub fn select_next(&mut self) {
        if self.selected_index + 1 < self.results.len() {
            self.selected_index += 1;
            self.ensure_selection_visible();
        }
    }

    /// Get result count
    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    /// Set the elapsed time for the search
    pub fn set_elapsed_time(&mut self, duration: std::time::Duration) {
        self.elapsed_time = Some(duration);
    }

    /// Get the elapsed time for the search
    pub fn get_elapsed_time(&self) -> Option<std::time::Duration> {
        self.elapsed_time
    }

    /// Ensure selected item is within viewport
    fn ensure_selection_visible(&mut self) {
        if self.selected_index < self.viewport_top {
            self.viewport_top = self.selected_index;
        } else if self.selected_index >= self.viewport_top + self.viewport_height {
            self.viewport_top = self.selected_index.saturating_sub(self.viewport_height - 1);
        }
    }

    /// Page up
    fn page_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(self.viewport_height);
        self.ensure_selection_visible();
    }

    /// Page down
    fn page_down(&mut self) {
        if self.results.len() > 0 {
            self.selected_index =
                (self.selected_index + self.viewport_height).min(self.results.len() - 1);
            self.ensure_selection_visible();
        }
    }

    /// Compute counts of matches grouped by column
    fn compute_column_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for result in &self.results {
            *counts.entry(result.column.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Render the results table on the left side
    fn render_results_table(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Create the Results block wrapper
        let results_block = Block::default()
            .title(" Results ")
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .style(theme.normal_style());

        let results_inner = results_block.inner(area);
        frame.render_widget(results_block, area);

        // Reserve space for help text (1 blank line + 1 help line)
        let help_height = 2;

        // Check if scrollbar is needed
        let scrollbar_needed =
            self.results.len() > (results_inner.height.saturating_sub(help_height + 1) as usize);
        let scrollbar_width = if scrollbar_needed { 1 } else { 0 };

        let table_area = Rect {
            x: results_inner.x,
            y: results_inner.y,
            width: results_inner.width.saturating_sub(scrollbar_width),
            height: results_inner.height.saturating_sub(help_height),
        };

        // Calculate viewport height (subtract 1 for header)
        let viewport_height = table_area.height.saturating_sub(1) as usize;

        // Ensure viewport_top is valid
        if self.viewport_top >= self.results.len() {
            self.viewport_top = self.results.len().saturating_sub(1);
        }

        // Calculate visible range of results
        let visible_end = (self.viewport_top + viewport_height).min(self.results.len());
        let visible_results = &self.results[self.viewport_top..visible_end];

        // Create table rows only for visible results
        let rows: Vec<Row> = visible_results
            .iter()
            .enumerate()
            .map(|(visible_idx, result)| {
                let actual_idx = self.viewport_top + visible_idx;
                let row_style = if actual_idx == self.selected_index {
                    theme.selected_style()
                } else {
                    Style::default()
                };

                // Truncate context to fit
                let max_context_len = (results_inner.width as usize).saturating_sub(30);
                let context = if result.context.len() > max_context_len {
                    format!(
                        "{}...",
                        &result.context[..max_context_len.saturating_sub(3)]
                    )
                } else {
                    result.context.clone()
                };

                Row::new(vec![
                    format!("{}", result.row),
                    result.column.clone(),
                    context,
                ])
                .style(row_style)
            })
            .collect();

        // Create table
        let table = Table::new(
            rows,
            [
                ratatui::layout::Constraint::Length(6),  // Row#
                ratatui::layout::Constraint::Length(15), // Column
                ratatui::layout::Constraint::Min(20),    // Context
            ],
        )
        .header(Row::new(vec!["Row", "Column", "Context"]).style(theme.header_style()))
        .style(theme.normal_style());

        frame.render_widget(table, table_area);

        // Render help text at bottom of results_inner area
        let help_y = results_inner.y + results_inner.height.saturating_sub(1);
        let help_area = Rect {
            x: results_inner.x,
            y: help_y,
            width: results_inner.width,
            height: 1,
        };

        let help_text = Line::from(vec![
            Span::styled("[Enter]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Jump  "),
            Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Close"),
        ]);

        let help = Paragraph::new(help_text).style(theme.normal_style());
        frame.render_widget(help, help_area);

        // Render scrollbar for results if needed
        if scrollbar_needed {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(self.results.len()).position(self.selected_index);

            // Scrollbar area is on the right edge of the results block (on the border)
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1, // Skip top border
                width: 1,
                height: area.height.saturating_sub(2), // Skip top and bottom borders
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }

        // Update viewport_height for next navigation
        self.viewport_height = viewport_height;
    }

    /// Render the aggregations panel on the right side
    fn render_aggregations(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Compute column counts
        let column_counts = self.compute_column_counts();

        // Sort columns by count (descending) then by name (alphabetically)
        let mut sorted_counts: Vec<_> = column_counts.into_iter().collect();
        sorted_counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        // Create the aggregation block
        let agg_block = Block::default()
            .title(" Column Counts ")
            .borders(Borders::ALL)
            .border_style(theme.border_style())
            .style(theme.normal_style());

        let agg_inner = agg_block.inner(area);
        frame.render_widget(agg_block, area);

        // Calculate available height for the table (subtract 1 for header)
        let available_height = agg_inner.height.saturating_sub(1) as usize;

        // Check if scrollbar is needed
        let scrollbar_needed = sorted_counts.len() > available_height;
        let scrollbar_width = if scrollbar_needed { 1 } else { 0 };

        let table_area = Rect {
            x: agg_inner.x,
            y: agg_inner.y,
            width: agg_inner.width.saturating_sub(scrollbar_width),
            height: agg_inner.height,
        };

        // For now, show all rows (in future, implement viewport scrolling if needed)
        // Create table rows for the aggregations
        let agg_rows: Vec<Row> = sorted_counts
            .iter()
            .map(|(column, count)| Row::new(vec![column.clone(), format!("{}", count)]))
            .collect();

        // Create aggregation table
        let agg_table = Table::new(
            agg_rows,
            [
                ratatui::layout::Constraint::Percentage(70), // Column name
                ratatui::layout::Constraint::Percentage(30), // Count
            ],
        )
        .header(Row::new(vec!["Column", "Count"]).style(theme.header_style()))
        .style(theme.normal_style());

        frame.render_widget(agg_table, table_area);

        // Render scrollbar if there are too many columns
        if scrollbar_needed {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(sorted_counts.len()).position(0);

            // Scrollbar area is on the right edge of the aggregation block (on the border)
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1, // Skip top border
                width: 1,
                height: area.height.saturating_sub(2), // Skip top and bottom borders
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }
}

impl Component for FindAllResultsDialog {
    fn name(&self) -> &str {
        "FindAllResultsDialog"
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::MoveUp => {
                self.select_previous();
                Ok(true)
            }
            Action::MoveDown => {
                self.select_next();
                Ok(true)
            }
            Action::PageUp => {
                self.page_up();
                Ok(true)
            }
            Action::PageDown => {
                self.page_down();
                Ok(true)
            }
            Action::Cancel => Ok(false), // Close dialog
            Action::Confirm => Ok(true), // Keep dialog open (App will handle jumping)
            _ => Ok(true),               // Consume all other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Use default theme - App will need to handle theme properly
        let theme = Theme::default();

        // Determine dialog area based on display mode
        let dialog_area = match self.display_mode {
            DisplayMode::Panel => {
                // Use the provided area directly (panel below DataTable)
                area
            }
            DisplayMode::Overlay => {
                // Center dialog on screen
                let width = area.width.saturating_sub(10).clamp(60, 100);
                let height = area.height.saturating_sub(6).clamp(15, 30);
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + (area.height.saturating_sub(height)) / 2;
                Rect {
                    x,
                    y,
                    width,
                    height,
                }
            }
        };

        // Clear background only for overlay mode
        if self.display_mode == DisplayMode::Overlay {
            frame.render_widget(Clear, dialog_area);
        }

        // Create title
        let title = if let Some(duration) = self.elapsed_time {
            let elapsed_ms = duration.as_millis();
            let elapsed_str = if elapsed_ms < 1000 {
                format!("{}ms", elapsed_ms)
            } else {
                format!("{:.2}s", duration.as_secs_f64())
            };
            format!(
                " Find All Results: \"{}\" ({} matches in {}) ",
                self.pattern,
                self.results.len(),
                elapsed_str
            )
        } else {
            format!(
                " Find All Results: \"{}\" ({} matches) ",
                self.pattern,
                self.results.len()
            )
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            })
            .style(theme.normal_style());

        let inner = block.inner(dialog_area);

        // Render block
        frame.render_widget(block, dialog_area);

        // Handle empty results
        if self.results.is_empty() {
            let empty_msg = Paragraph::new("No matches found")
                .style(theme.normal_style())
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_msg, inner);
            return;
        }

        // Split the inner area: 75% for results table, 25% for aggregations
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(inner);

        let results_area = chunks[0];
        let aggregation_area = chunks[1];

        // ===== RENDER RESULTS TABLE (LEFT SIDE) =====
        self.render_results_table(frame, results_area, &theme);

        // ===== RENDER AGGREGATIONS (RIGHT SIDE) =====
        self.render_aggregations(frame, aggregation_area, &theme);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::MoveUp,
            Action::MoveDown,
            Action::PageUp,
            Action::PageDown,
            Action::Confirm,
            Action::Cancel,
        ]
    }
}

impl crate::tui::Focusable for FindAllResultsDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
