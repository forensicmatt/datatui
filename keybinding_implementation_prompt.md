# Keybinding Configuration Implementation Prompt

**Prompt: Implement Keybinding Configuration for [Dialog Name]**

Please implement the keybinding configuration system for the [Dialog Name] following the established pattern. This involves:

## 1. Add New Action Variants (if needed)
In `src/action.rs`, add any dialog-specific actions that aren't covered by Global actions. Look for hardcoded `KeyCode` patterns in the dialog's `handle_key_event` method to identify what actions are needed.

For example:
```rust
/// [Dialog] specific actions
ActionName1,
ActionName2, 
ActionName3,
```

## 2. Add Mode to Config
In `src/config.rs`, add the new mode to the `Mode` enum:
```rust
[DialogMode],
```

## 3. Add Keybindings to Config
In `.config/config.json5`, add a new section with the dialog's keybindings:
```json5
"[DialogMode]": {
  "<key1>": "ActionName1",
  "<key2>": "ActionName2", 
  "<key3>": "ActionName3"
}
```

## 4. Update Dialog Implementation
In the dialog file:

### A. Add Config Field
```rust
#[serde(skip)]
pub config: Config,
```

### B. Initialize Config in Constructor
```rust
config: Config::default(),
```

### C. Update register_config_handler
```rust
fn register_config_handler(&mut self, _config: Config) -> Result<()> { 
    self.config = _config; 
    Ok(()) 
}
```

### D. Add Config-Based Instructions Method
```rust
/// Build instructions string from configured keybindings
fn build_instructions_from_config(&self) -> String {
    // For simple dialogs with one mode:
    self.config.actions_to_instructions(&[
        (crate::config::Mode::Global, crate::action::Action::Enter),
        (crate::config::Mode::Global, crate::action::Action::Escape),
        (crate::config::Mode::[DialogMode], crate::action::Action::ActionName1),
        (crate::config::Mode::[DialogMode], crate::action::Action::ActionName2),
        (crate::config::Mode::[DialogMode], crate::action::Action::ActionName3),
    ])
}
```

### E. Update Key Event Handling
Replace hardcoded key handling with config-driven actions:

```rust
pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Press {
        // Handle Ctrl+I for instructions toggle if applicable
        
        // First, honor config-driven Global actions
        if let Some(global_action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match global_action {
                Action::Escape => return Some(Action::DialogClose),
                Action::Enter => {
                    // Handle Enter logic based on dialog state
                }
                Action::Up => {
                    // Handle navigation
                }
                Action::Down => {
                    // Handle navigation  
                }
                // Add other Global actions as needed
                _ => {}
            }
        }

        // Next, check for dialog-specific actions
        if let Some(dialog_action) = self.config.action_for_key(crate::config::Mode::[DialogMode], key) {
            match dialog_action {
                Action::ActionName1 => {
                    // Handle dialog-specific action 1
                    return None;
                }
                Action::ActionName2 => {
                    // Handle dialog-specific action 2
                    return None;
                }
                // Add other dialog actions
                _ => {}
            }
        }

        // Fallback for character input or other unhandled keys
        match key.code {
            KeyCode::Char(c) => {
                // Handle character input if needed
            }
            _ => {}
        }
    }
    None
}
```

### F. Update Render Method
Replace hardcoded instruction strings with dynamic config-based instructions:

```rust
// Replace hardcoded instructions like:
// let instructions = "hardcoded instructions";

// With:
let instructions = self.build_instructions_from_config();
let layout = split_dialog_area(area, self.show_instructions, 
    if instructions.is_empty() { None } else { Some(instructions.as_str()) });
```

## 5. Testing Checklist
- [ ] All original functionality preserved
- [ ] Config-based keybindings work correctly
- [ ] Instructions display actual configured keys
- [ ] No hardcoded `KeyCode` patterns remain (except for character input)
- [ ] Multiple dialog modes handled appropriately
- [ ] Global actions work consistently
- [ ] Fallback behavior for unconfigured actions

## Notes:
- Keep character input handling (typing) as hardcoded `KeyCode::Char(c)`
- Preserve any complex navigation logic in Global action handlers
- Handle different dialog modes/states appropriately in both key handling and instructions
- Test with modified keybindings to ensure instructions update correctly
- Follow the existing pattern from SortDialog, FilterDialog, and CsvOptionsDialog
- Use `Config.actions_to_instructions()` instead of manual key formatting
- For simple modes with just Enter/Escape, hardcoded strings are acceptable
- Mix config-driven and hardcoded instructions when appropriate (see FilterDialog example)

## Implementation Examples
Reference the following completed implementations for patterns:
- `src/dialog/sort_dialog.rs` - Simple dialog with two modes
- `src/dialog/filter_dialog.rs` - Complex dialog with multiple modes and file operations
- `src/dialog/csv_options_dialog.rs` - Dialog with custom layout and instructions

### Simple Example: CsvOptionsDialog Implementation
For a simple dialog with one mode:

```rust
fn build_instructions_from_config(&self) -> String {
    self.config.actions_to_instructions(&[
        (crate::config::Mode::Global, crate::action::Action::Up),
        (crate::config::Mode::Global, crate::action::Action::Down),
        (crate::config::Mode::Global, crate::action::Action::Enter),
        (crate::config::Mode::Global, crate::action::Action::Escape),
        (crate::config::Mode::CsvOptions, crate::action::Action::Tab),
        (crate::config::Mode::CsvOptions, crate::action::Action::OpenFileBrowser),
        (crate::config::Mode::CsvOptions, crate::action::Action::Paste),
    ])
}
```

### Complex Example: FilterDialog Implementation
For a dialog with multiple modes:

```rust
fn build_instructions_from_config(&self) -> String {
    match &self.mode {
        FilterDialogMode::List => {
            self.config.actions_to_instructions(&[
                (crate::config::Mode::Global, crate::action::Action::Up),
                (crate::config::Mode::Global, crate::action::Action::Down),
                (crate::config::Mode::Global, crate::action::Action::Left),
                (crate::config::Mode::Global, crate::action::Action::Right),
                (crate::config::Mode::Global, crate::action::Action::Enter),
                (crate::config::Mode::Global, crate::action::Action::Escape),
                (crate::config::Mode::Filter, crate::action::Action::AddFilter),
                (crate::config::Mode::Filter, crate::action::Action::EditFilter),
                (crate::config::Mode::Filter, crate::action::Action::DeleteFilter),
                (crate::config::Mode::Filter, crate::action::Action::AddFilterGroup),
                (crate::config::Mode::Filter, crate::action::Action::SaveFilter),
                (crate::config::Mode::Filter, crate::action::Action::LoadFilter),
                (crate::config::Mode::Filter, crate::action::Action::ResetFilters),
            ])
        }
        FilterDialogMode::Add => {
            "Enter: OK  Esc: Cancel".to_string()
        }
        FilterDialogMode::Edit(_) => {
            "Enter: OK  Esc: Cancel".to_string()
        }
        FilterDialogMode::AddGroup => {
            let instructions = self.config.actions_to_instructions(&[
                (crate::config::Mode::Filter, crate::action::Action::ToggleFilterGroupType),
                (crate::config::Mode::Global, crate::action::Action::Enter),
                (crate::config::Mode::Global, crate::action::Action::Escape),
            ]);
            if instructions.is_empty() {
                "Enter: OK  Esc: Cancel".to_string()
            } else {
                format!("{instructions}  Enter: OK  Esc: Cancel")
            }
        }
        FilterDialogMode::FileBrowser(_) => {
            "Enter: OK  Esc: Cancel".to_string()
        }
    }
}
```

This generates instructions like: `Up: Up  Down: Down  Enter: Enter  Esc: Esc  A: Add Filter  E: Edit Filter  D: Delete Filter  G: Add Group  S: Save Filter  L: Load Filter  R: Reset Filters`

## Key Benefits of Using `actions_to_instructions`
- **Automatic key formatting**: No need to manually format key events
- **Consistent display**: All dialogs use the same key formatting logic
- **Dynamic updates**: Instructions automatically reflect current keybinding configuration
- **Fallback handling**: Gracefully handles unconfigured actions
- **Cleaner code**: Much simpler than manual key formatting and instruction building

This prompt provides a complete template for implementing the keybinding configuration system while following the established patterns and ensuring consistency across all dialogs.
