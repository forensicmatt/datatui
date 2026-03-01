//! Query Debug Dialog
//!
//! A simple read-only dialog for inspecting the current SQL QueryBuilder state and rendered SQL.

use crate::tui::{Action, Component, KeyEventResult, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

#[derive(Debug, Clone)]
pub enum DialogResult {
    Close,
}

pub struct QueryDebugDialog {
    /// Rendered SQL query
    sql: String,
    /// Base table name
    table: String,
    /// Calculated columns
    calculated: Vec<String>,
    /// Order by columns
    order_by: Vec<String>,
    /// Pending result
    pending_result: Option<DialogResult>,
    /// Scroll offset
    scroll: u16,
    /// Total lines in the content (calculated during render)
    total_lines: u16,
}

impl QueryDebugDialog {
    pub fn new(sql: String, table: String, calculated: Vec<String>, order_by: Vec<String>) -> Self {
        Self {
            sql,
            table,
            calculated,
            order_by,
            pending_result: None,
            scroll: 0,
            total_lines: 0,
        }
    }

    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }
}

impl Component for QueryDebugDialog {
    fn name(&self) -> &str {
        "QueryDebugDialog"
    }

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<KeyEventResult> {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Esc => {
                self.pending_result = Some(DialogResult::Close);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Char('q') | KeyCode::Char('Q')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.pending_result = Some(DialogResult::Close);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Down => {
                self.scroll = (self.scroll + 1).min(self.total_lines.saturating_sub(1));
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::PageDown => {
                self.scroll = (self.scroll + 10).min(self.total_lines.saturating_sub(1));
                Ok(KeyEventResult::Consumed)
            }
            _ => Ok(KeyEventResult::Ignored),
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Cancel | Action::Escape => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false)
            }
            Action::MoveUp => {
                self.scroll = self.scroll.saturating_sub(1);
                Ok(true)
            }
            Action::MoveDown => {
                self.scroll = (self.scroll + 1).min(self.total_lines.saturating_sub(1));
                Ok(true)
            }
            Action::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                Ok(true)
            }
            Action::PageDown => {
                self.scroll = (self.scroll + 10).min(self.total_lines.saturating_sub(1));
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let area = area.inner(Margin {
            vertical: 4,
            horizontal: 8,
        });

        let block = Block::default()
            .title(" Query Debug ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.focused_border_style());

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner_area);

        let mut lines = Vec::new();

        // Base Table
        lines.push(Line::from(vec![
            Span::styled("● ", theme.info_style()),
            Span::styled("Base Table", theme.header_style()),
        ]));
        lines.push(Line::from(vec![Span::raw("  "), Span::raw(&self.table)]));
        lines.push(Line::raw(""));

        // Calculated Columns
        lines.push(Line::from(vec![
            Span::styled("● ", theme.info_style()),
            Span::styled("Calculated Columns", theme.header_style()),
        ]));
        if self.calculated.is_empty() {
            lines.push(Line::raw("  (none)"));
        } else {
            for calc in &self.calculated {
                lines.push(Line::raw(format!("  • {}", calc)));
            }
        }
        lines.push(Line::raw(""));

        // Order By
        lines.push(Line::from(vec![
            Span::styled("● ", theme.info_style()),
            Span::styled("Order By", theme.header_style()),
        ]));
        if self.order_by.is_empty() {
            lines.push(Line::raw("  (none)"));
        } else {
            for order in &self.order_by {
                lines.push(Line::raw(format!("  • {}", order)));
            }
        }
        lines.push(Line::raw(""));

        // Rendered SQL
        lines.push(Line::from(vec![
            Span::styled("● ", theme.info_style()),
            Span::styled("Rendered SQL", theme.header_style()),
        ]));
        lines.push(Line::raw(&self.sql));
        lines.push(Line::raw(""));

        self.total_lines = lines.len() as u16;

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        // Content viewport with scrollbar
        let has_scrollbar = self.total_lines > chunks[0].height;
        let content_area = if has_scrollbar {
            Rect {
                width: chunks[0].width.saturating_sub(1),
                ..chunks[0]
            }
        } else {
            chunks[0]
        };

        frame.render_widget(content, content_area);

        if has_scrollbar {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("┃")
                .style(theme.focused_border_style());
            let mut scrollbar_state = ScrollbarState::new(self.total_lines as usize)
                .position(self.scroll as usize)
                .viewport_content_length(chunks[0].height as usize);
            frame.render_stateful_widget(scrollbar, chunks[0], &mut scrollbar_state);
        }

        // Instructions Footer
        let instructions = Line::from(vec![
            Span::styled(
                "Esc",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
            Span::raw(": Close | "),
            Span::styled(
                "↑/↓",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
            Span::raw(": Scroll | "),
            Span::styled(
                "PgUp/PgDn",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            ),
            Span::raw(": Page Scroll"),
        ]);
        let footer = Paragraph::new(instructions)
            .alignment(ratatui::layout::Alignment::Center)
            .style(theme.normal_style().add_modifier(Modifier::DIM));
        frame.render_widget(footer, chunks[1]);
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Escape,
            Action::MoveUp,
            Action::MoveDown,
            Action::PageUp,
            Action::PageDown,
        ]
    }
}
