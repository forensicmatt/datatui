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
    use std::fmt::Write as _;
    fn fmt_key_event(key: &crossterm::event::KeyEvent) -> String {
        // [Standard key formatting function - copy from existing implementation]
    }
    
    fn fmt_sequence(seq: &[crossterm::event::KeyEvent]) -> String {
        let parts: Vec<String> = seq.iter().map(fmt_key_event).collect();
        parts.join(", ")
    }

    let mut segments: Vec<String> = Vec::new();

    // Handle Global actions (Escape, Enter, Up, Down, Left, Right, Backspace)
    if let Some(global_bindings) = self.config.keybindings.0.get(&crate::config::Mode::Global) {
        // Add relevant Global actions for this dialog
    }

    // Handle dialog-specific actions  
    if let Some(dialog_bindings) = self.config.keybindings.0.get(&crate::config::Mode::[DialogMode]) {
        // Add dialog-specific actions
    }

    // Handle different dialog modes/states if applicable

    // Join segments
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 { let _ = write!(out, "  "); }
        let _ = write!(out, "{}", seg);
    }
    out
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

## Implementation Examples
Reference the following completed implementations for patterns:
- `src/dialog/sort_dialog.rs` - Simple dialog with two modes
- `src/dialog/filter_dialog.rs` - Complex dialog with multiple modes and file operations
- `src/dialog/csv_options_dialog.rs` - Dialog with custom layout and instructions

## Standard Key Formatting Function
Copy this helper function for consistent key display:

```rust
fn fmt_key_event(key: &crossterm::event::KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut parts: Vec<&'static str> = Vec::with_capacity(3);
    if key.modifiers.contains(KeyModifiers::CONTROL) { parts.push("Ctrl"); }
    if key.modifiers.contains(KeyModifiers::ALT) { parts.push("Alt"); }
    if key.modifiers.contains(KeyModifiers::SHIFT) { parts.push("Shift"); }
    let key_part = match key.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::SHIFT) { c.to_ascii_uppercase().to_string() } else { c.to_string() }
        }
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => "?".to_string(),
    };
    if parts.is_empty() { key_part } else { format!("{}+{}", parts.join("+"), key_part) }
}
```

This prompt provides a complete template for implementing the keybinding configuration system while following the established patterns and ensuring consistency across all dialogs.
