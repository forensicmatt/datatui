# Cursor Style Rules

This document defines the consistent cursor styling rules used throughout the DataTUI application.

## Cursor Types

### 1. Block Cursor
- **Usage**: Simple text input fields (e.g., configuration dialogs, form inputs)
- **Style**: Black text on white background (inverted colors)
- **Behavior**: Overlays the character at cursor position (doesn't push text)
- **Example**: Azure OpenAI config dialog, SQL dialog dataset name input

### 2. Highlighted Character Cursor
- **Usage**: Search/find fields where the character at cursor position is highlighted
- **Style**: Black foreground with yellow background
- **Example**: Find dialog search pattern, file browser filename input

### 3. Hidden Cursor
- **Usage**: TextArea widgets when not focused
- **Style**: Gray foreground (effectively invisible)
- **Example**: File path inputs when not active

## Implementation

### Config Integration

The cursor styles are now integrated into the main `Config` struct, providing several benefits:

- **Single Configuration Source**: All styling (including cursors) is managed through one `Config` struct
- **Reduced Parameter Passing**: No need to pass separate `StyleConfig` instances around
- **Consistent Access Pattern**: All dialogs access styles via `self.config.style_config`
- **Centralized Management**: Easy to modify cursor styles globally through configuration

### Using the CursorStyle

```rust
use crate::config::Config;

// Get cursor styles from config
let block_cursor_style = config.style_config.cursor.block();
let highlighted_style = config.style_config.cursor.highlighted();
let hidden_style = config.style_config.cursor.hidden();

// Render block cursor (overlay on existing text)
// First draw the full text
buf.set_string(x, y, text, normal_style);

// Then overlay the block cursor at cursor position
let cursor_x = x + text.chars().take(cursor_pos).map(|c| c.len_utf8()).sum::<usize>() as u16;
let char_at_cursor = text.chars().nth(cursor_pos).unwrap_or(' ');
buf.set_string(cursor_x, y, char_at_cursor.to_string(), block_cursor_style);

// Render highlighted cursor
buf.set_string(cursor_x, y, char_at_cursor.to_string(), highlighted_style);

// Hide cursor in TextArea
textarea.set_cursor_style(hidden_style);
```

### Custom Cursor Styles

```rust
use crate::style::{StyleConfig, CursorStyle};
use ratatui::style::{Style, Color};

// Create custom cursor style
let custom_cursor = CursorStyle::new(
    Style::default().fg(Color::Black).bg(Color::Cyan), // block cursor
    Style::default().fg(Color::White).bg(Color::Blue), // highlighted
    Style::default().fg(Color::DarkGray),       // hidden
);

// Apply to config
config.style_config = config.style_config.with_cursor(custom_cursor);
```

## Consistency Guidelines

1. **Always use the centralized cursor styles** from `config.style_config` instead of hardcoded styles
2. **Choose the appropriate cursor type** based on the input field context
3. **Maintain visual consistency** across similar input types
4. **Test cursor visibility** in different terminal themes and color schemes

## Migration

When updating existing cursor implementations:

1. Ensure your dialog has access to the `Config` struct
2. Replace hardcoded cursor styles with `self.config.style_config.cursor.underscore()`
3. Remove any separate `style_config` fields from dialog structs
4. Use `config.style_config` for all cursor styling needs

## Examples

### Before (Hardcoded)
```rust
buf.set_string(cursor_x, y, "_", Style::default().fg(Color::Yellow));
```

### After (Centralized Overlay Block Cursor)
```rust
// Draw full text first
buf.set_string(x, y, text, normal_style);

// Overlay block cursor at cursor position
let cursor_x = x + text.chars().take(cursor_pos).map(|c| c.len_utf8()).sum::<usize>() as u16;
let char_at_cursor = text.chars().nth(cursor_pos).unwrap_or(' ');
buf.set_string(cursor_x, y, char_at_cursor.to_string(), self.config.style_config.cursor.block());
```
