use ratatui::style::{Style, Color, Modifier};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    pub table_header: Style,
    pub table_cell: Style,
    pub table_border: Style,
    pub selected_row: Style,
    pub dialog: Style,
    pub error: Style,
    pub table_row_even: Style,
    pub table_row_odd: Style,
    pub cursor: CursorStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorStyle {
    /// Style for block cursor (used in simple text input fields)
    pub block: Style,
    /// Style for highlighted character cursor (used in search/find fields)
    pub highlighted: Style,
    /// Style for hidden cursor (used when field is not focused)
    pub hidden: Style,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            table_header: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            table_cell: Style::default().fg(Color::White),
            table_border: Style::default().fg(Color::Gray),
            selected_row: Style::default().fg(Color::Black).bg(Color::Yellow),
            dialog: Style::default().fg(Color::White),
            error: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            table_row_even: Style::default().bg(Color::Rgb(30, 30, 30)), // dark gray
            table_row_odd: Style::default().bg(Color::Rgb(40, 40, 40)),  // slightly lighter gray
            cursor: CursorStyle::default(),
        }
    }
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            block: Style::default().fg(Color::Black).bg(Color::White),
            highlighted: Style::default().fg(Color::Black).bg(Color::Yellow),
            hidden: Style::default().fg(Color::Gray),
        }
    }
}

impl StyleConfig {
    pub fn with_table_header(mut self, style: Style) -> Self {
        self.table_header = style;
        self
    }
    pub fn with_table_cell(mut self, style: Style) -> Self {
        self.table_cell = style;
        self
    }
    pub fn with_table_border(mut self, style: Style) -> Self {
        self.table_border = style;
        self
    }
    pub fn with_selected_row(mut self, style: Style) -> Self {
        self.selected_row = style;
        self
    }
    pub fn with_dialog(mut self, style: Style) -> Self {
        self.dialog = style;
        self
    }
    pub fn with_error(mut self, style: Style) -> Self {
        self.error = style;
        self
    }
    pub fn with_table_row_even(mut self, style: Style) -> Self {
        self.table_row_even = style;
        self
    }
    pub fn with_table_row_odd(mut self, style: Style) -> Self {
        self.table_row_odd = style;
        self
    }
    pub fn with_cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = cursor;
        self
    }
}

impl CursorStyle {
    /// Get the block cursor style
    pub fn block(&self) -> Style {
        self.block
    }
    
    /// Get the highlighted cursor style
    pub fn highlighted(&self) -> Style {
        self.highlighted
    }
    
    /// Get the hidden cursor style
    pub fn hidden(&self) -> Style {
        self.hidden
    }
    
    /// Create a custom cursor style
    pub fn new(block: Style, highlighted: Style, hidden: Style) -> Self {
        Self {
            block,
            highlighted,
            hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn test_default_styles() {
        let style = StyleConfig::default();
        assert_eq!(style.table_header.fg, Some(Color::Yellow));
        assert!(style.table_header.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.table_cell.fg, Some(Color::White));
        assert_eq!(style.table_border.fg, Some(Color::Gray));
        assert_eq!(style.selected_row.fg, Some(Color::Black));
        assert_eq!(style.selected_row.bg, Some(Color::Yellow));
        assert_eq!(style.dialog.fg, Some(Color::White));
        assert_eq!(style.error.fg, Some(Color::Red));
        assert!(style.error.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.table_row_even.bg, Some(Color::Rgb(30, 30, 30)));
        assert_eq!(style.table_row_odd.bg, Some(Color::Rgb(40, 40, 40)));
        assert_eq!(style.cursor.block.fg, Some(Color::Black));
        assert_eq!(style.cursor.block.bg, Some(Color::White));
        assert_eq!(style.cursor.highlighted.fg, Some(Color::Black));
        assert_eq!(style.cursor.highlighted.bg, Some(Color::Yellow));
        assert_eq!(style.cursor.hidden.fg, Some(Color::Gray));
    }

    #[test]
    fn test_custom_styles() {
        let custom = StyleConfig::default()
            .with_table_header(Style::default().fg(Color::Green))
            .with_error(Style::default().fg(Color::Magenta));
        assert_eq!(custom.table_header.fg, Some(Color::Green));
        assert_eq!(custom.error.fg, Some(Color::Magenta));
    }
} 