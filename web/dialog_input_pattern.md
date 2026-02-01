# Dialog Input Handling - Standardized Pattern

## Problem Identified

**Bug:** ColumnWidthDialog not receiving input because routing code was missing.

**Root Cause:** When creating the dialog, I added:
1. ✅ Dialog opening logic (Action::OpenColumnWidthDialog)
2. ✅ Dialog result handling
3. ❌ **MISSING:** Dialog action routing in the input pipeline

The dialog was created and rendered, but never received actions because there was no routing block to pass actions to it.

---

## Standard Dialog Pattern (Going Forward)

Every modal dialog implementation requires **3 components**:

### 1. Dialog Opening (in `handle_action`)
```rust
Action::OpenMyDialog => {
    // Create and configure dialog
    let dialog = MyDialog::new(...);
    self.my_dialog = Some(dialog);
    return Ok(()); // Early return to prevent further routing
}
```

### 2. Dialog Action Routing (CRITICAL - often forgotten)
```rust
// Route to my_dialog if active
if let Some(dialog) = &mut self.my_dialog {
    let keep_open = dialog.handle_action(action)?;
    
    // Check for pending results
    if let Some(result) = dialog.take_result() {
        self.handle_my_dialog_result(result)?;
    }
    
    if !keep_open {
        self.my_dialog = None;
    }
    return Ok(()); // Consume action, don't pass to other components
}
```

### 3. Dialog Result Handling
```rust
fn handle_my_dialog_result(&mut self, result: MyDialogResult) -> Result<()> {
    match result {
        MyDialogResult::DoSomething(data) => {
            // Execute action
        }
        MyDialogResult::Close => {}
    }
    Ok(())
}
```

---

## Action Routing Priority Order

The order matters! Process in this priority:

```rust
// 1. App-level actions (Quit, global shortcuts)
match action {
    Action::Quit => { /* ... */ return Ok(()); }
    Action::OpenSomeDialog => { /* ... */ return Ok(()); }
    // ... other action-specific handlers
    _ => {}
}

// 2. MODAL DIALOGS (highest priority for input capture)
// Always check dialogs BEFORE other components
if let Some(dialog) = &mut self.column_width_dialog {
    /* ... route and return */
}

if let Some(dialog) = &mut self.find_dialog {
    /* ... route and return */
}

// 3. PANEL DIALOGS (with focus checks)
if let Some(dialog) = &mut self.find_all_results_dialog {
    if dialog.is_focused() {
        /* ... route and return */
    }
}

// 4. PRIMARY COMPONENTS (last priority)
if let Some(table) = &mut self.data_table {
    if table.is_focused() {
        table.handle_action(action)?;
    }
}

Ok(())
```

**Key Principle:** More specific/modal UI elements get priority over general components.

---

## Dialog Component Requirements

Each dialog must implement:

### 1. Component Trait
```rust
impl Component for MyDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        // Return true to keep dialog open
        // Return false to close dialog
        match action {
            Action::Escape => {
                self.pending_result = Some(DialogResult::Close);
                Ok(false) // Close
            }
            Action::Confirm => {
                // Process and create result
                self.pending_result = Some(DialogResult::Apply(data));
                Ok(false) // Close
            }
            Action::MoveUp => {
                // Handle internally
                Ok(true) // Stay open
            }
            _ => Ok(false) // Unknown actions close
        }
    }
    
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Render dialog UI
    }
    
    fn supported_actions(&self) -> &[Action] { &[] }
    fn name(&self) -> &str { "MyDialog" }
}
```

### 2. Result Retrieval
```rust
pub fn take_result(&mut self) -> Option<DialogResult> {
    self.pending_result.take()
}
```

### 3. DialogResult Enum
```rust
pub enum DialogResult {
    Apply(Config),
    Cancel,
    Close,
}
```

---

## Checklist for New Dialogs

When creating a new dialog, ensure:

### Core Implementation
- [ ] 1. Dialog struct created with `pending_result: Option<DialogResult>`
- [ ] 2. `Component` trait implemented with `handle_action`
- [ ] 3. `take_result()` method implemented
- [ ] 4. `DialogResult` enum defined

### Actions & Keybindings (CRITICAL!)
- [ ] 5. **Dialog-specific actions** added to `Action` enum (e.g., `ToggleVisibility`, `MoveColumnUp`)
- [ ] 6. **Keybindings mapped** for ALL dialog actions in `keybindings.rs`
- [ ] 7. Dialog opening action added to `Action` enum
- [ ] 8. Keybinding added for dialog opening (e.g., `Ctrl+W`)

### App Integration
- [ ] 9. **Opening logic** added to `App::handle_action`
- [ ] 10. **Routing block** added to `App::handle_action` (CRITICAL!)
- [ ] 11. **Result handler** method created (e.g., `handle_my_dialog_result`)
- [ ] 12. Dialog field added to `App` struct
- [ ] 13. Dialog rendering added to `App::render`
- [ ] 14. All test `App` initializations updated

### Verification
- [ ] 15. Test all keybindings work in the dialog
- [ ] 16. Verify dialog receives input (not other components)
- [ ] 17. Verify dialog closes properly and returns results

---

## Keybinding Requirements

**IMPORTANT:** Every action that a dialog handles MUST have a keybinding, or it won't work!

### Example Issue
```rust
// Dialog handles this action
Action::ToggleVisibility => { /* ... */ }

// BUT if there's no keybinding in keybindings.rs:
KeyBinding::new("Space", Action::ToggleVisibility),  // ← REQUIRED!

// Then pressing Space won't trigger the action!
```

### Where to Add Keybindings
In `src/tui/keybindings.rs`, add to the default bindings:

```rust
KeyBindings::default() -> Self {
    let bindings_list = vec![
        // ... existing bindings
        
        // Dialog-specific actions
        KeyBinding::new("Ctrl+w", Action::OpenColumnWidthDialog),
        KeyBinding::new("Space", Action::ToggleVisibility),
        KeyBinding::new("Ctrl+Up", Action::MoveColumnUp),
        KeyBinding::new("Ctrl+Down", Action::MoveColumnDown),
    ];
    // ...
}
```

### Standard Dialog Keybindings
Most dialogs should support:
- **Esc** → Cancel/Close (mapped to `Action::Escape`)
- **Enter** → Confirm/Apply (mapped to `Action::Confirm`)
- **Up/Down** → Navigate (mapped to `Action::MoveUp/Down`)
- **Ctrl+i** → Toggle instructions (mapped to `Action::ToggleHelp`)

### Instructions Block Pattern
Dialogs should include toggleable instructions for better UX:

```rust
pub struct MyDialog {
    // ... other fields
    show_instructions: bool,  // Toggle with Ctrl+i
}

impl Component for MyDialog {
    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::ToggleHelp => {
                self.show_instructions = !self.show_instructions;
                Ok(true)
            }
            // ... other actions
        }
    }
    
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        // ... render main content ...
        
        // Render instructions block at bottom
        if self.show_instructions {
            let instructions_area = Rect {
                y: inner_area.bottom().saturating_sub(5),
                height: 5,
                ..inner_area
            };
            
            let instructions = vec![
                Line::from("Dialog Title:"),
                Line::from("  • Key: Description"),
                Line::from("  • Ctrl+i: Toggle this help"),
            ];
            
            let block = Block::default()
                .borders(Borders::TOP)
                .title("Instructions (Ctrl+i to hide)");
                
            let para = Paragraph::new(instructions)
                .block(block)
                .wrap(Wrap { trim: true });
                
            frame.render_widget(para, instructions_area);
        } else {
            // Show minimal hint
            frame.render_widget(
                Paragraph::new("Press Ctrl+i for help")
                    .style(Style::default().fg(Color::DarkGray)),
                hint_area
            );
        }
    }
}
```

**Benefits:**
- Prevents overlay issues - instructions in dedicated block
- Text wrapping handles long descriptions
- Toggleable to save screen space
- Consistent UX across all dialogs

---

## Common Mistakes to Avoid

1. ❌ **Forgetting routing block** - Dialog created but never receives input
2. ❌ **Wrong routing order** - DataTable gets input before dialog
3. ❌ **Missing early return** - Action falls through to other components
4. ❌ **No focus management** - Multiple components think they have focus
5. ❌ **Not calling `take_result()`** - Results never processed
6. ❌ **Missing keybindings** - Actions defined but not bound to keys (NEW!)
7. ❌ **Incomplete action enum** - Dialog uses actions not in Action enum (NEW!)
8. ❌ **Not testing keybindings** - Assuming keybindings work without testing (NEW!)

---

## Example: Full Implementation

See `ColumnWidthDialog` for reference implementation following this pattern.

**Files:**
- Dialog: `src/tui/components/column_width_dialog.rs`
- App Integration: `src/tui/app.rs` (lines 316-328, routing block to be added)
- Actions: `src/tui/action.rs`
