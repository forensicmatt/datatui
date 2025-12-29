//! CommandBar: Vim-like command bar for executing commands with Tab auto-completion
//!
//! This dialog provides a modal command bar (activated with `:`) that allows users to type
//! commands and execute actions. Commands are context-aware and provided by the active dialog
//! or container.
//!
//! ## Features
//!
//! - Vim-style activation with `:`
//! - Tab completion cycling through matching commands, subcommands, and field names
//! - Escape to cancel, Enter to execute
//! - Context-aware command suggestions based on active dialog
//! - Support for subcommands like `sort add descending <field_name>`

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};
use crate::action::Action;
use crate::config::Config;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode, KeyModifiers};
use tracing::debug;

/// Argument type for command parameters
#[derive(Debug, Clone, PartialEq)]
pub enum ArgType {
    /// A column/field name from the dataframe
    FieldName,
    /// A literal keyword (e.g., "ascending", "descending")
    Keyword(Vec<String>),
    /// Free-form text
    Text,
}

/// Defines an argument for a subcommand
#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub name: String,
    pub arg_type: ArgType,
    pub required: bool,
    pub description: String,
}

impl ArgSpec {
    pub fn field(name: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            arg_type: ArgType::FieldName,
            required,
            description: format!("<{}>", name),
        }
    }

    pub fn keyword(name: &str, options: Vec<&str>, required: bool) -> Self {
        let description = format!("[{}]", options.join("|"));
        Self {
            name: name.to_string(),
            arg_type: ArgType::Keyword(options.into_iter().map(String::from).collect()),
            required,
            description,
        }
    }
}

/// A subcommand definition
#[derive(Debug, Clone)]
pub struct SubCommand {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub args: Vec<ArgSpec>,
}

impl SubCommand {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            aliases: Vec::new(),
            description: description.to_string(),
            args: Vec::new(),
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<&str>) -> Self {
        self.aliases = aliases.into_iter().map(String::from).collect();
        self
    }

    pub fn with_args(mut self, args: Vec<ArgSpec>) -> Self {
        self.args = args;
        self
    }

    pub fn matches(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.name.to_lowercase().starts_with(&input_lower)
            || self.aliases.iter().any(|a| a.to_lowercase().starts_with(&input_lower))
    }

    pub fn exact_match(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.name.to_lowercase() == input_lower
            || self.aliases.iter().any(|a| a.to_lowercase() == input_lower)
    }

    /// Get usage string for this subcommand
    pub fn usage(&self) -> String {
        let args_str: Vec<String> = self.args.iter()
            .map(|a| if a.required { a.description.clone() } else { format!("[{}]", a.description) })
            .collect();
        if args_str.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, args_str.join(" "))
        }
    }
}

/// A command definition with name, description, and optional subcommands
#[derive(Debug, Clone)]
pub struct Command {
    /// The command name (what user types)
    pub name: String,
    /// Short description shown in completion hints
    pub description: String,
    /// The action to execute when this command is invoked (if no subcommands)
    pub action: Option<Action>,
    /// Optional aliases for the command
    pub aliases: Vec<String>,
    /// Optional subcommands
    pub subcommands: Vec<SubCommand>,
}

impl Command {
    pub fn new(name: impl Into<String>, description: impl Into<String>, action: Action) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            action: Some(action),
            aliases: Vec::new(),
            subcommands: Vec::new(),
        }
    }

    pub fn new_with_subcommands(name: impl Into<String>, description: impl Into<String>, subcommands: Vec<SubCommand>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            action: None,
            aliases: Vec::new(),
            subcommands,
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<&str>) -> Self {
        self.aliases = aliases.into_iter().map(String::from).collect();
        self
    }

    pub fn with_subcommands(mut self, subcommands: Vec<SubCommand>) -> Self {
        self.subcommands = subcommands;
        self.action = None; // Commands with subcommands don't have direct actions
        self
    }

    /// Check if this command matches the given input (name or alias prefix)
    pub fn matches(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.name.to_lowercase().starts_with(&input_lower)
            || self.aliases.iter().any(|a| a.to_lowercase().starts_with(&input_lower))
    }

    /// Check if this command exactly matches the given input
    pub fn exact_match(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.name.to_lowercase() == input_lower
            || self.aliases.iter().any(|a| a.to_lowercase() == input_lower)
    }

    /// Check if this command has subcommands
    pub fn has_subcommands(&self) -> bool {
        !self.subcommands.is_empty()
    }
}

/// Parsed command result
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub command: String,
    pub subcommand: Option<String>,
    pub args: Vec<String>,
}

/// Trait for components that can provide commands to the command bar
pub trait CommandProvider {
    /// Returns a list of commands available in this context
    fn get_commands(&self) -> Vec<Command>;
    
    /// Returns the context name for display purposes
    fn get_context_name(&self) -> &str;
}

/// Suggestion type for autocomplete
#[derive(Debug, Clone)]
pub enum Suggestion {
    Command { name: String, description: String },
    SubCommand { name: String, description: String },
    Keyword { value: String },
    Field { name: String },
}

impl Suggestion {
    pub fn display_name(&self) -> &str {
        match self {
            Suggestion::Command { name, .. } => name,
            Suggestion::SubCommand { name, .. } => name,
            Suggestion::Keyword { value } => value,
            Suggestion::Field { name } => name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Suggestion::Command { description, .. } => description,
            Suggestion::SubCommand { description, .. } => description,
            Suggestion::Keyword { .. } => "keyword",
            Suggestion::Field { .. } => "field",
        }
    }
}

/// Maximum visible suggestions in the dropdown
const MAX_VISIBLE_SUGGESTIONS: usize = 8;

/// The command bar dialog state
#[derive(Debug, Clone)]
pub struct CommandBar {
    /// The current input text
    pub input: String,
    /// Cursor position in the input
    pub cursor: usize,
    /// Available commands in the current context
    pub commands: Vec<Command>,
    /// Available field names for autocomplete
    pub field_names: Vec<String>,
    /// Current suggestions based on input
    pub suggestions: Vec<Suggestion>,
    /// Current completion index (for Tab cycling)
    pub completion_index: Option<usize>,
    /// Scroll offset for suggestions list
    pub suggestion_scroll_offset: usize,
    /// Whether the command bar is active
    pub active: bool,
    /// Context name (e.g., "DataTable", "Filter", etc.)
    pub context_name: String,
    /// Config for keybindings
    pub config: Config,
    /// Error message to display (if any)
    pub error_message: Option<String>,
}

impl Default for CommandBar {
    fn default() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            commands: Vec::new(),
            field_names: Vec::new(),
            suggestions: Vec::new(),
            completion_index: None,
            suggestion_scroll_offset: 0,
            active: false,
            context_name: String::new(),
            config: Config::default(),
            error_message: None,
        }
    }
}

impl CommandBar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the command bar with the given commands, field names, and context
    pub fn open(&mut self, commands: Vec<Command>, field_names: Vec<String>, context_name: impl Into<String>) {
        self.input.clear();
        self.cursor = 0;
        self.commands = commands;
        self.field_names = field_names;
        self.completion_index = None;
        self.suggestion_scroll_offset = 0;
        self.context_name = context_name.into();
        self.error_message = None;
        self.update_suggestions();
        self.active = true;
        debug!("CommandBar opened with {} commands, {} fields in context: {}", 
               self.commands.len(), self.field_names.len(), self.context_name);
    }

    /// Close the command bar
    pub fn close(&mut self) {
        self.active = false;
        self.input.clear();
        self.cursor = 0;
        self.completion_index = None;
        self.suggestion_scroll_offset = 0;
        self.suggestions.clear();
        self.error_message = None;
    }

    /// Parse the current input into tokens
    fn tokenize_input(&self) -> Vec<String> {
        // Simple tokenization - split by whitespace
        // TODO: Handle quoted strings for field names with spaces
        self.input.split_whitespace().map(String::from).collect()
    }

    /// Get the current token being typed (for autocomplete)
    fn current_token(&self) -> (usize, String) {
        let before_cursor = &self.input[..self.cursor];
        let tokens: Vec<&str> = before_cursor.split_whitespace().collect();
        let token_idx = tokens.len().saturating_sub(1);
        
        // Check if cursor is right after a space (starting new token)
        if before_cursor.ends_with(' ') || before_cursor.is_empty() {
            (tokens.len(), String::new())
        } else {
            (token_idx, tokens.last().map(|s| s.to_string()).unwrap_or_default())
        }
    }

    /// Update suggestions based on current input
    fn update_suggestions(&mut self) {
        self.suggestions.clear();
        let tokens = self.tokenize_input();
        let (current_token_idx, current_partial) = self.current_token();

        if tokens.is_empty() || (tokens.len() == 1 && current_token_idx == 0) {
            // Suggesting main commands
            for cmd in &self.commands {
                if cmd.matches(&current_partial) {
                    self.suggestions.push(Suggestion::Command {
                        name: cmd.name.clone(),
                        description: cmd.description.clone(),
                    });
                }
            }
        } else {
            // We have at least one token - find the command
            let cmd_name = &tokens[0];
            if let Some(cmd) = self.commands.iter().find(|c| c.exact_match(cmd_name)) {
                if cmd.has_subcommands() {
                    if tokens.len() == 1 || (tokens.len() == 2 && current_token_idx == 1) {
                        // Suggesting subcommands
                        let partial = if tokens.len() > 1 { &current_partial } else { "" };
                        for subcmd in &cmd.subcommands {
                            if subcmd.matches(partial) {
                                self.suggestions.push(Suggestion::SubCommand {
                                    name: subcmd.name.clone(),
                                    description: subcmd.description.clone(),
                                });
                            }
                        }
                    } else if tokens.len() >= 2 {
                        // Find the subcommand and suggest arguments
                        let subcmd_name = &tokens[1];
                        if let Some(subcmd) = cmd.subcommands.iter().find(|s| s.exact_match(subcmd_name)) {
                            // Determine which argument we're on
                            let arg_idx = current_token_idx.saturating_sub(2);
                            if arg_idx < subcmd.args.len() {
                                let arg_spec = &subcmd.args[arg_idx];
                                match &arg_spec.arg_type {
                                    ArgType::FieldName => {
                                        // Suggest field names
                                        for field in &self.field_names {
                                            if field.to_lowercase().starts_with(&current_partial.to_lowercase()) {
                                                self.suggestions.push(Suggestion::Field {
                                                    name: field.clone(),
                                                });
                                            }
                                        }
                                    }
                                    ArgType::Keyword(options) => {
                                        // Suggest keyword options
                                        for opt in options {
                                            if opt.to_lowercase().starts_with(&current_partial.to_lowercase()) {
                                                self.suggestions.push(Suggestion::Keyword {
                                                    value: opt.clone(),
                                                });
                                            }
                                        }
                                    }
                                    ArgType::Text => {
                                        // No autocomplete for free text
                                    }
                                }
                            }
                        }
                    }
                } else if !cmd.subcommands.is_empty() {
                    // Command has subcommands, suggest them
                    for subcmd in &cmd.subcommands {
                        if subcmd.matches(&current_partial) {
                            self.suggestions.push(Suggestion::SubCommand {
                                name: subcmd.name.clone(),
                                description: subcmd.description.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Reset completion index and scroll offset if out of bounds
        if let Some(idx) = self.completion_index {
            if idx >= self.suggestions.len() {
                self.completion_index = if self.suggestions.is_empty() { None } else { Some(0) };
            }
        }
        // Reset scroll offset when suggestions change
        self.suggestion_scroll_offset = 0;
    }

    /// Apply the current completion to the input
    fn apply_completion(&mut self) {
        if let Some(idx) = self.completion_index {
            if let Some(suggestion) = self.suggestions.get(idx) {
                let (token_idx, _) = self.current_token();
                let tokens: Vec<&str> = self.input.split_whitespace().collect();
                
                // Build new input with the completed token
                let mut new_tokens: Vec<String> = tokens.iter()
                    .take(token_idx)
                    .map(|s| s.to_string())
                    .collect();
                
                let completion_text = suggestion.display_name();
                // If field name contains spaces, quote it
                let completion = if completion_text.contains(' ') {
                    format!("\"{}\"", completion_text)
                } else {
                    completion_text.to_string()
                };
                
                new_tokens.push(completion);
                
                self.input = new_tokens.join(" ");
                // Add a trailing space if this isn't the final argument
                self.input.push(' ');
                self.cursor = self.input.len();
                self.completion_index = None;
                self.update_suggestions();
            }
        }
    }

    /// Cycle to the next completion suggestion
    fn next_completion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.completion_index = Some(match self.completion_index {
            None => 0,
            Some(idx) => (idx + 1) % self.suggestions.len(),
        });
        
        // Ensure the selected item is visible by adjusting scroll offset
        self.ensure_selected_visible();
    }

    /// Cycle to the previous completion suggestion
    fn prev_completion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.completion_index = Some(match self.completion_index {
            None => self.suggestions.len().saturating_sub(1),
            Some(0) => self.suggestions.len().saturating_sub(1),
            Some(idx) => idx - 1,
        });
        
        // Ensure the selected item is visible by adjusting scroll offset
        self.ensure_selected_visible();
    }
    
    /// Ensure the currently selected suggestion is visible in the scrollable area
    fn ensure_selected_visible(&mut self) {
        if let Some(idx) = self.completion_index {
            // If selected is before the visible area, scroll up
            if idx < self.suggestion_scroll_offset {
                self.suggestion_scroll_offset = idx;
            }
            // If selected is after the visible area, scroll down
            else if idx >= self.suggestion_scroll_offset + MAX_VISIBLE_SUGGESTIONS {
                self.suggestion_scroll_offset = idx.saturating_sub(MAX_VISIBLE_SUGGESTIONS - 1);
            }
        }
    }

    /// Try to execute the current input as a command
    fn try_execute(&mut self) -> Option<Action> {
        let input = self.input.trim();
        if input.is_empty() {
            self.close();
            return Some(Action::CloseCommandBar);
        }

        let tokens = self.tokenize_input();
        if tokens.is_empty() {
            self.close();
            return Some(Action::CloseCommandBar);
        }

        let cmd_name = &tokens[0];
        
        // Find the command
        let cmd = self.commands.iter().find(|c| c.exact_match(cmd_name));
        
        if let Some(cmd) = cmd {
            if cmd.has_subcommands() {
                // Command with subcommands
                if tokens.len() < 2 {
                    self.error_message = Some(format!("Usage: {} <subcommand>", cmd.name));
                    return None;
                }
                
                let subcmd_name = &tokens[1];
                if let Some(subcmd) = cmd.subcommands.iter().find(|s| s.exact_match(subcmd_name)) {
                    // Parse arguments
                    let args: Vec<String> = tokens.iter().skip(2).cloned().collect();
                    
                    // Check required arguments
                    let required_count = subcmd.args.iter().filter(|a| a.required).count();
                    if args.len() < required_count {
                        self.error_message = Some(format!("Usage: {} {}", cmd.name, subcmd.usage()));
                        return None;
                    }
                    
                    // Build the action based on command + subcommand + args
                    let action = self.build_action(&cmd.name, &subcmd.name, &args);
                    
                    if let Some(action) = action {
                        // Check if this is a closing action
                        if matches!(action, Action::DialogClose | Action::CloseCommandBar | Action::Quit) {
                            self.close();
                        } else {
                            // Clear input but keep command bar open
                            self.input.clear();
                            self.cursor = 0;
                            self.completion_index = None;
                            self.update_suggestions();
                        }
                        return Some(action);
                    } else {
                        self.error_message = Some(format!("Unknown subcommand: {} {}", cmd.name, subcmd_name));
                        return None;
                    }
                } else {
                    self.error_message = Some(format!("Unknown subcommand: {}", subcmd_name));
                    return None;
                }
            } else if let Some(action) = cmd.action.clone() {
                // Simple command with direct action
                if matches!(action, Action::DialogClose | Action::CloseCommandBar | Action::Quit) {
                    self.close();
                } else {
                    self.input.clear();
                    self.cursor = 0;
                    self.completion_index = None;
                    self.update_suggestions();
                }
                return Some(action);
            }
        }

        // No match found - show error
        self.error_message = Some(format!("Unknown command: {}", cmd_name));
        None
    }

    /// Build an action from parsed command parts
    fn build_action(&self, cmd: &str, subcmd: &str, args: &[String]) -> Option<Action> {
        match (cmd.to_lowercase().as_str(), subcmd.to_lowercase().as_str()) {
            // Sort commands
            ("sort", "add") => {
                let (direction, field) = self.parse_sort_args(args);
                Some(Action::SortAddColumn { 
                    column: field?, 
                    ascending: direction.map(|d| d == "ascending").unwrap_or(true)
                })
            }
            ("sort", "remove") | ("sort", "rm") => {
                let field = args.first()?.clone();
                Some(Action::SortRemoveColumn { column: field })
            }
            ("sort", "reverse") | ("sort", "toggle") => {
                let field = args.first()?.clone();
                Some(Action::SortToggleColumn { column: field })
            }
            ("sort", "clear") => Some(Action::SortClear),
            ("sort", "apply") => Some(Action::Enter),
            
            // Filter commands
            ("filter", "add") => {
                // filter add <column> <operator> <value>
                if args.len() >= 3 {
                    Some(Action::FilterAddCondition {
                        column: args[0].clone(),
                        operator: args[1].clone(),
                        value: args[2..].join(" "),
                    })
                } else {
                    None
                }
            }
            ("filter", "remove") | ("filter", "rm") => {
                Some(Action::DeleteFilter)
            }
            ("filter", "clear") => Some(Action::ResetFilters),
            ("filter", "apply") => Some(Action::Enter),
            
            // Generic close/cancel
            (_, "close") | (_, "q") => Some(Action::CloseCommandBar),
            (_, "cancel") => Some(Action::DialogClose),
            
            _ => None,
        }
    }

    /// Parse sort command arguments (handles optional direction keyword)
    fn parse_sort_args(&self, args: &[String]) -> (Option<String>, Option<String>) {
        if args.is_empty() {
            return (None, None);
        }
        
        let first = args[0].to_lowercase();
        if first == "ascending" || first == "asc" || first == "descending" || first == "desc" {
            let direction = if first.starts_with("asc") { "ascending" } else { "descending" };
            let field = args.get(1).cloned();
            (Some(direction.to_string()), field)
        } else {
            // First arg is the field name
            (None, Some(args[0].clone()))
        }
    }

    /// Handle a key event
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // Clear error message on any key press
        if self.error_message.is_some() && key.code != KeyCode::Esc {
            self.error_message = None;
        }

        match key.code {
            KeyCode::Esc => {
                self.close();
                return Some(Action::CloseCommandBar);
            }
            KeyCode::Enter => {
                return self.try_execute();
            }
            KeyCode::Tab => {
                if self.completion_index.is_some() {
                    // Apply the current completion
                    self.apply_completion();
                } else if !self.suggestions.is_empty() {
                    // Start completion cycling
                    self.completion_index = Some(0);
                }
            }
            KeyCode::BackTab => {
                self.prev_completion();
            }
            KeyCode::Backspace => {
                if self.cursor > 0 && !self.input.is_empty() {
                    let mut chars: Vec<char> = self.input.chars().collect();
                    chars.remove(self.cursor - 1);
                    self.input = chars.into_iter().collect();
                    self.cursor -= 1;
                    self.completion_index = None;
                    self.update_suggestions();
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.input.len() {
                    let mut chars: Vec<char> = self.input.chars().collect();
                    chars.remove(self.cursor);
                    self.input = chars.into_iter().collect();
                    self.completion_index = None;
                    self.update_suggestions();
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.completion_index = None;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                    self.completion_index = None;
                }
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.completion_index = None;
            }
            KeyCode::End => {
                self.cursor = self.input.len();
                self.completion_index = None;
            }
            KeyCode::Up => {
                self.prev_completion();
            }
            KeyCode::Down => {
                self.next_completion();
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) 
                    && !key.modifiers.contains(KeyModifiers::ALT) {
                    self.input.insert(self.cursor, c);
                    self.cursor += 1;
                    self.completion_index = None;
                    self.update_suggestions();
                }
            }
            _ => {}
        }

        None
    }

    /// Render the command bar
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.active {
            return;
        }

        // Calculate bar height based on completion suggestions and scroll indicators
        let total_suggestions = self.suggestions.len();
        let suggestions_to_show = total_suggestions.min(MAX_VISIBLE_SUGGESTIONS);
        let scroll_offset = self.suggestion_scroll_offset.min(total_suggestions.saturating_sub(MAX_VISIBLE_SUGGESTIONS));
        let can_scroll_up = scroll_offset > 0;
        let can_scroll_down = scroll_offset + suggestions_to_show < total_suggestions;
        let scroll_indicator_lines = (if can_scroll_up { 1 } else { 0 }) + (if can_scroll_down { 1 } else { 0 });
        let bar_height = if suggestions_to_show > 0 { 
            3 + suggestions_to_show as u16 + scroll_indicator_lines
        } else { 
            3 
        };
        
        // Position at the bottom of the screen
        let bar_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(bar_height),
            width: area.width,
            height: bar_height,
        };

        // Clear the area
        Clear.render(bar_area, buf);

        // Draw background
        let bg_style = Style::default().bg(Color::Rgb(30, 30, 40));
        for y in bar_area.y..bar_area.y + bar_area.height {
            for x in bar_area.x..bar_area.x + bar_area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(bg_style);
                }
            }
        }

        // Draw border
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 120)))
            .style(bg_style);
        block.render(bar_area, buf);

        let inner_y = bar_area.y + 1;

        // Draw the input line with colon prefix
        let prefix = ":";
        let prefix_style = Style::default().fg(Color::Rgb(255, 200, 100)).bold();
        buf.set_string(bar_area.x + 1, inner_y, prefix, prefix_style);

        // Draw input text with cursor
        let input_x = bar_area.x + 2;
        for (i, c) in self.input.chars().enumerate() {
            let style = if i == self.cursor {
                Style::default().fg(Color::Black).bg(Color::Rgb(255, 200, 100))
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(input_x + i as u16, inner_y, c.to_string(), style);
        }

        // Draw cursor at end if cursor is at end of input
        if self.cursor == self.input.len() {
            buf.set_string(
                input_x + self.input.len() as u16, 
                inner_y, 
                " ", 
                Style::default().fg(Color::Black).bg(Color::Rgb(255, 200, 100))
            );
        }

        // Draw context indicator on the right
        let context_text = format!("[{}]", self.context_name);
        let context_x = bar_area.x + bar_area.width.saturating_sub(context_text.len() as u16 + 2);
        buf.set_string(
            context_x, 
            inner_y, 
            &context_text, 
            Style::default().fg(Color::Rgb(100, 100, 120)).italic()
        );

        // Draw error message if present
        if let Some(ref error) = self.error_message {
            let error_x = input_x + self.input.len() as u16 + 3;
            buf.set_string(
                error_x,
                inner_y,
                error,
                Style::default().fg(Color::Red).italic()
            );
        }

        // Draw completion suggestions with scrolling
        if suggestions_to_show > 0 && self.error_message.is_none() {
            let suggestions_y = inner_y + 1;
            let visible_count = suggestions_to_show;
            
            // Show scroll up indicator if there are hidden items above
            let mut y_offset = 0;
            if can_scroll_up {
                buf.set_string(
                    bar_area.x + 2,
                    suggestions_y,
                    "▲ more above",
                    Style::default().fg(Color::Rgb(100, 100, 130)).italic()
                );
                y_offset = 1;
            }
            
            for (display_idx, suggestion) in self.suggestions.iter()
                .skip(scroll_offset)
                .take(MAX_VISIBLE_SUGGESTIONS)
                .enumerate()
            {
                let actual_idx = scroll_offset + display_idx;
                let is_selected = self.completion_index == Some(actual_idx);
                
                let name_style = if is_selected {
                    Style::default().fg(Color::Rgb(30, 30, 40)).bg(Color::Rgb(255, 200, 100)).bold()
                } else {
                    Style::default().fg(Color::Rgb(180, 180, 200))
                };
                
                let desc_style = if is_selected {
                    Style::default().fg(Color::Rgb(50, 50, 60)).bg(Color::Rgb(255, 200, 100))
                } else {
                    Style::default().fg(Color::Rgb(100, 100, 120)).italic()
                };

                // Add type indicator
                let type_indicator = match suggestion {
                    Suggestion::Command { .. } => "",
                    Suggestion::SubCommand { .. } => "→ ",
                    Suggestion::Keyword { .. } => "⚙ ",
                    Suggestion::Field { .. } => "◆ ",
                };

                let y = suggestions_y + y_offset + display_idx as u16;
                
                // Draw selection background if selected
                if is_selected {
                    for x in bar_area.x + 1..bar_area.x + bar_area.width - 1 {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_style(Style::default().bg(Color::Rgb(255, 200, 100)));
                        }
                    }
                }

                // Draw type indicator and name
                buf.set_string(bar_area.x + 2, y, type_indicator, name_style);
                let name_x = bar_area.x + 2 + type_indicator.chars().count() as u16;
                buf.set_string(name_x, y, suggestion.display_name(), name_style);
                
                // Draw description
                let desc_x = name_x + suggestion.display_name().len() as u16 + 2;
                let max_desc_len = (bar_area.width as usize).saturating_sub(suggestion.display_name().len() + 8);
                let desc: String = suggestion.description().chars().take(max_desc_len).collect();
                buf.set_string(desc_x, y, &desc, desc_style);
            }

            // Show scroll down indicator if there are hidden items below
            if can_scroll_down {
                let more_y = suggestions_y + y_offset + visible_count as u16;
                let remaining = total_suggestions - scroll_offset - visible_count;
                let more_text = format!("▼ {} more below", remaining);
                buf.set_string(
                    bar_area.x + 2,
                    more_y,
                    &more_text,
                    Style::default().fg(Color::Rgb(100, 100, 130)).italic()
                );
            }
        }
    }
}

// ============================================================================
// Command Definitions
// ============================================================================

/// Get the default commands available in the DataTableContainer context
pub fn get_datatable_commands() -> Vec<Command> {
    vec![
        // Sort command with subcommands
        Command::new_with_subcommands("sort", "Sort data by columns", vec![
            SubCommand::new("add", "Add column to sort")
                .with_aliases(vec!["a"])
                .with_args(vec![
                    ArgSpec::keyword("direction", vec!["ascending", "descending", "asc", "desc"], false),
                    ArgSpec::field("column", true),
                ]),
            SubCommand::new("remove", "Remove column from sort")
                .with_aliases(vec!["rm", "r"])
                .with_args(vec![ArgSpec::field("column", true)]),
            SubCommand::new("reverse", "Reverse sort direction for column")
                .with_aliases(vec!["toggle", "t"])
                .with_args(vec![ArgSpec::field("column", true)]),
            SubCommand::new("clear", "Clear all sort columns"),
            SubCommand::new("apply", "Apply current sort"),
            SubCommand::new("close", "Close command bar").with_aliases(vec!["q"]),
        ]).with_aliases(vec!["s"]),
        
        // Filter command with subcommands
        Command::new_with_subcommands("filter", "Filter data", vec![
            SubCommand::new("add", "Add filter condition")
                .with_aliases(vec!["a"])
                .with_args(vec![
                    ArgSpec::field("column", true),
                    ArgSpec::keyword("operator", vec!["=", "!=", ">", "<", ">=", "<=", "contains", "startswith", "endswith"], true),
                    ArgSpec { name: "value".to_string(), arg_type: ArgType::Text, required: true, description: "<value>".to_string() },
                ]),
            SubCommand::new("remove", "Remove selected filter").with_aliases(vec!["rm", "r"]),
            SubCommand::new("clear", "Clear all filters"),
            SubCommand::new("apply", "Apply filters"),
            SubCommand::new("close", "Close command bar").with_aliases(vec!["q"]),
        ]).with_aliases(vec!["f", "where"]),
        
        // Simple commands without subcommands
        Command::new("find", "Open find/search dialog", Action::OpenFindDialog)
            .with_aliases(vec!["search", "/"]),
        Command::new("sql", "Open SQL query dialog", Action::OpenSqlDialog),
        Command::new("jmes", "Open JMESPath transform dialog", Action::OpenJmesDialog)
            .with_aliases(vec!["jmespath", "transform"]),
        Command::new("columns", "Open column width/visibility dialog", Action::OpenColumnWidthDialog)
            .with_aliases(vec!["cols", "width"]),
        Command::new("details", "Open dataframe details dialog", Action::OpenDataframeDetailsDialog)
            .with_aliases(vec!["info", "describe"]),
        Command::new("export", "Open data export dialog", Action::OpenDataExportDialog)
            .with_aliases(vec!["save"]),
        Command::new("ops", "Open column operations dialog", Action::OpenColumnOperationsDialog)
            .with_aliases(vec!["operations", "colops"]),
        Command::new("copy", "Copy selected cell value", Action::CopySelectedCell)
            .with_aliases(vec!["yank", "y"]),
        Command::new("embeddings", "Open embeddings prompt dialog", Action::OpenEmbeddingsPromptDialog)
            .with_aliases(vec!["embed", "similarity"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("quit", "Exit the application", Action::Quit)
            .with_aliases(vec!["exit", "qa"]),
        Command::new("help", "Show help/keybindings", Action::OpenKeybindings)
            .with_aliases(vec!["h", "keys", "bindings"]),
    ]
}

/// Get commands for the data tab manager context
pub fn get_tab_manager_commands() -> Vec<Command> {
    vec![
        Command::new("import", "Open data import dialog", Action::OpenDataImportDialog)
            .with_aliases(vec!["i", "open", "load"]),
        Command::new("data", "Open data management dialog", Action::OpenDataManagementDialog)
            .with_aliases(vec!["manage", "sources"]),
        Command::new("settings", "Open project settings", Action::OpenProjectSettingsDialog)
            .with_aliases(vec!["config", "prefs", "preferences"]),
        Command::new("styles", "Open style set manager", Action::OpenStyleSetManagerDialog)
            .with_aliases(vec!["themes", "colors"]),
        Command::new("next", "Switch to next tab", Action::NextTab)
            .with_aliases(vec!["n", "tabn"]),
        Command::new("prev", "Switch to previous tab", Action::PrevTab)
            .with_aliases(vec!["p", "tabp"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("quit", "Exit the application", Action::Quit)
            .with_aliases(vec!["exit", "qa"]),
        Command::new("help", "Show help/keybindings", Action::OpenKeybindings)
            .with_aliases(vec!["h", "keys"]),
    ]
}

/// Get commands for the sort dialog context
pub fn get_sort_dialog_commands() -> Vec<Command> {
    vec![
        Command::new_with_subcommands("add", "Add sort column", vec![
            SubCommand::new("ascending", "Add column with ascending sort")
                .with_aliases(vec!["asc"])
                .with_args(vec![ArgSpec::field("column", true)]),
            SubCommand::new("descending", "Add column with descending sort")
                .with_aliases(vec!["desc"])
                .with_args(vec![ArgSpec::field("column", true)]),
        ]).with_aliases(vec!["a"]),
        Command::new("remove", "Remove selected sort column", Action::RemoveSortColumn)
            .with_aliases(vec!["r", "delete", "del"]),
        Command::new("toggle", "Toggle sort direction (asc/desc)", Action::ToggleSortDirection)
            .with_aliases(vec!["t", "direction", "dir"]),
        Command::new("apply", "Apply sort and close", Action::Enter),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close dialog without applying", Action::DialogClose),
    ]
}

/// Get commands for the filter dialog context
pub fn get_filter_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("add", "Add filter condition", Action::AddFilter)
            .with_aliases(vec!["a", "new"]),
        Command::new("edit", "Edit selected filter", Action::EditFilter)
            .with_aliases(vec!["e", "modify"]),
        Command::new("delete", "Delete selected filter", Action::DeleteFilter)
            .with_aliases(vec!["d", "remove", "del"]),
        Command::new("group", "Add filter group (AND/OR)", Action::AddFilterGroup)
            .with_aliases(vec!["g"]),
        Command::new("toggle", "Toggle group type (AND/OR)", Action::ToggleFilterGroupType)
            .with_aliases(vec!["t"]),
        Command::new("reset", "Reset all filters", Action::ResetFilters)
            .with_aliases(vec!["clear"]),
        Command::new("save", "Save filter preset", Action::SaveFilter),
        Command::new("load", "Load filter preset", Action::LoadFilter),
        Command::new("apply", "Apply filters and close", Action::Enter),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close dialog without applying", Action::DialogClose),
    ]
}

/// Get commands for the find dialog context
pub fn get_find_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("next", "Find next match", Action::Enter)
            .with_aliases(vec!["n", "find"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close find dialog", Action::DialogClose),
    ]
}

/// Get commands for the SQL dialog context
pub fn get_sql_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("run", "Execute SQL query", Action::RunQuery)
            .with_aliases(vec!["exec", "execute", "r"]),
        Command::new("new", "Create new dataset from query", Action::CreateNewDataset)
            .with_aliases(vec!["create", "dataset"]),
        Command::new("restore", "Restore original dataframe", Action::RestoreDataFrame)
            .with_aliases(vec!["reset", "undo"]),
        Command::new("clear", "Clear query text", Action::ClearText)
            .with_aliases(vec!["cls"]),
        Command::new("browse", "Browse SQL files", Action::OpenSqlFileBrowser)
            .with_aliases(vec!["open", "load"]),
        Command::new("copy", "Copy query text", Action::CopyText)
            .with_aliases(vec!["yank"]),
        Command::new("paste", "Paste text", Action::PasteText)
            .with_aliases(vec!["p"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close SQL dialog", Action::DialogClose),
    ]
}

/// Get commands for the JMESPath dialog context  
pub fn get_jmes_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("add", "Add new column", Action::AddColumn)
            .with_aliases(vec!["a", "new"]),
        Command::new("edit", "Edit selected column", Action::EditColumn)
            .with_aliases(vec!["e", "modify"]),
        Command::new("delete", "Delete selected column", Action::DeleteColumn)
            .with_aliases(vec!["d", "remove", "del"]),
        Command::new("apply", "Apply transformation", Action::ApplyTransform)
            .with_aliases(vec!["transform", "run"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close dialog", Action::DialogClose),
    ]
}

/// Get commands for the column width dialog context
pub fn get_column_width_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("auto", "Toggle auto-expand for column", Action::ToggleAutoExpand)
            .with_aliases(vec!["a", "autoexpand"]),
        Command::new("hide", "Toggle column visibility", Action::ToggleColumnHidden)
            .with_aliases(vec!["show", "visible"]),
        Command::new("up", "Move column up", Action::MoveColumnUp)
            .with_aliases(vec!["u"]),
        Command::new("down", "Move column down", Action::MoveColumnDown)
            .with_aliases(vec!["d"]),
        Command::new("apply", "Apply changes and close", Action::Enter),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close dialog without applying", Action::DialogClose),
    ]
}

/// Get commands for the dataframe details dialog context
pub fn get_details_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("next", "Next tab", Action::SwitchToNextTab)
            .with_aliases(vec!["n", "tabn"]),
        Command::new("prev", "Previous tab", Action::SwitchToPrevTab)
            .with_aliases(vec!["p", "tabp"]),
        Command::new("sort", "Open sort choice", Action::OpenSortChoice)
            .with_aliases(vec!["s"]),
        Command::new("cast", "Open cast overlay", Action::OpenCastOverlay)
            .with_aliases(vec!["c", "type"]),
        Command::new("filter", "Add filter from current value", Action::AddFilterFromValue)
            .with_aliases(vec!["f"]),
        Command::new("export", "Export current tab data", Action::ExportCurrentTab)
            .with_aliases(vec!["save"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close details dialog", Action::DialogClose),
    ]
}

/// Get commands for the style set manager dialog context
pub fn get_style_manager_commands() -> Vec<Command> {
    vec![
        Command::new("add", "Add new style set", Action::AddStyleSet)
            .with_aliases(vec!["a", "new"]),
        Command::new("remove", "Remove selected style set", Action::RemoveStyleSet)
            .with_aliases(vec!["r", "delete", "del"]),
        Command::new("import", "Import style set from file", Action::ImportStyleSet)
            .with_aliases(vec!["i", "load"]),
        Command::new("export", "Export style set to file", Action::ExportStyleSet)
            .with_aliases(vec!["e", "save"]),
        Command::new("edit", "Edit selected style set", Action::EditStyleSet)
            .with_aliases(vec!["modify"]),
        Command::new("toggle", "Enable/disable style set", Action::DisableStyleSet)
            .with_aliases(vec!["t", "enable", "disable"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close style manager", Action::DialogClose),
    ]
}

/// Get commands for the keybindings dialog context
pub fn get_keybindings_dialog_commands() -> Vec<Command> {
    vec![
        Command::new("rebind", "Start rebinding selected key", Action::StartRebinding)
            .with_aliases(vec!["r", "bind"]),
        Command::new("clear", "Clear selected binding", Action::ClearBinding)
            .with_aliases(vec!["c", "unbind"]),
        Command::new("save", "Save keybindings to file", Action::SaveKeybindings)
            .with_aliases(vec!["s"]),
        Command::new("saveas", "Save keybindings to custom file", Action::SaveKeybindingsAs)
            .with_aliases(vec!["export"]),
        Command::new("reset", "Reset all keybindings to defaults", Action::ResetKeybindings)
            .with_aliases(vec!["defaults"]),
        Command::new("close", "Close command bar", Action::CloseCommandBar)
            .with_aliases(vec!["q"]),
        Command::new("cancel", "Close keybindings dialog", Action::DialogClose),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_matching() {
        let cmd = Command::new("filter", "Open filter dialog", Action::OpenFilterDialog)
            .with_aliases(vec!["f", "where"]);
        
        assert!(cmd.matches("f"));
        assert!(cmd.matches("fi"));
        assert!(cmd.matches("filter"));
        assert!(cmd.matches("wh"));
        assert!(cmd.matches("where"));
        assert!(!cmd.matches("x"));
        assert!(!cmd.matches("sorting"));
    }

    #[test]
    fn test_subcommand_parsing() {
        let mut bar = CommandBar::new();
        bar.commands = get_datatable_commands();
        bar.field_names = vec!["name".to_string(), "age".to_string(), "city".to_string()];
        
        bar.input = "sort add ".to_string();
        bar.cursor = bar.input.len();
        bar.update_suggestions();
        
        // Should suggest direction keywords and field names
        assert!(!bar.suggestions.is_empty());
    }

    #[test]
    fn test_field_autocomplete() {
        let mut bar = CommandBar::new();
        bar.commands = get_datatable_commands();
        bar.field_names = vec!["name".to_string(), "age".to_string(), "city".to_string()];
        
        bar.input = "sort add ascending n".to_string();
        bar.cursor = bar.input.len();
        bar.update_suggestions();
        
        // Should suggest "name" field
        assert!(bar.suggestions.iter().any(|s| matches!(s, Suggestion::Field { name } if name == "name")));
    }
}
