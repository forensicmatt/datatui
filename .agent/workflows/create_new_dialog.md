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
     - **Instructions Area**:
       - If the dialog has keybinding instructions, render them at the bottom.
       - Use a separate `Rect` for instructions logic (e.g. `instructions_area`).
       - Style: `Color::Yellow` for text.
       - Block: `Borders::TOP` with title `Instructions (Ctrl+i to hide)` (or similar toggle key).
       - Content: Bulleted list of keys and actions (e.g. `• Enter: Submit`).
     - **Scrollbar**:
       - If content is scrollable, use `ratatui::widgets::Scrollbar`.
       - Place it on the right edge of the content area.
       - Use `active_index` for position if selection-based, or viewport offset if free-scrolling.
   - Implement `handle_key_event()` logic.
     - Return `Result<KeyEventResult>`.
     - Handle standard navigation (Esc, Enter).

3. **Register Module**
   - Add `pub mod [name]_dialog;` to `src/tui/components/mod.rs`.
   - Re-export `pub use [name]_dialog::[Name]Dialog;` in `mod.rs`.

4. **Integrate with App**
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

5. **Define Actions and Keybindings**
   - Add new variants to `Action` enum in `src/tui/action.rs` if needed (e.g., `Open[Name]Dialog`).
   - Add default keybindings to `.config/config.json5` under a new scope `[Name]Dialog` or `Global`.

6. **Verify**
   - customized `cargo check` to ensure no errors.
