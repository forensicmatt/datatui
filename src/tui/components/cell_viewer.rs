use crate::tui::{Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Information about a selected cell
#[derive(Debug, Clone, Default)]
pub struct CellInfo {
    /// Row index (0-based)
    pub row: usize,
    /// Column index (0-based)
    pub col: usize,
    /// Column name
    pub column_name: String,
    /// Formatted cell value
    pub value: String,
    /// Data type of the value
    pub data_type: Option<String>,
}

impl CellInfo {
    /// Create a new CellInfo
    pub fn new(
        row: usize,
        col: usize,
        column_name: String,
        value: String,
        data_type: Option<String>,
    ) -> Self {
        Self {
            row,
            col,
            column_name,
            value,
            data_type,
        }
    }

    /// Check if cell info is empty
    pub fn is_empty(&self) -> bool {
        self.column_name.is_empty() && self.value.is_empty()
    }
}

/// Configuration for the cell viewer display
#[derive(Debug, Clone)]
pub struct ViewerConfig {
    /// Height mode: Fixed or AutoFit
    pub height_mode: HeightMode,
    /// Maximum height when using AutoFit (in lines, excluding borders)
    pub max_height: u16,
}

/// Height mode for the cell viewer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightMode {
    /// Fixed height (1 line + borders = 3 total)
    Fixed,
    /// Auto-fit to content size, up to max_height
    AutoFit,
}

impl Default for ViewerConfig {
    fn default() -> Self {
        Self {
            height_mode: HeightMode::Fixed,
            max_height: 10, // Default max 10 lines of content
        }
    }
}

/// Component for displaying the currently selected cell value
///
/// This component displays detailed information about the selected cell,
/// including its position, column name, data type, and value.
/// It's designed to be decoupled from the DataTable for flexibility.
pub struct CellViewer {
    /// Current cell information
    cell_info: Option<CellInfo>,
    /// Whether the component has focus
    focused: bool,
    /// Display configuration
    config: ViewerConfig,
}

impl CellViewer {
    /// Create a new CellViewer
    pub fn new() -> Self {
        Self {
            cell_info: None,
            focused: false,
            config: ViewerConfig::default(),
        }
    }

    /// Create a new CellViewer with custom configuration
    pub fn with_config(config: ViewerConfig) -> Self {
        Self {
            cell_info: None,
            focused: false,
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &ViewerConfig {
        &self.config
    }

    /// Set the configuration
    pub fn set_config(&mut self, config: ViewerConfig) {
        self.config = config;
    }

    /// Calculate the required height for the viewer (including borders)
    pub fn calculate_height(&self, available_width: u16) -> u16 {
        match self.config.height_mode {
            HeightMode::Fixed => 3, // 1 line content + 2 borders
            HeightMode::AutoFit => {
                if let Some(ref info) = self.cell_info {
                    // Account for borders (2 chars) and padding
                    let content_width = available_width.saturating_sub(2).max(1);

                    // Calculate how many lines the value will take when wrapped
                    let value_len = info.value.len() as u16;
                    let lines_needed = (value_len + content_width - 1) / content_width;

                    // Clamp to max_height and add borders
                    let content_lines = lines_needed.min(self.config.max_height).max(1);
                    content_lines + 2 // +2 for top and bottom borders
                } else {
                    3 // Default to 1 line + borders when no content
                }
            }
        }
    }

    /// Update the displayed cell information
    pub fn set_cell_info(&mut self, cell_info: Option<CellInfo>) {
        self.cell_info = cell_info;
    }

    /// Get the current cell information
    pub fn cell_info(&self) -> Option<&CellInfo> {
        self.cell_info.as_ref()
    }

    /// Clear the cell information
    pub fn clear(&mut self) {
        self.cell_info = None;
    }
}

impl Default for CellViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for CellViewer {
    fn handle_action(&mut self, _action: crate::tui::Action) -> Result<bool> {
        // CellViewer doesn't handle any actions itself
        Ok(false)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = Theme::default();

        // If no cell info, show a placeholder
        let Some(ref info) = self.cell_info else {
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Cell Viewer")
                .border_style(if self.focused {
                    theme.focused_border_style()
                } else {
                    theme.border_style()
                });
            let placeholder = Paragraph::new("No cell selected")
                .block(block)
                .wrap(Wrap { trim: true });
            frame.render_widget(placeholder, area);
            return;
        };

        // Create title with column name and type
        let title = if let Some(ref data_type) = info.data_type {
            format!("{} ({})", info.column_name, data_type)
        } else {
            info.column_name.clone()
        };

        // Create the block with column info in title
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(if self.focused {
                theme.focused_border_style()
            } else {
                theme.border_style()
            });

        // Render just the cell value with wrapping
        let value_content = Paragraph::new(info.value.as_str())
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(value_content, area);
    }

    fn supported_actions(&self) -> &[crate::tui::Action] {
        // CellViewer doesn't support any actions
        &[]
    }

    fn name(&self) -> &str {
        "CellViewer"
    }
}

impl crate::tui::Focusable for CellViewer {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::Focusable;

    #[test]
    fn test_cell_info_creation() {
        let info = CellInfo::new(
            5,
            2,
            "column_name".to_string(),
            "test_value".to_string(),
            Some("String".to_string()),
        );

        assert_eq!(info.row, 5);
        assert_eq!(info.col, 2);
        assert_eq!(info.column_name, "column_name");
        assert_eq!(info.value, "test_value");
        assert_eq!(info.data_type, Some("String".to_string()));
        assert!(!info.is_empty());
    }

    #[test]
    fn test_cell_info_empty() {
        let info = CellInfo::default();
        assert!(info.is_empty());
    }

    #[test]
    fn test_cell_viewer_creation() {
        let viewer = CellViewer::new();
        assert!(viewer.cell_info().is_none());
        assert!(!viewer.is_focused());
    }

    #[test]
    fn test_cell_viewer_set_info() {
        let mut viewer = CellViewer::new();
        let info = CellInfo::new(1, 0, "test".to_string(), "value".to_string(), None);

        viewer.set_cell_info(Some(info));
        assert!(viewer.cell_info().is_some());

        let cell = viewer.cell_info().unwrap();
        assert_eq!(cell.row, 1);
        assert_eq!(cell.column_name, "test");
    }

    #[test]
    fn test_cell_viewer_clear() {
        let mut viewer = CellViewer::new();
        let info = CellInfo::new(1, 0, "test".to_string(), "value".to_string(), None);

        viewer.set_cell_info(Some(info));
        assert!(viewer.cell_info().is_some());

        viewer.clear();
        assert!(viewer.cell_info().is_none());
    }
}
