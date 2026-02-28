//! Column Operations Dialog
//!
//! Entry-point dialog that lists all available column operations. When the user
//! selects an operation and presses Enter, a `ColumnOperationOptionsDialog` is
//! open for that specific operation.

use crate::tui::components::column_operation_options_dialog::{
    ColumnOperationKind, ColumnOperationOptionsDialog, DialogResult as OptionsDialogResult,
};
use crate::tui::{Action, Component, Focusable, KeyEventResult, Theme};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

// Re-export so app.rs only needs to use this module
pub use crate::tui::components::column_operation_options_dialog::{
    ColumnOperationConfig, DialogResult as OptionsResult,
};

// ── Operation metadata ────────────────────────────────────────────────────────

const OPERATIONS: &[ColumnOperationKind] = &[
    ColumnOperationKind::GenerateEmbeddings,
    ColumnOperationKind::Pca,
    ColumnOperationKind::Cluster,
    ColumnOperationKind::SortByPromptSimilarity,
];

fn operation_description(op: ColumnOperationKind) -> &'static str {
    match op {
        ColumnOperationKind::GenerateEmbeddings => {
            "Convert text data into numerical vectors for machine learning"
        }
        ColumnOperationKind::Pca => {
            "Reduce dimensionality while preserving most of the data variance"
        }
        ColumnOperationKind::Cluster => {
            "Group similar data points together using clustering algorithms"
        }
        ColumnOperationKind::SortByPromptSimilarity => {
            "Compute cosine similarity of an embedding column to a user prompt and sort by score"
        }
    }
}

fn operation_requirements(op: ColumnOperationKind) -> &'static str {
    match op {
        ColumnOperationKind::GenerateEmbeddings => "Requires: text columns, LLM API key",
        ColumnOperationKind::Pca => "Requires: numerical (embedding) columns",
        ColumnOperationKind::Cluster => "Requires: numerical columns",
        ColumnOperationKind::SortByPromptSimilarity => "Requires: at least one embedding column",
    }
}

// ── Dialog result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum DialogResult {
    /// User confirmed an operation via the sub-dialog
    Applied(ColumnOperationConfig),
    /// User cancelled at any point
    Cancelled,
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct ColumnOperationsDialog {
    pub focused: bool,
    pub closed: bool,
    pub show_instructions: bool,

    /// Column list passed down to the sub-dialog
    columns: Vec<String>,
    /// Cursor column index in the data table
    col_idx: usize,

    /// Currently highlighted operation in the list
    selected_index: usize,

    /// Active sub-dialog (None = we are in the picker)
    sub_dialog: Option<ColumnOperationOptionsDialog>,

    /// Final result (set when closed)
    result: Option<DialogResult>,
}

impl ColumnOperationsDialog {
    pub fn new(columns: Vec<String>, col_idx: usize) -> Self {
        Self {
            focused: true,
            closed: false,
            show_instructions: false,
            columns,
            col_idx,
            selected_index: 0,
            sub_dialog: None,
            result: None,
        }
    }

    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.result.take()
    }

    /// Returns true when the options sub-dialog is currently open.
    pub fn has_sub_dialog(&self) -> bool {
        self.sub_dialog.is_some()
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn open_sub_dialog(&mut self) {
        let kind = OPERATIONS[self.selected_index];
        let dialog = ColumnOperationOptionsDialog::new(kind, self.columns.clone(), self.col_idx);
        self.sub_dialog = Some(dialog);
    }

    fn move_up(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = OPERATIONS.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    fn move_down(&mut self) {
        self.selected_index = (self.selected_index + 1) % OPERATIONS.len();
    }
}

// ── Focusable ─────────────────────────────────────────────────────────────────

impl Focusable for ColumnOperationsDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

// ── Component ─────────────────────────────────────────────────────────────────

impl Component for ColumnOperationsDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        if !self.focused {
            return Ok(KeyEventResult::Ignored);
        }

        // Delegate to sub-dialog first
        if let Some(sub) = &mut self.sub_dialog {
            let kr = sub.handle_key_event(key)?;

            // Check if sub-dialog produced a result
            if let Some(result) = sub.take_result() {
                self.sub_dialog = None;
                match result {
                    OptionsDialogResult::Applied(config) => {
                        self.result = Some(DialogResult::Applied(config));
                        self.closed = true;
                    }
                    OptionsDialogResult::Cancelled => {
                        // Return to picker — sub-dialog is gone, we stay open
                    }
                }
            }

            return Ok(kr);
        }

        // Picker key handling
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                self.result = Some(DialogResult::Cancelled);
                self.closed = true;
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Up => {
                self.move_up();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Down => {
                self.move_down();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Enter => {
                self.open_sub_dialog();
                Ok(KeyEventResult::Consumed)
            }
            KeyCode::Char('i') | KeyCode::Char('I') if ctrl => {
                self.show_instructions = !self.show_instructions;
                Ok(KeyEventResult::Consumed)
            }
            _ => Ok(KeyEventResult::Ignored),
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Delegate to sub-dialog if open
        if let Some(sub) = &mut self.sub_dialog {
            let handled = sub.handle_action(action.clone())?;

            if let Some(result) = sub.take_result() {
                self.sub_dialog = None;
                match result {
                    OptionsDialogResult::Applied(config) => {
                        self.result = Some(DialogResult::Applied(config));
                        self.closed = true;
                    }
                    OptionsDialogResult::Cancelled => {}
                }
            }

            return Ok(handled);
        }

        // Picker action handling
        match action {
            Action::Cancel | Action::Escape => {
                self.result = Some(DialogResult::Cancelled);
                self.closed = true;
                Ok(true)
            }
            Action::Confirm => {
                self.open_sub_dialog();
                Ok(true)
            }
            Action::MoveUp => {
                self.move_up();
                Ok(true)
            }
            Action::MoveDown => {
                self.move_down();
                Ok(true)
            }
            Action::ToggleInstructions => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn supported_actions(&self) -> &[Action] {
        &[
            Action::Cancel,
            Action::Escape,
            Action::Confirm,
            Action::MoveUp,
            Action::MoveDown,
            Action::ToggleInstructions,
        ]
    }

    fn name(&self) -> &str {
        "ColumnOperationsDialog"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // If sub-dialog is open, render it instead
        if let Some(sub) = &mut self.sub_dialog {
            sub.render(frame, area, theme);
            return;
        }

        frame.render_widget(Clear, area);

        let border_style = if self.focused {
            theme.focused_border_style()
        } else {
            theme.border_style()
        };

        let outer_block = Block::default()
            .title(" Column Operations ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style);

        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Split: instructions at bottom (conditional)
        let (content_area, instr_area) = if self.show_instructions {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(5)])
                .split(inner);
            (chunks[0], Some(chunks[1]))
        } else {
            (inner, None)
        };

        // Render the picker list + detail panel
        self.render_picker(frame, content_area, theme);

        // Instructions
        if let Some(ia) = instr_area {
            self.render_instructions(frame, ia, theme);
        }
    }
}

impl ColumnOperationsDialog {
    fn render_picker(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Split vertically: list (top) | detail panel (bottom)
        let list_height = OPERATIONS.len() as u16 + 2; // +2 for padding
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(list_height), Constraint::Min(4)])
            .split(area);

        self.render_list(frame, chunks[0], theme);
        self.render_detail(frame, chunks[1], theme);
    }

    fn render_list(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = Vec::new();

        for (i, &op) in OPERATIONS.iter().enumerate() {
            let is_selected = i == self.selected_index;
            let marker = if is_selected { "▶ " } else { "  " };
            let label = op.title();

            let style = if is_selected {
                theme.selected_style().add_modifier(Modifier::BOLD)
            } else {
                theme.normal_style()
            };

            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(label, style),
            ]));
        }

        let para = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(theme.border_style()),
        );
        frame.render_widget(para, area);
    }

    fn render_detail(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let op = OPERATIONS[self.selected_index];
        let desc = operation_description(op);
        let reqs = operation_requirements(op);

        let text = vec![
            Line::from(vec![
                Span::styled(
                    "Description:  ",
                    theme.header_style().add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc, theme.normal_style()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Requirements: ",
                    theme.warning_style().add_modifier(Modifier::BOLD),
                ),
                Span::styled(reqs, theme.normal_style()),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Press Enter to configure →",
                theme.success_style(),
            )]),
        ];

        let para = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(theme.border_style())
                    .title(" Details "),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(para, area);
    }

    fn render_instructions(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let text = vec![
            Line::from("• ↑/↓  Navigate operations"),
            Line::from("• Enter  Open operation configuration"),
            Line::from("• Esc  Close"),
            Line::from("• Ctrl+i  Toggle this panel"),
        ];

        let para = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(theme.border_style())
                    .title(" Instructions (Ctrl+i to hide) "),
            )
            .style(theme.warning_style())
            .wrap(Wrap { trim: true });

        frame.render_widget(para, area);
    }
}
