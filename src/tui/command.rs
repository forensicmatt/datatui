//! Command Module
//!
//! Handles parsing and execution of command bar commands.

use crate::tui::components::{FindDialog, SortColumn};
use crate::tui::Action;
use color_eyre::Result;

/// Represents a parsed command
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Quit the application
    Quit,
    /// Perform a find operation with a pattern
    Find { pattern: String },
    /// Perform a sort operation on one or more columns
    Sort { columns: Vec<SortColumn> },
    /// Open a dialog
    Dialog { dialog_type: DialogType },
    /// Show help
    Help,
    /// Navigate to a specific row and optional column
    GotoRow { row: usize, column: Option<usize> },
    /// Manage columns (set visibility, hide, width)
    Columns(ColumnsCommand),
}

/// Subcommands for the columns command
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnsCommand {
    Set(Vec<(String, Option<u16>)>),
    Hide(Vec<String>),
    Width(String, Option<u16>), // None = auto
}

/// Types of dialogs that can be opened
#[derive(Debug, Clone, PartialEq)]
pub enum DialogType {
    Sort,
    Find,
}

impl Command {
    /// Get all available command names
    fn all_commands() -> &'static [&'static str] {
        &[
            "quit", "q", "find", "sort", "dialog", "help", "goto", "columns",
        ]
    }

    /// Find similar commands using simple string distance
    fn find_similar_commands(input: &str) -> Vec<&'static str> {
        let mut matches: Vec<(&str, usize)> = Self::all_commands()
            .iter()
            .filter_map(|&cmd| {
                let distance = levenshtein_distance(input, cmd);
                // Only suggest if distance is small relative to command length
                if distance <= 2 {
                    Some((cmd, distance))
                } else {
                    None
                }
            })
            .collect();

        // Sort by distance (closest first)
        matches.sort_by_key(|(_, dist)| *dist);

        // Return up to 3 suggestions
        matches.into_iter().take(3).map(|(cmd, _)| cmd).collect()
    }

    /// Parse a command string into a Command
    pub fn parse(input: &str) -> Result<Self, String> {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();

        if parts.is_empty() {
            return Err("Empty command".to_string());
        }

        let cmd = parts[0];

        match cmd {
            "q" | "quit" => Ok(Command::Quit),

            "find" => {
                // find command now requires a pattern
                if parts.len() < 2 {
                    return Err("Usage: find <pattern>".to_string());
                }
                let pattern = parts[1..].join(" ");
                Ok(Command::Find { pattern })
            }

            "sort" => {
                // sort command now requires at least one column name
                // Format: column1 [desc], column2 [asc], ...
                if parts.len() < 2 {
                    return Err("Usage: sort <column> [desc] [, <column> [desc]]...".to_string());
                }

                // Reconstruct the full arguments string to handle comma splitting
                let args_str = parts[1..].join(" ");
                let col_args: Vec<&str> = args_str.split(',').collect();

                let mut sort_columns = Vec::new();

                for col_arg in col_args {
                    let col_parts: Vec<&str> = col_arg.trim().split_whitespace().collect();
                    if col_parts.is_empty() {
                        continue;
                    }

                    let name = col_parts[0].to_string();
                    let ascending = if col_parts.len() > 1 {
                        !col_parts[1].eq_ignore_ascii_case("desc")
                    } else {
                        true // Default to ascending
                    };

                    sort_columns.push(SortColumn { name, ascending });
                }

                if sort_columns.is_empty() {
                    return Err("No valid columns specified for sort".to_string());
                }

                Ok(Command::Sort {
                    columns: sort_columns,
                })
            }

            "dialog" => {
                // dialog command requires a dialog type
                if parts.len() < 2 {
                    return Err("Usage: dialog <sort|find>".to_string());
                }
                let dialog_type = match parts[1] {
                    "sort" => DialogType::Sort,
                    "find" => DialogType::Find,
                    other => {
                        return Err(format!(
                            "Unknown dialog type '{}'. Available: sort, find",
                            other
                        ))
                    }
                };
                Ok(Command::Dialog { dialog_type })
            }

            "help" => Ok(Command::Help),

            "columns" => {
                if parts.len() < 2 {
                    return Err("Usage: columns <set|hide|width> ...".to_string());
                }
                let subcommand = parts[1];
                let args_str = parts[2..].join(" ");

                match subcommand {
                    "set" => {
                        if parts.len() < 3 {
                            return Err(
                                "Usage: columns set <col> [width] [, <col> [width]...]".to_string()
                            );
                        }
                        // Re-join and split by comma to handle multiple columns
                        let input = parts[2..].join(" ");
                        let col_entries: Vec<&str> = input.split(',').collect();
                        let mut columns = Vec::new();

                        for entry in col_entries {
                            let entry_parts: Vec<&str> = entry.trim().split_whitespace().collect();
                            if entry_parts.is_empty() {
                                continue;
                            }
                            let name = entry_parts[0].to_string();
                            let width = if entry_parts.len() > 1 {
                                entry_parts[1].parse::<u16>().ok()
                            } else {
                                None
                            };
                            columns.push((name, width));
                        }
                        if columns.is_empty() {
                            return Err("No columns specified".to_string());
                        }
                        Ok(Command::Columns(ColumnsCommand::Set(columns)))
                    }
                    "hide" => {
                        if parts.len() < 3 {
                            return Err("Usage: columns hide <col> [, <col>...]".to_string());
                        }
                        let input = parts[2..].join(" ");
                        let cols: Vec<String> = input
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        if cols.is_empty() {
                            return Err("No columns specified".to_string());
                        }
                        Ok(Command::Columns(ColumnsCommand::Hide(cols)))
                    }
                    "width" => {
                        if parts.len() < 4 {
                            return Err("Usage: columns width <col> <width|auto>".to_string());
                        }
                        let col = parts[2].to_string();
                        let width_arg = parts[3];
                        let width = if width_arg == "auto" {
                            None
                        } else {
                            match width_arg.parse::<u16>() {
                                Ok(w) => Some(w),
                                Err(_) => {
                                    return Err("Width must be a number or 'auto'".to_string())
                                }
                            }
                        };
                        Ok(Command::Columns(ColumnsCommand::Width(col, width)))
                    }
                    _ => Err(format!("Unknown columns subcommand: {}", subcommand)),
                }
            }

            "goto" => {
                // Check if subcommand is provided
                if parts.len() < 2 {
                    return Err("Missing subcommand for 'goto'. Usage: goto row <row_index> [<column_index>]".to_string());
                }

                let subcommand = parts[1];
                match subcommand {
                    "row" => {
                        // Parse: goto row <row_index> [<column_index>]
                        if parts.len() < 3 {
                            return Err(
                                "Missing row index. Usage: goto row <row_index> [<column_index>]"
                                    .to_string(),
                            );
                        }

                        // Parse row index
                        let row = parts[2].parse::<usize>().map_err(|_| {
                            format!("Invalid row index: '{}'. Expected a number.", parts[2])
                        })?;

                        // Parse optional column index
                        let column = if parts.len() > 3 {
                            Some(parts[3].parse::<usize>().map_err(|_| {
                                format!("Invalid column index: '{}'. Expected a number.", parts[3])
                            })?)
                        } else {
                            None
                        };

                        Ok(Command::GotoRow { row, column })
                    }
                    _ => {
                        // Unknown subcommand for goto
                        Err(format!(
                            "Unknown subcommand 'goto {}'. Available: 'goto row <index>'",
                            subcommand
                        ))
                    }
                }
            }

            _ => {
                // Unknown command - try to suggest similar commands
                let suggestions = Self::find_similar_commands(cmd);
                if suggestions.is_empty() {
                    Err(format!(
                        "Unknown command: '{}'. Type ':help' for available commands.",
                        cmd
                    ))
                } else {
                    Err(format!(
                        "Unknown command: '{}'. Did you mean: {}?",
                        cmd,
                        suggestions.join(", ")
                    ))
                }
            }
        }
    }

    /// Get a description of this command
    pub fn description(&self) -> &'static str {
        match self {
            Command::Quit => "Quit the application",
            Command::Find { .. } => "Perform a find operation",
            Command::Sort { .. } => "Sort data by columns",
            Command::Dialog { .. } => "Open a dialog",
            Command::Help => "Show command help",
            Command::GotoRow { .. } => "Navigate to a specific row and column",
            Command::Columns(_) => "Manage columns (set, hide, width)",
        }
    }
}

/// Calculate Levenshtein distance between two strings
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}

/// Command execution context
///
/// This provides the necessary context for executing commands without
/// exposing the entire App structure.
pub struct CommandContext<'a> {
    pub should_quit: &'a mut bool,
    pub find_dialog: &'a mut Option<FindDialog>,
    pub sort_dialog: &'a mut Option<crate::tui::components::SortDialog>,
    pub data_table: &'a mut Option<crate::tui::components::DataTable>,
}

impl Command {
    /// Execute this command with the given context
    pub fn execute(&self, ctx: &mut CommandContext) -> Result<(), String> {
        match self {
            Command::Quit => {
                *ctx.should_quit = true;
                Ok(())
            }

            Command::Find { pattern } => {
                // Perform find operation - open find dialog with the pattern
                let mut dialog = FindDialog::new();
                dialog.search_pattern = pattern.clone();
                dialog.search_pattern_cursor = pattern.len();
                *ctx.find_dialog = Some(dialog);
                Ok(())
            }

            Command::Sort { columns } => {
                // Perform sort operation on the data table
                if let Some(table) = ctx.data_table {
                    // Apply sort order to dataset
                    if let Err(e) = table.dataset_mut().set_sort_order(columns.clone()) {
                        return Err(format!("Failed to set sort order: {}", e));
                    }
                    // Refresh layout to reflect changes
                    if let Err(e) = table.refresh_layout() {
                        return Err(format!("Failed to refresh table: {}", e));
                    }
                    Ok(())
                } else {
                    Err("No data table available".to_string())
                }
            }

            Command::Dialog { dialog_type } => {
                // Open the specified dialog
                match dialog_type {
                    DialogType::Sort => {
                        // Get columns from data table to pass to SortDialog
                        if let Some(table) = ctx.data_table {
                            let columns = table.get_all_columns();
                            *ctx.sort_dialog =
                                Some(crate::tui::components::SortDialog::new(columns));
                            Ok(())
                        } else {
                            Err("No data table available".to_string())
                        }
                    }
                    DialogType::Find => {
                        *ctx.find_dialog = Some(FindDialog::new());
                        Ok(())
                    }
                }
            }

            Command::Help => {
                // Will be handled specially in app.rs
                Ok(())
            }

            Command::GotoRow { row, column } => {
                if let Some(table) = ctx.data_table {
                    let columns = table.get_all_columns();

                    // Determine target column
                    let column_name = if let Some(col_idx) = column {
                        if *col_idx >= columns.len() {
                            return Err(format!(
                                "Column index {} out of range (max: {})",
                                col_idx,
                                columns.len() - 1
                            ));
                        }
                        columns[*col_idx].clone()
                    } else {
                        // Use current column
                        let (_, current_col_idx) = table.get_cursor_position();
                        columns
                            .get(current_col_idx)
                            .cloned()
                            .unwrap_or_else(|| columns[0].clone())
                    };

                    // Navigate to the cell
                    table
                        .goto_cell(*row, &column_name)
                        .map_err(|e| format!("Navigation failed: {}", e))?;
                }
                Ok(())
            }

            Command::Columns(cmd) => {
                let table = ctx.data_table.as_mut().ok_or("No active data table")?;
                match cmd {
                    ColumnsCommand::Set(columns) => {
                        let available_cols =
                            table.dataset().column_names().map_err(|e| e.to_string())?;

                        // Create set of columns to be visible
                        let visible_set: std::collections::HashSet<String> =
                            columns.iter().map(|(n, _)| n.clone()).collect();

                        // Validate existence
                        for name in &visible_set {
                            if !available_cols.contains(name) {
                                return Err(format!("Column '{}' does not exist", name));
                            }
                        }

                        // Apply visibility
                        // Note: We iterate available cols to ensure we handle all of them
                        for col in &available_cols {
                            let should_be_visible = visible_set.contains(col);
                            if let Err(e) = table
                                .dataset_mut()
                                .set_column_visible(col, should_be_visible)
                            {
                                return Err(format!(
                                    "Failed to update visibility for '{}': {}",
                                    col, e
                                ));
                            }
                        }

                        // Apply widths
                        for (name, width) in columns {
                            if let Err(e) = table.dataset_mut().set_column_width(name, *width) {
                                return Err(format!("Failed to set width for '{}': {}", name, e));
                            }
                        }
                    }
                    ColumnsCommand::Hide(columns) => {
                        for col in columns {
                            let curr_vis = table.dataset().is_column_visible(col);
                            if let Err(e) = table.dataset_mut().set_column_visible(col, !curr_vis) {
                                return Err(format!(
                                    "Failed to toggle visibility for '{}': {}",
                                    col, e
                                ));
                            }
                        }
                    }
                    ColumnsCommand::Width(col, width) => {
                        if let Err(e) = table.dataset_mut().set_column_width(col, *width) {
                            return Err(format!("Failed to set width for '{}': {}", col, e));
                        }
                    }
                }
                table.refresh_layout().map_err(|e| format!("{}", e))?;
                Ok(())
            }
        }
    }

    /// Check if this command needs to trigger an Action
    pub fn requires_action(&self) -> Option<Action> {
        match self {
            // Dialog opening commands will trigger action to open the dialog
            Command::Dialog { dialog_type } => match dialog_type {
                DialogType::Sort => Some(Action::Sort),
                DialogType::Find => None, // Handled directly in execute
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quit() {
        assert_eq!(Command::parse("quit").unwrap(), Command::Quit);
        assert_eq!(Command::parse("q").unwrap(), Command::Quit);
    }

    #[test]
    fn test_parse_find() {
        // "find" without pattern is now an error or different behavior
        let result = Command::parse("find");
        assert!(result.is_err()); // or checks specifically for usage error

        assert_eq!(
            Command::parse("find test").unwrap(),
            Command::Find {
                pattern: "test".to_string()
            }
        );
        assert_eq!(
            Command::parse("find test pattern").unwrap(),
            Command::Find {
                pattern: "test pattern".to_string()
            }
        );
    }

    #[test]
    fn test_parse_goto() {
        assert_eq!(
            Command::parse("goto row 10").unwrap(),
            Command::GotoRow {
                row: 10,
                column: None
            }
        );
        assert_eq!(
            Command::parse("goto row 5 2").unwrap(),
            Command::GotoRow {
                row: 5,
                column: Some(2)
            }
        );
    }

    #[test]
    fn test_unknown_command() {
        let result = Command::parse("unknown");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown command"));
        assert!(err.contains("unknown"));
    }

    #[test]
    fn test_unknown_command_with_suggestions() {
        let result = Command::parse("quut");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Did you mean"));
        assert!(err.contains("quit"));
    }

    #[test]
    fn test_goto_missing_subcommand() {
        let result = Command::parse("goto");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Missing subcommand"));
        assert!(err.contains("Usage"));
    }

    #[test]
    fn test_goto_unknown_subcommand() {
        let result = Command::parse("goto column 5");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown subcommand"));
        assert!(err.contains("goto column"));
    }

    #[test]
    fn test_goto_missing_row_index() {
        let result = Command::parse("goto row");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Missing row index"));
    }

    #[test]
    fn test_goto_invalid_row_index() {
        let result = Command::parse("goto row abc");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid row index"));
        assert!(err.contains("abc"));
        assert!(err.contains("Expected a number"));
    }

    #[test]
    fn test_goto_invalid_column_index() {
        let result = Command::parse("goto row 5 xyz");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid column index"));
        assert!(err.contains("xyz"));
        assert!(err.contains("Expected a number"));
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("quit", "quit"), 0);
        assert_eq!(levenshtein_distance("quit", "quut"), 1);
        assert_eq!(levenshtein_distance("quit", "qit"), 1);
        assert_eq!(levenshtein_distance("sort", "srot"), 2);
    }

    #[test]
    fn test_empty_command() {
        let result = Command::parse("   ");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, "Empty command");
    }

    #[test]
    fn test_parse_sort_multi() {
        // Single column
        let cmd = Command::parse("sort name").unwrap();
        if let Command::Sort { columns } = cmd {
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "name");
            assert!(columns[0].ascending);
        } else {
            panic!("Expected Sort command");
        }

        // Single column desc
        let cmd = Command::parse("sort age desc").unwrap();
        if let Command::Sort { columns } = cmd {
            assert_eq!(columns.len(), 1);
            assert_eq!(columns[0].name, "age");
            assert!(!columns[0].ascending);
        } else {
            panic!("Expected Sort command");
        }

        // Multi column mixed
        let cmd = Command::parse("sort age desc, name, date asc").unwrap();
        if let Command::Sort { columns } = cmd {
            assert_eq!(columns.len(), 3);
            assert_eq!(columns[0].name, "age");
            assert!(!columns[0].ascending);
            assert_eq!(columns[1].name, "name");
            assert!(columns[1].ascending);
            assert_eq!(columns[2].name, "date");
            assert!(columns[2].ascending);
        } else {
            panic!("Expected Sort command");
        }
    }
    #[test]
    fn test_parse_columns() {
        // Test Set
        let cmd = Command::parse("columns set col1, col2 10").unwrap();
        if let Command::Columns(ColumnsCommand::Set(cols)) = cmd {
            assert_eq!(cols.len(), 2);
            assert_eq!(cols[0].0, "col1");
            assert_eq!(cols[0].1, None);
            assert_eq!(cols[1].0, "col2");
            assert_eq!(cols[1].1, Some(10));
        } else {
            panic!("Expected Columns Set");
        }

        // Test Hide
        let cmd = Command::parse("columns hide col1, col2").unwrap();
        if let Command::Columns(ColumnsCommand::Hide(cols)) = cmd {
            assert_eq!(cols.len(), 2);
            assert_eq!(cols[0], "col1");
            assert_eq!(cols[1], "col2");
        } else {
            panic!("Expected Columns Hide");
        }

        // Test Width
        let cmd = Command::parse("columns width col1 20").unwrap();
        if let Command::Columns(ColumnsCommand::Width(col, width)) = cmd {
            assert_eq!(col, "col1");
            assert_eq!(width, Some(20));
        } else {
            panic!("Expected Columns Width");
        }

        // Test Width Auto
        let cmd = Command::parse("columns width col1 auto").unwrap();
        if let Command::Columns(ColumnsCommand::Width(col, width)) = cmd {
            assert_eq!(col, "col1");
            assert_eq!(width, None);
        } else {
            panic!("Expected Columns Width Auto");
        }
    }
}
