# DataTUI

A fast, keyboard‑first terminal data viewer built with Rust and Ratatui. DataTUI lets you explore CSV/TSV, Excel, and SQLite data with tabs, sorting, filtering, SQL (via Polars), and more.

## Features

- Tabbed data views with quick navigation
- CSV/TSV, Excel, and SQLite import flows
- Polars‑backed SQL queries and lazy evaluation
- Sorting, filtering (builder dialog + quick filters), column width management
- Find, Find All with contextual results, and value viewer with optional auto‑expand
- JMESPath transforms and Add Columns from expressions
- Workspace persistence (state + current views) with Parquet snapshots

## Install

From source in this repo:

```bash
cargo build --release
# binary at target/release/datatui
```

Or run locally:

```bash
cargo run --release --bin datatui
```

## Usage

```bash
datatui [--logging error|warn|info|debug|trace]
```

Quick keys inside DataTUI:

- Ctrl+M: Data Management
- Alt+S: Project Settings
- Ctrl+S/E/T/J/W/D/F/I: Sort / Filter / SQL / JMES / Widths / Details / Find / Toggle instructions
- Alt+Left/Right: Switch tabs
- Alt+F/B/L/R: Reorder tabs (front/back/left/right)

## Workspaces and persistence

When a valid workspace folder is set in Project Settings, DataTUI persists:

- State file: `datatui_workspace_state.json`
- Settings file: `datatui_workspace_settings.json`
- Current DataFrame snapshots: `.datatui/tabs/<dataset-id>.parquet`

On exit or when requested, TDV writes the current view to Parquet per tab (if applicable) so you can quickly resume where you left off.

## Development

- Rust toolchain required
- Build: `cargo build` (or `--release`)
- Run tests (if present): `cargo test`

