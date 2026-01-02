//! Find All Results Dialog Component
//!
//! Displays all search results in a navigable table format with column aggregations.
//! Supports multiple tabs, one for each search pattern.

use crate::services::search_service::FindAllResult;
use crate::tui::components::find_all_tab::FindAllTab;
use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table, Tabs,
    },
    Frame,
};
use std::time::Duration;

/// Focus panel within the dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelFocus {
    /// Focus on results table (left panel)
    Results,
    /// Focus on column counts (right panel)
    ColumnCounts,
}

impl Default for PanelFocus {
    fn default() -> Self {
        Self::Results
    }
}

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

/// Dialog showing all search results with tabbed interface for multiple patterns
pub struct FindAllResultsDialog {
    /// Tabs containing search results
    tabs: Vec<FindAllTab>,

    /// Currently active tab index
    active_tab_index: usize,

    /// Display mode
    display_mode: DisplayMode,

    /// Whether this component has focus
    focused: bool,

    /// Which panel has focus within the dialog
    panel_focus: PanelFocus,

    /// Scroll position for column counts panel
    column_counts_scroll: usize,
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
        let tab = FindAllTab::new(pattern, results);
        Self {
            tabs: vec![tab],
            active_tab_index: 0,
            display_mode,
            focused: false,
            panel_focus: PanelFocus::default(),
            column_counts_scroll: 0,
        }
    }

    /// Add a new tab with search results
    pub fn add_tab(&mut self, pattern: String, results: Vec<FindAllResult>) {
        let tab = FindAllTab::new(pattern, results);
        self.tabs.push(tab);
        // Switch to the newly added tab
        self.active_tab_index = self.tabs.len() - 1;
    }

    /// Add a new tab with elapsed time
    pub fn add_tab_with_time(
        &mut self,
        pattern: String,
        results: Vec<FindAllResult>,
        elapsed_time: Duration,
    ) {
        let tab = FindAllTab::with_elapsed_time(pattern, results, elapsed_time);
        self.tabs.push(tab);
        // Switch to the newly added tab
        self.active_tab_index = self.tabs.len() - 1;
    }

    /// Close the current tab
    pub fn close_current_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            // Closing the last tab should close the dialog
            return false;
        }

        self.tabs.remove(self.active_tab_index);

        // Adjust active tab index
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }

        true
    }

    /// Switch to next tab
    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
        }
    }

    /// Switch to previous tab
    pub fn previous_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
        }
    }

    /// Get the active tab
    fn get_active_tab(&self) -> Option<&FindAllTab> {
        self.tabs.get(self.active_tab_index)
    }

    /// Get the active tab mutably
    fn get_active_tab_mut(&mut self) -> Option<&mut FindAllTab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    /// Get the currently selected result from active tab
    pub fn get_selected(&self) -> Option<&FindAllResult> {
        self.get_active_tab()?.get_selected()
    }

    /// Navigate selection up in active tab
    pub fn select_previous(&mut self) {
        if let Some(tab) = self.get_active_tab_mut() {
            tab.select_previous();
        }
    }

    /// Navigate selection down in active tab
    pub fn select_next(&mut self) {
        if let Some(tab) = self.get_active_tab_mut() {
            tab.select_next();
        }
    }

    /// Get result count from active tab
    pub fn result_count(&self) -> usize {
        self.get_active_tab()
            .map(|tab| tab.result_count())
            .unwrap_or(0)
    }

    /// Set the elapsed time for the active tab
    pub fn set_elapsed_time(&mut self, duration: Duration) {
        if let Some(tab) = self.get_active_tab_mut() {
            tab.set_elapsed_time(duration);
        }
    }

    /// Get the elapsed time for the active tab
    pub fn get_elapsed_time(&self) -> Option<Duration> {
        self.get_active_tab()?.get_elapsed_time()
    }

    /// Page up in active tab
    fn page_up(&mut self) {
        if let Some(tab) = self.get_active_tab_mut() {
            tab.page_up();
        }
    }

    /// Page down in active tab
    fn page_down(&mut self) {
        if let Some(tab) = self.get_active_tab_mut() {
            tab.page_down();
        }
    }

    /// Render tab bar at top
    fn render_tab_bar(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let tab_titles: Vec<String> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(_i, tab)| {
                let truncated_pattern = if tab.pattern.len() > 15 {
                    format!("{}...", &tab.pattern[..12])
                } else {
                    tab.pattern.clone()
                };
                format!(" {} ({}) ", truncated_pattern, tab.result_count())
            })
            .collect();

        let tabs_widget = Tabs::new(tab_titles)
            .select(self.active_tab_index)
            .style(theme.normal_style())
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("|");

        frame.render_widget(tabs_widget, area);
    }

    /// Render the results table on the left side
    fn render_results_table(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Check tab count and panel focus before getting mutable reference
        let has_multiple_tabs = self.tabs.len() > 1;
        let results_focused = self.panel_focus == PanelFocus::Results;

        let Some(tab) = self.get_active_tab_mut() else {
            return;
        };

        // Determine border style based on panel focus
        let border_style = if results_focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        // Create the Results block wrapper
        let results_block = Block::default()
            .title(" Results ")
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.normal_style());

        let results_inner = results_block.inner(area);
        frame.render_widget(results_block, area);

        // Reserve space for help text (1 blank line + 1 help line)
        let help_height = 2;

        // Check if scrollbar is needed
        let scrollbar_needed =
            tab.results.len() > (results_inner.height.saturating_sub(help_height + 1) as usize);
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
        if tab.viewport_top >= tab.results.len() {
            tab.viewport_top = tab.results.len().saturating_sub(1);
        }

        // Calculate visible range of results
        let visible_end = (tab.viewport_top + viewport_height).min(tab.results.len());
        let visible_results = &tab.results[tab.viewport_top..visible_end];

        // Create table rows only for visible results
        let rows: Vec<Row> = visible_results
            .iter()
            .enumerate()
            .map(|(visible_idx, result)| {
                let actual_idx = tab.viewport_top + visible_idx;
                let row_style = if actual_idx == tab.selected_index {
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

        // Use the has_multiple_tabs flag set earlier
        let help_text = if has_multiple_tabs {
            Line::from(vec![
                Span::styled("[←/→]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Tab  "),
                Span::styled("[Tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Panel  "),
                Span::styled("[Enter]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Jump  "),
                Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Close"),
            ])
        } else {
            Line::from(vec![
                Span::styled("[Tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Panel  "),
                Span::styled("[Enter]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Jump  "),
                Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Close"),
            ])
        };

        let help = Paragraph::new(help_text).style(theme.normal_style());
        frame.render_widget(help, help_area);

        // Render scrollbar for results if needed
        if scrollbar_needed {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(tab.results.len()).position(tab.selected_index);

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
        tab.set_viewport_height(viewport_height);
    }

    /// Render the aggregations panel on the right side
    fn render_aggregations(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let Some(tab) = self.get_active_tab() else {
            return;
        };

        // Compute column counts
        let column_counts = tab.compute_column_counts();

        // Sort columns by count (descending) then by name (alphabetically)
        let mut sorted_counts: Vec<_> = column_counts.into_iter().collect();
        sorted_counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        // Determine border style based on panel focus
        let border_style = if self.panel_focus == PanelFocus::ColumnCounts {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        // Create the aggregation block
        let agg_block = Block::default()
            .title(" Column Counts ")
            .borders(Borders::ALL)
            .border_style(border_style)
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

        // Clamp scroll position
        let max_scroll = sorted_counts.len().saturating_sub(available_height);
        self.column_counts_scroll = self.column_counts_scroll.min(max_scroll);

        // Calculate visible range
        let visible_end = (self.column_counts_scroll + available_height).min(sorted_counts.len());
        let visible_counts = &sorted_counts[self.column_counts_scroll..visible_end];

        // Create table rows for visible aggregations only
        let agg_rows: Vec<Row> = visible_counts
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

            let mut scrollbar_state =
                ScrollbarState::new(sorted_counts.len()).position(self.column_counts_scroll);

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
            Action::NextTab => {
                // Tab key toggles between Results and Column Counts panels
                self.panel_focus = match self.panel_focus {
                    PanelFocus::Results => PanelFocus::ColumnCounts,
                    PanelFocus::ColumnCounts => PanelFocus::Results,
                };
                Ok(true)
            }
            Action::MoveUp => {
                match self.panel_focus {
                    PanelFocus::Results => self.select_previous(),
                    PanelFocus::ColumnCounts => {
                        self.column_counts_scroll = self.column_counts_scroll.saturating_sub(1);
                    }
                }
                Ok(true)
            }
            Action::MoveDown => {
                match self.panel_focus {
                    PanelFocus::Results => self.select_next(),
                    PanelFocus::ColumnCounts => {
                        // Scroll down (will be clamped during render)
                        self.column_counts_scroll += 1;
                    }
                }
                Ok(true)
            }
            Action::PageUp => {
                match self.panel_focus {
                    PanelFocus::Results => self.page_up(),
                    PanelFocus::ColumnCounts => {
                        // Page up in column counts
                        self.column_counts_scroll = self.column_counts_scroll.saturating_sub(10);
                    }
                }
                Ok(true)
            }
            Action::PageDown => {
                match self.panel_focus {
                    PanelFocus::Results => self.page_down(),
                    PanelFocus::ColumnCounts => {
                        // Page down in column counts
                        self.column_counts_scroll += 10;
                    }
                }
                Ok(true)
            }
            Action::MoveLeft => {
                // Left/Right for tab switching
                self.previous_tab();
                Ok(true)
            }
            Action::MoveRight => {
                // Left/Right for tab switching
                self.next_tab();
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

        // Get active tab info
        let (title, has_results) = if let Some(tab) = self.get_active_tab() {
            let title_text = if let Some(duration) = tab.get_elapsed_time() {
                let elapsed_ms = duration.as_millis();
                let elapsed_str = if elapsed_ms < 1000 {
                    format!("{}ms", elapsed_ms)
                } else {
                    format!("{:.2}s", duration.as_secs_f64())
                };
                if self.tabs.len() > 1 {
                    format!(
                        " Find All: \"{}\" ({} matches in {}) [Tab {}/{}] ",
                        tab.pattern,
                        tab.result_count(),
                        elapsed_str,
                        self.active_tab_index + 1,
                        self.tabs.len()
                    )
                } else {
                    format!(
                        " Find All Results: \"{}\" ({} matches in {}) ",
                        tab.pattern,
                        tab.result_count(),
                        elapsed_str
                    )
                }
            } else {
                if self.tabs.len() > 1 {
                    format!(
                        " Find All: \"{}\" ({} matches) [Tab {}/{}] ",
                        tab.pattern,
                        tab.result_count(),
                        self.active_tab_index + 1,
                        self.tabs.len()
                    )
                } else {
                    format!(
                        " Find All Results: \"{}\" ({} matches) ",
                        tab.pattern,
                        tab.result_count()
                    )
                }
            };
            (title_text, !tab.results.is_empty())
        } else {
            (" Find All Results ".to_string(), false)
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
        if !has_results {
            let empty_msg = Paragraph::new("No matches found")
                .style(theme.normal_style())
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(empty_msg, inner);
            return;
        }

        // Split area for tab bar and content
        let chunks = if self.tabs.len() > 1 {
            // Reserve space for tab bar
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(inner)
        } else {
            // No tab bar needed for single tab
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(0), Constraint::Min(0)])
                .split(inner)
        };

        // Render tab bar if multiple tabs
        if self.tabs.len() > 1 {
            self.render_tab_bar(frame, chunks[0], &theme);
        }

        let content_area = if self.tabs.len() > 1 {
            chunks[1]
        } else {
            inner
        };

        // Split the content area: 75% for results table, 25% for aggregations
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(content_area);

        let results_area = content_chunks[0];
        let aggregation_area = content_chunks[1];

        // ===== RENDER RESULTS TABLE (LEFT SIDE) =====
        self.render_results_table(frame, results_area, &theme);

        // ===== RENDER AGGREGATIONS (RIGHT SIDE) =====
        self.render_aggregations(frame, aggregation_area, &theme);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::NextTab, // For panel switching
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,  // For tab navigation
            Action::MoveRight, // For tab navigation
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
