//! StyleRuleEditorDialog: Dialog for editing individual style rules
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::style::StyleConfig;
use crate::components::Component;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::styling::style_set::{StyleRule, ScopeEnum};
use crate::dialog::filter_dialog::FilterDialog;
use ratatui::style::Color;

/// StyleRuleEditorDialog: UI for editing a single StyleRule
#[derive(Debug)]
pub struct StyleRuleEditorDialog {
    pub rule: StyleRule,
    pub filter_dialog: FilterDialog,
    pub show_filter_dialog: bool,
    pub column_scope_input: String,
    pub scope: ScopeEnum,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
    pub show_instructions: bool,
    pub config: Config,
    pub style: StyleConfig,
}

impl StyleRuleEditorDialog {
    /// Create a new StyleRuleEditorDialog
    pub fn new(rule: StyleRule, columns: Vec<String>) -> Self {
        let mut filter_dialog = FilterDialog::new(columns);
        filter_dialog.set_root_expr(rule.match_expr.clone());
        
        Self {
            rule,
            filter_dialog,
            show_filter_dialog: false,
            column_scope_input: String::new(),
            scope: ScopeEnum::Row,
            fg_color: None,
            bg_color: None,
            show_instructions: true,
            config: Config::default(),
            style: StyleConfig::default(),
        }
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (Mode::Global, Action::Escape),
            (Mode::Global, Action::Enter),
            (Mode::Global, Action::ToggleInstructions),
        ])
    }

    /// Render the dialog
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let instructions = self.build_instructions_from_config();

        let outer_block = Block::default()
            .title("Style Rule Editor")
            .borders(Borders::ALL)
            .border_type(BorderType::Double);
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        let layout = split_dialog_area(inner_area, self.show_instructions, if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;

        let block = Block::default()
            .title("Edit Rule")
            .borders(Borders::ALL);
        block.render(content_area, buf);

        let start_x = content_area.x + 1;
        let start_y = content_area.y + 1;

        // Column scope
        buf.set_string(start_x, start_y, "Column Scope (glob patterns, comma-separated):", Style::default());
        buf.set_string(start_x, start_y + 1, &self.column_scope_input, Style::default().fg(Color::Cyan));

        // Scope
        let scope_label = format!("Scope: {:?}", self.scope);
        buf.set_string(start_x, start_y + 3, &scope_label, Style::default());

        // Colors
        let fg_label = format!("Foreground: {}", self.fg_color.as_deref().unwrap_or("None"));
        buf.set_string(start_x, start_y + 5, &fg_label, Style::default());
        let bg_label = format!("Background: {}", self.bg_color.as_deref().unwrap_or("None"));
        buf.set_string(start_x, start_y + 6, &bg_label, Style::default());

        // Render instructions
        if self.show_instructions {
            if let Some(instr_area) = instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, buf);
            }
        }
    }

    /// Handle a key event
    pub fn handle_key_event_pub(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind == KeyEventKind::Press {
            // Check Global actions first
            if let Some(global_action) = self.config.action_for_key(Mode::Global, key) {
                match global_action {
                    Action::Escape => {
                        return Some(Action::CloseStyleRuleEditorDialog);
                    }
                    Action::Enter => {
                        // Build the rule and return it
                        // This would need to parse colors and build the rule properly
                        return Some(Action::StyleRuleEditorDialogApplied(self.rule.clone()));
                    }
                    Action::ToggleInstructions => {
                        self.show_instructions = !self.show_instructions;
                        return None;
                    }
                    _ => {}
                }
            }
        }

        None
    }
}

impl Component for StyleRuleEditorDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        let config_clone = config.clone();
        self.config = config;
        self.filter_dialog.register_config_handler(config_clone)?;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        Ok(self.handle_key_event_pub(key))
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        self.render(area, frame.buffer_mut());
        Ok(())
    }
}

