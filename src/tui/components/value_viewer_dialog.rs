//! Value Viewer Dialog Component
//!
//! A simple popup dialog for viewing values that are too long to fit in table columns.
//! Provides scrolling support for very long values.

use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

/// Simple dialog for viewing a single value
pub struct ValueViewerDialog {
    /// The value to display
    value: String,

    /// Title for the dialog
    title: String,

    /// Whether the component has focus
    focused: bool,

    /// Current scroll offset (line number)
    scroll_offset: usize,

    /// Total number of wrapped lines (for scrollbar)
    total_lines: usize,
}

impl ValueViewerDialog {
    /// Create a new value viewer dialog
    pub fn new(title: String, value: String) -> Self {
        Self {
            value,
            title,
            focused: true, // Start focused since it's a modal dialog
            scroll_offset: 0,
            total_lines: 0, // Will be calculated during render
        }
    }

    /// Render the dialog
    fn render_dialog(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // let theme = Theme::default(); // Removed

        // Clear background
        frame.render_widget(Clear, area);

        // Main block
        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .title_bottom(" Press ↑/↓ to scroll, Esc to close ")
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(theme.normal_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Calculate how many lines the content will be when wrapped
        let content_width = inner.width as usize;
        let lines: Vec<Line> = self
            .value
            .split('\n')
            .flat_map(|line| {
                if line.is_empty() {
                    vec![Line::from("")]
                } else {
                    // Split long lines to fit width
                    let mut result = Vec::new();
                    let mut chars = line.chars().collect::<Vec<_>>();
                    while !chars.is_empty() {
                        let take = content_width.min(chars.len());
                        let chunk: String = chars.drain(..take).collect();
                        result.push(Line::from(chunk));
                    }
                    result
                }
            })
            .collect();

        self.total_lines = lines.len();

        // Calculate visible range
        let viewport_height = inner.height as usize;
        let visible_start = self.scroll_offset.min(self.total_lines.saturating_sub(1));
        let visible_end = (visible_start + viewport_height).min(self.total_lines);

        let visible_lines: Vec<Line> = if self.total_lines > 0 {
            lines[visible_start..visible_end].to_vec()
        } else {
            vec![Line::from("")]
        };

        // Render the content
        let paragraph = Paragraph::new(visible_lines)
            .style(theme.normal_style())
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, inner);

        // Render scrollbar if needed
        if self.total_lines > viewport_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(self.total_lines).position(self.scroll_offset);

            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }
}

impl Component for ValueViewerDialog {
    fn name(&self) -> &str {
        "ValueViewerDialog"
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::MoveUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                Ok(true)
            }
            Action::MoveDown => {
                let max_scroll = self.total_lines.saturating_sub(1);
                if self.scroll_offset < max_scroll {
                    self.scroll_offset += 1;
                }
                Ok(true)
            }
            Action::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                Ok(true)
            }
            Action::PageDown => {
                let max_scroll = self.total_lines.saturating_sub(1);
                self.scroll_offset = (self.scroll_offset + 10).min(max_scroll);
                Ok(true)
            }
            Action::Home => {
                self.scroll_offset = 0;
                Ok(true)
            }
            Action::End => {
                self.scroll_offset = self.total_lines.saturating_sub(1);
                Ok(true)
            }
            Action::Cancel => Ok(false), // Close dialog
            _ => Ok(true),               // Consume other actions
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.render_dialog(frame, area, theme);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::MoveUp,
            Action::MoveDown,
            Action::PageUp,
            Action::PageDown,
            Action::Home,
            Action::End,
            Action::Cancel,
        ]
    }
}

impl Focusable for ValueViewerDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}
