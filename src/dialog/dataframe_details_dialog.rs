//! DataFrameDetailsDialog: Popup dialog to inspect DataFrame statistics
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::widgets::{Row, Cell, Table};
use crate::components::dialog_layout::split_dialog_area;
use crate::action::Action;
use crate::components::Component;
use color_eyre::Result;
use crossterm::event::{KeyEvent, KeyEventKind, KeyCode};
use polars::prelude::*;
use std::sync::Arc;
use crate::dialog::table_export_dialog::TableExportDialog;
use crate::style::StyleConfig;
use crate::dialog::filter_dialog::{ColumnFilter, FilterCondition};
use serde::{Deserialize, Serialize};
use arboard::Clipboard;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailsTab {
    UniqueValues,
    Columns,
    Describe,
    Heatmap,
    Embeddings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SortBy {
    Value,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusField {
    ColumnDropdown,
    Table,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataFrameDetailsDialog {
    #[serde(skip)]
    pub df: Option<Arc<DataFrame>>,
    pub columns: Vec<String>,
    pub selected_column_idx: usize,
    pub tab: DetailsTab,
    pub focus: FocusField,
    pub show_instructions: bool,
    pub selected_row: usize,
    pub scroll_offset: usize,
    #[serde(skip)]
    unique_counts: Vec<(String, u64)>,
    #[serde(skip)]
    pub export_dialog: Option<TableExportDialog>,
    // Sorting state
    sort_by: SortBy,
    // Inline sort choice overlay state
    sort_choice_open: bool,
    sort_choice_index: usize, // 0 => Value, 1 => Count
    // Columns info for Columns tab
    #[serde(skip)]
    columns_info: Vec<(String, String)>,
    // Styles
    #[serde(skip)]
    style: StyleConfig,
    // Describe tab rows
    #[serde(skip)]
    describe_rows: Vec<DescribeRow>,
    // Horizontal scroll offset for Describe stats columns
    describe_col_offset: usize,
    // Maximum allowed horizontal scroll offset (computed during render based on visible width)
    describe_col_max_offset: usize,
    // Heatmap state
    heatmap_x_col_idx: usize,
    heatmap_y_col_idx: usize,
    #[serde(skip)]
    heatmap_cols: Vec<String>,
    #[serde(skip)]
    heatmap_matrix: Vec<Vec<f64>>,
    // Cast overlay state (Columns tab)
    cast_overlay_open: bool,
    #[serde(skip)]
    cast_options: Vec<(String, DataType)>,
    cast_selected_idx: usize,
    #[serde(skip)]
    cast_error: Option<crate::dialog::error_dialog::ErrorDialog>,
    // Config
    #[serde(skip)]
    pub config: crate::config::Config,
    // Embeddings mapping for Embeddings tab
    #[serde(skip)]
    pub embedding_column_config_mapping: std::collections::HashMap<String, crate::components::datatable_container::EmbeddingColumnConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DescribeRow {
    column: String,
    count: u64,
    mean: Option<f64>,
    std: Option<f64>,
    median: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
}

impl Default for DataFrameDetailsDialog {
    fn default() -> Self { Self::new() }
}

impl DataFrameDetailsDialog {
    pub fn new() -> Self {
        Self {
            df: None,
            columns: Vec::new(),
            selected_column_idx: 0,
            tab: DetailsTab::UniqueValues,
            focus: FocusField::ColumnDropdown,
            show_instructions: true,
            selected_row: 0,
            scroll_offset: 0,
            unique_counts: Vec::new(),
            export_dialog: None,
            sort_by: SortBy::Count,
            sort_choice_open: false,
            sort_choice_index: 1,
            columns_info: Vec::new(),
            style: StyleConfig::default(),
            describe_rows: Vec::new(),
            describe_col_offset: 0,
            describe_col_max_offset: 5, // Default: assume all 6 stats columns, max offset = 5
            heatmap_x_col_idx: 0,
            heatmap_y_col_idx: 0,
            heatmap_cols: Vec::new(),
            heatmap_matrix: Vec::new(),
            cast_overlay_open: false,
            cast_options: Vec::new(),
            cast_selected_idx: 0,
            cast_error: None,
            config: crate::config::Config::default(),
            embedding_column_config_mapping: std::collections::HashMap::new(),
        }
    }

    pub fn set_cast_error(&mut self, message: impl Into<String>) {
        let msg: String = message.into();
        self.cast_error = Some(crate::dialog::error_dialog::ErrorDialog::with_title(msg, "Cast Error"));
    }

    pub fn clear_cast_error(&mut self) {
        self.cast_error = None;
    }

    /// Build instructions string from configured keybindings
    fn build_instructions_from_config(&self) -> String {
        match self.tab {
            DetailsTab::UniqueValues => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToPrevTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToNextTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ChangeColumnLeft),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ChangeColumnRight),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::OpenSortChoice),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::AddFilterFromValue),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ExportCurrentTab),
                    (crate::config::Mode::Global, crate::action::Action::CopyText),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ])
            }
            DetailsTab::Columns => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToPrevTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToNextTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::OpenCastOverlay),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ExportCurrentTab),
                    (crate::config::Mode::Global, crate::action::Action::CopyText),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ])
            }
            DetailsTab::Describe => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToPrevTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToNextTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ScrollStatsLeft),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ScrollStatsRight),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ExportCurrentTab),
                    (crate::config::Mode::Global, crate::action::Action::CopyText),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ])
            }
            DetailsTab::Heatmap => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToPrevTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToNextTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapLeft),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapRight),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapUp),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapDown),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapPageUp),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapPageDown),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapHome),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::NavigateHeatmapEnd),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ])
            }
            DetailsTab::Embeddings => {
                self.config.actions_to_instructions(&[
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToPrevTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::SwitchToNextTab),
                    (crate::config::Mode::DataFrameDetails, crate::action::Action::ExportCurrentTab),
                    (crate::config::Mode::Global, crate::action::Action::ToggleInstructions),
                    (crate::config::Mode::Global, crate::action::Action::Escape),
                ])
            }
        }
    }

    pub fn close_cast_overlay(&mut self) {
        self.cast_overlay_open = false;
        self.cast_error = None;
    }

    pub fn set_columns(&mut self, columns: Vec<String>, current_index: usize) {
        self.columns = columns;
        self.selected_column_idx = current_index.min(self.columns.len().saturating_sub(1));
        self.selected_row = 0;
        self.scroll_offset = 0;
        // Initialize heatmap axes to current and next column if available
        self.heatmap_x_col_idx = self.selected_column_idx;
        self.heatmap_y_col_idx = if self.columns.len() > 1 {
            (self.selected_column_idx + 1).min(self.columns.len() - 1)
        } else { self.selected_column_idx };
        // Keep Columns tab order in sync with DataTable column order
        if self.df.is_some() { self.recompute_columns_info(); }
    }

    pub fn set_dataframe(&mut self, df: Arc<DataFrame>) {
        self.df = Some(df);
        self.recompute_unique_counts();
        self.recompute_columns_info();
        self.recompute_describe();
        self.recompute_heatmap();
    }

    fn current_column_name(&self) -> Option<&str> {
        self.columns.get(self.selected_column_idx).map(|s| s.as_str())
    }

    fn recompute_unique_counts(&mut self) {
        self.unique_counts.clear();
        let df = match &self.df { Some(df) => df, None => return };
        let Some(col_name) = self.current_column_name() else { return };
        // Compute unique values and counts by scanning the Series
        let dfr: &DataFrame = df.as_ref();
        let Ok(series) = dfr.column(col_name) else { return };
        let mut map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for i in 0..series.len() {
            let key = match series.get(i).map_err(|_| ()).unwrap_or(AnyValue::Null) {
                AnyValue::Null => "<NULL>".to_string(),
                v => v.str_value().to_string()
            };
            *map.entry(key).or_insert(0) += 1;
        }
        let mut pairs: Vec<(String, u64)> = map.into_iter().collect();
        match self.sort_by {
            SortBy::Count => {
                // Primary: count descending, Secondary: value ascending
                pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            }
            SortBy::Value => {
                // Primary: value ascending, Secondary: count descending
                pairs.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
            }
        }
        self.unique_counts = pairs;
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    fn recompute_columns_info(&mut self) {
        self.columns_info.clear();
        let Some(df) = &self.df else { return };
        // Prefer the order from self.columns (which mirrors the DataTable order)
        if !self.columns.is_empty() {
            // First, push in the order provided by self.columns
            for name in &self.columns {
                if let Ok(series) = df.column(name) {
                    let dtype = format!("{}", series.dtype());
                    self.columns_info.push((name.clone(), dtype));
                }
            }
            // Then, append any columns present in the DataFrame but not in self.columns, preserving DataFrame order
            let provided: std::collections::HashSet<String> = self.columns
                .iter()
                .cloned()
                .collect();
            for series in df.get_columns() {
                let name = series.name();
                if !provided.contains(name.as_str()) {
                    let dtype = format!("{}", series.dtype());
                    self.columns_info.push((name.to_string(), dtype));
                }
            }
        } else {
            // Fallback: preserve DataFrame column order
            for series in df.get_columns() {
                let name = series.name().to_string();
                let dtype = format!("{}", series.dtype());
                self.columns_info.push((name, dtype));
            }
        }
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    // Determine allowed cast targets based on current dtype
    fn allowed_casts_for(dtype: &DataType) -> Vec<DataType> {
        use DataType::*;
        match dtype {
            Int8 | Int16 | Int32 | Int64 | Int128 | UInt8 | UInt16 | UInt32 | UInt64 => vec![
                Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64, Float32, Float64, Boolean, DataType::String,
            ],
            Float32 | Float64 => vec![
                Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64, Float32, Float64, Boolean, DataType::String,
            ],
            Boolean => vec![Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64, Float32, Float64, Boolean, DataType::String],
            DataType::String => vec![
                Int8, Int16, Int32, Int64, Int128, UInt8, UInt16, UInt32, UInt64, Float32, Float64, Boolean, DataType::String,
            ],
            Date => vec![Date, Datetime(TimeUnit::Milliseconds, None), Int64, DataType::String],
            Datetime(_, _) => vec![Datetime(TimeUnit::Milliseconds, None), Date, Int64, DataType::String],
            Time => vec![Time, Int64, DataType::String],
            Duration(_) => vec![Duration(TimeUnit::Milliseconds), Int64, DataType::String],
            other => vec![other.clone(), DataType::String],
        }
    }

    fn recompute_describe(&mut self) {
        self.describe_rows.clear();
        let Some(df) = &self.df else { return };
        for series in df.get_columns() {
            let dtype = series.dtype();
            let is_numeric = matches!(dtype,
                DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
                DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 |
                DataType::Float32 | DataType::Float64
            );
            if !is_numeric { continue; }
            // Welford's algorithm for mean and std
            let mut n: u64 = 0;
            let mut mean: f64 = 0.0;
            let mut m2: f64 = 0.0;
            let mut min_v: f64 = f64::INFINITY;
            let mut max_v: f64 = f64::NEG_INFINITY;
            let mut values: Vec<f64> = Vec::new();
            for i in 0..series.len() {
                let any = series.get(i).map_err(|_| ()).unwrap_or(AnyValue::Null);
                let opt_x: Option<f64> = match any {
                    AnyValue::Int8(v) => Some(v as f64),
                    AnyValue::Int16(v) => Some(v as f64),
                    AnyValue::Int32(v) => Some(v as f64),
                    AnyValue::Int64(v) => Some(v as f64),
                    AnyValue::UInt8(v) => Some(v as f64),
                    AnyValue::UInt16(v) => Some(v as f64),
                    AnyValue::UInt32(v) => Some(v as f64),
                    AnyValue::UInt64(v) => Some(v as f64),
                    AnyValue::Float32(v) => if v.is_nan() { None } else { Some(v as f64) },
                    AnyValue::Float64(v) => if v.is_nan() { None } else { Some(v) },
                    _ => None,
                };
                if let Some(x) = opt_x {
                    n += 1;
                    let delta = x - mean;
                    mean += delta / (n as f64);
                    let delta2 = x - mean;
                    m2 += delta * delta2;
                    if x < min_v { min_v = x; }
                    if x > max_v { max_v = x; }
                    values.push(x);
                }
            }
            let std = if n > 1 { Some((m2 / ((n as f64) - 1.0)).sqrt()) } else { None };
            let mean_opt = if n > 0 { Some(mean) } else { None };
            let median_opt = if values.is_empty() { None } else {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let len = values.len();
                if len % 2 == 1 {
                    Some(values[len / 2])
                } else {
                    let a = values[len / 2 - 1];
                    let b = values[len / 2];
                    Some((a + b) / 2.0)
                }
            };
            let min_opt = if n > 0 { Some(min_v) } else { None };
            let max_opt = if n > 0 { Some(max_v) } else { None };
            self.describe_rows.push(DescribeRow {
                column: series.name().to_string(),
                count: n,
                mean: mean_opt,
                std,
                median: median_opt,
                min: min_opt,
                max: max_opt,
            });
        }
        // Sort by column name
        self.describe_rows.sort_by(|a, b| a.column.cmp(&b.column));
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    fn recompute_heatmap(&mut self) {
        self.heatmap_cols.clear();
        self.heatmap_matrix.clear();
        let Some(df) = &self.df else { return; };
        // Select numeric columns only
        for s in df.get_columns() {
            if matches!(s.dtype(),
                DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
                DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 |
                DataType::Float32 | DataType::Float64
            ) {
                self.heatmap_cols.push(s.name().to_string());
            }
        }
        let n = self.heatmap_cols.len();
        if n == 0 { return; }
        // Pre-extract columns as f64 vectors, skipping non-finite as None
        let mut cols: Vec<Vec<Option<f64>>> = Vec::with_capacity(n);
        for name in &self.heatmap_cols {
            let s = match df.column(name) { Ok(s) => s, Err(_) => continue };
            let mut vecv: Vec<Option<f64>> = Vec::with_capacity(s.len());
            for i in 0..s.len() {
                let any = s.get(i).unwrap_or(AnyValue::Null);
                let v = match any {
                    AnyValue::Int8(v) => Some(v as f64),
                    AnyValue::Int16(v) => Some(v as f64),
                    AnyValue::Int32(v) => Some(v as f64),
                    AnyValue::Int64(v) => Some(v as f64),
                    AnyValue::UInt8(v) => Some(v as f64),
                    AnyValue::UInt16(v) => Some(v as f64),
                    AnyValue::UInt32(v) => Some(v as f64),
                    AnyValue::UInt64(v) => Some(v as f64),
                    AnyValue::Float32(v) => if v.is_nan() { None } else { Some(v as f64) },
                    AnyValue::Float64(v) => if v.is_nan() { None } else { Some(v) },
                    _ => None,
                };
                vecv.push(v);
            }
            cols.push(vecv);
        }
        let n = cols.len();
        if n == 0 { return; }
        self.heatmap_cols.truncate(n);
        self.heatmap_matrix = vec![vec![0.0; n]; n];
        // Compute Pearson correlation for each pair using pairwise valid entries
        for i in 0..n {
            for j in 0..n {
                let mut xs: Vec<f64> = Vec::new();
                let mut ys: Vec<f64> = Vec::new();
                let len = cols[i].len().min(cols[j].len());
                for k in 0..len {
                    match (cols[i][k], cols[j][k]) {
                        (Some(a), Some(b)) if a.is_finite() && b.is_finite() => { xs.push(a); ys.push(b); }
                        _ => {}
                    }
                }
                let r = if xs.len() >= 2 {
                    // compute correlation
                    let nobs = xs.len() as f64;
                    let mean_x = xs.iter().sum::<f64>() / nobs;
                    let mean_y = ys.iter().sum::<f64>() / nobs;
                    let mut num = 0.0;
                    let mut den_x = 0.0;
                    let mut den_y = 0.0;
                    for idx in 0..xs.len() {
                        let dx = xs[idx] - mean_x;
                        let dy = ys[idx] - mean_y;
                        num += dx * dy;
                        den_x += dx * dx;
                        den_y += dy * dy;
                    }
                    if den_x > 0.0 && den_y > 0.0 { (num / (den_x.sqrt() * den_y.sqrt())).clamp(-1.0, 1.0) } else { 0.0 }
                } else { 0.0 };
                self.heatmap_matrix[i][j] = r;
            }
        }
        // Ensure selection indices are in range
        self.heatmap_x_col_idx = self.heatmap_x_col_idx.min(n.saturating_sub(1));
        self.heatmap_y_col_idx = self.heatmap_y_col_idx.min(n.saturating_sub(1));
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> usize {
        Clear.render(area, buf);
        if let Some(export) = &self.export_dialog {
            // While export dialog is open, render it full-screen and short-circuit
            export.render(area, buf);
            return 0;
        }
        // Outer container with double border around entire dialog
        let outer_block = Block::default()
            .title("DataFrame Details")
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Double);
        let outer_inner_area = outer_block.inner(area);
        outer_block.render(area, buf);
        // Instructions per-tab
        let instructions = self.build_instructions_from_config();
        let layout = split_dialog_area(outer_inner_area, self.show_instructions, 
            if instructions.is_empty() { None } else { Some(instructions.as_str()) });
        let content_area = layout.content_area;
        let instructions_area = layout.instructions_area;
        // Inner single border content frame
        let block = Block::default()
            .borders(Borders::ALL);
        block.render(content_area, buf);

        // Inner content layout within the block
        let inner = Rect {
            x: content_area.x + 1,
            y: content_area.y + 1,
            width: content_area.width.saturating_sub(2),
            height: content_area.height.saturating_sub(2),
        };

        // Header: tabs + column dropdown line
        let header_y = inner.y;
        // Tabs header
        let t1 = "[Unique Values]";
        let t2 = "[Columns]";
        let t3 = "[Describe]";
        let t4 = "[Heatmap]";
        let t5 = "[Embeddings]";
        let t1_style = if matches!(self.tab, DetailsTab::UniqueValues) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
        let t2_style = if matches!(self.tab, DetailsTab::Columns) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
        buf.set_string(inner.x, header_y, t1, t1_style);
        let t2_x = inner.x + t1.len() as u16 + 2;
        buf.set_string(t2_x, header_y, t2, t2_style);
        let t3_x = t2_x + t2.len() as u16 + 2;
        let t3_style = if matches!(self.tab, DetailsTab::Describe) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
        buf.set_string(t3_x, header_y, t3, t3_style);
        let t4_x = t3_x + t3.len() as u16 + 2;
        let t4_style = if matches!(self.tab, DetailsTab::Heatmap) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
        buf.set_string(t4_x, header_y, t4, t4_style);
        let t5_x = t4_x + t4.len() as u16 + 2;
        let t5_style = if matches!(self.tab, DetailsTab::Embeddings) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
        buf.set_string(t5_x, header_y, t5, t5_style);

        // Column dropdown line (UniqueValues) or axes line (Heatmap)
        if matches!(self.tab, DetailsTab::UniqueValues) {
            let col_label = format!(
                "   Column: {}",
                self.current_column_name().unwrap_or("<none>")
            );
            let mut col_style = Style::default();
            if matches!(self.focus, FocusField::ColumnDropdown) {
                col_style = col_style.fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
            }
            buf.set_string(inner.x, header_y + 1, col_label, col_style);
        } else if matches!(self.tab, DetailsTab::Heatmap) {
            let x_name = self.heatmap_cols.get(self.heatmap_x_col_idx).cloned().unwrap_or_else(|| "<none>".to_string());
            let y_name = self.heatmap_cols.get(self.heatmap_y_col_idx).cloned().unwrap_or_else(|| "<none>".to_string());
            let axes_label = format!("   X: {x_name}   vs   Y: {y_name}");
            let axes_style = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
            buf.set_string(inner.x, header_y + 1, axes_label, axes_style);
			// Draw color legend on the next line (diverging palette for correlation [-1,1])
			let legend_y = header_y + 2;
			if legend_y < inner.y + inner.height {
				let total_width = inner.width;
				if total_width > 0 {
					let segments: u16 = total_width.clamp(10, 50);
					let step = (total_width as f32) / (segments as f32);
					let color_for = |r: f64| -> Color {
						let t = ((r + 1.0) / 2.0).clamp(0.0, 1.0);
						if t < 0.2 { Color::Rgb(178, 24, 43) }
						else if t < 0.4 { Color::Rgb(239, 138, 98) }
						else if t < 0.6 { Color::Rgb(247, 247, 247) }
						else if t < 0.8 { Color::Rgb(103, 169, 207) }
						else { Color::Rgb(33, 102, 172) }
					};
					for i in 0..segments {
						let t = if segments <= 1 { 0.0 } else { (i as f64) / ((segments - 1) as f64) };
						let r = 2.0 * t - 1.0; // [-1,1]
						let bg = color_for(r);
						let x = inner.x + ((i as f32) * step).floor() as u16;
						if x >= inner.x + inner.width { break; }
						buf.set_string(x, legend_y, " ", Style::default().bg(bg));
					}
					// Overlay labels -1, 0, +1
					let left_label = "-1";
					let mid_label = "0";
					let right_label = "+1";
					buf.set_string(inner.x, legend_y, left_label, Style::default().fg(Color::Black));
					let mid_x = inner.x + total_width / 2;
					if mid_x < inner.x + total_width { buf.set_string(mid_x, legend_y, mid_label, Style::default().fg(Color::Black)); }
					let right_x = inner.x + total_width.saturating_sub(right_label.len() as u16);
					if right_x < inner.x + total_width { buf.set_string(right_x, legend_y, right_label, Style::default().fg(Color::Black)); }
				}
			}
        }

        // Table area depends on tab (header height differs)
        let header_height = if matches!(self.tab, DetailsTab::UniqueValues | DetailsTab::Heatmap) { 3 } else { 2 };
        let table_area = Rect {
            x: inner.x,
            y: inner.y + header_height,
            width: inner.width,
            height: inner.height.saturating_sub(header_height),
        };

        let max_rows = table_area.height.saturating_sub(1) as usize; // -1 header
        match self.tab {
            DetailsTab::UniqueValues => self.render_unique_values_table(table_area, buf, max_rows),
            DetailsTab::Columns => self.render_columns_table(table_area, buf, max_rows),
            DetailsTab::Describe => self.render_describe_table(table_area, buf, max_rows),
            DetailsTab::Heatmap => self.render_heatmap(table_area, buf),
            DetailsTab::Embeddings => self.render_embeddings_columns(table_area, buf, max_rows),
        }

        if let Some(instructions_area) = instructions_area {
            let p = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL).title("Instructions"))
                .style(Style::default().fg(Color::Yellow))
                .wrap(ratatui::widgets::Wrap { trim: true });
            p.render(instructions_area, buf);
        }
        // Inline sort choice overlay
        if self.sort_choice_open {
            // small centered block
            let block_width = outer_inner_area.width.saturating_sub(10).clamp(20, 30);
            let block_height: u16 = 7;
            let block_x = outer_inner_area.x + (outer_inner_area.width.saturating_sub(block_width)) / 2;
            let block_y = outer_inner_area.y + (outer_inner_area.height.saturating_sub(block_height)) / 2;
            let sort_area = Rect { x: block_x, y: block_y, width: block_width, height: block_height };
            // Fill background
            for y in sort_area.y..sort_area.y + sort_area.height {
                let line = " ".repeat(sort_area.width as usize);
                buf.set_string(sort_area.x, y, &line, Style::default().bg(Color::White));
            }
            let sort_block = Block::default()
                .title("Sort Unique Values")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .border_style(Style::default().fg(Color::Black))
                .style(Style::default().bg(Color::White));
            sort_block.render(sort_area, buf);
            // Content
            let start_x = sort_area.x + 2;
            let mut y = sort_area.y + 2;
            let sort_by_style = Style::default()
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
                .bg(Color::White);
            buf.set_string(start_x, y, "Sort by:", sort_by_style);
            y += 1;
            let options = ["Value", "Count"];
            for (idx, label) in options.iter().enumerate() {
                let selected = idx == self.sort_choice_index;
                let radio = if selected { "(o)" } else { "( )" };
                let style = if selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Black).bg(Color::White)
                };
                buf.set_string(start_x, y, radio, style);
                buf.set_string(start_x + 4, y, *label, style);
                y += 1;
            }
            // Footer hint
            let hint = "Enter: Apply  Esc: Cancel";
            buf.set_string(start_x, sort_area.y + sort_area.height - 2, hint, Style::default().fg(Color::DarkGray).bg(Color::White));
        }
        // Cast overlay
        if self.cast_overlay_open {
            let block_width = outer_inner_area.width.saturating_sub(10).clamp(24, 50);
            let max_vis = outer_inner_area.height.saturating_sub(10).max(5) as usize;
            let block_height: u16 = (self.cast_options.len().min(max_vis) as u16).saturating_add(6);
            let block_x = outer_inner_area.x + (outer_inner_area.width.saturating_sub(block_width)) / 2;
            let block_y = outer_inner_area.y + (outer_inner_area.height.saturating_sub(block_height)) / 2;
            let modal = Rect { x: block_x, y: block_y, width: block_width, height: block_height };
            for y in modal.y..modal.y + modal.height {
                let line = " ".repeat(modal.width as usize);
                buf.set_string(modal.x, y, &line, Style::default().bg(Color::Black));
            }
            let block = Block::default()
                .title("Cast Column Type")
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .border_style(Style::default().fg(Color::White))
                .style(Style::default().bg(Color::Black));
            let inner = block.inner(modal);
            block.render(modal, buf);
            // Header with current column/type
            let cur = if let Some(df) = &self.df {
                let name_opt = self.columns_info.get(self.selected_row).map(|(n, _)| n.clone());
                if let Some(col) = name_opt.as_deref() {
                    if let Ok(s) = df.column(col) {
                        Some((col.to_string(), format!("{}", s.dtype())))
                    } else { None }
                } else { None }
            } else { None };
            if let Some((name, dtype)) = cur {
                let header = format!("{name} [{dtype}]");
                buf.set_string(inner.x, inner.y, &header, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            }
            // List options
            let list_start_y = inner.y + 2;
            let mut y = list_start_y;
            let start_idx = 0usize;
            let end_idx = self.cast_options.len();
            for (i, (label, _dt)) in self.cast_options[start_idx..end_idx].iter().enumerate() {
                let selected = i == self.cast_selected_idx;
                let style = if selected { Style::default().fg(Color::Black).bg(Color::Cyan) } else { Style::default().fg(Color::White) };
                if y < inner.y + inner.height.saturating_sub(2) {
                    buf.set_string(inner.x, y, label, style);
                }
                y = y.saturating_add(1);
            }
            // Footer
            let hint = "Enter: Apply  Esc: Cancel";
            if inner.height >= 2 {
                buf.set_string(inner.x, inner.y + inner.height - 1, hint, Style::default().fg(Color::DarkGray));
            }
            // Inline error overlay on top
            if let Some(err) = &self.cast_error {
                crate::dialog::error_dialog::render_error_dialog(err, outer_inner_area, buf);
            }
        }
        max_rows
    }

    fn render_unique_values_table(&self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        // Scroll handling values
        let total_items = self.unique_counts.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;
        let table_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        if show_scroll_bar {
            let scroll_bar_x = area.x + area.width.saturating_sub(1);
            let scroll_bar_height = max_rows;
            let scroll_bar_y_start = area.y + 1; // below header
            let visible_items = max_rows;
            let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
            let thumb_position = if total_items > visible_items {
                (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
            } else { 0 };
            for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
            }
            let thumb_start = scroll_bar_y_start + thumb_position as u16;
            let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
            for y in thumb_start..thumb_end {
                buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
            }
        }

        let rows: Vec<Row> = self.unique_counts[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(i, (value, count))| {
                let row_idx = start_idx + i;
                let is_selected = matches!(self.focus, FocusField::Table) && row_idx == self.selected_row;
                let is_zebra = row_idx.is_multiple_of(2);
                let style = if is_selected {
                    self.style.selected_row
                } else if is_zebra {
                    self.style.table_row_even
                } else {
                    self.style.table_row_odd
                };
                Row::new(vec![
                    Cell::from(value.to_string()).style(style),
                    Cell::from(format!("{count}")).style(style),
                ])
            })
            .collect();

        let unique_total = self.unique_counts.len();
        let value_header = format!("Value [{unique_total}]");
        let table = Table::new(rows, [Constraint::Percentage(80), Constraint::Percentage(20)])
            .header(Row::new(vec![
                Cell::from(value_header).style(self.style.table_header),
                Cell::from("Count").style(self.style.table_header),
            ]))
            .column_spacing(1);

        let render_area = Rect { x: area.x, y: area.y, width: table_width, height: area.height };
        ratatui::prelude::Widget::render(table, render_area, buf);
    }

    fn render_embeddings_columns(&self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        let mut cols: Vec<(String, String, String, String)> = self
            .embedding_column_config_mapping
            .iter()
            .map(|(name, cfg)| {
                let provider = cfg.provider.display_name().to_string();
                let model = cfg.model_name.clone();
                let dims = cfg.num_dimensions.to_string();
                (name.clone(), provider, model, dims)
            })
            .collect();
        cols.sort_by(|a, b| a.0.cmp(&b.0));

        let total_items = cols.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;
        let table_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        if show_scroll_bar {
            let scroll_bar_x = area.x + area.width.saturating_sub(1);
            let scroll_bar_height = max_rows;
            let scroll_bar_y_start = area.y + 1; // below header
            let visible_items = max_rows;
            let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
            let thumb_position = if total_items > visible_items {
                (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
            } else { 0 };
            for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
            }
            let thumb_start = scroll_bar_y_start + thumb_position as u16;
            let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
            for y in thumb_start..thumb_end {
                buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
            }
        }

        let rows: Vec<Row> = cols[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(i, (name, provider, model, dims))| {
                let row_idx = start_idx + i;
                let is_selected = matches!(self.focus, FocusField::Table) && row_idx == self.selected_row;
                let is_zebra = row_idx.is_multiple_of(2);
                let style = if is_selected { self.style.selected_row } else if is_zebra { self.style.table_row_even } else { self.style.table_row_odd };
                Row::new(vec![
                    Cell::from(name.to_string()).style(style),
                    Cell::from(provider.to_string()).style(style),
                    Cell::from(model.to_string()).style(style),
                    Cell::from(dims.to_string()).style(style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(35),
                Constraint::Percentage(20),
                Constraint::Percentage(35),
                Constraint::Percentage(10),
            ],
        )
        .header(Row::new(vec![
            Cell::from("Column").style(self.style.table_header),
            Cell::from("Provider").style(self.style.table_header),
            Cell::from("Model").style(self.style.table_header),
            Cell::from("Dims").style(self.style.table_header),
        ]))
        .column_spacing(1);

        let render_area = Rect { x: area.x, y: area.y, width: table_width, height: area.height };
        ratatui::prelude::Widget::render(table, render_area, buf);
    }

    fn render_columns_table(&self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        let total_items = self.columns_info.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;
        let table_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        if show_scroll_bar {
            let scroll_bar_x = area.x + area.width.saturating_sub(1);
            let scroll_bar_height = max_rows;
            let scroll_bar_y_start = area.y + 1; // below header
            let visible_items = max_rows;
            let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
            let thumb_position = if total_items > visible_items {
                (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
            } else { 0 };
            for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
            }
            let thumb_start = scroll_bar_y_start + thumb_position as u16;
            let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
            for y in thumb_start..thumb_end {
                buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
            }
        }

        let rows: Vec<Row> = self.columns_info[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(i, (name, dtype))| {
                let row_idx = start_idx + i;
                let is_selected = matches!(self.focus, FocusField::Table) && row_idx == self.selected_row;
                let is_zebra = row_idx.is_multiple_of(2);
                let style = if is_selected {
                    self.style.selected_row
                } else if is_zebra {
                    self.style.table_row_even
                } else {
                    self.style.table_row_odd
                };
                Row::new(vec![
                    Cell::from(name.to_string()).style(style),
                    Cell::from(dtype.to_string()).style(style),
                ])
            })
            .collect();

        let table = Table::new(rows, [Constraint::Percentage(70), Constraint::Percentage(30)])
            .header(Row::new(vec![
                Cell::from("Column").style(self.style.table_header),
                Cell::from("Type").style(self.style.table_header),
            ]))
            .column_spacing(1);

        let render_area = Rect { x: area.x, y: area.y, width: table_width, height: area.height };
        ratatui::prelude::Widget::render(table, render_area, buf);
    }

    fn render_describe_table(&mut self, area: Rect, buf: &mut Buffer, max_rows: usize) {
        let total_items = self.describe_rows.len();
        let start_idx = self.scroll_offset.min(total_items);
        let end_idx = (start_idx + max_rows).min(total_items);
        let show_scroll_bar = total_items > max_rows;

        let table_width = if show_scroll_bar { area.width.saturating_sub(1) } else { area.width };

        if show_scroll_bar {
            let scroll_bar_x = area.x + area.width.saturating_sub(1);
            let scroll_bar_height = max_rows;
            let scroll_bar_y_start = area.y + 1; // below header
            let visible_items = max_rows;
            let thumb_size = std::cmp::max(1, (visible_items * visible_items) / total_items);
            let thumb_position = if total_items > visible_items {
                (self.scroll_offset * (visible_items - thumb_size)) / (total_items - visible_items)
            } else { 0 };
            for y in scroll_bar_y_start..scroll_bar_y_start + scroll_bar_height as u16 {
                buf.set_string(scroll_bar_x, y, "│", Style::default().fg(Color::DarkGray));
            }
            let thumb_start = scroll_bar_y_start + thumb_position as u16;
            let thumb_end = (thumb_start + thumb_size as u16).min(scroll_bar_y_start + scroll_bar_height as u16);
            for y in thumb_start..thumb_end {
                buf.set_string(scroll_bar_x, y, "█", Style::default().fg(Color::Cyan));
            }
        }

        fn fmt_opt(x: Option<f64>) -> String {
            match x {
                Some(v) => {
                    // Format with up to 6 decimals, trim trailing zeros
                    let s = format!("{v:.6}");
                    s
                }
                None => String::new(),
            }
        }
        // All stats columns (fixed order)
        let stats_headers = ["count", "mean", "std", "median", "min", "max"];
        let stats_constraints = [
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(12),
        ];

        // Determine visible stats columns window based on width and describe_col_offset
        let mut remaining_width = table_width.saturating_sub(18); // subtract name column
        let mut visible_stats: Vec<usize> = Vec::new();
        let mut i = self.describe_col_offset.min(stats_headers.len().saturating_sub(1));
        while i < stats_headers.len() {
            let need = match stats_constraints[i] {
                Constraint::Length(n) => n,
                Constraint::Min(n) | Constraint::Max(n) => n,
                _ => 12,
            };
            if remaining_width <= need { break; }
            remaining_width = remaining_width.saturating_sub(need);
            visible_stats.push(i);
            i += 1;
        }
        if visible_stats.is_empty() {
            // Ensure at least one stats column is visible when space is very tight
            visible_stats.push(self.describe_col_offset.min(stats_headers.len() - 1));
        }

        // Build headers and constraints
        let mut header_cells: Vec<Cell> = vec![Cell::from("Column").style(self.style.table_header)];
        let mut constraints: Vec<Constraint> = vec![Constraint::Min(18)];
        for &idx in &visible_stats {
            header_cells.push(Cell::from(stats_headers[idx]).style(self.style.table_header));
            constraints.push(stats_constraints[idx]);
        }

        // Build rows matching visible columns
        let rows: Vec<Row> = self.describe_rows[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(row_vis_idx, r)| {
                let row_idx = start_idx + row_vis_idx;
                let is_selected = matches!(self.focus, FocusField::Table) && row_idx == self.selected_row;
                let is_zebra = row_idx.is_multiple_of(2);
                let style = if is_selected { self.style.selected_row } else if is_zebra { self.style.table_row_even } else { self.style.table_row_odd };
                let mut cells: Vec<Cell> = vec![Cell::from(r.column.clone()).style(style)];
                for &idx in &visible_stats {
                    let val = match idx {
                        0 => Some(r.count as f64),
                        1 => r.mean,
                        2 => r.std,
                        3 => r.median,
                        4 => r.min,
                        5 => r.max,
                        _ => None,
                    };
                    let s = if idx == 0 { r.count.to_string() } else { fmt_opt(val) };
                    cells.push(Cell::from(s).style(style));
                }
                Row::new(cells)
            })
            .collect();

        let table = Table::new(rows, constraints)
            .header(Row::new(header_cells))
            .column_spacing(1);

        let render_area = Rect { x: area.x, y: area.y, width: table_width, height: area.height };
        ratatui::prelude::Widget::render(table, render_area, buf);

        // Draw horizontal scrollbar (for stats columns) at bottom of area
        let total_stats = stats_headers.len();
        let visible_capacity = visible_stats.len().max(1);
        let offset_max = total_stats.saturating_sub(visible_capacity);
        
        // Store the computed max offset for use in key handling
        self.describe_col_max_offset = offset_max;
        
        if offset_max > 0 && table_width > 4 {
            let track_x = area.x;
            let track_y = area.y + area.height.saturating_sub(1);
            let track_len = table_width as usize;
            // Draw track
            let track_str = "─".repeat(track_len);
            buf.set_string(track_x, track_y, &track_str, Style::default().fg(Color::DarkGray));
            // Thumb
            let thumb_size = std::cmp::max(1, (track_len * visible_capacity) / total_stats);
            let thumb_pos = (self.describe_col_offset.min(offset_max) * (track_len.saturating_sub(thumb_size))) / offset_max;
            let thumb = "█".repeat(thumb_size);
            buf.set_string(track_x + thumb_pos as u16, track_y, &thumb, Style::default().fg(Color::Cyan));
        }
    }

    fn render_heatmap(&self, area: Rect, buf: &mut Buffer) {
        // Guard: need a computed square correlation matrix
        if self.heatmap_cols.is_empty() || self.heatmap_matrix.is_empty() { return; }

        let n_total = self.heatmap_cols.len();
        // Title and border
        let block = Block::default().borders(Borders::ALL).title("Correlation Heatmap");
        block.render(area, buf);

        let inner = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(2) };
        if inner.width == 0 || inner.height == 0 { return; }

        // Determine visible window to keep grid square and selection visible
        let max_dim = std::cmp::min(inner.width as usize, inner.height as usize);
        if max_dim == 0 { return; }
        let visible_dim = std::cmp::min(n_total, max_dim);
        if visible_dim == 0 { return; }
        let cell_w = (inner.width / (visible_dim as u16)).max(1);
        let cell_h = (inner.height / (visible_dim as u16)).max(1);
        let sel_x = self.heatmap_x_col_idx.min(n_total.saturating_sub(1));
        let sel_y = self.heatmap_y_col_idx.min(n_total.saturating_sub(1));
        let half = visible_dim / 2;
        let offset_x = sel_x.saturating_sub(half).min(n_total.saturating_sub(visible_dim));
        let offset_y = sel_y.saturating_sub(half).min(n_total.saturating_sub(visible_dim));

        // Color scale for correlation [-1..1]
        let color_for = |r: f64| -> Color {
            let t = ((r + 1.0) / 2.0).clamp(0.0, 1.0);
            if t < 0.2 { Color::Rgb(178, 24, 43) } // strong negative
            else if t < 0.4 { Color::Rgb(239, 138, 98) }
            else if t < 0.6 { Color::Rgb(247, 247, 247) }
            else if t < 0.8 { Color::Rgb(103, 169, 207) }
            else { Color::Rgb(33, 102, 172) } // strong positive
        };

        for yi_vis in 0..visible_dim {
            let yi = yi_vis + offset_y;
            for xi_vis in 0..visible_dim {
                let xi = xi_vis + offset_x;
                let r = self.heatmap_matrix[yi][xi];
                let bg = color_for(r);
                let x0 = inner.x + (xi_vis as u16) * cell_w;
                let y0 = inner.y + (yi_vis as u16) * cell_h;
                if x0 >= inner.x + inner.width || y0 >= inner.y + inner.height { continue; }
                let max_w = (inner.x + inner.width).saturating_sub(x0);
                let draw_w = cell_w.min(max_w);
                let y_end = (y0.saturating_add(cell_h)).min(inner.y + inner.height);
                for yy in y0..y_end {
                    if draw_w == 0 { break; }
                    let line = " ".repeat(draw_w as usize);
                    buf.set_string(x0, yy, &line, Style::default().bg(bg));
                }
            }
        }

        // Highlight current selection with outlined square overlay within viewport
        let sel_x_vis = sel_x.saturating_sub(offset_x);
        let sel_y_vis = sel_y.saturating_sub(offset_y);
        if sel_x_vis < visible_dim && sel_y_vis < visible_dim {
            let cx = inner.x + (sel_x_vis as u16) * cell_w;
            let cy = inner.y + (sel_y_vis as u16) * cell_h;
            if cx < inner.x + inner.width && cy < inner.y + inner.height {
                // Draw a 1px border rectangle using the same bg as the cell and black fg for the outline
                let border_fg = Style::default().fg(Color::Black);
                let cell_r = self.heatmap_matrix[sel_y][sel_x];
                let bg = color_for(cell_r);
                let fill_style = Style::default().bg(bg);
                let width = cell_w.min((inner.x + inner.width).saturating_sub(cx));
                let height = cell_h.min((inner.y + inner.height).saturating_sub(cy));
                if width > 0 && height > 0 {
                    // Top and bottom edges
                    let horiz = "─".repeat(width as usize);
                    buf.set_string(cx, cy, &horiz, border_fg.bg(bg));
                    if height > 1 { buf.set_string(cx, cy + height - 1, &horiz, border_fg.bg(bg)); }
                    // Left/right edges
                    for yy in 0..height {
                        buf.set_string(cx, cy + yy, "│", border_fg.bg(bg));
                        if width > 1 { buf.set_string(cx + width - 1, cy + yy, "│", border_fg.bg(bg)); }
                    }
                    // Fill interior to keep bg color consistent
                    if height > 2 && width > 2 {
                        for yy in 1..(height - 1) {
                            let line = " ".repeat((width - 2) as usize);
                            buf.set_string(cx + 1, cy + yy, &line, fill_style);
                        }
                    }
                }
            }
        }
    }

    /// Handle key events for the DataFrame Details dialog.
    ///
    /// # Event Processing Flow
    ///
    /// The method processes events in a strict priority order:
    ///
    /// 1. **Modal Overlays** (highest priority - consume all input)
    ///    - Export dialog: Full-screen file export UI
    ///    - Sort choice overlay: Value/Count sort selector (UniqueValues tab)
    ///    - Cast overlay: Column type casting selector (Columns tab)
    ///
    /// 2. **Global Actions** (dialog-level controls)
    ///    - Escape: Close dialog
    ///    - Toggle instructions visibility
    ///
    /// 3. **Tab Switching**
    ///    - Navigate between UniqueValues, Columns, Describe, Heatmap, Embeddings
    ///
    /// 4. **Tab-Specific Actions** (delegated to specialized handlers)
    ///    - Each tab has its own navigation and action handling
    ///
    /// # Arguments
    /// * `key` - The key event to process
    /// * `max_rows` - Maximum visible rows for scroll calculations
    ///
    /// # Returns
    /// * `Some(Action)` - An action to propagate to the parent component
    /// * `None` - Event was handled internally or ignored
    pub fn handle_key_event(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        // Only process key press events (ignore release/repeat)
        if key.kind != KeyEventKind::Press { 
            return None; 
        }

        // ─────────────────────────────────────────────────────────────────────
        // PHASE 1: Modal Overlays (highest priority, consume all input)
        // ─────────────────────────────────────────────────────────────────────
        // Modal overlays have exclusive input focus. When active, they must
        // handle or consume all key events before any other processing.

        // 1a. Export dialog - full-screen file export UI
        if self.export_dialog.is_some() {
            return self.handle_export_dialog_input(key);
        }

        // 1b. Sort choice overlay - value/count sorting selector
        if self.sort_choice_open {
            return self.handle_sort_choice_input(key);
        }

        // 1c. Cast overlay - column type casting selector
        if self.cast_overlay_open {
            return self.handle_cast_overlay_input(key);
        }

        // ─────────────────────────────────────────────────────────────────────
        // PHASE 2: Global Actions (dialog-level controls)
        // ─────────────────────────────────────────────────────────────────────
        // These actions apply regardless of which tab is active.

        if let Some(action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match action {
                Action::Escape => {
                    return Some(Action::DialogClose);
                }
                Action::ToggleInstructions => {
                    self.show_instructions = !self.show_instructions;
                    return None;
                }
                Action::CopyText => {
                    // Handle copy for tabs that support it
                    match self.tab {
                        DetailsTab::UniqueValues => {
                            self.copy_unique_values();
                        }
                        DetailsTab::Columns => {
                            self.copy_columns();
                        }
                        DetailsTab::Describe => {
                            self.copy_describe();
                        }
                        _ => {}
                    }
                    return None;
                }
                // Other global actions (navigation) are handled in tab-specific phase
                _ => {}
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // PHASE 3: Tab Switching
        // ─────────────────────────────────────────────────────────────────────
        // Switch between tabs before processing tab-specific actions.

        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            match action {
                Action::SwitchToNextTab => {
                    self.switch_to_next_tab();
                    return None;
                }
                Action::SwitchToPrevTab => {
                    self.switch_to_prev_tab();
                    return None;
                }
                _ => {} // Other actions handled in tab-specific phase
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // PHASE 4: Tab-Specific Actions
        // ─────────────────────────────────────────────────────────────────────
        // Delegate to specialized handlers based on the active tab.
        // Each handler manages its own navigation, scrolling, and actions.

        match self.tab {
            DetailsTab::UniqueValues => self.handle_unique_values_input(key, max_rows),
            DetailsTab::Columns => self.handle_columns_input(key, max_rows),
            DetailsTab::Describe => self.handle_describe_input(key, max_rows),
            DetailsTab::Heatmap => self.handle_heatmap_input(key),
            DetailsTab::Embeddings => self.handle_embeddings_input(key, max_rows),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Modal Overlay Handlers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle input when the export dialog is active.
    fn handle_export_dialog_input(&mut self, key: KeyEvent) -> Option<Action> {
        if let Some(dialog) = &mut self.export_dialog {
            if let Some(action) = dialog.handle_key_event(key) {
                if action == Action::DialogClose {
                    self.export_dialog = None;
                }
            }
        }
        None // Export dialog consumes all input
    }

    /// Handle input when the sort choice overlay is active.
    /// Allows selecting between Value and Count sorting for unique values.
    fn handle_sort_choice_input(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Up => {
                self.sort_choice_index = if self.sort_choice_index == 0 { 1 } else { 0 };
            }
            KeyCode::Down => {
                self.sort_choice_index = (self.sort_choice_index + 1) % 2;
            }
            KeyCode::Enter => {
                self.sort_by = if self.sort_choice_index == 0 { SortBy::Value } else { SortBy::Count };
                self.sort_choice_open = false;
                self.recompute_unique_counts();
            }
            KeyCode::Esc => {
                self.sort_choice_open = false;
            }
            _ => {}
        }
        None // Sort choice overlay consumes all input
    }

    /// Handle input when the cast overlay is active.
    /// Allows selecting a target data type for column casting.
    fn handle_cast_overlay_input(&mut self, key: KeyEvent) -> Option<Action> {
        // If an error overlay is present, only allow dismissing it
        if self.cast_error.is_some() {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                self.clear_cast_error();
            }
            return None;
        }

        match key.code {
            KeyCode::Up => {
                self.cast_selected_idx = if self.cast_selected_idx == 0 {
                    self.cast_options.len().saturating_sub(1)
                } else {
                    self.cast_selected_idx - 1
                };
            }
            KeyCode::Down => {
                if !self.cast_options.is_empty() {
                    self.cast_selected_idx = (self.cast_selected_idx + 1) % self.cast_options.len();
                }
            }
            KeyCode::Enter => {
                if let Some((_, dt)) = self.cast_options.get(self.cast_selected_idx).cloned() {
                    if let Some((col, _)) = self.columns_info.get(self.selected_row) {
                        return Some(Action::ColumnCastRequested { 
                            column: col.clone(), 
                            dtype: format!("{dt:?}") 
                        });
                    }
                }
            }
            KeyCode::Esc => {
                self.cast_overlay_open = false;
            }
            _ => {}
        }
        None // Cast overlay consumes all input
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Tab Navigation
    // ═══════════════════════════════════════════════════════════════════════════

    /// Switch to the next tab in sequence.
    fn switch_to_next_tab(&mut self) {
        self.tab = match self.tab {
            DetailsTab::UniqueValues => DetailsTab::Columns,
            DetailsTab::Columns => DetailsTab::Describe,
            DetailsTab::Describe => DetailsTab::Heatmap,
            DetailsTab::Heatmap => DetailsTab::Embeddings,
            DetailsTab::Embeddings => DetailsTab::UniqueValues,
        };
        self.reset_tab_state();
    }

    /// Switch to the previous tab in sequence.
    fn switch_to_prev_tab(&mut self) {
        self.tab = match self.tab {
            DetailsTab::UniqueValues => DetailsTab::Embeddings,
            DetailsTab::Columns => DetailsTab::UniqueValues,
            DetailsTab::Describe => DetailsTab::Columns,
            DetailsTab::Heatmap => DetailsTab::Describe,
            DetailsTab::Embeddings => DetailsTab::Heatmap,
        };
        self.reset_tab_state();
    }

    /// Reset selection and scroll state when switching tabs.
    fn reset_tab_state(&mut self) {
        self.selected_row = 0;
        self.scroll_offset = 0;
        // UniqueValues is the only tab with a column dropdown focus
        if !matches!(self.tab, DetailsTab::UniqueValues) {
            self.focus = FocusField::Table;
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Tab-Specific Input Handlers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle input for the Unique Values tab.
    /// Supports: vertical navigation, column switching, sort selection, filtering, export.
    fn handle_unique_values_input(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        let list_len = self.unique_counts.len();

        // Check for dialog-specific actions first
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            match action {
                Action::ChangeColumnLeft => {
                    self.navigate_column(-1);
                    return None;
                }
                Action::ChangeColumnRight => {
                    self.navigate_column(1);
                    return None;
                }
                Action::OpenSortChoice => {
                    self.sort_choice_open = true;
                    self.sort_choice_index = match self.sort_by { SortBy::Value => 0, SortBy::Count => 1 };
                    return None;
                }
                Action::AddFilterFromValue => {
                    return self.create_filter_from_selected_value();
                }
                Action::ExportCurrentTab => {
                    self.export_unique_values();
                    return None;
                }
                _ => {}
            }
        }

        // Handle global navigation actions
        if let Some(action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match action {
                Action::Up => {
                    self.navigate_row_up();
                    return None;
                }
                Action::Down => {
                    self.navigate_row_down(list_len, max_rows);
                    return None;
                }
                Action::Left => {
                    self.navigate_column(-1);
                    return None;
                }
                Action::Right => {
                    self.navigate_column(1);
                    return None;
                }
                Action::PageUp => {
                    self.navigate_page_up(list_len, max_rows);
                    return None;
                }
                Action::PageDown => {
                    self.navigate_page_down(list_len, max_rows);
                    return None;
                }
                Action::Tab => {
                    // Toggle between column dropdown and table focus
                    self.focus = match self.focus {
                        FocusField::ColumnDropdown => FocusField::Table,
                        FocusField::Table => FocusField::ColumnDropdown,
                    };
                    return None;
                }
                _ => {}
            }
        }

        None
    }

    /// Handle input for the Columns tab.
    /// Supports: vertical navigation, cast overlay, export.
    fn handle_columns_input(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        let list_len = self.columns_info.len();

        // Check for dialog-specific actions first
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            match action {
                Action::OpenCastOverlay => {
                    self.open_cast_overlay();
                    return None;
                }
                Action::ExportCurrentTab => {
                    self.export_columns();
                    return None;
                }
                _ => {}
            }
        }

        // Handle global navigation actions
        self.handle_list_navigation(key, list_len, max_rows)
    }

    /// Handle input for the Describe tab.
    /// Supports: vertical navigation, horizontal stats scrolling, export.
    fn handle_describe_input(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        let list_len = self.describe_rows.len();

        // Check for dialog-specific actions first
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            match action {
                Action::ScrollStatsLeft => {
                    self.describe_col_offset = self.describe_col_offset.saturating_sub(1);
                    return None;
                }
                Action::ScrollStatsRight => {
                    // Only scroll if we haven't reached the max (computed during render)
                    if self.describe_col_offset < self.describe_col_max_offset {
                        self.describe_col_offset += 1;
                    }
                    return None;
                }
                Action::ExportCurrentTab => {
                    self.export_describe();
                    return None;
                }
                _ => {}
            }
        }

        // Handle global navigation actions
        if let Some(action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match action {
                Action::Left => {
                    self.describe_col_offset = self.describe_col_offset.saturating_sub(1);
                    return None;
                }
                Action::Right => {
                    if self.describe_col_offset < self.describe_col_max_offset {
                        self.describe_col_offset += 1;
                    }
                    return None;
                }
                _ => {}
            }
        }

        self.handle_list_navigation(key, list_len, max_rows)
    }

    /// Handle input for the Heatmap tab.
    /// Supports: 2D grid navigation using X/Y column indices.
    fn handle_heatmap_input(&mut self, key: KeyEvent) -> Option<Action> {
        if self.heatmap_cols.is_empty() {
            return None;
        }

        let n = self.heatmap_cols.len();

        // Check for dialog-specific actions
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            match action {
                Action::NavigateHeatmapLeft => {
                    self.heatmap_x_col_idx = if self.heatmap_x_col_idx == 0 { 
                        n.saturating_sub(1) 
                    } else { 
                        self.heatmap_x_col_idx - 1 
                    };
                }
                Action::NavigateHeatmapRight => {
                    self.heatmap_x_col_idx = (self.heatmap_x_col_idx + 1) % n;
                }
                Action::NavigateHeatmapUp => {
                    self.heatmap_y_col_idx = if self.heatmap_y_col_idx == 0 { 
                        n.saturating_sub(1) 
                    } else { 
                        self.heatmap_y_col_idx - 1 
                    };
                }
                Action::NavigateHeatmapDown => {
                    self.heatmap_y_col_idx = (self.heatmap_y_col_idx + 1) % n;
                }
                Action::NavigateHeatmapPageUp => {
                    let step = std::cmp::max(1, n / 5);
                    self.heatmap_y_col_idx = self.heatmap_y_col_idx.saturating_sub(step);
                }
                Action::NavigateHeatmapPageDown => {
                    let step = std::cmp::max(1, n / 5);
                    self.heatmap_y_col_idx = (self.heatmap_y_col_idx + step).min(n.saturating_sub(1));
                }
                Action::NavigateHeatmapHome => {
                    self.heatmap_x_col_idx = 0;
                    self.heatmap_y_col_idx = 0;
                }
                Action::NavigateHeatmapEnd => {
                    let last = n.saturating_sub(1);
                    self.heatmap_x_col_idx = last;
                    self.heatmap_y_col_idx = last;
                }
                _ => {}
            }
        }

        // Handle global arrow keys for heatmap navigation
        if let Some(action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match action {
                Action::Left => {
                    self.heatmap_x_col_idx = if self.heatmap_x_col_idx == 0 { 
                        n.saturating_sub(1) 
                    } else { 
                        self.heatmap_x_col_idx - 1 
                    };
                }
                Action::Right => {
                    self.heatmap_x_col_idx = (self.heatmap_x_col_idx + 1) % n;
                }
                Action::Up => {
                    self.heatmap_y_col_idx = if self.heatmap_y_col_idx == 0 { 
                        n.saturating_sub(1) 
                    } else { 
                        self.heatmap_y_col_idx - 1 
                    };
                }
                Action::Down => {
                    self.heatmap_y_col_idx = (self.heatmap_y_col_idx + 1) % n;
                }
                _ => {}
            }
        }

        None
    }

    /// Handle input for the Embeddings tab.
    /// Supports: vertical navigation, export.
    fn handle_embeddings_input(&mut self, key: KeyEvent, max_rows: usize) -> Option<Action> {
        let list_len = self.embedding_column_config_mapping.len();

        // Check for dialog-specific actions
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataFrameDetails, key) {
            if action == Action::ExportCurrentTab {
                self.export_embeddings();
                return None;
            }
        }

        // Handle global navigation actions
        self.handle_list_navigation(key, list_len, max_rows)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Navigation Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle common list navigation (Up/Down/PageUp/PageDown) for tabs with vertical lists.
    fn handle_list_navigation(&mut self, key: KeyEvent, list_len: usize, max_rows: usize) -> Option<Action> {
        if let Some(action) = self.config.action_for_key(crate::config::Mode::Global, key) {
            match action {
                Action::Up => self.navigate_row_up(),
                Action::Down => self.navigate_row_down(list_len, max_rows),
                Action::PageUp => self.navigate_page_up(list_len, max_rows),
                Action::PageDown => self.navigate_page_down(list_len, max_rows),
                _ => {}
            }
        }
        None
    }

    /// Move selection up by one row, scrolling if necessary.
    fn navigate_row_up(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        }
    }

    /// Move selection down by one row, scrolling if necessary.
    fn navigate_row_down(&mut self, list_len: usize, max_rows: usize) {
        let max_idx = list_len.saturating_sub(1);
        if self.selected_row < max_idx {
            self.selected_row += 1;
        }
        let visible_end = self.scroll_offset + max_rows.saturating_sub(1);
        if self.selected_row > visible_end {
            self.scroll_offset = self.selected_row.saturating_sub(max_rows.saturating_sub(1));
        }
    }

    /// Move selection up by one page.
    fn navigate_page_up(&mut self, list_len: usize, max_rows: usize) {
        if list_len == 0 { return; }
        let page = max_rows.max(1);
        self.selected_row = self.selected_row.saturating_sub(page);
        if self.selected_row < self.scroll_offset {
            self.scroll_offset = self.selected_row;
        }
    }

    /// Move selection down by one page.
    fn navigate_page_down(&mut self, list_len: usize, max_rows: usize) {
        if list_len == 0 { return; }
        let page = max_rows.max(1);
        self.selected_row = (self.selected_row + page).min(list_len.saturating_sub(1));
        let visible_end = self.scroll_offset + max_rows.saturating_sub(1);
        if self.selected_row > visible_end {
            self.scroll_offset = self.selected_row.saturating_sub(max_rows.saturating_sub(1));
        }
    }

    /// Navigate to adjacent column (UniqueValues tab).
    /// `direction`: -1 for left, +1 for right
    fn navigate_column(&mut self, direction: i32) {
        if self.columns.is_empty() { return; }
        let n = self.columns.len();
        self.selected_column_idx = if direction < 0 {
            if self.selected_column_idx == 0 { n - 1 } else { self.selected_column_idx - 1 }
        } else {
            (self.selected_column_idx + 1) % n
        };
        self.recompute_unique_counts();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Action Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Open the cast overlay for the currently selected column.
    fn open_cast_overlay(&mut self) {
        let Some(df) = &self.df else { return };
        let Some((col_name, _)) = self.columns_info.get(self.selected_row) else { return };
        let Ok(series) = df.column(col_name) else { return };

        let cur_dt = series.dtype().clone();
        let mut opts: Vec<(String, DataType)> = Self::allowed_casts_for(&cur_dt)
            .into_iter()
            .map(|dt| (format!("{dt:?}"), dt))
            .collect();

        // Deduplicate by label
        let mut seen = std::collections::HashSet::new();
        opts.retain(|(label, _)| seen.insert(label.clone()));

        // Remove current dtype (no-op cast)
        let cur_label = format!("{cur_dt:?}");
        opts.retain(|(label, _)| label != &cur_label);

        // Sort for stable UI ordering
        opts.sort_by(|a, b| a.0.cmp(&b.0));

        self.cast_options = opts;
        self.cast_selected_idx = 0;
        self.cast_overlay_open = true;
    }

    /// Create a filter condition from the currently selected unique value.
    fn create_filter_from_selected_value(&self) -> Option<Action> {
        let (value, _) = self.unique_counts.get(self.selected_row)?;
        let col = self.current_column_name()?;
        
        let filter = ColumnFilter {
            column: col.to_string(),
            condition: FilterCondition::Equals { 
                value: value.clone(), 
                case_sensitive: false 
            },
        };
        Some(Action::AddFilterCondition(filter))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Export Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Export unique values tab data.
    fn export_unique_values(&mut self) {
        let headers = vec!["Value".to_string(), "Count".to_string()];
        let rows: Vec<Vec<String>> = self.unique_counts
            .iter()
            .map(|(v, c)| vec![v.clone(), c.to_string()])
            .collect();
        let suggested = self.current_column_name()
            .map(|c| format!("unique_values_{c}.csv"))
            .or(Some("unique_values.csv".to_string()));
        let mut dialog = TableExportDialog::new(headers, rows, suggested);
        let _ = dialog.register_config_handler(self.config.clone());
        self.export_dialog = Some(dialog);
    }

    /// Export columns tab data.
    fn export_columns(&mut self) {
        let headers = vec!["Column".to_string(), "Type".to_string()];
        let rows: Vec<Vec<String>> = self.columns_info
            .iter()
            .map(|(n, t)| vec![n.clone(), t.clone()])
            .collect();
        let mut dialog = TableExportDialog::new(headers, rows, Some("columns.csv".to_string()));
        let _ = dialog.register_config_handler(self.config.clone());
        self.export_dialog = Some(dialog);
    }

    /// Export describe tab data.
    fn export_describe(&mut self) {
        let headers = vec![
            "Column".to_string(), 
            "count".to_string(), 
            "mean".to_string(), 
            "std".to_string(), 
            "median".to_string(), 
            "min".to_string(), 
            "max".to_string(),
        ];
        let rows: Vec<Vec<String>> = self.describe_rows
            .iter()
            .map(|r| vec![
                r.column.clone(),
                r.count.to_string(),
                r.mean.map(|v| format!("{v}")).unwrap_or_default(),
                r.std.map(|v| format!("{v}")).unwrap_or_default(),
                r.median.map(|v| format!("{v}")).unwrap_or_default(),
                r.min.map(|v| format!("{v}")).unwrap_or_default(),
                r.max.map(|v| format!("{v}")).unwrap_or_default(),
            ])
            .collect();
        let mut dialog = TableExportDialog::new(headers, rows, Some("describe.csv".to_string()));
        let _ = dialog.register_config_handler(self.config.clone());
        self.export_dialog = Some(dialog);
    }

    /// Export embeddings tab data.
    fn export_embeddings(&mut self) {
        let mut cols: Vec<(String, String, String, String)> = self.embedding_column_config_mapping
            .iter()
            .map(|(name, cfg)| (
                name.clone(),
                cfg.provider.display_name().to_string(),
                cfg.model_name.clone(),
                cfg.num_dimensions.to_string(),
            ))
            .collect();
        cols.sort_by(|a, b| a.0.cmp(&b.0));

        let headers = vec![
            "Column".to_string(),
            "Provider".to_string(),
            "Model".to_string(),
            "Dims".to_string(),
        ];
        let rows: Vec<Vec<String>> = cols
            .into_iter()
            .map(|(n, p, m, d)| vec![n, p, m, d])
            .collect();
        let mut dialog = TableExportDialog::new(headers, rows, Some("embeddings.csv".to_string()));
        let _ = dialog.register_config_handler(self.config.clone());
        self.export_dialog = Some(dialog);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Copy Helpers
    // ═══════════════════════════════════════════════════════════════════════════

    /// Copy unique values tab data to clipboard in tabular format.
    fn copy_unique_values(&self) {
        let mut text = String::from("Value\tCount\n");
        for (value, count) in &self.unique_counts {
            // Escape tabs and newlines in values
            let escaped_value = value.replace('\t', " ").replace('\n', " ").replace('\r', " ");
            text.push_str(&format!("{escaped_value}\t{count}\n"));
        }
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }

    /// Copy columns tab data to clipboard in tabular format.
    fn copy_columns(&self) {
        let mut text = String::from("Column\tType\n");
        for (name, dtype) in &self.columns_info {
            // Escape tabs and newlines in values
            let escaped_name = name.replace('\t', " ").replace('\n', " ").replace('\r', " ");
            let escaped_dtype = dtype.replace('\t', " ").replace('\n', " ").replace('\r', " ");
            text.push_str(&format!("{escaped_name}\t{escaped_dtype}\n"));
        }
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }

    /// Copy describe tab data to clipboard in tabular format.
    fn copy_describe(&self) {
        let mut text = String::from("Column\tcount\tmean\tstd\tmedian\tmin\tmax\n");
        for r in &self.describe_rows {
            // Escape tabs and newlines in column name
            let escaped_column = r.column.replace('\t', " ").replace('\n', " ").replace('\r', " ");
            let count_str = r.count.to_string();
            let mean_str = r.mean.map(|v| format!("{v}")).unwrap_or_default();
            let std_str = r.std.map(|v| format!("{v}")).unwrap_or_default();
            let median_str = r.median.map(|v| format!("{v}")).unwrap_or_default();
            let min_str = r.min.map(|v| format!("{v}")).unwrap_or_default();
            let max_str = r.max.map(|v| format!("{v}")).unwrap_or_default();
            text.push_str(&format!("{escaped_column}\t{count_str}\t{mean_str}\t{std_str}\t{median_str}\t{min_str}\t{max_str}\n"));
        }
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
}

impl Component for DataFrameDetailsDialog {
    fn register_action_handler(&mut self, _tx: tokio::sync::mpsc::UnboundedSender<Action>) -> Result<()> { Ok(()) }
    fn register_config_handler(&mut self, _config: crate::config::Config) -> Result<()> { 
        self.config = _config; 
        Ok(()) 
    }
    fn init(&mut self, _area: ratatui::layout::Size) -> Result<()> { Ok(()) }
    fn handle_events(&mut self, _event: Option<crate::tui::Event>) -> Result<Option<Action>> { Ok(None) }
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> { Ok(None) }
    fn handle_mouse_event(&mut self, _mouse: crossterm::event::MouseEvent) -> Result<Option<Action>> { Ok(None) }
    fn update(&mut self, _action: Action) -> Result<Option<Action>> { Ok(None) }
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let _ = self.render(area, frame.buffer_mut());
        Ok(())
    }
}


