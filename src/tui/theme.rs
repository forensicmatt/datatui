use ratatui::style::{Color, Modifier, Style};

/// A theme defines the color scheme for the TUI
///
/// This is a simple color-based theme, NOT the complex StyleSet system
/// with conditional rules (that's deferred to later phases).
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,

    // General UI colors
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub border_focused: Color,

    // Table colors
    pub header_fg: Color,
    pub header_bg: Color,
    pub selected_fg: Color,
    pub selected_bg: Color,
    pub row_alt_bg: Color, // For zebra striping

    // Status/feedback colors
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
}

impl Theme {
    /// Default dark theme
    pub fn default() -> Self {
        Self {
            name: "Default Dark".to_string(),
            background: Color::Reset,
            foreground: Color::Gray,
            border: Color::DarkGray,
            border_focused: Color::Cyan,
            header_fg: Color::Cyan,
            header_bg: Color::Reset,
            selected_fg: Color::Black,
            selected_bg: Color::Cyan,
            row_alt_bg: Color::Rgb(25, 25, 35), // Slightly lighter than pure black
            success: Color::Green,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
        }
    }

    /// Light theme
    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            background: Color::White,
            foreground: Color::Black,
            border: Color::Gray,
            border_focused: Color::Blue,
            header_fg: Color::Blue,
            header_bg: Color::Rgb(240, 240, 240),
            selected_fg: Color::White,
            selected_bg: Color::Blue,
            row_alt_bg: Color::Rgb(250, 250, 250),
            success: Color::Green,
            error: Color::Red,
            warning: Color::Rgb(200, 150, 0), // Darker yellow for light bg
            info: Color::Blue,
        }
    }

    /// Helper methods to get commonly used styles

    pub fn header_style(&self) -> Style {
        Style::default()
            .fg(self.header_fg)
            .bg(self.header_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected_style(&self) -> Style {
        Style::default()
            .fg(self.selected_fg)
            .bg(self.selected_bg)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for the currently active cell
    pub fn selected_cell_style(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal_style(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    pub fn alt_row_style(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.row_alt_bg)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn focused_border_style(&self) -> Style {
        Style::default().fg(self.border_focused)
    }

    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn info_style(&self) -> Style {
        Style::default().fg(self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme() {
        let theme = Theme::default();
        assert_eq!(theme.name, "Default Dark");

        // Should have valid colors
        assert_ne!(theme.header_fg, Color::Reset);
        assert_ne!(theme.selected_bg, Color::Reset);
    }

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "Light");

        // Light theme should have different background
        assert_eq!(theme.background, Color::White);
        assert_eq!(theme.foreground, Color::Black);
    }

    #[test]
    fn test_style_helpers() {
        let theme = Theme::default();

        // Header should be bold
        let header = theme.header_style();
        assert!(header.add_modifier.contains(Modifier::BOLD));

        // Selected should have distinct colors
        let selected = theme.selected_style();
        assert_eq!(selected.fg, Some(theme.selected_fg));
        assert_eq!(selected.bg, Some(theme.selected_bg));
    }
}
