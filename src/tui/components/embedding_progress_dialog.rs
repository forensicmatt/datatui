//! Embedding Progress Dialog
//!
//! Non-interactive overlay displayed while embedding generation runs in the
//! background. Shows a progress bar, row counter, elapsed time, and a spinner
//! to indicate activity. Closes automatically when the job completes.

use crate::tui::{Action, Component, Focusable, Theme};
use color_eyre::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph},
    Frame,
};
use std::time::Instant;

// Spinner frames — a simple braille rotation
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct EmbeddingProgressDialog {
    pub focused: bool,

    /// Column name being generated
    pub column_name: String,
    /// Source column
    pub source_column: String,

    /// Rows completed so far
    pub rows_done: usize,
    /// Total rows
    pub rows_total: usize,

    /// Time the job started (for elapsed display)
    started_at: Instant,
    /// Spinner frame index
    spinner_frame: usize,
    /// How many ticks since the last spinner advance
    tick_count: usize,
}

impl EmbeddingProgressDialog {
    pub fn new(column_name: String, source_column: String, rows_total: usize) -> Self {
        Self {
            focused: true,
            column_name,
            source_column,
            rows_done: 0,
            rows_total,
            started_at: Instant::now(),
            spinner_frame: 0,
            tick_count: 0,
        }
    }

    /// Called by `App::update` when a progress tick arrives.
    pub fn set_progress(&mut self, done: usize, total: usize) {
        self.rows_done = done;
        self.rows_total = total;
    }

    /// Advance the spinner (call every TUI tick).
    pub fn tick(&mut self) {
        self.tick_count += 1;
        if self.tick_count % 3 == 0 {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        }
    }

    fn elapsed_str(&self) -> String {
        let secs = self.started_at.elapsed().as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else {
            format!("{}m {}s", secs / 60, secs % 60)
        }
    }
}

// ── Focusable ─────────────────────────────────────────────────────────────────

impl Focusable for EmbeddingProgressDialog {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
}

// ── Component ─────────────────────────────────────────────────────────────────

impl Component for EmbeddingProgressDialog {
    fn handle_action(&mut self, _action: Action) -> Result<bool> {
        // Not interactive — all actions fall through
        Ok(true)
    }

    fn supported_actions(&self) -> &[Action] {
        &[]
    }

    fn name(&self) -> &str {
        "EmbeddingProgressDialog"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Advance spinner each render
        self.tick();

        frame.render_widget(Clear, area);

        let border_style = theme.focused_border_style();
        let spinner = SPINNER_FRAMES[self.spinner_frame];

        let outer_block = Block::default()
            .title(format!(" {} Generating Embeddings ", spinner))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style)
            .style(theme.normal_style());

        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Layout: info line | gap | gauge | gap | details
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Column info
                Constraint::Length(1), // spacer
                Constraint::Length(1), // Gauge
                Constraint::Length(1), // spacer
                Constraint::Length(1), // Row counter + elapsed
            ])
            .split(inner);

        // Line 1 — operation description
        let info_line = Line::from(vec![
            Span::styled("  Column: ", theme.warning_style()),
            Span::styled(
                self.source_column.clone(),
                theme.normal_style().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  →  ", theme.normal_style()),
            Span::styled(
                self.column_name.clone(),
                theme.success_style().add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(info_line), chunks[0]);

        // Line 3 — progress gauge
        let (ratio, label) = if self.rows_total == 0 {
            (0.0, "Initialising…".to_string())
        } else {
            let r = self.rows_done as f64 / self.rows_total as f64;
            let pct = (r * 100.0) as u64;
            (r, format!(" {}% ", pct))
        };

        let gauge = Gauge::default()
            .gauge_style(theme.selected_style().add_modifier(Modifier::BOLD))
            .label(label)
            .ratio(ratio.clamp(0.0, 1.0));
        frame.render_widget(gauge, chunks[2]);

        // Line 5 — row counter and elapsed
        let counter_line = Line::from(vec![
            Span::styled("  Rows: ", theme.warning_style()),
            Span::styled(
                format!("{} / {}", self.rows_done, self.rows_total),
                theme.normal_style(),
            ),
            Span::styled("     Elapsed: ", theme.warning_style()),
            Span::styled(self.elapsed_str(), theme.normal_style()),
        ]);
        frame.render_widget(Paragraph::new(counter_line), chunks[4]);
    }
}
