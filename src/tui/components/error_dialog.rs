//! Simple Error Dialog Component
//!
//! Displays error messages in a centered popup dialog.

use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Simple error dialog that displays an error message
pub struct ErrorDialog {
    /// The error message to display
    message: String,
    /// Optional title
    title: String,
}

impl ErrorDialog {
    /// Create a new error dialog with a message
    pub fn new(message: String) -> Self {
        Self {
            message,
            title: "Error".to_string(),
        }
    }

    /// Create an error dialog with a custom title
    pub fn with_title(message: String, title: String) -> Self {
        Self { message, title }
    }
}

impl Component for ErrorDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Cancel | Action::Confirm | Action::Escape => {
                Ok(false) // Close dialog on any confirmation/cancel
            }
            _ => Ok(true), // Keep open for other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let theme = Theme::default();

        // Clear area
        frame.render_widget(Clear, area);

        // Create block with border
        let block = Block::default()
            .title(self.title.as_str())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.focused_border_style())
            .style(theme.normal_style());

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Render error message with wrapping
        let paragraph = Paragraph::new(self.message.as_str())
            .style(theme.normal_style())
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, inner_area);
    }

    fn supported_actions(&self) -> &[Action] {
        &[Action::Cancel, Action::Confirm, Action::Escape]
    }

    fn name(&self) -> &str {
        "ErrorDialog"
    }
}
