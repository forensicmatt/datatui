use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Column width configuration for a dataset
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnWidthConfig {
    /// Auto-expand columns to fit content
    pub auto_expand: bool,

    /// Manual column widths (column name -> width in characters)
    pub manual_widths: HashMap<String, u16>,

    /// Hidden columns (column name -> is hidden)
    pub hidden_columns: HashMap<String, bool>,

    /// Column display order (list of column names)
    pub column_order: Vec<String>,
}

impl Default for ColumnWidthConfig {
    fn default() -> Self {
        Self {
            auto_expand: true,
            manual_widths: HashMap::new(),
            hidden_columns: HashMap::new(),
            column_order: Vec::new(),
        }
    }
}

impl ColumnWidthConfig {
    /// Create a new config from a list of columns
    pub fn from_columns(columns: Vec<String>) -> Self {
        let mut config = Self::default();
        config.column_order = columns.clone();

        // Initialize all columns as visible
        for col in columns {
            config.hidden_columns.insert(col, false);
        }

        config
    }

    /// Check if a column is visible
    pub fn is_column_visible(&self, column: &str) -> bool {
        !self.hidden_columns.get(column).copied().unwrap_or(false)
    }

    /// Get the effective width for a column
    /// Returns Some(width) if manually set, None if auto
    pub fn get_effective_width(&self, column: &str) -> Option<u16> {
        self.manual_widths.get(column).copied()
    }

    /// Get list of visible columns in order
    pub fn get_visible_columns(&self) -> Vec<String> {
        self.column_order
            .iter()
            .filter(|col| self.is_column_visible(col))
            .cloned()
            .collect()
    }

    /// Get list of hidden columns
    pub fn get_hidden_columns(&self) -> Vec<String> {
        self.column_order
            .iter()
            .filter(|col| !self.is_column_visible(col))
            .cloned()
            .collect()
    }

    /// Validate that the config is consistent with a list of columns
    /// Returns true if valid, false otherwise
    pub fn validate(&self, columns: &[String]) -> bool {
        // Check that column_order contains exactly the same columns
        if self.column_order.len() != columns.len() {
            return false;
        }

        for col in columns {
            if !self.column_order.contains(col) {
                return false;
            }
        }

        // Check that at least one column is visible
        if self.get_visible_columns().is_empty() {
            return false;
        }

        // Check that manual widths are in valid range
        for &width in self.manual_widths.values() {
            if !(4..=255).contains(&width) {
                return false;
            }
        }

        true
    }

    /// Clean up config to match current columns
    /// Removes references to non-existent columns
    pub fn clean_for_columns(&mut self, columns: &[String]) {
        // Remove manual widths for non-existent columns
        self.manual_widths.retain(|k, _| columns.contains(k));

        // Remove hidden status for non-existent columns
        self.hidden_columns.retain(|k, _| columns.contains(k));

        // Rebuild column order to match current columns
        let mut new_order = Vec::new();

        // First, add columns that are in the current order
        for col in &self.column_order {
            if columns.contains(col) {
                new_order.push(col.clone());
            }
        }

        // Then add any new columns that weren't in the order
        for col in columns {
            if !new_order.contains(col) {
                new_order.push(col.clone());
                // Initialize new columns as visible
                self.hidden_columns.insert(col.clone(), false);
            }
        }

        self.column_order = new_order;

        // Ensure at least one column is visible
        if self.get_visible_columns().is_empty() && !self.column_order.is_empty() {
            // Make the first column visible
            if let Some(first) = self.column_order.first() {
                self.hidden_columns.insert(first.clone(), false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_columns() {
        let columns = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let config = ColumnWidthConfig::from_columns(columns.clone());

        assert_eq!(config.column_order, columns);
        assert_eq!(config.get_visible_columns(), columns);
        assert!(config.get_hidden_columns().is_empty());
        assert!(config.auto_expand);
    }

    #[test]
    fn test_visibility() {
        let mut config = ColumnWidthConfig::from_columns(vec!["A".to_string(), "B".to_string()]);

        assert!(config.is_column_visible("A"));

        config.hidden_columns.insert("A".to_string(), true);
        assert!(!config.is_column_visible("A"));
        assert!(config.is_column_visible("B"));
    }

    #[test]
    fn test_validate() {
        let columns = vec!["A".to_string(), "B".to_string()];
        let mut config = ColumnWidthConfig::from_columns(columns.clone());

        assert!(config.validate(&columns));

        // Invalid width
        config.manual_widths.insert("A".to_string(), 2);
        assert!(!config.validate(&columns));
        config.manual_widths.insert("A".to_string(), 10);
        assert!(config.validate(&columns));

        // Hide all columns - invalid
        config.hidden_columns.insert("A".to_string(), true);
        config.hidden_columns.insert("B".to_string(), true);
        assert!(!config.validate(&columns));
    }

    #[test]
    fn test_clean_for_columns() {
        let mut config = ColumnWidthConfig::from_columns(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
        ]);
        config.manual_widths.insert("A".to_string(), 10);
        config.manual_widths.insert("C".to_string(), 15);

        // New columns list removes B, adds D
        let new_columns = vec!["A".to_string(), "C".to_string(), "D".to_string()];
        config.clean_for_columns(&new_columns);

        assert_eq!(config.column_order.len(), 3);
        assert!(config.column_order.contains(&"A".to_string()));
        assert!(config.column_order.contains(&"C".to_string()));
        assert!(config.column_order.contains(&"D".to_string()));

        assert!(config.manual_widths.contains_key("A"));
        assert!(config.manual_widths.contains_key("C"));
        assert!(!config.manual_widths.contains_key("B"));
    }
}
