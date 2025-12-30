# DataTUI

A fast, keyboard‑first terminal data viewer built with Rust. DataTUI lets you explore CSV, Parquet, and other data formats with intuitive keyboard navigation, customizable keybindings, and powerful data operations.

> **Note:** DataTUI is currently being rewritten with a modern architecture. The `main` branch contains the stable version. The `rewrite` branch (this code) is the new foundation with improved design.

## Features

### Current (Rewrite Branch - Phase 2 Complete)

- ✅ **Efficient Data Import** - CSV/Parquet with streaming conversion
- ✅ **Memory Efficient** - DuckDB-backed, data stays on disk
- ✅ **Workspace Management** - Project-based data organization
- ✅ **Type-Safe Models** - Fully tested core foundation
- 🚧 **TUI & Navigation** - In progress (Phase 3)

### Planned (Phases 3-8)

- 🔜 Tabbed data views with keyboard navigation
- 🔜 Sorting, filtering, and search
- 🔜 SQL queries via DuckDB
- 🔜 Customizable keybindings and themes
- 🔜 Column operations and data transformations

## Installation

From source:

```bash
git clone https://github.com/strozfriedberg/datatui
cd datatui
git checkout rewrite  # For new architecture
cargo build --release
# Binary at target/release/datatui
```

Or run locally:

```bash
cargo run --release
```

## Usage

```bash
datatui [FILE]
```

## Keybindings

DataTUI uses a flexible, user-customizable keybinding system. All keybindings are configured via JSON files.

### Configuration Location

`~/.datatui/keybindings.json` (auto-created with defaults on first run)

### Key Notation

**Prefer Actual Characters** (recommended for international keyboards):
```json
{"key": "G", "action": "GoToBottom"}
{"key": "?", "action": "ToggleHelp"}
{"key": ":", "action": "Query"}
{"key": "$", "action": "End"}
```

**Explicit Shift Notation** also supported:
```json
{"key": "Shift+g", "action": "GoToBottom"}
{"key": "Shift+/", "action": "ToggleHelp"}
{"key": "Shift+;", "action": "Query"}
{"key": "Shift+4", "action": "End"}
```

Both formats work identically. **Actual characters are preferred** for:
- ✅ International keyboard layout compatibility (AZERTY, QWERTZ, etc.)
- ✅ Follows vim/less/tmux conventions
- ✅ More intuitive and readable
- ✅ Less verbose

### Modifier Keys

- `Ctrl+key` - Control modifier
- `Alt+key` - Alt/Option modifier
- `Shift+key` - Shift modifier (or use uppercase char)
- `Cmd+key` - Mac Command key (auto-mapped to Ctrl on other platforms)

### Default Keybindings

| Key | Action | Description |
|-----|--------|-------------|
| **Navigation** |||
| `↑`,`k` | MoveUp | Move cursor up |
| `↓`,`j` | MoveDown | Move cursor down |
| `←`,`h` | MoveLeft | Move cursor left |
| `→`,`l` | MoveRight | Move cursor right |
| `PageUp`, `Ctrl+u` | PageUp | Page up |
| `PageDown`, `Ctrl+d` | PageDown | Page down |
| `Home`, `0` | Home | Go to start of row |
| `End`, `$` | End | Go to end of row |
| `g` | GoToTop | Go to first row |
| `G` | GoToBottom | Go to last row |
| **Data Operations** |||
| `s` | Sort | Sort column |
| `f` | Filter | Filter data |
| `/` | Find | Find in data |
| `:` | Query | SQL query |
| `r`, `F5` | Refresh | Refresh view |
| **Application** |||
| `q` | Quit | Quit application |
| `?`, `F1` | ToggleHelp | Toggle help screen |
| `Esc` | Cancel | Cancel action |
| `Enter` | Confirm | Confirm action |
| **Tabs** |||
| `Tab` | NextTab | Next tab |
| `Shift+Tab` | PrevTab | Previous tab |
| `t` | NewTab | New tab |
| `w` | CloseTab | Close tab |
| **Clipboard** |||
| `c` | Copy | Copy cell |
| `C` | CopyWithHeaders | Copy with headers |
| **File** |||
| `o` | Import | Import data |
| `e` | Export | Export data |

### Customizing Keybindings

1. **View current config**:
   ```bash
   cat ~/.datatui/keybindings.json
   ```

2. **Edit** (example: change Quit to 'x'):
   ```json
   {
     "bindings": [
       {"key": "x", "action": "Quit"},
       ...
     ]
   }
   ```

3. **Reload** - Automatically loaded on next launch

4. **Restore defaults** - Delete config file, will be recreated

### Example Config

See [`examples/keybindings.json`](examples/keybindings.json) for the complete default configuration.

### Validation

DataTUI automatically validates keybindings on load:
- ✅ Warns about duplicate key assignments
- ✅ Detects actions with no keybindings
- ✅ Checks for invalid key patterns

## Development

### Project Structure

```
src/
├── core/           # Domain models & DuckDB schemas
├── services/       # Business logic (DataService)
├── tui/            # Terminal UI (actions, keybindings)
└── api/            # Unified API layer (planned)
```

### Testing

```bash
# All tests
cargo test

# Specific module
cargo test --lib core::
cargo test --lib services::
cargo test --lib tui::
```

**Current test status**: 25/25 passing ✅

### Building

```bash
# Development
cargo build

# Release (optimized)
cargo build --release

# With all features
cargo build --all-features
```

## Architecture

DataTUI uses a layered architecture:

1. **Core** - Type-safe domain models, DuckDB schemas
2. **Services** - Data import, workspace management
3. **TUI** - Components, actions, keybindings  
4. **API** - Unified interface (in progress)

**Data Flow**:
- CSV/Parquet → DuckDB → Memory-mapped queries → TUI
- Everything stays on disk, only visible data in memory

**Design Principles**:
- Memory efficiency first
- User customization at every level
- Type safety via Rust
- Extensive testing

## Contributing

Contributions welcome! This is an active rewrite, so check the project board and open issues.

### Development Setup

1. Clone repo
2. `cargo build`
3. `cargo test` (should pass all tests)
4. Read [`keybindings_styling_plan.md`](keybindings_styling_plan.md) for architecture

## License

Apache 2.0

## Credits

Built with:
- [Ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [DuckDB](https://duckdb.org/) - Analytical database
- [Crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
