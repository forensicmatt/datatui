//! Find All Tab - Individual search result tab
//!
//! Encapsulates the state of a single search pattern's results.

use crate::services::search_service::FindAllResult;
use std::collections::HashMap;
use std::time::Duration;

/// Individual tab in the FindAllResultsDialog
#[derive(Debug, Clone)]
pub struct FindAllTab {
    /// Search pattern for this tab
    pub pattern: String,

    /// Search results
    pub results: Vec<FindAllResult>,

    /// Currently selected result index
    pub selected_index: usize,

    /// First visible result index (for scrolling)
    pub viewport_top: usize,

    /// Number of visible rows (updated during render)
    pub viewport_height: usize,

    /// Time taken for this search
    pub elapsed_time: Option<Duration>,
}

impl FindAllTab {
    /// Create a new tab from search results
    pub fn new(pattern: String, results: Vec<FindAllResult>) -> Self {
        Self {
            pattern,
            results,
            selected_index: 0,
            viewport_top: 0,
            viewport_height: 10, // Default, will be updated during render
            elapsed_time: None,
        }
    }

    /// Create a new tab with elapsed time
    pub fn with_elapsed_time(
        pattern: String,
        results: Vec<FindAllResult>,
        elapsed_time: Duration,
    ) -> Self {
        Self {
            pattern,
            results,
            selected_index: 0,
            viewport_top: 0,
            viewport_height: 10,
            elapsed_time: Some(elapsed_time),
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

    /// Ensure selected item is within viewport
    pub fn ensure_selection_visible(&mut self) {
        if self.selected_index < self.viewport_top {
            self.viewport_top = self.selected_index;
        } else if self.selected_index >= self.viewport_top + self.viewport_height {
            self.viewport_top = self.selected_index.saturating_sub(self.viewport_height - 1);
        }
    }

    /// Page up
    pub fn page_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(self.viewport_height);
        self.ensure_selection_visible();
    }

    /// Page down
    pub fn page_down(&mut self) {
        if self.results.len() > 0 {
            self.selected_index =
                (self.selected_index + self.viewport_height).min(self.results.len() - 1);
            self.ensure_selection_visible();
        }
    }

    /// Compute counts of matches grouped by column
    pub fn compute_column_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for result in &self.results {
            *counts.entry(result.column.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Set elapsed time
    pub fn set_elapsed_time(&mut self, duration: Duration) {
        self.elapsed_time = Some(duration);
    }

    /// Get elapsed time
    pub fn get_elapsed_time(&self) -> Option<Duration> {
        self.elapsed_time
    }

    /// Update viewport height (called during render)
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_results(count: usize) -> Vec<FindAllResult> {
        (0..count)
            .map(|i| FindAllResult {
                row: i,
                column: format!("col{}", i % 3),
                context: format!("test result {}", i),
            })
            .collect()
    }

    #[test]
    fn test_tab_creation() {
        let results = create_test_results(10);
        let tab = FindAllTab::new("test".to_string(), results);

        assert_eq!(tab.pattern, "test");
        assert_eq!(tab.result_count(), 10);
        assert_eq!(tab.selected_index, 0);
        assert_eq!(tab.viewport_top, 0);
    }

    #[test]
    fn test_navigation() {
        let results = create_test_results(5);
        let mut tab = FindAllTab::new("test".to_string(), results);

        tab.select_next();
        assert_eq!(tab.selected_index, 1);

        tab.select_next();
        assert_eq!(tab.selected_index, 2);

        tab.select_previous();
        assert_eq!(tab.selected_index, 1);

        // Test boundary
        tab.selected_index = 0;
        tab.select_previous();
        assert_eq!(tab.selected_index, 0);

        tab.selected_index = 4;
        tab.select_next();
        assert_eq!(tab.selected_index, 4);
    }

    #[test]
    fn test_column_counts() {
        let results = create_test_results(10);
        let tab = FindAllTab::new("test".to_string(), results);

        let counts = tab.compute_column_counts();

        // 10 results distributed across col0, col1, col2
        // indices 0,3,6,9 -> col0 (4 items)
        // indices 1,4,7 -> col1 (3 items)
        // indices 2,5,8 -> col2 (3 items)
        assert_eq!(counts.get("col0"), Some(&4));
        assert_eq!(counts.get("col1"), Some(&3));
        assert_eq!(counts.get("col2"), Some(&3));
    }

    #[test]
    fn test_get_selected() {
        let results = create_test_results(5);
        let mut tab = FindAllTab::new("test".to_string(), results);

        let selected = tab.get_selected().unwrap();
        assert_eq!(selected.row, 0);

        tab.select_next();
        let selected = tab.get_selected().unwrap();
        assert_eq!(selected.row, 1);
    }

    #[test]
    fn test_elapsed_time() {
        let results = create_test_results(5);
        let duration = Duration::from_millis(123);
        let tab = FindAllTab::with_elapsed_time("test".to_string(), results, duration);

        assert_eq!(tab.get_elapsed_time(), Some(duration));
    }
}
