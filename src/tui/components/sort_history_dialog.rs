use crate::core::models::SortHistoryRecord;
use crate::tui::{Action, Component, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

#[derive(Debug, Clone)]
pub enum DialogResult {
    Selected(SortHistoryRecord),
    Cancel,
}

pub struct SortHistoryDialog {
    records: Vec<SortHistoryRecord>,
    list_state: ListState,
    closed: bool,
    result: Option<DialogResult>,
}

impl SortHistoryDialog {
    pub fn new(records: Vec<SortHistoryRecord>) -> Self {
        let mut list_state = ListState::default();
        if !records.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            records,
            list_state,
            closed: false,
            result: None,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn result(&self) -> Option<DialogResult> {
        self.result.clone()
    }

    pub fn move_up(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.records.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn move_down(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.records.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn apply(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if let Some(record) = self.records.get(i) {
                self.result = Some(DialogResult::Selected(record.clone()));
                self.closed = true;
            }
        }
    }

    pub fn cancel(&mut self) {
        self.result = Some(DialogResult::Cancel);
        self.closed = true;
    }
}

impl Component for SortHistoryDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::MoveUp => {
                self.move_up();
                Ok(true)
            }
            Action::MoveDown => {
                self.move_down();
                Ok(true)
            }
            Action::Confirm => {
                self.apply();
                Ok(true)
            }
            Action::Cancel => {
                self.cancel();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::MoveUp,
            Action::MoveDown,
            Action::Confirm,
            Action::Cancel,
        ]
    }

    fn name(&self) -> &str {
        "Sort History"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Sort History ")
            .border_style(Style::default().fg(theme.border_focused));

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let inner_area = area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 1,
        });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(inner_area);

        if self.records.is_empty() {
            let p = Paragraph::new("No sort history available for this dataset.")
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(p, chunks[0]);
        } else {
            let items: Vec<ListItem> = self
                .records
                .iter()
                .map(|r| {
                    let text = format!(
                        "[{}] \"{}\" on col '{}' (at {})",
                        r.provider,
                        r.prompt,
                        r.source_column,
                        r.executed_at.format("%Y-%m-%d %H:%M:%S")
                    );
                    ListItem::new(text)
                })
                .collect();

            let list = List::new(items)
                .highlight_style(theme.selected_style())
                .highlight_symbol(">> ");

            frame.render_stateful_widget(list, chunks[0], &mut self.list_state);
        }

        let instructions = Paragraph::new("Enter: Select | Esc: Cancel | ↑/↓: Navigate")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center);

        frame.render_widget(instructions, chunks[1]);
    }
}
