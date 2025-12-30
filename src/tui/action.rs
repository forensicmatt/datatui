use serde::{Deserialize, Serialize};
use std::fmt;

/// All possible actions in the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Action {
    // Navigation
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    Home,
    End,
    GoToTop,
    GoToBottom,

    // Data Operations
    Sort,
    Filter,
    Find,
    Query,

    // View
    ToggleHelp,
    Refresh,

    // Tab Management
    NextTab,
    PrevTab,
    CloseTab,
    NewTab,

    // File Operations
    Import,
    Export,

    // Application
    Quit,
    Confirm,
    Cancel,

    // Clipboard
    Copy,
    CopyWithHeaders,

    // Column Operations
    ResizeColumn,
    HideColumn,
    ShowAllColumns,
}

impl Action {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Action::MoveUp => "Move cursor up",
            Action::MoveDown => "Move cursor down",
            Action::MoveLeft => "Move cursor left",
            Action::MoveRight => "Move cursor right",
            Action::PageUp => "Page up",
            Action::PageDown => "Page down",
            Action::Home => "Go to start of row",
            Action::End => "Go to end of row",
            Action::GoToTop => "Go to first row",
            Action::GoToBottom => "Go to last row",
            Action::Sort => "Sort column",
            Action::Filter => "Filter data",
            Action::Find => "Find in data",
            Action::Query => "SQL query",
            Action::ToggleHelp => "Toggle help screen",
            Action::Refresh => "Refresh current view",
            Action::NextTab => "Next tab",
            Action::PrevTab => "Previous tab",
            Action::CloseTab => "Close current tab",
            Action::NewTab => "New tab",
            Action::Import => "Import data",
            Action::Export => "Export data",
            Action::Quit => "Quit application",
            Action::Confirm => "Confirm action",
            Action::Cancel => "Cancel action",
            Action::Copy => "Copy cell",
            Action::CopyWithHeaders => "Copy with headers",
            Action::ResizeColumn => "Resize column",
            Action::HideColumn => "Hide column",
            Action::ShowAllColumns => "Show all columns",
        }
    }

    /// Get category for grouping in help screen
    pub fn category(&self) -> ActionCategory {
        match self {
            Action::MoveUp
            | Action::MoveDown
            | Action::MoveLeft
            | Action::MoveRight
            | Action::PageUp
            | Action::PageDown
            | Action::Home
            | Action::End
            | Action::GoToTop
            | Action::GoToBottom => ActionCategory::Navigation,

            Action::Sort | Action::Filter | Action::Find | Action::Query => ActionCategory::DataOps,

            Action::ToggleHelp | Action::Refresh => ActionCategory::View,

            Action::NextTab | Action::PrevTab | Action::CloseTab | Action::NewTab => {
                ActionCategory::Tabs
            }

            Action::Import | Action::Export => ActionCategory::FileOps,

            Action::Quit | Action::Confirm | Action::Cancel => ActionCategory::Application,

            Action::Copy | Action::CopyWithHeaders => ActionCategory::Clipboard,

            Action::ResizeColumn | Action::HideColumn | Action::ShowAllColumns => {
                ActionCategory::Columns
            }
        }
    }

    /// Get all possible actions (for validation)
    pub fn all() -> Vec<Action> {
        vec![
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::PageUp,
            Action::PageDown,
            Action::Home,
            Action::End,
            Action::GoToTop,
            Action::GoToBottom,
            Action::Sort,
            Action::Filter,
            Action::Find,
            Action::Query,
            Action::ToggleHelp,
            Action::Refresh,
            Action::NextTab,
            Action::PrevTab,
            Action::CloseTab,
            Action::NewTab,
            Action::Import,
            Action::Export,
            Action::Quit,
            Action::Confirm,
            Action::Cancel,
            Action::Copy,
            Action::CopyWithHeaders,
            Action::ResizeColumn,
            Action::HideColumn,
            Action::ShowAllColumns,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionCategory {
    Navigation,
    DataOps,
    View,
    Tabs,
    FileOps,
    Application,
    Clipboard,
    Columns,
}

impl fmt::Display for ActionCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionCategory::Navigation => write!(f, "Navigation"),
            ActionCategory::DataOps => write!(f, "Data Operations"),
            ActionCategory::View => write!(f, "View"),
            ActionCategory::Tabs => write!(f, "Tabs"),
            ActionCategory::FileOps => write!(f, "File Operations"),
            ActionCategory::Application => write!(f, "Application"),
            ActionCategory::Clipboard => write!(f, "Clipboard"),
            ActionCategory::Columns => write!(f, "Columns"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_actions_have_descriptions() {
        for action in Action::all() {
            assert!(!action.description().is_empty());
        }
    }

    #[test]
    fn test_all_actions_have_categories() {
        for action in Action::all() {
            let _ = action.category(); // Should not panic
        }
    }

    #[test]
    fn test_action_serialization() {
        let action = Action::MoveUp;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"MoveUp\"");

        let restored: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, action);
    }
}
