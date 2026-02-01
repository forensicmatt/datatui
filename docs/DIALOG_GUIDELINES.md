# DataTUI Dialog Creation Guidelines

This document defines the rules and best practices for creating new UI dialogs in DataTUI. All new functionality should adhere to these standards to ensure consistency, maintainability, and a unified user experience.

## 1. Component Structure

All dialogs must implement the `Component` trait defined in `src/tui/component.rs`.

```rust
use crate::tui::{Action, Component, KeyEventResult, Theme};

pub struct MyDialog {
    // State fields
}

impl Component for MyDialog {
    fn name(&self) -> &str {
        "MyDialog"
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<KeyEventResult> {
        // ...
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // ...
    }
}
```

## 2. Event Handling

### Use `KeyEventResult`
Do not return `Option<Action>` directly for key events. Use `Result<KeyEventResult>`:
- `KeyEventResult::Consumed`: The key was handled internally (e.g., text input) and should not propagate.
- `KeyEventResult::Action(Action::...)`: The key triggered a high-level action to be handled by `App`.
- `KeyEventResult::Ignored`: The key was not handled.

### No Hardcoded Keys
**NEVER** hardcode key checks (e.g., `if key.code == KeyCode::Char('q')`) for actions.
- Use `Action` enum for logical operations (e.g., `Action::Close`, `Action::Submit`).
- If specific text input is needed (like a search bar), check for `KeyCode::Char` but ensure it doesn't conflict with global navigation if not focused.
- Escape should usually return `Action::DialogClose` or `Action::Cancel`.

## 3. Styling and Themes

All rendering must use the `Theme` passed to the `render` method. **Do NOT** instantiate `Theme::default()` locally.

```rust
// Correct
fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
    let style = theme.normal_style();
    // ...
}

// Incorrect
fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
    let theme = Theme::default(); // WRONG: Ignores global theme
    // ...
}
```

### Standard Elements
- **Borders**: Use `Borders::ALL` with `BorderType::Rounded`.
- **Title**: all dialogs should have a title.
- **Focus**: The active element should be visually distinct (e.g., usually via `theme.selected_style()` or `theme.border_focused`).

## 4. Navigation and Layout

### Scrollbars
Any content that can exceed the available view area **MUST** include a `Scrollbar`.
- Use `ratatui::widgets::Scrollbar`.
- Track `ScrollbarState` in your component struct.

### Input Fields
- Use `Paragraph` or `tui-input` (if included) for text entry.
- Support standard editing keys: `Backspace`, `Delete`, `Left`, `Right`.
- Support `Home`, `End` for cursor movement.

## 5. Integration with App

- Add the component to `App` struct (usually as `Option<MyDialog>`).
- Update `App::handle_key_event` to delegate to your new component when active.
- Define a new scope in `KeyBindings` if complex navigation is required, or use "Global" actions.

## 6. Complex Data Return (Result Pattern)

For dialogs that return complex data (e.g., a SQL query string or search options) rather than just triggering an action:

1.  **Define a Result Enum**:
    ```rust
    #[derive(Debug, Clone)]
    pub enum DialogResult {
        ExecuteQuery(String),
        Close,
    }
    ```

2.  **Store Pending Result**:
    Add `pending_result: Option<DialogResult>` to your struct.

3.  **Implement `take_result`**:
    ```rust
    pub fn take_result(&mut self) -> Option<DialogResult> {
        self.pending_result.take()
    }
    ```

4.  **Handle in App**:
    In `App::handle_key_event` (or `update`), check for results:
    ```rust
    if let Some(result) = dialog.take_result() {
        self.handle_dialog_result(result)?;
    }
    ```

## 7. Dependencies and Purity

-   **Pure UI**: Dialogs should generally be "Pure UI" components. They should not hold references to `DataService` or perform heavy backend operations directly.
-   **Delegation**: Return the intent (via `Action` or `DialogResult`) and let `App` execute the business logic.
