---
description: Create a new UI dialog component
---

This workflow guides the creation of a new dialog component following project standards.

1. **Scaffold the Component**
   - Create a new file in `src/tui/components/[name]_dialog.rs`.
   - Define the struct with necessary state (including `ScrollbarState` if needed).
   - Implement `new()` method.

2. **Implement Component Trait**
   - Implement `crate::tui::Component` for the struct.
   - Define `name()` method.
   - Define `DialogResult` enum if the dialog needs to return data (e.g. input text).
   - Implement `take_result()` helper.
   - Implement `render()` using `crate::tui::Theme`.
     - Signature: `fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme)`.
     - Use the passed `theme` argument (do not use `Theme::default()`).
     - Use `Borders::ALL` and `BorderType::Double`.
     - **Focused Border**:
       - Add `focused: bool` field to struct (initialize to `true` in `new()`).
       - Implement `Focusable` trait (see step 3 below).
       - Use conditional border style:
         ```rust
         let border_style = if self.focused {
             theme.focused_border_style()
         } else {
             theme.border_style()
         };
         ```
     - **Theme Colors**:
       - Use `theme.normal_style()` for default text
       - Use `theme.selected_style()` for selected items/rows
       - Use `theme.selected_cell_style()` for cursor/active cell
       - Use `theme.alt_row_style()` for zebra striping in tables
       - Use `theme.header_style()` for table headers
       - Use `theme.error_style()` for error messages
       - Use `theme.warning_style()` for warnings/instructions
       - Use `theme.success_style()` for success messages
       - **Never use hardcoded `Color::*` values** - always use theme methods
     - **Instructions Area**:
       - If the dialog has keybinding instructions, render them at the bottom.
       - Use a separate `Rect` for instructions logic (e.g. `instructions_area`).
       - Add `show_instructions: bool` field, default to `false`.
       - Add `toggle_instructions()` method and `Action::ToggleInstructions` handler.
       - Use `Layout` to split area when `show_instructions` is true.
       - Style: `theme.warning_style()` for text.
       - Block: `Borders::TOP` with title `Instructions (Ctrl+i to hide)`.
       - Content: Bulleted list of keys and actions (e.g. `• Enter: Submit`).
     - **Scrollbar**:
       - If content is scrollable, use `ratatui::widgets::Scrollbar`.
       - Place it on the right edge of the content area.
       - Use `theme.focused_border_style()` for scrollbar style.
       - Use `active_index` for position if selection-based, or viewport offset if free-scrolling.
       - **Adjust content width** to prevent selection overlap:
         ```rust
         let has_scrollbar = entries.len() > viewport_height;
         let content_width = if has_scrollbar {
             area.width.saturating_sub(1)
         } else {
             area.width
         };
         ```
   - Implement `handle_action()` logic.
     - Return `Result<bool>`.
     - Handle standard navigation (Esc, Enter).
     - Handle `Action::ToggleInstructions` if instructions are present.

3. **Implement Focusable Trait**
   - Add `focused: bool` field to struct.
   - Implement `crate::tui::Focusable`:
     ```rust
     impl Focusable for MyDialog {
         fn is_focused(&self) -> bool {
             self.focused
         }
         fn set_focused(&mut self, focused: bool) {
             self.focused = focused;
         }
     }
     ```

4. **Register Module**
   - Add `pub mod [name]_dialog;` to `src/tui/components/mod.rs`.
   - Re-export `pub use [name]_dialog::[Name]Dialog;` in `mod.rs`.

5. **Integrate with App**
   - Add field `pub [name]_dialog: Option<[Name]Dialog>` to `App` struct in `src/tui/app.rs`.
   - Initialize as `None` in `App::new`.
   - Update `App::handle_key_event` to delegate to the new dialog:
   ```rust
   } else if let Some(dialog) = &mut self.[name]_dialog {
       match dialog.handle_key_event(key)? {
           KeyEventResult::Consumed => return Ok(()),
           KeyEventResult::Action(a) => { self.handle_action(a)?; return Ok(()); }
           KeyEventResult::Ignored => {}
       }
       // Check for results
       if let Some(result) = dialog.take_result() {
           self.handle_[name]_result(result)?;
       }
   ```

6. **Define Actions and Keybindings**
   - Add new variants to `Action` enum in `src/tui/action.rs` if needed (e.g., `Open[Name]Dialog`).
   - Add default keybindings to `.config/config.json5` under a new scope `[Name]Dialog` or `Global`.

7. **Verify**
   - Run `cargo check` to ensure no errors.
