//! KeybindingsDialog: Configure keybindings per grouping (mode) with dropdown, list, and capture overlay
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, BorderType};
use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind};

use crate::action::Action;
use crate::components::Component;
use crate::config::{Config, Mode};
use crate::style::StyleConfig;
use crate::components::dialog_layout::split_dialog_area;
use crate::dialog::KeybindingCaptureDialog;
use crate::dialog::file_browser_dialog::{FileBrowserDialog, FileBrowserAction, FileBrowserMode};
use crate::dialog::MessageDialog;

// No explicit focus enum for now; dropdown and list are navigated via configured keys

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeybindingEntry {
    pub action: Action,
    pub key_display: String,
}

// capture handled by a separate dialog now


#[derive(Debug, Serialize, Deserialize)]
pub struct KeybindingsDialog {
    pub show_instructions: bool,
    #[serde(skip)]
    pub config: Config,
    pub styles: StyleConfig,
    pub selected_grouping: usize,
    pub selected_index: usize,
    pub scroll_offset: usize,
	#[serde(skip)]
	pub groupings_scroll_start: usize,
    #[serde(skip)]
    capture_dialog: Option<KeybindingCaptureDialog>,
    #[serde(skip)]
    pending_rebind_index: Option<usize>,
    #[serde(skip)]
    file_browser: Option<FileBrowserDialog>,
    #[serde(skip)]
    message_dialog: Option<MessageDialog>,
}

impl Default for KeybindingsDialog {
    fn default() -> Self { Self::new() }
}

impl KeybindingsDialog {
    pub fn new() -> Self {
        Self {
            show_instructions: true,
            config: Config::default(),
            styles: StyleConfig::default(),
            selected_grouping: 0,
            selected_index: 0,
            scroll_offset: 0,
			groupings_scroll_start: 0,
            capture_dialog: None,
            pending_rebind_index: None,
            file_browser: None,
            message_dialog: None,
        }
    }

    pub fn get_config(&self) -> Config {
        self.config.clone()
    }

    fn groupings(&self) -> Vec<Mode> {
        vec![
            Mode::DataTabManager,
            Mode::Global,
            Mode::DataTableContainer,
            Mode::DataManagement,
            Mode::DataImport,
            Mode::CsvOptions,
            Mode::Sort,
            Mode::Filter,
            Mode::Find,
            Mode::FindAllResults,
            Mode::JmesPath,
            Mode::SqlDialog,
            Mode::XlsxOptionsDialog,
            Mode::ParquetOptionsDialog,
            Mode::SqliteOptionsDialog,
            Mode::FileBrowser,
            Mode::ColumnWidthDialog,
            Mode::JsonOptionsDialog,
            Mode::AliasEdit,
            Mode::ColumnOperationOptions,
            Mode::ColumnOperations,
            Mode::DataFrameDetails,
            Mode::ProjectSettings,
            Mode::TableExport,
            Mode::KeybindingsDialog,
        ]
    }

    fn current_mode(&self) -> Mode { self.groupings()[self.selected_grouping] }

    fn entries_for_mode(&self, mode: Mode) -> Vec<KeybindingEntry> {
        let mut entries: Vec<KeybindingEntry> = vec![];
        if let Some(map) = self.config.keybindings.0.get(&mode) {
            for (_seq, action) in map.iter() {
                let key_display = self.config.key_for_action(mode, action).unwrap_or_default();
                entries.push(KeybindingEntry { action: action.clone(), key_display });
            }
        }
        entries.sort_by(|a, b| format!("{}", a.action).cmp(&format!("{}", b.action)));
        entries
    }

    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (Mode::Global, Action::Enter),
            (Mode::Global, Action::Escape),
            (Mode::KeybindingsDialog, Action::OpenGroupingDropdown),
            (Mode::KeybindingsDialog, Action::StartRebinding),
            (Mode::KeybindingsDialog, Action::SaveKeybindings),
            (Mode::KeybindingsDialog, Action::SaveKeybindingsAs),
            (Mode::KeybindingsDialog, Action::ResetKeybindings),
        ])
    }

	fn render_dropdown(&mut self, area: Rect, buf: &mut Buffer, focused: bool) {
        let modes = self.groupings();
        let title = "Select Grouping";
        let block = Block::default()
            .borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if modes.is_empty() { return; }

        // Build titles
        let titles: Vec<String> = modes.iter().map(|m| {
            serde_json::to_string(m).unwrap_or_else(|_| format!("{m:?}")).trim_matches('"').to_string()
        }).collect();

		// Measure available width, reserving space for scroll indicators when overflowed
		let available_width_total = inner.width as usize;
        let total_tabs = titles.len();
        let divider_width = 2usize; // spaces between tabs
		let total_text_width: usize = titles.iter().map(|t| t.len()).sum();
        let total_dividers_width = if total_tabs > 1 { (total_tabs - 1) * divider_width } else { 0 };
		let total_required_width = total_text_width + total_dividers_width;

        // Determine overflow precisely based on actual required width vs available
        let overflow = total_required_width > available_width_total.saturating_sub(1);

        if !overflow {
            // Render all titles, clipped to avoid writing into the right border
            let mut x = inner.x;
            let end_exclusive = inner.x
                .saturating_add(inner.width)
                .saturating_sub(1); // leave 1 column margin from border
            for (i, title) in titles.iter().enumerate() {
                if x >= end_exclusive { break; }
                let remaining = end_exclusive.saturating_sub(x) as usize;
                if remaining == 0 { break; }
                let draw_len = remaining.min(title.len());
                let to_draw = &title[..draw_len];
                let mut style = Style::default();
                if i == self.selected_grouping {
                    style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
                }
                if focused { style = style.add_modifier(Modifier::UNDERLINED); }
                buf.set_string(x, inner.y, to_draw, style);
                x = x.saturating_add(draw_len as u16 + divider_width as u16);
            }
            return;
        }

		// Maintain a stable visible window using a persistent scroll start
		let selected = self.selected_grouping.min(total_tabs - 1);
		let mut start_index = self.groupings_scroll_start.min(total_tabs.saturating_sub(1));
		let mut used_width: usize = 0;
		// Provisional capacity: keep 1 right margin and assume both arrows
		let mut cap = inner.width as usize;
		cap = cap
			.saturating_sub(1) // right margin from border
			.saturating_sub(if start_index > 0 { 1 } else { 0 })
			.saturating_sub(1); // tentatively reserve right arrow
		let mut end_index = start_index;
		while end_index < total_tabs {
			let w = titles[end_index].len();
			let needed = if end_index == start_index { w } else { used_width + divider_width + w };
			if needed > cap { break; }
			used_width = needed;
			end_index += 1;
		}
		let mut has_left = start_index > 0;
		let mut has_right = end_index < total_tabs;
		// If no right overflow, reclaim the tentative arrow space and try to fit more
		if !has_right {
			let mut new_cap = inner.width as usize;
			new_cap = new_cap
				.saturating_sub(1)
				.saturating_sub(if has_left { 1 } else { 0 });
			if new_cap > cap {
				cap = new_cap;
				used_width = 0;
				end_index = start_index;
				while end_index < total_tabs {
					let w = titles[end_index].len();
					let needed = if end_index == start_index { w } else { used_width + divider_width + w };
					if needed > cap { break; }
					used_width = needed;
					end_index += 1;
				}
			}
		}
		// New behavior: if the selected tab is the last visible (and there is more to the right),
		// advance the window by one so we reveal the next tab; likewise for left edge.
		if has_right && end_index > start_index && selected + 1 == end_index {
			start_index = start_index.saturating_add(1);
			// recompute fit from new start_index
			used_width = 0;
			cap = inner.width as usize;
			cap = cap
				.saturating_sub(1)
				.saturating_sub(if start_index > 0 { 1 } else { 0 })
				.saturating_sub(1);
			end_index = start_index;
			while end_index < total_tabs {
				let w = titles[end_index].len();
				let needed = if end_index == start_index { w } else { used_width + divider_width + w };
				if needed > cap { break; }
				used_width = needed;
				end_index += 1;
			}
			has_left = start_index > 0;
			has_right = end_index < total_tabs;
		}
		if has_left && selected == start_index {
			start_index = start_index.saturating_sub(1);
			// recompute fit from new start_index
			used_width = 0;
			cap = inner.width as usize;
			cap = cap
				.saturating_sub(1)
				.saturating_sub(if start_index > 0 { 1 } else { 0 })
				.saturating_sub(1);
			end_index = start_index;
			while end_index < total_tabs {
				let w = titles[end_index].len();
				let needed = if end_index == start_index { w } else { used_width + divider_width + w };
				if needed > cap { break; }
				used_width = needed;
				end_index += 1;
			}
			has_left = start_index > 0;
			has_right = end_index < total_tabs;
		}
		// Ensure selection is within the visible window; if it's beyond the right edge,
		// shift start to the right until it becomes visible. Likewise for left edge.
		while selected >= end_index && start_index < total_tabs.saturating_sub(1) {
			start_index += 1;
			// recompute fit from new start_index
			used_width = 0;
			cap = inner.width as usize;
			cap = cap
				.saturating_sub(1)
				.saturating_sub(if start_index > 0 { 1 } else { 0 })
				.saturating_sub(1);
			end_index = start_index;
			while end_index < total_tabs {
				let w = titles[end_index].len();
				let needed = if end_index == start_index { w } else { used_width + divider_width + w };
				if needed > cap { break; }
				used_width = needed;
				end_index += 1;
			}
			has_left = start_index > 0;
			has_right = end_index < total_tabs;
			if !has_right {
				let mut new_cap = inner.width as usize;
				new_cap = new_cap
					.saturating_sub(1)
					.saturating_sub(if has_left { 1 } else { 0 });
				if new_cap > cap {
					cap = new_cap;
					used_width = 0;
					end_index = start_index;
					while end_index < total_tabs {
						let w = titles[end_index].len();
						let needed = if end_index == start_index { w } else { used_width + divider_width + w };
						if needed > cap { break; }
						used_width = needed;
						end_index += 1;
					}
				}
			}
		}
		while selected < start_index {
			if start_index == 0 { break; }
			start_index -= 1;
			used_width = 0;
			cap = inner.width as usize;
			cap = cap
				.saturating_sub(1)
				.saturating_sub(if start_index > 0 { 1 } else { 0 })
				.saturating_sub(1);
			end_index = start_index;
			while end_index < total_tabs {
				let w = titles[end_index].len();
				let needed = if end_index == start_index { w } else { used_width + divider_width + w };
				if needed > cap { break; }
				used_width = needed;
				end_index += 1;
			}
			has_left = start_index > 0;
			has_right = end_index < total_tabs;
			if !has_right {
				let mut new_cap = inner.width as usize;
				new_cap = new_cap
					.saturating_sub(1)
					.saturating_sub(if has_left { 1 } else { 0 });
				if new_cap > cap {
					cap = new_cap;
					used_width = 0;
					end_index = start_index;
					while end_index < total_tabs {
						let w = titles[end_index].len();
						let needed = if end_index == start_index { w } else { used_width + divider_width + w };
						if needed > cap { break; }
						used_width = needed;
						end_index += 1;
					}
				}
			}
		}
		// Persist updated start index
		self.groupings_scroll_start = start_index;

		// Draw left/right indicators
		if has_left {
			buf.set_string(inner.x, inner.y, "◀", Style::default().fg(Color::Yellow));
		}
		if has_right {
			let x = inner.x + inner.width.saturating_sub(1);
			buf.set_string(x, inner.y, "▶", Style::default().fg(Color::Yellow));
		}

        // Render visible titles, shifted if indicators are shown
        let mut x = inner.x + if has_left { 1 } else { 0 };
        let end_exclusive = inner.x
            .saturating_add(inner.width)
            .saturating_sub(1) // margin from border
            .saturating_sub(if has_right { 1 } else { 0 }); // space for right indicator
		for i in start_index..end_index {
            if x >= end_exclusive { break; }
            let title = &titles[i];
            let remaining = end_exclusive.saturating_sub(x) as usize;
            if remaining == 0 { break; }
            let draw_len = remaining.min(title.len());
            let to_draw = &title[..draw_len];

            let mut style = Style::default();
            if i == self.selected_grouping {
                style = style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
            }
            if focused { style = style.add_modifier(Modifier::UNDERLINED); }

            buf.set_string(x, inner.y, to_draw, style);
			x = x.saturating_add(draw_len as u16 + divider_width as u16);
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        let entries = self.entries_for_mode(self.current_mode());
        let total_items = entries.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;
        let content_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        // Header
        let header = Line::from(vec![
            Span::styled("Key", self.styles.table_header),
            Span::raw("  "),
            Span::styled("Action", self.styles.table_header),
        ]);
        buf.set_line(area.x, area.y, &header, content_width);

        // Rows
        for (vis_idx, i) in (start_idx..end_idx).enumerate() {
            let y = area.y + 1 + vis_idx as u16;
            let is_selected = i == self.selected_index;
            let zebra = i % 2 == 0;
            let base = if zebra { self.styles.table_row_even } else { self.styles.table_row_odd };
            let style = if is_selected { self.styles.selected_row } else { base };
            let e = &entries[i];
            let text = format!("{:<20}  {}", e.key_display, self.config.action_to_friendly_name(&e.action));
            buf.set_string(area.x, y, text, style);
        }

        // Scrollbar
        if show_scroll_bar {
            let viewport = max_rows;
            let position_for_bar = if self.scroll_offset == 0 { 0 } else {
                self.scroll_offset
                    .saturating_add(viewport.saturating_sub(1))
                    .min(total_items.saturating_sub(1))
            };
            let scrollbar = ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::Cyan));
            let mut state = ratatui::widgets::ScrollbarState::new(total_items)
                .position(position_for_bar)
                .viewport_content_length(viewport);
            scrollbar.render(area, buf, &mut state);
        }
    }

    fn commit_rebind(&mut self, pressed_keys: Vec<KeyEvent>) {
        let Some(action_index) = self.pending_rebind_index.take() else { return; };
        let mode = self.current_mode();
        let entries_vec = self.entries_for_mode(mode);
        let action_to_set = entries_vec.get(action_index).map(|e| e.action.clone());
        if let Some(action) = action_to_set {
            if let Some(entries) = self.config.keybindings.0.get_mut(&mode) {
                let to_remove: Vec<Vec<KeyEvent>> = entries
                    .iter()
                    .filter_map(|(keys, act)| if act == &action { Some(keys.clone()) } else { None })
                    .collect();
                for k in to_remove { entries.remove(&k); }
                entries.insert(pressed_keys, action);
            }
        }
    }
}

impl Component for KeybindingsDialog {
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if key.kind != KeyEventKind::Press { return Ok(None); }

        // If message dialog is active, handle it first and block other events
        if let Some(ref mut msg) = self.message_dialog {
            if let Some(a) = Component::handle_key_event(msg, key)? {
                if a == Action::DialogClose {
                    self.message_dialog = None;
                    return Ok(None);
                }
            }
            return Ok(None);
        }

        // If capture dialog is active, forward events to it first
        if let Some(ref mut dialog) = self.capture_dialog {
            if let Some(a) = Component::handle_key_event(dialog, key)? {
                match a {
                    Action::ConfirmRebinding => {
                        let pressed = dialog.pressed_keys.clone();
                        if pressed.is_empty() {
                            // Ignore confirm with no key chosen
                            self.capture_dialog = None;
                            self.pending_rebind_index = None;
                            return Ok(None);
                        }
                        self.capture_dialog = None;
                        self.commit_rebind(pressed);
                        return Ok(None);
                    }
                    Action::CancelRebinding => {
                        self.capture_dialog = None;
                        self.pending_rebind_index = None;
                        return Ok(None);
                    }
                    _ => return Ok(Some(a)),
                }
            }
            return Ok(None);
        }

        // If file browser is active (Save As), forward events to it first
        if let Some(ref mut browser) = self.file_browser {
            if let Some(action) = browser.handle_key_event(key) {
                browser.register_config_handler(self.config.clone());
                match action {
                    FileBrowserAction::Selected(path) => {
                        let serialized = self.config.keybindings_to_json5();
                        let _ = std::fs::write(&path, serialized);
                        self.file_browser = None;
                        // Show success message with saved path
                        let message = format!("Saved keybindings to {}", path.display());
                        let mut dlg = MessageDialog::with_title(message, "Saved");
                        let _ = dlg.register_config_handler(self.config.clone());
                        self.message_dialog = Some(dlg);
                        return Ok(None);
                    }
                    FileBrowserAction::Cancelled => {
                        self.file_browser = None;
                        return Ok(None);
                    }
                }
            }
            return Ok(None);
        }

        // Global first
        if let Some(a) = self.config.action_for_key(Mode::Global, key) {
            match a {
                Action::Escape => { return Ok(Some(Action::DialogClose)); }
                Action::Up => {
                    if self.selected_index > 0 { self.selected_index -= 1; }
                    if self.selected_index < self.scroll_offset { self.scroll_offset = self.selected_index; }
                }
                Action::Down => {
                    self.selected_index = self.selected_index.saturating_add(1);
                }
                _ => {}
            }
        }

        // Dialog-specific
        if let Some(a) = self.config.action_for_key(Mode::KeybindingsDialog, key) {
            match a {
                Action::OpenGroupingDropdown | Action::SelectNextGrouping | Action::SelectPrevGrouping => {
                    match a {
                        Action::SelectNextGrouping => {
                            let max = self.groupings().len();
                            if self.selected_grouping + 1 < max { self.selected_grouping += 1; }
                            self.selected_index = 0; self.scroll_offset = 0;
                        }
                        Action::SelectPrevGrouping => {
                            if self.selected_grouping > 0 { self.selected_grouping -= 1; }
                            self.selected_index = 0; self.scroll_offset = 0;
                        }
                        _ => {}
                    }
                    return Ok(None);
                }
                Action::StartRebinding => {
                    let mut dlg = KeybindingCaptureDialog::new();
                    let _ = dlg.register_config_handler(self.config.clone());
                    self.capture_dialog = Some(dlg);
                    self.pending_rebind_index = Some(self.selected_index);
                    return Ok(None);
                }
                Action::ClearBinding => {
                    let mode = self.current_mode();
                    let entries_vec = self.entries_for_mode(mode);
                    let action_to_clear = entries_vec
                        .get(self.selected_index)
                        .map(|e| e.action.clone());
                    if let Some(action) = action_to_clear {
                        if let Some(entries) = self.config.keybindings.0.get_mut(&mode) {
                            let to_remove: Vec<Vec<KeyEvent>> = entries.iter()
                                .filter_map(|(keys, act)| if act == &action { Some(keys.clone()) } else { None })
                                .collect();
                            for k in to_remove { entries.remove(&k); }
                        }
                    }
                    return Ok(None);
                }
                Action::SaveKeybindings => {
                    // Persist to config file path
                    // For now, signal caller to save workspace (reuse existing action)
                    return Ok(Some(Action::SaveWorkspaceState));
                }
                Action::SaveKeybindingsAs => {
                    // Open file browser in Save mode with json/json5 filters
                    let mut browser = FileBrowserDialog::new(None, Some(vec!["json5", "json"]), false, FileBrowserMode::Save);
                    browser.register_config_handler(self.config.clone());
                    browser.filename_input = ".datatui-config.json5".to_string();
                    browser.filename_cursor = browser.filename_input.len();
                    browser.filename_active = true;
                    self.file_browser = Some(browser);
                    return Ok(None);
                }
                Action::ResetKeybindings => {
                    // Restore defaults
                    self.config.reset_keybindings_to_default();
                    // Reset selection and scroll for safety
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                    return Ok(None);
                }
                _ => {}
            }
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame, area: Rect) -> Result<()> {
        let instructions = self.build_instructions_from_config();
        // Clamp selection/scroll based on current entries
        let total_entries = self.entries_for_mode(self.current_mode()).len();
        if total_entries == 0 { self.selected_index = 0; self.scroll_offset = 0; }
        else if self.selected_index >= total_entries { self.selected_index = total_entries - 1; }
        let block: Block<'_> = Block::default()
            .title("Keybindings")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(self.styles.dialog);

        let inner = block.inner(area);
        Clear.render(area, frame.buffer_mut());
        block.render(area, frame.buffer_mut());

        let inner_layout = split_dialog_area(inner, self.show_instructions,
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content = inner_layout.content_area;

        let [dropdown_area, list_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(4)])
            .areas(content);

        self.render_dropdown(dropdown_area, frame.buffer_mut(), true);

        let max_rows = list_area.height.saturating_sub(1) as usize;
        if total_entries > 0 {
            if self.selected_index < self.scroll_offset { self.scroll_offset = self.selected_index; }
            let bottom = self.scroll_offset.saturating_add(max_rows.saturating_sub(1));
            if self.selected_index > bottom { self.scroll_offset = self.selected_index.saturating_sub(max_rows.saturating_sub(1)); }
        }
        self.render_list(list_area, frame.buffer_mut(), max_rows);

        if self.show_instructions {
            if let Some(instr_area) = inner_layout.instructions_area {
                let p = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                p.render(instr_area, frame.buffer_mut());
            }
        }

        // Capture dialog overlay
        if let Some(ref mut dlg) = self.capture_dialog {
            let _ = dlg.draw(frame, content);
        }

        // File browser overlay for Save As
        if let Some(ref mut browser) = self.file_browser {
            let browser_area = Rect {
                x: content.x + content.width / 20,
                y: content.y + content.height / 20,
                width: content.width.saturating_sub(content.width / 10),
                height: content.height.saturating_sub(content.height / 10),
            };
            browser.render(browser_area, frame.buffer_mut());
        }

        // Message dialog overlay (centered, smaller)
        if let Some(ref mut msg) = self.message_dialog {
            let dialog_width = 60.min(content.width.saturating_sub(4));
            let dialog_height = 7.min(content.height.saturating_sub(4));
            let area = Rect {
                x: content.x + (content.width.saturating_sub(dialog_width)) / 2,
                y: content.y + (content.height.saturating_sub(dialog_height)) / 2,
                width: dialog_width,
                height: dialog_height,
            };
            msg.render(area, frame.buffer_mut());
        }

        Ok(())
    }
}
