//! DataTableContainer: Composite widget for DataTable with viewing box and instructions
//!
//! # Overview
//!
//! `DataTableContainer` is a composite widget that combines a viewing box, a data table, and an instruction area into a single UI component. It manages the state and interactions for sorting, filtering, and running SQL queries on a tabular dataset, providing a user-friendly interface for data exploration and manipulation.
//!
//! ## Layout
//!
//! The layout consists of three main vertical sections:
//!
//! - **Viewing Box**: Displays the value of the currently selected cell in the data table.
//! - **DataTable**: The main interactive table for viewing and navigating data.
//! - **Instruction Area**: Shows user instructions and available keyboard shortcuts.
//!
//! ## Features
//!
//! - Keyboard navigation and shortcuts for sorting, filtering, and SQL queries
//! - Popup dialogs for sort, filter, and SQL operations
//! - Integration with Polars DataFrame and SQLContext for data manipulation
//! - Toggleable instruction area (Ctrl+i to toggle)
//! - Modular design for easy extension and customization
//!
//! ## Usage
//!
//! Typically used as the main data viewing component in a TUI application. Instantiate with a `DataTable` and a `StyleConfig`, then integrate into your application's component tree.
//!
//! ```rust
//! use tdv::components::datatable::DataTable;
//! use tdv::components::datatable_container::DataTableContainer;
//! use tdv::style::StyleConfig;
//! use tdv::dataframe::manager::ManagedDataFrame;
//! use polars::prelude::*;
//! 
//! // Create a sample DataFrame
//! let s1 = Series::new("col1".into(), &[1, 2, 3]);
//! let s2 = Series::new("col2".into(), &["a", "b", "c"]);
//! let df = DataFrame::new(vec![s1.into(), s2.into()]).unwrap();
//! let managed_df = ManagedDataFrame::new(df, "Test".to_string(), None, None);
//! 
//! let datatable = DataTable::new(managed_df, StyleConfig::default());
//! let style = StyleConfig::default();
//! let mut container = DataTableContainer::new(datatable, style);
//! ```
//!
//! See method-level documentation for details on customization and event handling.

use crate::components::{Component, datatable::DataTable};
use crate::style::StyleConfig;
use crate::dataframe::manager::SortableDataFrame;
use crate::action::Action;
use crate::config::{Config, Mode};
use crate::tui::Event;
use crate::sql::register_all;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::prelude::{Frame, Rect, Size};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap, Widget, Clear};
use ratatui::layout::{Layout, Direction, Constraint};
use tokio::sync::mpsc::UnboundedSender;
use std::collections::{BTreeSet, HashSet};
use crate::dialog::sort_dialog::{SortDialog, SortDialogMode};
use crate::dialog::filter_dialog::FilterDialog;
use crate::dialog::sql_dialog::SqlDialog;
use crate::dialog::column_width_dialog::ColumnWidthDialog;
use crate::dialog::find_dialog::FindDialog;
use crate::dialog::find_dialog::{FindOptions, SearchMode};
use crate::dialog::find_all_results_dialog::FindAllResultsDialog;
use crate::dialog::dataframe_details_dialog::DataFrameDetailsDialog;
use crate::dialog::data_management_dialog::LoadedDataset;
use crate::dialog::JmesPathDialog;
use crate::dialog::jmes_dialog::JmesPathKeyValuePair;
use crate::dialog::TransformScope;
use crate::dialog::ColumnOperationsDialog;
use crate::dialog::ColumnOperationOptionsDialog;
use crate::dialog::ColumnOperationOptionsMode;
use crate::dialog::ColumnOperationKind;
use crate::dialog::filter_dialog::{FilterExpr, FilterCondition, FilterDialogMode};
use crate::dialog::LlmClientCreateDialog;
// use polars_sql::SQLContext; // replaced by custom new_sql_context
use crate::sql::new_sql_context;
use std::sync::Arc;
use std::collections::HashMap;
use polars_lazy::frame::IntoLazy;
use arboard::Clipboard;
use tracing::{debug, info, error};
use textwrap::wrap;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use regex::Regex;
use serde_json::{Value as JsonValue, Map as JsonMap};
use jmespath;
use serde_json::Value;
use polars::prelude::{NamedFrom, IntoColumn};
use crate::dialog::OperationOptions;
use crate::dialog::{ClusterAlgorithm, KmeansOptions, DbscanOptions};
use crate::dialog::styling::{StyleLogic, Condition, ApplicationScope, GrepCapture, matches_column};
// use crate::dialog::DataExportDialog; // moved to DataTabManagerDialog
use linfa::prelude::{Fit, Predict};
use linfa_reduction::Pca as LinfaPca;
use ndarray::{Array2, ArrayBase, Ix2, OwnedRepr};
use linfa_clustering::KMeans;
use linfa::DatasetBase;


#[derive(Debug, Clone)]
pub struct EmbeddingColumnConfig {
    pub provider: crate::dialog::LlmProvider,
    pub model_name: String,
    pub num_dimensions: usize
}

/// DataTableContainer: Composite widget for DataTable with viewing box and instructions
///
/// This struct manages the state and UI for a composite data table widget, including:
/// - The main data table
/// - A viewing box for the selected cell
/// - Instruction area
/// - Sort, filter, and SQL dialogs
///
/// # Fields
/// - `datatable`: The main DataTable widget
/// - `style`: Style configuration for the UI
/// - `instructions`: Instruction text displayed to the user
/// - `show_instructions`: Flag to control whether the instruction area is shown (default: true)
/// - `sort_dialog`, `filter_dialog`, `sql_dialog`: Dialogs for sort, filter, and SQL operations
/// - `*_active`: Flags indicating if a dialog is currently active
/// - `last_*_area`, `last_*_max_rows`: Caches for dialog rendering
pub struct DataTableContainer {
    pub datatable: DataTable,
    pub style: StyleConfig,
    pub config: Config,
    pub additional_instructions: Option<String>,
    pub show_instructions: bool,
    pub auto_expand_value_display: bool,
    #[allow(dead_code)]
    pub jmes_runtime: jmespath::Runtime,
    // Name to register the current DataFrame under in SQLContext (e.g., tab alias or name)
    pub sql_current_df_name: String,
    pub sort_dialog: SortDialog,
    pub sort_dialog_active: bool,
    pub filter_dialog: FilterDialog,
    pub filter_dialog_active: bool,
    pub sql_dialog: SqlDialog,
    pub sql_dialog_active: bool,
    pub column_width_dialog: ColumnWidthDialog,
    pub column_width_dialog_active: bool,
    pub find_dialog: FindDialog,
    pub find_dialog_active: bool,
    pub find_all_results_dialog: Option<FindAllResultsDialog>,
    pub find_all_results_dialog_active: bool,
    pub dataframe_details_dialog: DataFrameDetailsDialog,
    pub dataframe_details_dialog_active: bool,
    pub jmes_dialog: JmesPathDialog,
    pub jmes_dialog_active: bool,
    pub column_operations_dialog: ColumnOperationsDialog,
    pub column_operations_dialog_active: bool,
    pub column_operation_options_dialog: Option<ColumnOperationOptionsDialog>,
    pub column_operation_options_dialog_active: bool,
    // Data export dialog moved to DataTabManagerDialog
    pub last_sort_dialog_area: Option<ratatui::layout::Rect>,
    pub last_sort_dialog_max_rows: Option<usize>,
    pub last_filter_dialog_area: Option<ratatui::layout::Rect>,
    pub last_filter_dialog_max_rows: Option<usize>,
    pub last_sql_dialog_area: Option<ratatui::layout::Rect>,
    pub last_column_width_dialog_area: Option<ratatui::layout::Rect>,
    pub last_column_width_dialog_max_rows: Option<usize>,
    pub last_find_dialog_area: Option<ratatui::layout::Rect>,
    pub last_find_all_results_dialog_area: Option<ratatui::layout::Rect>,
    pub last_dataframe_details_dialog_area: Option<ratatui::layout::Rect>,
    pub last_dataframe_details_dialog_max_rows: Option<usize>,
    pub last_jmes_dialog_area: Option<ratatui::layout::Rect>,
    pub last_column_operations_dialog_area: Option<ratatui::layout::Rect>,
    pub last_embeddings_prompt_dialog_area: Option<ratatui::layout::Rect>,
    pub current_search_pattern: Option<String>,
    pub current_search_mode: Option<SearchMode>,
    pub current_search_options: Option<FindOptions>,
    pub available_datasets: HashMap<String, LoadedDataset>,
    // Progress overlay and pending long-running operation
    pub busy_active: bool,
    pub busy_message: String,
    pub busy_progress: f64,
    pub queued_embeddings: Option<QueuedEmbeddings>,
    pub in_progress_embeddings: Option<EmbeddingsJob>,
    pub queued_pca: Option<QueuedPca>,
    pub queued_cluster: Option<QueuedCluster>,
    // LLM client creation dialog for ad-hoc operations (e.g., embeddings)
    pub llm_client_create_dialog: Option<LlmClientCreateDialog>,
    pub llm_client_create_dialog_active: bool,
    pub last_llm_client_create_dialog_area: Option<ratatui::layout::Rect>,
    pub pending_embeddings_after_llm_selection: Option<QueuedEmbeddings>,
    // Map embedding column name -> LLM config snapshot used to generate it
    pub embedding_column_config_mapping: HashMap<String, EmbeddingColumnConfig>,
    // Prompt similarity dialog
    pub embeddings_prompt_dialog: Option<crate::dialog::EmbeddingsPromptDialog>,
    pub embeddings_prompt_dialog_active: bool,
    // Pending prompt flow to reopen after embeddings generation
    pub pending_prompt_flow: Option<PendingPromptFlow>,
}

#[derive(Debug, Clone)]
pub struct PendingPromptFlow {
    pub prompt_text: String,
    pub similarity_new_column: String,
    pub embeddings_column_name: Option<String>,
}

impl core::fmt::Debug for DataTableContainer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataTableContainer")
            .field("datatable", &"DataTable{..}")
            .field("style", &"StyleConfig{..}")
            .field("show_instructions", &self.show_instructions)
            .field("sql_current_df_name", &self.sql_current_df_name)
            .field("sort_dialog_active", &self.sort_dialog_active)
            .field("filter_dialog_active", &self.filter_dialog_active)
            .field("sql_dialog_active", &self.sql_dialog_active)
            .field("column_width_dialog_active", &self.column_width_dialog_active)
            .field("find_dialog_active", &self.find_dialog_active)
            .field("find_all_results_dialog_active", &self.find_all_results_dialog_active)
            .field("dataframe_details_dialog_active", &self.dataframe_details_dialog_active)
            .field("jmes_dialog_active", &self.jmes_dialog_active)
            .field("current_search_pattern", &self.current_search_pattern)
            .field("current_search_mode", &self.current_search_mode)
            .field("current_search_options", &self.current_search_options)
            .field("available_datasets", &"HashMap{..}")
            .finish()
    }
}

impl DataTableContainer {
    fn execute_prompt_similarity(&mut self, source_column: &str, new_column_name: &str, prompt_embedding: &[f32]) -> color_eyre::Result<()> {
        use polars::prelude::*;
        let df_arc = self.datatable.get_dataframe()?;
        let df_ref = df_arc.as_ref();
        let s = df_ref.column(source_column).map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        match s.dtype() {
            DataType::List(inner) => {
                match inner.as_ref() {
                    DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
                    DataType::Int128 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 |
                    DataType::UInt64 | DataType::Float32 | DataType::Float64 => {}
                    _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
                }
            }
            _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
        }
        let nrows = s.len();
        if nrows == 0 { return Ok(()); }
        let list = s.list().map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        // Precompute prompt norm
        let mut prompt_norm_sq: f64 = 0.0;
        for v in prompt_embedding.iter() { let fv = *v as f64; prompt_norm_sq += fv * fv; }
        let prompt_norm = prompt_norm_sq.sqrt().max(f64::EPSILON);
        // Compute cosine similarity for each row
        let mut sims: Vec<f32> = Vec::with_capacity(nrows);
        for i in 0..nrows {
            let maybe_sub = list.get_as_series(i);
            if maybe_sub.is_none() { sims.push(0.0); continue; }
            let sub = maybe_sub.unwrap();
            // Extract as f64 vec
            let row_vals_f64: Vec<f64> = match sub.dtype() {
                DataType::Float64 => sub.f64().unwrap().into_no_null_iter().collect::<Vec<f64>>(),
                DataType::Float32 => sub.f32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int64 => sub.i64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int32 => sub.i32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int16 => sub.i16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int8  => sub.i8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt64 => sub.u64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt32 => sub.u32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt16 => sub.u16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt8  => sub.u8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                other => return Err(color_eyre::eyre::eyre!("Unsupported inner dtype in list: {:?}", other)),
            };
            // Length check
            if row_vals_f64.len() != prompt_embedding.len() {
                sims.push(0.0);
                continue;
            }
            // Dot and norm
            let mut dot: f64 = 0.0;
            let mut row_norm_sq: f64 = 0.0;
            for (a_f64, b_f32) in row_vals_f64.iter().zip(prompt_embedding.iter()) {
                let b = *b_f32 as f64;
                dot += (*a_f64) * b;
                row_norm_sq += (*a_f64) * (*a_f64);
            }
            let row_norm = row_norm_sq.sqrt().max(f64::EPSILON);
            let sim = dot / (row_norm * prompt_norm);
            sims.push(sim as f32);
        }
        // Append similarity column
        let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(df_ref.width() + 1);
        for c in df_ref.get_columns() { cols.push(c.clone()); }
        let mut new_name = if new_column_name.trim().is_empty() { format!("{source_column}__prompt_sim") } else { new_column_name.to_string() };
        if df_ref.get_column_names_owned().into_iter().any(|n| n.as_str() == new_name) { new_name = format!("{new_name}__sim"); }
        let series = Series::new(new_name.as_str().into(), sims);
        cols.push(series.into_column());
        let new_df = polars::prelude::DataFrame::new(cols)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame: {}", e))?;
        self.datatable.dataframe.set_current_df(new_df);
        // Auto sort by the similarity column (descending)
        let sort_cols = vec![crate::dialog::sort_dialog::SortColumn { name: new_name.clone(), ascending: false }];
        let _ = self.datatable.dataframe.sort_by_columns(&sort_cols);
        Ok(())
    }
    fn process_next_embeddings_batch(&mut self) -> color_eyre::Result<Option<bool>> {
        let job = if let Some(j) = self.in_progress_embeddings.take() { j } else { return Ok(None) };
        if job.next_start >= job.total_uniques { self.in_progress_embeddings = Some(job); return Ok(Some(true)); }
        let start = job.next_start;
        let end = (start + job.batch_size).min(job.total_uniques);
        let slice: Vec<String> = job.uniques[start..end].to_vec();
        let dims_opt = if job.num_dimensions > 0 { Some(job.num_dimensions) } else { None };
        let provider = job.provider.clone();
        let model_name = job.model_name.clone();
        // Temporarily release `job` mutable borrow before calling into &self method
        self.in_progress_embeddings = Some(job);
        let batch = self.config.llm_config.fetch_embeddings_via_provider(provider, &model_name, &slice, dims_opt)?;
        let mut job = self.in_progress_embeddings.take().expect("embeddings job should exist");
        if batch.len() != slice.len() { return Err(color_eyre::eyre::eyre!("Embeddings provider returned wrong length for batch")); }
        if job.unique_embeddings.is_empty() {
            job.unique_embeddings = vec![Vec::new(); job.total_uniques];
        }
        for (i, emb) in batch.into_iter().enumerate() {
            job.unique_embeddings[start + i] = emb;
        }
        job.next_start = end;
        let progress = end as f64 / job.total_uniques.max(1) as f64;
        self.busy_progress = progress;
        self.busy_message = format!(
            "Generating embeddings with {}... {}/{}",
            job.provider.display_name(),
            end,
            job.total_uniques
        );
        let finished = job.next_start >= job.total_uniques;
        self.in_progress_embeddings = Some(job);
        Ok(Some(finished))
    }

    fn finalize_embeddings_job(&mut self) -> color_eyre::Result<()> {
        use polars::prelude::*;
        let Some(job) = self.in_progress_embeddings.take() else { return Ok(()); };
        // Build ListChunked per row using unique_index and computed embeddings
        let row_embeddings_iter = job.row_texts.into_iter().map(|opt_text| {
            opt_text.map(|t| {
                let idx = job.unique_index.get(&t).copied().unwrap();
                let v: &Vec<f32> = &job.unique_embeddings[idx];
                Series::new(PlSmallStr::EMPTY, v.clone())
            })
        });
        let mut lc: ListChunked = row_embeddings_iter.collect();
        // Determine column name (avoid collisions)
        let df_arc = self.datatable.get_dataframe()?;
        let df_ref = df_arc.as_ref();
        let mut new_name = if job.new_column_name.trim().is_empty() { format!("{}_emb", job.source_column) } else { job.new_column_name };
        if df_ref.get_column_names_owned().into_iter().any(|n| n.as_str() == new_name) { new_name = format!("{new_name}__emb"); }
        lc.rename(PlSmallStr::from_str(&new_name));
        let list_series = lc.into_series();
        // Append column to DataFrame
        let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(df_ref.width() + 1);
        for c in df_ref.get_columns() { cols.push(c.clone()); }
        cols.push(list_series.into_column());
        let new_df = polars::prelude::DataFrame::new(cols)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame: {}", e))?;
        self.datatable.dataframe.set_current_df(new_df);
        // Hide the new column if requested
        if job.hide_new_column {
            self.datatable.dataframe.column_width_config.hidden_columns.insert(new_name.clone(), true);
        }
        Ok(())
    }

    /// Create a new DataTableContainer with the given DataTable and style configuration.
    ///
    /// # Arguments
    /// * `datatable` - The main DataTable widget to display
    /// * `style` - Style configuration for the UI
    ///
    /// # Returns
    /// A new `DataTableContainer` instance
    pub fn new(datatable: DataTable, style: StyleConfig) -> Self {
        Self::new_with_dataframes(datatable, style, HashMap::new())
    }


    fn execute_pca(&mut self, source_column: &str, new_column_name: &str, target_k: usize) -> color_eyre::Result<()> {
        use polars::prelude::*;
        let df_arc = self.datatable.get_dataframe()?;
        let df_ref = df_arc.as_ref();
        let s = df_ref.column(source_column).map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        // Validate
        match s.dtype() {
            DataType::List(inner) => {
                match inner.as_ref() {
                    DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
                    DataType::Int128 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 |
                    DataType::UInt64 | DataType::Float32 | DataType::Float64 => {}
                    _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
                }
            }
            _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
        }
        let nrows = s.len();
        if nrows == 0 { return Ok(()); }
        let list = s.list().map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        let mut data: Vec<Vec<f64>> = Vec::with_capacity(nrows);
        let mut d_opt: Option<usize> = None;
        for i in 0..nrows {
            let sub = list
                .get_as_series(i)
                .ok_or_else(|| color_eyre::eyre::eyre!("Row {} is null or missing in '{}'", i, source_column))?;
            let sub_f64 = match sub.dtype() {
                DataType::Float64 => sub.f64().unwrap().into_no_null_iter().collect::<Vec<f64>>(),
                DataType::Float32 => sub.f32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int64 => sub.i64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int32 => sub.i32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int16 => sub.i16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int8  => sub.i8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt64 => sub.u64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt32 => sub.u32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt16 => sub.u16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt8  => sub.u8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                other => return Err(color_eyre::eyre::eyre!("Unsupported inner dtype in list: {:?}", other)),
            };
            if let Some(d) = d_opt { if sub_f64.len() != d { return Err(color_eyre::eyre::eyre!("Inconsistent vector length at row {}: expected {}, got {}", i, d, sub_f64.len())); } } else { d_opt = Some(sub_f64.len()); }
            data.push(sub_f64);
        }
        let d = d_opt.unwrap_or(0);
        if d == 0 { return Err(color_eyre::eyre::eyre!("Vectors have zero length")); }
        let k = target_k.clamp(1, d);
        // Build ndarray matrix (nrows x d)
        let mut x = Array2::<f64>::zeros((nrows, d));
        for i in 0..nrows { for j in 0..d { x[[i, j]] = data[i][j]; } }
        // Fit PCA with linfa on a dataset wrapper
        let ds = linfa::DatasetBase::from(x);
        let pca = LinfaPca::params(k).fit(&ds).map_err(|e| color_eyre::eyre::eyre!("PCA fit failed: {:?}", e))?;
        let y_ds = pca.predict(ds);
        let y: ArrayBase<OwnedRepr<f64>, Ix2> = y_ds.records;
        // Build new column
        let row_iter = (0..nrows).map(|i| {
            let row_vals: Vec<f32> = (0..k).map(|j| y[(i, j)] as f32).collect();
            Some(Series::new(PlSmallStr::EMPTY, row_vals))
        });
        let mut lc: ListChunked = row_iter.collect();
        let mut new_name = if new_column_name.trim().is_empty() { format!("{source_column}_pca") } else { new_column_name.to_string() };
        if df_ref.get_column_names_owned().into_iter().any(|n| n.as_str() == new_name) { new_name = format!("{new_name}__pca"); }
        lc.rename(PlSmallStr::from_str(&new_name));
        let list_series = lc.into_series();
        let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(df_ref.width() + 1);
        for c in df_ref.get_columns() { cols.push(c.clone()); }
        cols.push(list_series.into_column());
        let new_df = polars::prelude::DataFrame::new(cols)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame: {}", e))?;
        self.datatable.dataframe.set_current_df(new_df);
        Ok(())
    }

    fn execute_cluster(
        &mut self,
        source_column: &str,
        new_column_name: &str,
        algorithm: ClusterAlgorithm,
        kmeans: Option<KmeansOptions>,
        _dbscan: Option<DbscanOptions>,
    ) -> color_eyre::Result<()> {
        use polars::prelude::*;
        let df_arc = self.datatable.get_dataframe()?;
        let df_ref = df_arc.as_ref();
        let s = df_ref.column(source_column).map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        match s.dtype() {
            DataType::List(inner) => {
                match inner.as_ref() {
                    DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
                    DataType::Int128 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 |
                    DataType::UInt64 | DataType::Float32 | DataType::Float64 => {}
                    _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
                }
            }
            _ => return Err(color_eyre::eyre::eyre!("Source column '{}' must be vector of numbers", source_column)),
        }
        let nrows = s.len();
        if nrows == 0 { return Ok(()); }
        let list = s.list().map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        let mut data: Vec<Vec<f64>> = Vec::with_capacity(nrows);
        let mut d_opt: Option<usize> = None;
        for i in 0..nrows {
            let sub = list
                .get_as_series(i)
                .ok_or_else(|| color_eyre::eyre::eyre!("Row {} is null or missing in '{}'", i, source_column))?;
            let sub_f64 = match sub.dtype() {
                DataType::Float64 => sub.f64().unwrap().into_no_null_iter().collect::<Vec<f64>>(),
                DataType::Float32 => sub.f32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int64 => sub.i64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int32 => sub.i32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int16 => sub.i16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::Int8  => sub.i8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt64 => sub.u64().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt32 => sub.u32().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt16 => sub.u16().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                DataType::UInt8  => sub.u8().unwrap().into_no_null_iter().map(|v| v as f64).collect::<Vec<f64>>(),
                other => return Err(color_eyre::eyre::eyre!("Unsupported inner dtype in list: {:?}", other)),
            };
            if let Some(d) = d_opt { if sub_f64.len() != d { return Err(color_eyre::eyre::eyre!("Inconsistent vector length at row {}: expected {}, got {}", i, d, sub_f64.len())); } } else { d_opt = Some(sub_f64.len()); }
            data.push(sub_f64);
        }
        let d = d_opt.unwrap_or(0);
        if d == 0 { return Err(color_eyre::eyre::eyre!("Vectors have zero length")); }
        // ndarray features
        let mut x = Array2::<f64>::zeros((nrows, d));
        for i in 0..nrows { for j in 0..d { x[[i, j]] = data[i][j]; } }
        // Build dataset for algorithms that expect DatasetBase
        let ds = DatasetBase::from(x.clone());
        // Run clustering
        let labels: Vec<usize> = match algorithm {
            ClusterAlgorithm::Kmeans => {
                let k = kmeans.map(|o| o.number_of_clusters).unwrap_or(8);
                let model = KMeans::params(k).fit(&ds).map_err(|e| color_eyre::eyre::eyre!("KMeans fit failed: {:?}", e))?;
                let pred = model.predict(ds);
                pred.targets.to_vec()
            }
            ClusterAlgorithm::Dbscan => {
                // DBSCAN not available with current linfa_clustering API in this build; return error
                return Err(color_eyre::eyre::eyre!("DBSCAN clustering is currently unsupported in this build"));
            }
        };
        // Append labels as a new Int32 column
        let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(df_ref.width() + 1);
        for c in df_ref.get_columns() { cols.push(c.clone()); }
        let mut new_name = if new_column_name.trim().is_empty() { format!("{source_column}_cluster") } else { new_column_name.to_string() };
        if df_ref.get_column_names_owned().into_iter().any(|n| n.as_str() == new_name) { new_name = format!("{new_name}__cluster"); }
        // Convert labels to i32; for DBSCAN we used usize::MAX to denote noise -> -1
        let labels_i32: Vec<i32> = labels
            .into_iter()
            .map(|v| if v == usize::MAX { -1 } else { v as i32 })
            .collect();
        let series = Series::new(new_name.as_str().into(), labels_i32);
        cols.push(series.into_column());
        let new_df = polars::prelude::DataFrame::new(cols)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame: {}", e))?;
        self.datatable.dataframe.set_current_df(new_df);
        Ok(())
    }
    
    /// Set the SQL statement for the SQL dialog
    pub fn set_sql_statement(&mut self, sql_statement: String) {
        self.sql_dialog.set_textarea_content(sql_statement);
    }

    /// Set the filter expression for the filter dialog
    pub fn set_filter_expression(&mut self, filter_expression: FilterExpr) {
        self.datatable.dataframe.filter = Some(filter_expression.clone());
        self.filter_dialog.set_root_expr(filter_expression);
    }

    /// Create a new DataTableContainer with available DataFrames for SQL context.
    ///
    /// # Arguments
    /// * `datatable` - The main DataTable widget to display
    /// * `style` - Style configuration for the UI
    /// * `available_datasets` - Map of dataset names to DataFrames for SQL queries
    ///
    /// # Returns
    /// A new `DataTableContainer` instance
    pub fn new_with_dataframes(
        datatable: DataTable,
        style: StyleConfig, 
        available_datasets: HashMap<String, LoadedDataset>
    ) -> Self {
        let sort_dialog = SortDialog::new(
            datatable
                .dataframe
                .current_df
                .as_ref()
                .map(|a| a.get_column_names_owned())
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.to_string())
                .collect()
        );
        let filter_dialog = FilterDialog::new(
            datatable
                .dataframe
                .current_df
                .as_ref()
                .map(|a| a.get_column_names_owned())
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.to_string())
                .collect()
        );
        let sql_dialog = SqlDialog::new();
        let column_width_dialog = ColumnWidthDialog::new(
            datatable
                .dataframe
                .current_df
                .as_ref()
                .map(|a| a.get_column_names_owned())
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.to_string())
                .collect()
        );
        let find_dialog = FindDialog::new();
        
        // Capture SQL registration name before moving `datatable`
        let sql_name = datatable.dataframe.metadata.name.clone();
        let dataframe_details_dialog = DataFrameDetailsDialog::new();
        let jmes_dialog = JmesPathDialog::new();
        let column_operations_dialog = ColumnOperationsDialog::new();
        let jmes_runtime = crate::jmes::new_runtime();
        Self {
            datatable,
            style,
            config: Config::default(),
            additional_instructions: None,
            show_instructions: true,
            auto_expand_value_display: false,
            jmes_runtime,
            sql_current_df_name: sql_name,
            sort_dialog,
            sort_dialog_active: false,
            filter_dialog,
            filter_dialog_active: false,
            sql_dialog,
            sql_dialog_active: false,
            column_width_dialog,
            column_width_dialog_active: false,
            find_dialog,
            find_dialog_active: false,
            find_all_results_dialog: None,
            find_all_results_dialog_active: false,
            dataframe_details_dialog,
            dataframe_details_dialog_active: false,
            jmes_dialog,
            jmes_dialog_active: false,
            column_operations_dialog,
            column_operations_dialog_active: false,
            column_operation_options_dialog: None,
            column_operation_options_dialog_active: false,
            last_sort_dialog_area: None,
            last_sort_dialog_max_rows: None,
            last_filter_dialog_area: None,
            last_filter_dialog_max_rows: None,
            last_sql_dialog_area: None,
            last_column_width_dialog_area: None,
            last_column_width_dialog_max_rows: None,
            last_find_dialog_area: None,
            last_find_all_results_dialog_area: None,
            last_dataframe_details_dialog_area: None,
            last_dataframe_details_dialog_max_rows: None,
            last_jmes_dialog_area: None,
            last_column_operations_dialog_area: None,
            last_embeddings_prompt_dialog_area: None,
            current_search_pattern: None,
            current_search_mode: None,
            current_search_options: None,
            available_datasets,
            busy_active: false,
            busy_message: String::new(),
            busy_progress: 0.0,
            queued_embeddings: None,
            in_progress_embeddings: None,
            queued_pca: None,
            queued_cluster: None,
            llm_client_create_dialog: None,
            llm_client_create_dialog_active: false,
            last_llm_client_create_dialog_area: None,
            pending_embeddings_after_llm_selection: None,
            embedding_column_config_mapping: HashMap::new(),
            embeddings_prompt_dialog: None,
            embeddings_prompt_dialog_active: false,
            pending_prompt_flow: None,
        }
    }

    /// Helper: return source DataFrame based on `TransformScope`.
    fn get_source_df_for_scope(&self, scope: TransformScope) -> color_eyre::Result<polars::prelude::DataFrame> {
        let source_df = match scope {
            TransformScope::Original => self.datatable.dataframe.collect_base_df()?,
            TransformScope::Current => self.datatable.get_dataframe()?.as_ref().clone(),
        };
        Ok(source_df)
    }

    /// Helper: get column names as owned `String`s.
    fn get_column_names_vec(df: &polars::prelude::DataFrame) -> Vec<String> {
        df
            .get_column_names_owned()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Helper: convert an optional Polars AnyValue into serde_json::Value for JMES input.
    fn anyvalue_opt_to_json(v: Option<polars::prelude::AnyValue>) -> serde_json::Value {
        match v {
            Some(polars::prelude::AnyValue::Null) | None => serde_json::Value::Null,
            Some(polars::prelude::AnyValue::Boolean(b)) => serde_json::Value::Bool(b),
            Some(polars::prelude::AnyValue::Int8(x)) => serde_json::Value::Number((x as i64).into()),
            Some(polars::prelude::AnyValue::Int16(x)) => serde_json::Value::Number((x as i64).into()),
            Some(polars::prelude::AnyValue::Int32(x)) => serde_json::Value::Number((x as i64).into()),
            Some(polars::prelude::AnyValue::Int64(x)) => serde_json::Value::Number(x.into()),
            Some(polars::prelude::AnyValue::UInt8(x)) => serde_json::Value::Number((x as u64).into()),
            Some(polars::prelude::AnyValue::UInt16(x)) => serde_json::Value::Number((x as u64).into()),
            Some(polars::prelude::AnyValue::UInt32(x)) => serde_json::Value::Number((x as u64).into()),
            Some(polars::prelude::AnyValue::UInt64(x)) => serde_json::Value::Number(serde_json::Number::from(x)),
            Some(polars::prelude::AnyValue::Float32(f)) => {
                if f.is_finite() { serde_json::Number::from_f64(f as f64).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null) } else { serde_json::Value::Null }
            }
            Some(polars::prelude::AnyValue::Float64(f)) => {
                if f.is_finite() { serde_json::Number::from_f64(f).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null) } else { serde_json::Value::Null }
            }
            Some(polars::prelude::AnyValue::String(s)) => serde_json::Value::String(s.into()),
            Some(other) => serde_json::Value::String(other.to_string()),
        }
    }

    /// Helper: convert optional AnyValue directly to display string.
    fn anyvalue_opt_to_string(v: Option<polars::prelude::AnyValue>) -> String {
        match v {
            Some(polars::prelude::AnyValue::Null) | None => String::new(),
            Some(av) => av.str_value().to_string(),
        }
    }

    /// Helper: build a JSON object for a single row from a DataFrame.
    fn build_row_object_json(
        df: &polars::prelude::DataFrame,
        col_names: &[String],
        row_idx: usize,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut row_obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for col in col_names {
            let v = df.column(col)
                .ok()
                .and_then(|s| s.get(row_idx).ok());
            let json_v = Self::anyvalue_opt_to_json(v);
            row_obj.insert(col.clone(), json_v);
        }
        row_obj
    }

    /// Helper: ensure a JMES result is an object and convert it to a serde_json::Map.
    fn jmes_result_to_object_map(
        result: &jmespath::Variable,
        row_idx: usize,
    ) -> color_eyre::Result<serde_json::Map<String, serde_json::Value>> {
        if !result.is_object() {
            return Err(color_eyre::eyre::eyre!(
                "JMESPath result must be an object; got non-object at row {}",
                row_idx
            ));
        }
        let json_str = result.to_string();
        let json_val: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to parse JMES result as JSON at row {}: {}", row_idx, e))?;
        let obj = json_val
            .as_object()
            .cloned()
            .ok_or_else(|| color_eyre::eyre::eyre!("JMESPath result is not an object at row {}", row_idx))?;
        Ok(obj)
    }

    /// Helper: stringify a serde_json::Value with special-casing for Null.
    fn json_to_string(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => s.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    /// Helper: build a DataFrame where each column is string-typed from a vector of JSON objects.
    fn build_df_from_json_maps(
        objs: &[serde_json::Map<String, serde_json::Value>],
    ) -> color_eyre::Result<polars::prelude::DataFrame> {
        let mut all_keys: BTreeSet<String> = BTreeSet::new();
        for obj in objs {
            for k in obj.keys() {
                all_keys.insert(k.clone());
            }
        }
        let mut series_list: Vec<polars::prelude::Series> = Vec::with_capacity(all_keys.len());
        for key in all_keys {
            let mut col_vals: Vec<String> = Vec::with_capacity(objs.len());
            for obj in objs {
                let s = obj.get(&key)
                    .map(Self::json_to_string)
                    .unwrap_or_default();
                col_vals.push(s);
            }
            series_list.push(polars::prelude::Series::new(key.as_str().into(), col_vals));
        }
        let columns: Vec<polars::prelude::Column> = series_list
            .into_iter()
            .map(|s| s.into_column())
            .collect();
        let df = polars::prelude::DataFrame::new(columns)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame from JMES results: {}", e))?;
        Ok(df)
    }

    /// Helper: build a DataFrame from row string maps, preserving a desired column order.
    /// Any columns not present in `ordered_columns` will be appended afterwards in alphabetical order.
    fn build_df_from_string_maps_with_order(
        rows: &[std::collections::BTreeMap<String, String>],
        ordered_columns: &[String],
    ) -> color_eyre::Result<polars::prelude::DataFrame> {
        // Determine union of keys across all rows
        let mut all_keys: BTreeSet<String> = BTreeSet::new();
        for m in rows { for k in m.keys() { all_keys.insert(k.clone()); } }

        // Compute final order: keep ordered_columns first as-is, then any remaining keys in sorted order
        let mut seen: HashSet<String> = HashSet::new();
        let mut final_order: Vec<String> = Vec::new();
        for c in ordered_columns {
            if all_keys.contains(c) && seen.insert(c.clone()) {
                final_order.push(c.clone());
            }
        }
        for k in all_keys {
            if seen.insert(k.clone()) {
                final_order.push(k);
            }
        }

        // Materialize columns in the decided order
        let mut series_list: Vec<polars::prelude::Series> = Vec::with_capacity(final_order.len());
        for key in final_order {
            let mut col_vals: Vec<String> = Vec::with_capacity(rows.len());
            for row in rows {
                col_vals.push(row.get(&key).cloned().unwrap_or_default());
            }
            series_list.push(polars::prelude::Series::new(key.as_str().into(), col_vals));
        }
        let columns: Vec<polars::prelude::Column> = series_list.into_iter().map(|s| s.into_column()).collect();
        let df = polars::prelude::DataFrame::new(columns)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to build DataFrame from Add Columns results: {}", e))?;
        Ok(df)
    }

    /// Apply a JMESPath transform to the DataFrame according to the provided scope.
    ///
    /// - When scope is `TransformScope::Original`, iterate the base DataFrame (not `current_df`).
    /// - The JMESPath query must return an object for each record; otherwise an error is returned.
    fn apply_jmes_transform(&mut self, query: &str, scope: TransformScope) -> color_eyre::Result<()> {
        // Compile the JMESPath expression using stored custom runtime
        let expr = self.jmes_runtime
            .compile(query)
            .map_err(|e| color_eyre::eyre::eyre!("JMESPath compile error: {}", e))?;

        // Choose the source DataFrame per scope
        let source_df = self.get_source_df_for_scope(scope)?;

        let nrows = source_df.height();
        if nrows == 0 {
            // Empty source -> empty result
            self.datatable.set_current_df(polars::prelude::DataFrame::empty());
            return Ok(());
        }

        // Prepare column names once
        let col_names: Vec<String> = Self::get_column_names_vec(&source_df);

        // Convert each row to a JSON object, run JMES, ensure object, collect results
        let mut transformed_rows: Vec<JsonMap<String, JsonValue>> = Vec::with_capacity(nrows);
        for row_idx in 0..nrows {
            // Build input object for this row
            let row_obj: JsonMap<String, JsonValue> = Self::build_row_object_json(&source_df, &col_names, row_idx);

            // Evaluate JMESPath
            let var = jmespath::Variable::try_from(JsonValue::Object(row_obj))
                .map_err(|e| color_eyre::eyre::eyre!("Failed to convert row to JMES variable at row {}: {}", row_idx, e))?;
            let result = expr
                .search(&var)
                .map_err(|e| color_eyre::eyre::eyre!("JMESPath search error at row {}: {}", row_idx, e))?;

            // Ensure object result and convert
            let obj = Self::jmes_result_to_object_map(&result, row_idx)?;
            transformed_rows.push(obj);
        }

        // Build DataFrame from object rows
        let new_df = Self::build_df_from_json_maps(&transformed_rows)?;

        // Set as current view
        self.datatable.set_current_df(new_df);
        Ok(())
    }

    /// Add/merge columns using JMESPath expressions per provided key/value pairs.
    ///
    /// Behavior per pair:
    /// - name != "": evaluate query and set column `name` to the stringified result for each row.
    /// - name == "": evaluate query; it must be null or an object. If an object, merge all fields
    ///   into the row (stringified). Conflicts override existing values in that row's result.
    ///
    /// Notes:
    /// - Input scope determines whether Original (`df`) or Current (`current_df`) is used as source.
    /// - Output is a new DataFrame where all values are string-typed for consistency.
    fn apply_jmes_add_columns(&mut self, pairs: Vec<JmesPathKeyValuePair>, scope: TransformScope) -> color_eyre::Result<()> {
        if pairs.is_empty() { return Ok(()); }

        // Pre-compile expressions (with stored custom runtime)
        let mut compiled = Vec::with_capacity(pairs.len());
        for p in &pairs {
            let expr = self.jmes_runtime
                .compile(&p.value)
                .map_err(|e| color_eyre::eyre::eyre!("JMESPath compile error for '{}': {}", p.name, e))?;
            compiled.push((p.name.clone(), expr));
        }

        // Select source DataFrame
        let source_df = self.get_source_df_for_scope(scope)?;

        let nrows = source_df.height();
        if nrows == 0 {
            return Ok(());
        }

        let col_names: Vec<String> = Self::get_column_names_vec(&source_df);

        // Per-row final field map (string values), initialized from original row string values
        let mut row_maps: Vec<std::collections::BTreeMap<String, String>> = Vec::with_capacity(nrows);
        row_maps.resize_with(nrows, std::collections::BTreeMap::new);

        // Initialize with existing columns as strings
        for (row_idx, map) in row_maps.iter_mut().enumerate().take(nrows) {
            for col in &col_names {
                let v = source_df.column(col).ok().and_then(|s| s.get(row_idx).ok());
                map.insert(col.clone(), Self::anyvalue_opt_to_string(v));
            }
        }

        // Apply expressions row-wise
        for (row_idx, row_map) in row_maps.iter_mut().enumerate().take(nrows) {
            // Build JSON input for this row
            let row_obj: JsonMap<String, JsonValue> = Self::build_row_object_json(&source_df, &col_names, row_idx);
            let var = jmespath::Variable::try_from(JsonValue::Object(row_obj))
                .map_err(|e| color_eyre::eyre::eyre!("Failed to convert row to JMES variable at row {}: {}", row_idx, e))?;

            // For each pair, evaluate and assign/merge
            for (name, expr) in &compiled {
                let result = expr
                    .search(&var)
                    .map_err(|e| color_eyre::eyre::eyre!("JMESPath search error at row {}: {}", row_idx, e))?;
                if !name.is_empty() {
                    // Assign to named column
                    let s = if result.is_null() {
                        String::new()
                    } else {
                        result.to_string()
                    };
                    row_map.insert(name.clone(), s);
                } else {
                    // Merge object or accept null
                    if result.is_null() {
                        continue;
                    }
                    // Parse object and merge keys
                    let obj = Self::jmes_result_to_object_map(&result, row_idx)?;
                    for (k, v) in obj {
                        let s = Self::json_to_string(&v);
                        row_map.insert(k.clone(), s);
                    }
                }
            }
        }

        // Build DataFrame from per-row string maps, preserving original column order and appending new ones
        let new_df = Self::build_df_from_string_maps_with_order(&row_maps, &col_names)?;
        self.datatable.set_current_df(new_df);
        Ok(())
    }

    /// Get the value of the currently selected cell as a string.
    ///
    /// Returns an empty string if the selection is out of bounds or the value cannot be retrieved.
    pub fn selected_cell_json_value(&self) -> Result<Value> {
        self.datatable.selected_cell_json_value()
    }

    /// Get the value of the currently selected cell with highlighted search matches.
    ///
    /// Returns a Line with spans, highlighting any matches of the current search pattern.
    /// Also applies RegexGroup styles from style rules.
    pub fn selected_cell_value_with_highlighting(&self) -> Result<Line<'static>> {
        let cell_value = self.selected_cell_json_value()?;
        let cell_value = match cell_value {
            Value::String(s) => s,
            v => v.to_string(),
        };
        
        // Collect styled ranges (start, end, style) for regex group styling
        let mut styled_ranges: Vec<(usize, usize, ratatui::style::Style)> = Vec::new();
        
        // Get current column name for column matching
        let current_col_name = self.selected_column_name().unwrap_or_default();
        
        // Collect RegexGroup styles from style rules
        for style_set in &self.datatable.style_sets {
            for rule in &style_set.rules {
                if let StyleLogic::Conditional(cond) = &rule.logic {
                    // Only process Regex conditions
                    if let Condition::Regex { pattern, columns } = &cond.condition {
                        // Check if current column matches condition columns
                        let column_matches = columns.as_ref()
                            .map(|cols| cols.is_empty() || matches_column(&current_col_name, cols))
                            .unwrap_or(true);
                        
                        if !column_matches {
                            continue;
                        }
                        
                        // Check if the regex matches the cell value
                        let re = match Regex::new(pattern) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        
                        if !re.is_match(&cell_value) {
                            continue;
                        }
                        
                        // Apply RegexGroup style applications
                        for app in &cond.applications {
                            if let ApplicationScope::RegexGroup(capture) = &app.scope {
                                // Check if target columns include current column
                                let target_matches = app.target_columns.as_ref()
                                    .or(columns.as_ref())
                                    .map(|cols| cols.is_empty() || matches_column(&current_col_name, cols))
                                    .unwrap_or(true);
                                
                                if !target_matches {
                                    continue;
                                }
                                
                                let style = app.style.to_ratatui_style();
                                
                                // Find all capture group matches and collect their ranges
                                for caps in re.captures_iter(&cell_value) {
                                    let capture_match = match capture {
                                        GrepCapture::Group(n) => caps.get(*n),
                                        GrepCapture::Name(name) => caps.name(name),
                                    };
                                    
                                    if let Some(m) = capture_match {
                                        styled_ranges.push((m.start(), m.end(), style));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // If we have search highlighting, that takes precedence
        if let (Some(pattern), Some(search_mode), Some(options)) = 
            (&self.current_search_pattern, &self.current_search_mode, &self.current_search_options)
            && !pattern.is_empty() {
                let cell_value_safe = cell_value.replace("\n", "").replace("\r", "");
                match search_mode {
                    SearchMode::Normal => {
                        // Normal text search
                        let search_text = if options.match_case { 
                            pattern.clone() 
                        } else { 
                            pattern.to_lowercase() 
                        };
                        let cell_text = if options.match_case { 
                            cell_value_safe.clone() 
                        } else { 
                            cell_value_safe.to_lowercase() 
                        };
                        
                        if options.whole_word {
                            // Whole word matching
                            let words: Vec<&str> = cell_text.split_whitespace().collect();
                            let search_word = search_text.trim();
                            if words.contains(&search_word) {
                                // Find the actual position in the original text
                                if let Some(pos) = cell_value.to_lowercase().find(search_word) {
                                    let mut spans = Vec::new();
                                    if pos > 0 {
                                        spans.push(Span::raw(cell_value_safe[..pos].to_string()));
                                    }
                                    spans.push(Span::styled(
                                        cell_value_safe[pos..pos + search_word.len()].to_string(),
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Black)
                                            .bg(ratatui::style::Color::Yellow)
                                            .add_modifier(ratatui::style::Modifier::BOLD)
                                    ));
                                    if pos + search_word.len() < cell_value.len() {
                                        spans.push(Span::raw(cell_value_safe[pos + search_word.len()..].to_string()));
                                    }
                                    return Ok(Line::from(spans));
                                }
                            }
                        } else {
                            // Substring matching
                            if let Some(pos) = cell_text.find(&search_text) {
                                let mut spans = Vec::new();
                                if pos > 0 {
                                    spans.push(Span::raw(cell_value_safe[..pos].to_string()));
                                }
                                spans.push(Span::styled(
                                    cell_value_safe[pos..pos + search_text.len()].to_string(),
                                    ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::Black)
                                        .bg(ratatui::style::Color::Yellow)
                                        .add_modifier(ratatui::style::Modifier::BOLD)
                                ));
                                if pos + search_text.len() < cell_value_safe.len() {
                                    spans.push(Span::raw(cell_value_safe[pos + search_text.len()..].to_string()));
                                }
                                return Ok(Line::from(spans));
                            }
                        }
                    }
                    SearchMode::Regex => {
                        // Regex search
                        let re = if options.match_case {
                            Regex::new(pattern)
                        } else {
                            Regex::new(&format!("(?i){pattern}"))
                        };
                        if let Ok(re) = re {
                            let mut spans = Vec::new();
                            let mut last_end = 0;
                            let cell_value_safe = cell_value.replace("\n", "").replace("\r", "");
                            for mat in re.find_iter(&cell_value_safe) {
                                // Add text before match
                                if mat.start() > last_end {
                                    spans.push(Span::raw(cell_value_safe[last_end..mat.start()].to_string()));
                                }
                                // Add highlighted match
                                spans.push(Span::styled(
                                    cell_value_safe[mat.start()..mat.end()].to_string(),
                                    ratatui::style::Style::default()
                                        .fg(ratatui::style::Color::Black)
                                        .bg(ratatui::style::Color::Yellow)
                                        .add_modifier(ratatui::style::Modifier::BOLD)
                                ));
                                last_end = mat.end();
                            }
                            // Add remaining text
                            if last_end < cell_value.len() {
                                spans.push(Span::raw(cell_value_safe[last_end..].to_string()));
                            }
                            return Ok(Line::from(spans));
                        }
                    }
                }
            }
        
        // Apply RegexGroup styles if present
        if !styled_ranges.is_empty() {
            // Sort ranges by start position
            styled_ranges.sort_by_key(|(start, _, _)| *start);
            
            // Remove overlapping ranges (keep first one for overlaps)
            let mut non_overlapping: Vec<(usize, usize, ratatui::style::Style)> = Vec::new();
            for (start, end, style) in styled_ranges {
                if non_overlapping.is_empty() || start >= non_overlapping.last().unwrap().1 {
                    non_overlapping.push((start, end, style));
                }
            }
            
            // Build spans
            let mut spans = Vec::new();
            let mut last_end = 0;
            
            for (start, end, style) in non_overlapping {
                // Add unstyled text before this match
                if start > last_end {
                    spans.push(Span::raw(cell_value[last_end..start].to_string()));
                }
                
                // Add styled match
                spans.push(Span::styled(cell_value[start..end].to_string(), style));
                last_end = end;
            }
            
            // Add remaining text after last match
            if last_end < cell_value.len() {
                spans.push(Span::raw(cell_value[last_end..].to_string()));
            }
            
            return Ok(Line::from(spans));
        }
        
        // No highlighting needed
        Ok(Line::from(cell_value))
    }

    /// Get the name of the currently selected column as a string.
    ///
    /// Returns an empty string if the selection is out of bounds.
    pub fn selected_column_name(&self) -> Result<String> {
        let visible_columns = self.datatable.get_visible_columns()?;
        let col = self.datatable.selection.col;
        if col < visible_columns.len() {
            Ok(visible_columns[col].clone())
        } else {
            Ok("".to_string())
        }
    }

    /// Toggle the visibility of the instructions area.
    pub fn toggle_instructions(&mut self) {
        self.show_instructions = !self.show_instructions;
    }

    /// Update the available DataFrames for SQL context.
    pub fn set_available_datasets(&mut self, available_datasets: HashMap<String, LoadedDataset>) {
        self.available_datasets = available_datasets;
    }

    /// Get the available DataFrames for SQL context.
    pub fn get_available_datasets(&self) -> &HashMap<String, LoadedDataset> {
        &self.available_datasets
    }

    fn build_instructions_from_config(&self) -> String {
        self.config.actions_to_instructions(&[
            (Mode::DataTableContainer, Action::OpenSortDialog),
            (Mode::DataTableContainer, Action::QuickSortCurrentColumn),
            (Mode::DataTableContainer, Action::OpenFilterDialog),
            (Mode::DataTableContainer, Action::QuickFilterEqualsCurrentValue),
            (Mode::DataTableContainer, Action::MoveSelectedColumnLeft),
            (Mode::DataTableContainer, Action::MoveSelectedColumnRight),
            // (Mode::DataTableContainer, Action::OpenDataExportDialog),
            (Mode::DataTableContainer, Action::OpenSqlDialog),
            (Mode::DataTableContainer, Action::OpenJmesDialog),
            (Mode::DataTableContainer, Action::OpenColumnOperationsDialog),
            (Mode::DataTableContainer, Action::OpenFindDialog),
            (Mode::DataTableContainer, Action::OpenDataframeDetailsDialog),
            (Mode::DataTableContainer, Action::OpenColumnWidthDialog),
            (Mode::DataTableContainer, Action::CopySelectedCell),
            (Mode::Global, Action::ToggleInstructions),
        ])
    }

    fn get_instructions(&self) -> String {
        let base_instructions = self.build_instructions_from_config();
        
        if let Some(additional_instructions) = &self.additional_instructions {
            format!("{additional_instructions}  {base_instructions}")
        } else {
            base_instructions
        }
    }

    pub fn set_additional_instructions(&mut self, instructions: String) {
        self.additional_instructions = Some(instructions);
    }
}

impl Component for DataTableContainer {
    /// Register an action handler for the component.
    ///
    /// This method is a no-op for DataTableContainer, as it does not use external action handlers.
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        let _ = tx;
        Ok(())
    }
    /// Register a configuration handler for the component.
    ///
    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config.clone();
        // Propagate to subcomponents that implement config registration
        let _ = self.datatable.register_config_handler(config.clone());
        let _ = self.sort_dialog.register_config_handler(config.clone());
        let _ = self.filter_dialog.register_config_handler(config.clone());
        let _ = self.sql_dialog.register_config_handler(config.clone());
        let _ = self.column_width_dialog.register_config_handler(config.clone());
        let _ = self.find_dialog.register_config_handler(config.clone());
        let _ = self.dataframe_details_dialog.register_config_handler(config.clone());
        let _ = self.jmes_dialog.register_config_handler(config.clone());
        let _ = self.column_operations_dialog.register_config_handler(config.clone());
        // DataExportDialog moved to DataTabManagerDialog
        Ok(())
    }
    /// Initialize the component with the given area size.
    ///
    /// This method is a no-op for DataTableContainer, as it does not require area-based initialization.
    fn init(&mut self, area: Size) -> Result<()> {
        let _ = area;
        Ok(())
    }
    /// Handle incoming events (keyboard or mouse) and dispatch actions.
    ///
    /// This method routes events to the appropriate dialog or the underlying DataTable, activating dialogs or performing actions as needed.
    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        // Forward events to DataTable or handle container-specific events
        let action = match event {
            Some(Event::Key(key_event)) => self.handle_key_event(key_event)?,
            Some(Event::Mouse(mouse_event)) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }
    /// Handle keyboard events for the component.
    ///
    /// This method manages dialog activation, dialog event handling, and forwards navigation events to the DataTable.
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        debug!("DataTableContainer handle_key_event: {:?}", key);
        // If busy overlay is active, consume input except maybe Esc; block cell navigation
        if self.busy_active {
            // Ignore all input while busy to prevent navigation/interaction
            return Ok(None);
        }

        // Route key events to FindAllResultsDialog if active (check this first)
        if self.find_all_results_dialog_active {
            if let Some(dialog) = &mut self.find_all_results_dialog
                && let Some(action) = dialog.handle_key_event(key) {
                    match action {
                        Action::DialogClose => {
                            self.find_all_results_dialog_active = false;
                            // Keep the dialog instance for persistence
                        }
                        Action::GoToResult { row, column } => {
                            // Find the column index in the visible columns
                            let visible_columns = self.datatable.get_visible_columns()?;
                            if let Some(col_idx) = visible_columns.iter().position(|col| col == &column) {
                                self.datatable.selection.row = row;
                                self.datatable.selection.col = col_idx;
                                self.datatable.scroll_to_selection()?;
                                // Optionally close the dialog after going to result
                                // self.find_all_results_dialog_active = false;
                            }
                        }
                        _ => {}
                    }
                }
            return Ok(None);
        }
        // Route key events to FindDialog if active
        if self.find_dialog_active {
            if let Some(action) = self.find_dialog.handle_key_event(key) {
                match action {
                    Action::DialogClose => {
                        self.find_dialog_active = false;
                        self.current_search_pattern = None;
                        self.current_search_mode = None;
                        self.current_search_options = None;
                    }
                    Action::FindNext { pattern, options, search_mode } => {
                        self.find_dialog.searching = true;
                        self.find_dialog.search_progress = 0.0;
                        self.current_search_pattern = Some(pattern.clone());
                        self.current_search_mode = Some(search_mode.clone());
                        self.current_search_options = Some(options.clone());
                        let result = self.datatable.find_next(&pattern, &options, &search_mode);
                        self.find_dialog.searching = false;
                        match result {
                            Ok(Some((row, col))) => {
                                self.datatable.selection.row = row;
                                self.datatable.selection.col = col;
                                self.datatable.scroll_to_selection()?;
                                // Optionally close dialog or keep open for repeated search
                            }
                            Ok(None) => {
                                self.find_dialog.mode = crate::dialog::find_dialog::FindDialogMode::Error("No match found".to_string());
                            }
                            Err(e) => {
                                self.find_dialog.mode = crate::dialog::find_dialog::FindDialogMode::Error(format!("Search error: {e}"));
                            }
                        }
                    }
                    Action::FindCount { pattern, options, search_mode } => {
                        self.find_dialog.searching = true;
                        self.find_dialog.search_progress = 0.0;
                        let result = self.datatable.count_matches(&pattern, &options, &search_mode);
                        self.find_dialog.searching = false;
                        match result {
                            Ok(count) => {
                                self.find_dialog.mode = crate::dialog::find_dialog::FindDialogMode::Count(format!("Found {count} matches"));
                            }
                            Err(e) => {
                                self.find_dialog.mode = crate::dialog::find_dialog::FindDialogMode::Error(format!("Count error: {e}"));
                            }
                        }
                    }
                    Action::FindAll { pattern, options, search_mode } => {
                        self.find_all_results_dialog_active = true;
                        let results = self.datatable.find_all_matches(&pattern, &options, &search_mode, 20); // 20 chars context
                        match results {
                            Ok(results) => {
                                let instructions = "Up/Down: Navigate  Enter: Go to result  Esc: Close".to_string();
                                let mut find_all_results_dialog = FindAllResultsDialog::new(results, instructions, pattern.clone());
                                find_all_results_dialog.register_config_handler(self.config.clone())?;
                                self.find_all_results_dialog = Some(find_all_results_dialog);
                            }
                            Err(e) => {
                                self.find_dialog.mode = crate::dialog::find_dialog::FindDialogMode::Error(format!("Find All error: {e}"));
                            }
                        }
                    }
                    Action::GoToResult { row, column } => {
                        // Find the column index in the visible columns
                        let visible_columns = self.datatable.get_visible_columns()?;
                        if let Some(col_idx) = visible_columns.iter().position(|col| col == &column) {
                            self.datatable.selection.row = row;
                            self.datatable.selection.col = col_idx;
                            self.datatable.scroll_to_selection()?;
                        }
                    }
                    _ => {}
                }
            }
            return Ok(None);
        }
        // Route key events to DataFrameDetailsDialog if active (after other modal checks similar to find dialogs)
        if self.dataframe_details_dialog_active {
            // Prefer the exact max_rows returned by the last render to avoid layout mismatches
            let max_rows = if let Some(max_rows) = self.last_dataframe_details_dialog_max_rows {
                max_rows
            } else {
                // If not available, render into a temporary buffer to compute it accurately
                let area = self.last_dataframe_details_dialog_area.unwrap_or(ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                });
                let mut fake_buf = ratatui::buffer::Buffer::empty(area);
                let max_rows = self.dataframe_details_dialog.render(area, &mut fake_buf);
                self.last_dataframe_details_dialog_max_rows = Some(max_rows);
                max_rows
            };
            if let Some(action) = self.dataframe_details_dialog.handle_key_event(key, max_rows) {
                match action {
                    Action::DialogClose => {
                        self.dataframe_details_dialog_active = false;
                    }
                    Action::ColumnCastRequested { column, dtype } => {
                        use polars::prelude::DataType;
                        // Parse dtype string back to DataType using simple matching on Debug strings we produced
                        let target_dt: Option<DataType> = match dtype.as_str() {
                            "Int8" => Some(DataType::Int8),
                            "Int16" => Some(DataType::Int16),
                            "Int32" => Some(DataType::Int32),
                            "Int64" => Some(DataType::Int64),
                            "Int128" => Some(DataType::Int128),
                            "UInt8" => Some(DataType::UInt8),
                            "UInt16" => Some(DataType::UInt16),
                            "UInt32" => Some(DataType::UInt32),
                            "UInt64" => Some(DataType::UInt64),
                            "Float32" => Some(DataType::Float32),
                            "Float64" => Some(DataType::Float64),
                            "Boolean" => Some(DataType::Boolean),
                            "String" | "Utf8" => Some(DataType::String),
                            "Date" => Some(DataType::Date),
                            s if s.starts_with("Datetime(") => Some(DataType::Datetime(polars::prelude::TimeUnit::Milliseconds, None)),
                            "Time" => Some(DataType::Time),
                            s if s.starts_with("Duration(") => Some(DataType::Duration(polars::prelude::TimeUnit::Milliseconds)),
                            _ => None,
                        };
                        match target_dt {
                            Some(dt) => {
                                // Try casting the selected column into a new Series first
                                match self.datatable.get_dataframe() {
                                    Ok(df_arc) => {
                                        let df = df_arc.as_ref();
                                        match df.column(&column) {
                                            Ok(s) => match s.cast_with_options(&dt, polars::chunked_array::cast::CastOptions::Strict) {
                                                Ok(mut casted) => {
                                                    // Build a new DataFrame replacing only the target column
                                                    let mut cols: Vec<polars::prelude::Column> = Vec::with_capacity(df.width());
                                                    for c in df.get_columns() {
                                                        if c.name().as_str() == column {
                                                            casted.rename(polars::prelude::PlSmallStr::from_str(&column));
                                                            cols.push(casted.clone());
                                                        } else {
                                                            cols.push(c.clone());
                                                        }
                                                    }
                                                    match polars::prelude::DataFrame::new(cols) {
                                                        Ok(new_df) => {
                                                            // Overwrite current view with the new casted column
                                                            self.datatable.dataframe.set_current_df(new_df);
                                                            // Close overlay and refresh dialog view
                                                            self.dataframe_details_dialog.close_cast_overlay();
                                                            if let Ok(df_arc2) = self.datatable.get_dataframe() {
                                                                self.dataframe_details_dialog.set_dataframe(df_arc2);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            self.dataframe_details_dialog.set_cast_error(format!("{e}"));
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    self.dataframe_details_dialog.set_cast_error(format!("{e}"));
                                                }
                                            },
                                            Err(e) => {
                                                self.dataframe_details_dialog.set_cast_error(format!("{e}"));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.dataframe_details_dialog.set_cast_error(format!("{e}"));
                                    }
                                }
                            }
                            None => {
                                // Invalid/unsupported dtype mapping
                                self.dataframe_details_dialog.set_cast_error(format!("Unsupported dtype: {dtype}"));
                            }
                        }
                    }
                    Action::AddFilterCondition(filter) => {
                        // Open filter dialog pre-populated with the selected column and value
                        // First, merge with existing expression: append under root AND
                        let mut expr = self.datatable.dataframe.filter.clone().unwrap_or_else(|| FilterExpr::And(vec![]));
                        match &mut expr {
                            FilterExpr::And(children) | FilterExpr::Or(children) => {
                                children.push(FilterExpr::Condition(filter.clone()));
                            }
                            FilterExpr::Condition(_) => {
                                expr = FilterExpr::And(vec![expr, FilterExpr::Condition(filter.clone())]);
                            }
                        }
                        // Update container and dialog state
                        self.set_filter_expression(expr);
                        // Initialize dialog columns and index to the filter's column
                        let df = self.datatable.get_dataframe()?;
                        let df_ref = df.as_ref();
                        let columns: Vec<String> = df_ref
                            .get_column_names_owned()
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect();
                        let col_index = columns.iter().position(|c| c == &filter.column).unwrap_or(0);
                        self.filter_dialog.set_columns(columns, col_index);
                        self.filter_dialog_active = true;
                        // Keep details dialog open or close? Close to focus on filtering
                        self.dataframe_details_dialog_active = false;
                        return Ok(None);
                    }
                    _ => {}
                }
            }
            return Ok(None);
        }
        // DataExportDialog moved to DataTabManagerDialog
        // Route key events to JmesPathDialog if active
        if self.jmes_dialog_active {
            if let Some(action) = self.jmes_dialog.handle_key_event(key) {
                match action {
                    Action::DialogClose => {
                        self.jmes_dialog_active = false;
                    }
                    Action::JmesTransformDataset((query, scope)) => {
                        match self.apply_jmes_transform(&query, scope) {
                            Ok(()) => {
                                self.jmes_dialog_active = false;
                                return Ok(Some(Action::SaveWorkspaceState));
                            }
                            Err(e) => {
                                self.jmes_dialog.set_error(format!("{e}"));
                            }
                        }
                    }
                    Action::JmesTransformAddColumns(key_value_pairs, scope) => {
                        // Persist the latest add_columns on the dialog so state capture can save them
                        self.jmes_dialog.add_columns = key_value_pairs.clone();
                        match self.apply_jmes_add_columns(key_value_pairs, scope) {
                            Ok(()) => {
                                self.jmes_dialog_active = false;
                                return Ok(Some(Action::SaveWorkspaceState));
                            }
                            Err(e) => {
                                self.jmes_dialog.set_error(format!("{e}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
            return Ok(None);
        }
        // Route key events to ColumnOperationsDialog if active
        if self.column_operations_dialog_active {
            if let Some(action) = self.column_operations_dialog.handle_key_event(key)? {
                match action {
                    Action::DialogClose => {
                        self.column_operations_dialog_active = false;
                    }
                    Action::ColumnOperationRequested(which) => {
                        // Open the options dialog for the selected operation
                        let op = match which.as_str() {
                            "GenerateEmbeddings" => ColumnOperationKind::GenerateEmbeddings,
                            "Pca" => ColumnOperationKind::Pca,
                            "Cluster" => ColumnOperationKind::Cluster,
                            "SortByPromptSimilarity" => ColumnOperationKind::SortByPromptSimilarity,
                            _ => ColumnOperationKind::GenerateEmbeddings,
                        };
                        // Special-case: open Prompt Similarity dialog directly (uses embedding column mapping)
                        if matches!(op, ColumnOperationKind::SortByPromptSimilarity) {
                            let mapping = self.embedding_column_config_mapping.clone();
                            let initial = {
                                let visible_columns = self.datatable.get_visible_columns().unwrap_or_default();
                                let idx = self.datatable.selection.col.min(visible_columns.len().saturating_sub(1));
                                let name = visible_columns.get(idx).cloned().unwrap_or_default();
                                if mapping.contains_key(&name) { Some(name) } else { None }
                            };
                            let mut dialog = crate::dialog::EmbeddingsPromptDialog::new_with_mapping(mapping, initial);
                            dialog.register_config_handler(self.config.clone())?;
                            self.embeddings_prompt_dialog = Some(dialog);
                            self.embeddings_prompt_dialog_active = true;
                            self.column_operations_dialog_active = false;
                            return Ok(None);
                        }
                        // Seed with filtered DF columns (only compatible types) and current selection
                        let df = self.datatable.get_dataframe()?;
                        let df_ref = df.as_ref();
                        let all_names: Vec<String> = df_ref
                            .get_column_names_owned()
                            .into_iter()
                            .map(|s| s.to_string())
                            .collect();
                        use polars::prelude::DataType;
                        let is_numeric = |dt: &DataType| matches!(
                            dt,
                            DataType::Int8
                                | DataType::Int16
                                | DataType::Int32
                                | DataType::Int64
                                | DataType::Int128
                                | DataType::UInt8
                                | DataType::UInt16
                                | DataType::UInt32
                                | DataType::UInt64
                                | DataType::Float32
                                | DataType::Float64
                        );
                        let filtered: Vec<String> = match op {
                            ColumnOperationKind::GenerateEmbeddings => all_names
                                .into_iter()
                                .filter(|name| df_ref.column(name).ok().map(|s| s.dtype() == &DataType::String).unwrap_or(false))
                                .collect(),
                            ColumnOperationKind::Pca | ColumnOperationKind::Cluster => all_names
                                .into_iter()
                                .filter(|name| {
                                    if let Ok(s) = df_ref.column(name) {
                                        match s.dtype() {
                                            DataType::List(inner) => is_numeric(inner.as_ref()),
                                            _ => false,
                                        }
                                    } else { false }
                                })
                                .collect(),
                            ColumnOperationKind::SortByPromptSimilarity => Vec::new(),
                        };
                        // Compute initial selected index based on current table selection
                        let current_col_name = {
                            let visible_columns = self.datatable.get_visible_columns().unwrap_or_default();
                            let idx = self.datatable.selection.col.min(visible_columns.len().saturating_sub(1));
                            visible_columns.get(idx).cloned().unwrap_or_default()
                        };
                        let selected_idx = filtered.iter().position(|n| n == &current_col_name).unwrap_or(0);
                        let mut dialog = ColumnOperationOptionsDialog::new_with_columns(op, filtered, selected_idx);
                        dialog.register_config_handler(self.config.clone())?;
                        if dialog.columns.is_empty() {
                            dialog.mode = ColumnOperationOptionsMode::Error("No compatible columns found for this operation".to_string());
                        }
                        self.column_operation_options_dialog = Some(dialog);
                        self.column_operation_options_dialog_active = true;
                        self.column_operations_dialog_active = false;
                        return Ok(None);
                    }
                    _ => {}
                }
            }
            return Ok(None);
        }
        // Route key events to EmbeddingsPromptDialog if active
        if self.embeddings_prompt_dialog_active {
            if let Some(dialog) = &mut self.embeddings_prompt_dialog {
                if let Some(action) = dialog.handle_key_event(key)? {
                    match action {
                        Action::DialogClose => {
                            self.embeddings_prompt_dialog_active = false;
                            self.embeddings_prompt_dialog = None;
                        }
                        Action::EmbeddingsPromptDialogApplied { source_column, new_column_name, prompt_embedding } => {
                            if let Err(e) = self.execute_prompt_similarity(&source_column, &new_column_name, &prompt_embedding) {
                                tracing::error!("Prompt similarity apply failed: {}", e);
                            }
                            self.embeddings_prompt_dialog_active = false;
                            self.embeddings_prompt_dialog = None;
                            return Ok(None);
                        }
                        Action::EmbeddingsPromptDialogRequestGenerateEmbeddings { prompt_text, new_similarity_column } => {
                            // Record pending prompt flow to restore later
                            self.pending_prompt_flow = Some(PendingPromptFlow {
                                prompt_text,
                                similarity_new_column: new_similarity_column,
                                embeddings_column_name: None,
                            });
                            // Open GenerateEmbeddings options dialog directly
                            let op = ColumnOperationKind::GenerateEmbeddings;
                            let df = self.datatable.get_dataframe()?;
                            let df_ref = df.as_ref();
                            let all_names: Vec<String> = df_ref
                                .get_column_names_owned()
                                .into_iter()
                                .map(|s| s.to_string())
                                .collect();
                            use polars::prelude::DataType;
                            let filtered: Vec<String> = all_names
                                .into_iter()
                                .filter(|name| df_ref.column(name).ok().map(|s| s.dtype() == &DataType::String).unwrap_or(false))
                                .collect();
                            // Compute initial selected index based on current table selection
                            let current_col_name = {
                                let visible_columns = self.datatable.get_visible_columns().unwrap_or_default();
                                let idx = self.datatable.selection.col.min(visible_columns.len().saturating_sub(1));
                                visible_columns.get(idx).cloned().unwrap_or_default()
                            };
                            let selected_idx = filtered.iter().position(|n| n == &current_col_name).unwrap_or(0);
                            let mut dialog = ColumnOperationOptionsDialog::new_with_columns(op, filtered, selected_idx);
                            dialog.register_config_handler(self.config.clone())?;
                            if dialog.columns.is_empty() {
                                dialog.mode = ColumnOperationOptionsMode::Error("No compatible columns found for this operation".to_string());
                            }
                            self.column_operation_options_dialog = Some(dialog);
                            self.column_operation_options_dialog_active = true;
                            self.embeddings_prompt_dialog_active = false;
                            self.embeddings_prompt_dialog = None;
                            return Ok(None);
                        }
                        _ => {}
                    }
                }
            }
            return Ok(None);
        }
        // Route key events to LlmClientCreateDialog if active
        if self.llm_client_create_dialog_active {
            if let Some(dialog) = &mut self.llm_client_create_dialog {
                if let Some(action) = dialog.handle_key_event(key)? {
                    match action {
                        Action::DialogClose => {
                            self.llm_client_create_dialog_active = false;
                            self.llm_client_create_dialog = None;
                            // Do not clear pending; user cancelled
                        }
                        Action::LlmClientCreateDialogApplied(selection) => {
                            // Close dialog
                            self.llm_client_create_dialog_active = false;
                            self.llm_client_create_dialog = None;
                            // If we have a pending embeddings request, schedule it with the selected provider
                            if let Some(mut pending) = self.pending_embeddings_after_llm_selection.take() {
                                pending.selected_provider = Some(selection.provider.clone());
                                // Queue for execution on next Render tick with busy overlay
                                self.busy_active = true;
                                self.busy_message = format!("Generating embeddings with {}...", selection.provider.display_name());
                                self.busy_progress = 0.0;
                                self.queued_embeddings = Some(pending);
                            }
                        }
                        _ => {}
                    }
                }
            }
            return Ok(None);
        }
        // Route key events to ColumnOperationOptionsDialog if active
        if self.column_operation_options_dialog_active {
            if let Some(dialog) = &mut self.column_operation_options_dialog
                && let Some(action) = dialog.handle_key_event(key)? {
                    match action {
                        Action::DialogClose => {
                            // Go back to operation selection instead of exiting
                            self.column_operation_options_dialog_active = false;
                            self.column_operations_dialog_active = true;
                        }
                        Action::ColumnOperationOptionsApplied(cfg) => {
                            debug!("ColumnOperationOptionsApplied: {:?}", cfg);
                            // Validate source column dtype per operation requirements
                            let df_arc = self.datatable.get_dataframe()?;
                            let df_ref = df_arc.as_ref();
                            let dtype_opt = df_ref.column(&cfg.source_column).ok().map(|s| s.dtype().clone());
                            use polars::prelude::DataType;
                            let mut is_ok = false;
                            let mut err_msg = String::new();
                            if let Some(dtype) = dtype_opt {
                                match cfg.operation {
                                    ColumnOperationKind::GenerateEmbeddings => {
                                        is_ok = matches!(dtype, DataType::String);
                                        if !is_ok {
                                            err_msg = format!("Source column '{}' must be String", cfg.source_column);
                                            error!("GenerateEmbeddings error: {}", err_msg);
                                        }
                                    }
                                    ColumnOperationKind::Pca | ColumnOperationKind::Cluster | ColumnOperationKind::SortByPromptSimilarity => {
                                        // Must be a vector of numbers: List(Numeric)
                                        let is_vec_num = matches!(
                                            dtype,
                                            DataType::List(inner)
                                                if matches!(*inner,
                                                    DataType::Int8
                                                        | DataType::Int16
                                                        | DataType::Int32
                                                        | DataType::Int64
                                                        | DataType::Int128
                                                        | DataType::UInt8
                                                        | DataType::UInt16
                                                        | DataType::UInt32
                                                        | DataType::UInt64
                                                        | DataType::Float32
                                                        | DataType::Float64
                                                )
                                        );
                                        is_ok = is_vec_num;
                                        if !is_ok { err_msg = format!("Source column '{}' must be a vector of numbers", cfg.source_column); }
                                    }
                                }
                            } else {
                                err_msg = format!("Source column '{}' not found", cfg.source_column);
                            }

                            if !is_ok {
                                dialog.mode = ColumnOperationOptionsMode::Error(err_msg.clone());
                                error!("ColumnOperationOptionsApplied error: {}", &err_msg);
                                return Ok(None);
                            }

                            // Apply operation
                            match cfg.operation {
                                ColumnOperationKind::GenerateEmbeddings => {
                                    debug!("ColumnOperationOptionsApplied: GenerateEmbeddings");
                                    // Extract options
                                    let (model_name, num_dims) = match &cfg.options {
                                        OperationOptions::GenerateEmbeddings {
                                            model_name,
                                            num_dimensions 
                                        } => (model_name.clone(), *num_dimensions),
                                        _ => ("text-embedding-3-small".to_string(), 0),
                                    };
                                    // Queue embeddings to show overlay first, then execute on Render
                                    self.busy_active = true;
                                    let provider = if let Some(dialog_ref) = &self.column_operation_options_dialog { dialog_ref.selected_provider.clone() } else { crate::dialog::LlmProvider::OpenAI };
                                    self.busy_message = format!("Generating embeddings with {}...", provider.display_name());
                                    self.busy_progress = 0.0;
                                    // Snapshot provider config (non-secret fields) for reproducibility
                                    let snapshot = EmbeddingColumnConfig {
                                        provider: provider.clone(),
                                        model_name: model_name.clone(),
                                        num_dimensions: num_dims
                                    };
                                    self.embedding_column_config_mapping.insert(cfg.new_column_name.clone(), snapshot);
                                    self.queued_embeddings = Some(QueuedEmbeddings {
                                        source_column: cfg.source_column.clone(),
                                        new_column_name: cfg.new_column_name.clone(),
                                        model_name,
                                        num_dimensions: num_dims,
                                        selected_provider: Some(provider),
                                        hide_new_column: cfg.hide_new_column,
                                    });
                                    // If prompt flow is pending, remember new embeddings column name
                                    if let Some(ref mut pending) = self.pending_prompt_flow { pending.embeddings_column_name = Some(cfg.new_column_name.clone()); }
                                    self.column_operation_options_dialog_active = false;
                                    // Do not trigger Render immediately; allow one frame to draw the overlay first
                                    return Ok(None);
                                }
                                ColumnOperationKind::Pca => {
                                    // Extract k
                                    let k = match &cfg.options {
                                        OperationOptions::Pca { target_embedding_size } => *target_embedding_size,
                                        _ => 2,
                                    };
                                    self.busy_active = true;
                                    self.busy_message = "Running PCA...".to_string();
                                    self.busy_progress = 0.0;
                                    self.queued_pca = Some(QueuedPca {
                                        source_column: cfg.source_column.clone(),
                                        new_column_name: cfg.new_column_name.clone(),
                                        k,
                                    });
                                    self.column_operation_options_dialog_active = false;
                                    return Ok(None);
                                }
                                ColumnOperationKind::Cluster => {
                                    // Prepare dataset (nrows x dim) from List(Numeric) source
                                    let (algo, kmeans_opts, dbscan_opts) = match &cfg.options {
                                        OperationOptions::Cluster { algorithm, kmeans, dbscan } => (algorithm.clone(), kmeans.clone(), dbscan.clone()),
                                        _ => (crate::dialog::ClusterAlgorithm::Kmeans, None, None),
                                    };
                                    self.busy_active = true;
                                    self.busy_message = "Clustering...".to_string();
                                    self.busy_progress = 0.0;
                                    self.queued_cluster = Some(QueuedCluster {
                                        source_column: cfg.source_column.clone(),
                                        new_column_name: cfg.new_column_name.clone(),
                                        algorithm: algo,
                                        kmeans: kmeans_opts,
                                        dbscan: dbscan_opts,
                                    });
                                    self.column_operation_options_dialog_active = false;
                                    return Ok(None);
                                }
                                ColumnOperationKind::SortByPromptSimilarity => {
                                    // Not applied via options dialog; handled by dedicated prompt dialog
                                    self.column_operation_options_dialog_active = false;
                                    return Ok(None);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            return Ok(None);
        }
        if self.sort_dialog_active {
            // Ensure last_sort_dialog_max_rows is set
            let max_rows = if let Some(max_rows) = self.last_sort_dialog_max_rows {
                max_rows
            } else {
                // Use last_sort_dialog_area or a default area
                let area = self.last_sort_dialog_area.unwrap_or(ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                });
                let mut fake_buf = ratatui::buffer::Buffer::empty(area);
                let max_rows = self.sort_dialog.render(area, &mut fake_buf);
                self.last_sort_dialog_max_rows = Some(max_rows);
                max_rows
            };
            if let Some(action) = self.sort_dialog.handle_key_event(key, max_rows) {
                match action {
                    crate::action::Action::SortDialogApplied(sort_columns) => {
                        // Apply sort to DataTable/DataFrame here
                        if let Err(e) = self.datatable.dataframe.sort_by_columns(&sort_columns) {
                            error!("Sort error: {e}");
                        }
                        self.sort_dialog_active = false;
                        return Ok(Some(Action::SaveWorkspaceState));
                    }
                    _ => {
                        self.sort_dialog_active = false;
                    }
                }
            }
            return Ok(None);
        }
        if self.filter_dialog_active {
            // Ensure last_filter_dialog_max_rows is set
            let max_rows = if let Some(max_rows) = self.last_filter_dialog_max_rows {
                max_rows
            } else {
                // Use last_filter_dialog_area or a default area
                let area = self.last_filter_dialog_area.unwrap_or(ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                });
                let mut fake_buf = ratatui::buffer::Buffer::empty(area);
                let max_rows = self.filter_dialog.render(area, &mut fake_buf);
                self.last_filter_dialog_max_rows = Some(max_rows);
                max_rows
            };

            if let Some(action) = self.filter_dialog.handle_key_event(key, max_rows) {
                if let Action::FilterDialogApplied(filter) = action {
                    info!("FilterDialogApplied: {:?}", filter);
                    // Persist the filter expression on the dataframe for workspace capture
                    self.datatable.dataframe.filter = Some(filter.clone());
                    let base_df = self.datatable.dataframe.collect_base_df()?;
                    let mask = filter.create_mask(&base_df)?;
                    let filtered_df = base_df.filter(&mask)?;
                    self.datatable.dataframe.current_df = Some(Arc::new(filtered_df));
                    // Signal to persist workspace state
                    return Ok(Some(Action::SaveWorkspaceState));
                }
                self.filter_dialog_active = false;
            }
            return Ok(None);
        }
        if self.sql_dialog_active {
            // Use last_sql_dialog_area or a default area
            let _area = self.last_sql_dialog_area.unwrap_or(ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            });
            // No max_rows needed for SQL dialog
            if let Some(action) = self.sql_dialog.handle_key_event(key) {
                match action {
                    Action::SqlDialogApplied(query_or_command) => {
                        // Check if this is a new dataset creation command
                        if query_or_command.starts_with("NEW_DATASET:") {
                            let parts: Vec<&str> = query_or_command.splitn(3, ':').collect();
                            if parts.len() == 3 {
                                let dataset_name = parts[1].to_string();
                                let query = parts[2].to_string();
                                
                                // Execute the SQL query to create a new dataset
                                let mut ctx = new_sql_context();
                                register_all(&mut ctx)?;
                                // Register all available DataFrames
                                for data_context in self.available_datasets.values() {
                                    let name = &data_context.dataset.alias.clone().unwrap_or(data_context.dataset.name.clone());
                                    ctx.register(name, (*data_context.dataframe).clone().lazy());
                                }
                                
                                match ctx.execute(&query) {
                                    Ok(lf) => match lf.collect() {
                                        Ok(new_df) => {
                                            self.sql_dialog_active = false;
                                            // Return the action to be handled by parent component
                                            return Ok(Some(Action::SqlDialogAppliedNewDataset { 
                                                dataset_name, 
                                                dataframe: Arc::new(new_df) 
                                            }));
                                        }
                                        Err(e) => {
                                            self.sql_dialog.set_error(format!("Collect error: {e}"));
                                        }
                                    },
                                    Err(e) => {
                                        self.sql_dialog.set_error(format!("SQL error: {e}"));
                                    }
                                }
                            }
                        } else {
                            // Regular SQL query - update current DataFrame
                            let mut ctx = new_sql_context();
                            if let Err(error) = register_all(&mut ctx){
                                error!("SQL error: {error}");
                                self.sql_dialog.set_error(format!("SQL error: {error}"));
                                return Ok(None);
                            };
                            // Register all available DataFrames
                            for data_context in self.available_datasets.values() {
                                let name = &data_context.dataset.alias.clone().unwrap_or(data_context.dataset.name.clone());
                                ctx.register(name, (*data_context.dataframe).clone().lazy());
                            }
                            
                            match ctx.execute(&query_or_command) {
                                Ok(lf) => match lf.collect() {
                                    Ok(new_df) => {
                                        // record last sql
                                        self.datatable.dataframe.last_sql_query = Some(query_or_command.clone());
                                        self.datatable.set_current_df(new_df);
                                        self.sql_dialog_active = false;
                                        // Signal to persist workspace state
                                        return Ok(Some(Action::SaveWorkspaceState));
                                    }
                                    Err(e) => {
                                        self.sql_dialog.set_error(format!("Collect error: {e}"));
                                    }
                                },
                                Err(e) => {
                                    self.sql_dialog.set_error(format!("SQL error: {e}"));
                                }
                            }
                        }
                    }
                    Action::SqlDialogRestore => {
                        // Restore the original DataFrame
                        self.datatable.reset_current_df();
                        self.sql_dialog_active = false;
                    }
                    Action::DialogClose => {
                        self.sql_dialog_active = false;
                    }
                    _ => {}
                }
            }
            return Ok(None);
        }
        if self.column_width_dialog_active {
            // Ensure last_column_width_dialog_max_rows is set
            let max_rows = if let Some(max_rows) = self.last_column_width_dialog_max_rows {
                max_rows
            } else {
                // Use last_column_width_dialog_area or a default area
                let area = self.last_column_width_dialog_area.unwrap_or(ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                });
                let mut fake_buf = ratatui::buffer::Buffer::empty(area);
                let max_rows = self.column_width_dialog.render(area, &mut fake_buf);
                self.last_column_width_dialog_max_rows = Some(max_rows);
                max_rows
            };
            if let Some(action) = self.column_width_dialog.handle_key_event(key, max_rows) {
                match action {
                    Action::ColumnWidthDialogApplied(config) => {
                        // Apply column width configuration to the DataFrame
                        self.datatable.set_column_width_config(config);
                        self.column_width_dialog_active = false;
                    }
                    Action::ColumnWidthDialogReordered(column_order) => {
                        // Reorder columns in the DataFrame
                        if let Err(e) = self.datatable.dataframe.reorder_columns(&column_order) {
                            error!("Failed to reorder columns: {}", e);
                        } else {
                            // Update the dialog's column list to match the new order
                            self.column_width_dialog.set_columns(column_order);
                        }
                    }
                    _ => {
                        self.column_width_dialog_active = false;
                    }
                }
            }
            return Ok(None);
        }
        
        if let Some(action) = self.config.action_for_key(crate::config::Mode::DataTableContainer, key) {
            match action {
                Action::OpenSortDialog => {
                    self.sort_dialog_active = true;
                    return Ok(None);
                }
                Action::OpenEmbeddingsPromptDialog => {
                    let mapping = self.embedding_column_config_mapping.clone();
                    let visible_columns = self.datatable.get_visible_columns().unwrap_or_default();
                    let idx = self.datatable.selection.col.min(visible_columns.len().saturating_sub(1));
                    let name = visible_columns.get(idx).cloned().unwrap_or_default();
                    let initial = if mapping.contains_key(&name) { Some(name) } else { None };
                    let mut dialog = crate::dialog::EmbeddingsPromptDialog::new_with_mapping(mapping, initial);
                    dialog.register_config_handler(self.config.clone())?;
                    self.embeddings_prompt_dialog = Some(dialog);
                    self.embeddings_prompt_dialog_active = true;
                    return Ok(None);
                }
                Action::QuickSortCurrentColumn => {
                    let visible_columns = self.datatable.get_visible_columns()?;
                    let col_idx = self.datatable.selection.col.min(visible_columns.len().saturating_sub(1));
                    if let Some(col_name) = visible_columns.get(col_idx) {
                        if let Some(existing_idx) = self
                            .sort_dialog
                            .sort_columns
                            .iter()
                            .position(|sc| sc.name == *col_name)
                        {
                            self.sort_dialog.active_index = existing_idx;
                        } else {
                            self.sort_dialog.sort_columns.push(crate::dialog::sort_dialog::SortColumn {
                                name: col_name.clone(),
                                ascending: true,
                            });
                            self.sort_dialog.active_index = self.sort_dialog.sort_columns.len().saturating_sub(1);
                        }
                        self.sort_dialog.mode = SortDialogMode::List;
                        self.sort_dialog_active = true;
                    }
                    return Ok(None);
                }
                Action::OpenFilterDialog => {
                    let df_arc = self.datatable.get_dataframe()?;
                    let df = df_arc.as_ref();
                    let columns: Vec<String> = df
                        .get_column_names_owned()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    let col_index = self.datatable.selection.col.min(columns.len().saturating_sub(1));
                    self.filter_dialog.set_columns(columns, col_index);
                    self.filter_dialog_active = true;
                    return Ok(None);
                }
                Action::QuickFilterEqualsCurrentValue => {
                    let col_index = self.datatable.selection.col;
                    let df = self.datatable.get_dataframe()?;
                    let df = df.as_ref();
                    let columns: Vec<String> = df
                        .get_column_names_owned()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    self.filter_dialog.set_columns(columns, col_index);
                    let selected_value = self.datatable.selected_cell_value()?;
                    self.filter_dialog.add_value = selected_value.clone();
                    self.filter_dialog.add_condition = Some(FilterCondition::Equals { value: selected_value, case_sensitive: false });
                    let root_children = self.filter_dialog.get_root_expr().child_count();
                    self.filter_dialog.add_insertion_path = Some(vec![root_children]);
                    self.filter_dialog.mode = FilterDialogMode::Add;
                    self.filter_dialog_active = true;
                    return Ok(None);
                }
                Action::MoveSelectedColumnLeft | Action::MoveSelectedColumnRight => {
                    let df_arc = self.datatable.get_dataframe()?;
                    let df = df_arc.as_ref();
                    let mut columns: Vec<String> = df
                        .get_column_names_owned()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    let col_idx = self.datatable.selection.col.min(columns.len().saturating_sub(1));
                    if matches!(action, Action::MoveSelectedColumnLeft) && col_idx > 0 {
                        columns.swap(col_idx, col_idx - 1);
                        if let Err(e) = self.datatable.dataframe.reorder_columns(&columns) {
                            error!("Failed to move column left: {}", e);
                        } else {
                            self.datatable.selection.col = col_idx - 1;
                            let _ = self.datatable.scroll_to_selection();
                        }
                    } else if matches!(action, Action::MoveSelectedColumnRight) && col_idx + 1 < columns.len() {
                        columns.swap(col_idx, col_idx + 1);
                        if let Err(e) = self.datatable.dataframe.reorder_columns(&columns) {
                            error!("Failed to move column right: {}", e);
                        } else {
                            self.datatable.selection.col = col_idx + 1;
                            let _ = self.datatable.scroll_to_selection();
                        }
                    }
                    return Ok(None);
                }
                Action::OpenDataExportDialog => {
                    // Bubble up to DataTabManagerDialog to handle multi-dataset export
                    return Ok(Some(Action::OpenDataExportDialog));
                }
                Action::OpenSqlDialog => { self.sql_dialog_active = true; return Ok(None); }
                Action::OpenJmesDialog => { self.jmes_dialog_active = true; return Ok(None); }
                Action::OpenColumnOperationsDialog => { self.column_operations_dialog_active = true; return Ok(None); }
                Action::OpenFindDialog => { self.find_dialog_active = true; return Ok(None); }
                Action::OpenDataframeDetailsDialog => {
                    let df_arc = self.datatable.get_dataframe()?;
                    let df_ref = df_arc.as_ref();
                    let columns: Vec<String> = df_ref
                        .get_column_names_owned()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect();
                    let col_index = self.datatable.selection.col.min(columns.len().saturating_sub(1));
                    self.dataframe_details_dialog.set_columns(columns, col_index);
                    self.dataframe_details_dialog.set_dataframe(df_arc.clone());
                    // Provide embeddings mapping for Embeddings tab
                    self.dataframe_details_dialog.embedding_column_config_mapping = self.embedding_column_config_mapping.clone();
                    self.dataframe_details_dialog_active = true;
                    return Ok(None);
                }
                Action::OpenColumnWidthDialog => {
                    let df = self.datatable.get_dataframe()?;
                    let df = df.as_ref();
                    let columns: Vec<String> = df
                        .get_column_names_owned()
                        .into_iter()
                        .map(|s| s.to_string()).collect();
                    self.column_width_dialog.set_columns(columns);
                    let config = self.datatable.get_column_width_config();
                    self.column_width_dialog.set_config(config);
                    // Pass current calculated widths so columns can be locked when auto_expand is disabled
                    if let Ok(widths) = self.datatable.get_all_column_widths() {
                        self.column_width_dialog.set_current_calculated_widths(widths);
                    }
                    self.column_width_dialog_active = true;
                    return Ok(None);
                }
                Action::CopySelectedCell => {
                    let cell_value = self.selected_cell_json_value()?;
                    let cell_value = match cell_value { Value::String(s) => s, v => v.to_string() };
                    if let Err(e) = Clipboard::new().and_then(|mut clipboard| clipboard.set_text(cell_value.clone())) {
                        error!("Failed to copy to clipboard: {}", e);
                    }
                    return Ok(None);
                }
                Action::ToggleInstructions => { self.toggle_instructions(); return Ok(None); }
                _ => {
                    debug!("DataTableContainer unhandled action: {:?} for key: {:?}", action, key);
                }
            }
        }

        self.datatable.handle_key_event(key)
    }
    /// Handle mouse events for the component.
    ///
    /// This method is a no-op for DataTableContainer, as mouse events are not handled.
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        let _ = mouse;
        Ok(None)
    }
    /// Update the component state in response to an action.
    ///
    /// This method is a no-op for DataTableContainer, as it does not use external actions for updates.
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        debug!("update: {:?}", action);
        match action {
            Action::Tick => {
                self.find_dialog.tick_search_progress();
                // Advance busy progress animation if active
                if self.busy_active {
                    self.busy_progress += 0.02;
                    if self.busy_progress >= 1.0 { self.busy_progress = 0.0; }
                }
                // No background job polling
            }
            Action::Render => {
                if let Some(q) = self.queued_embeddings.take() {
                    // Initialize progressive embeddings job
                    let provider = q.selected_provider.clone().unwrap_or(crate::dialog::LlmProvider::OpenAI);
                    // Prepare source series as strings and build unique lists
                    let df_arc = self.datatable.get_dataframe()?;
                    let df_ref = df_arc.as_ref();
                    use polars::prelude::DataType;
                    let mut s = df_ref.column(&q.source_column)
                        .map_err(|e| color_eyre::eyre::eyre!("{}", e))?
                        .clone();
                    if s.dtype() != &DataType::String { s = s.cast(&DataType::String).map_err(|e| color_eyre::eyre::eyre!("{}", e))?; }
                    let len = s.len();
                    let mut row_texts: Vec<Option<String>> = Vec::with_capacity(len);
                    let mut unique_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    let mut uniques: Vec<String> = Vec::new();
                    for i in 0..len {
                        let av_res = s.get(i);
                        if let Ok(av) = av_res {
                            if av.is_null() { row_texts.push(None); continue; }
                            let text_val = av.str_value().to_string();
                            row_texts.push(Some(text_val.clone()));
                            if !unique_index.contains_key(&text_val) {
                                let idx = uniques.len();
                                unique_index.insert(text_val.clone(), idx);
                                uniques.push(text_val);
                            }
                        } else { row_texts.push(None); }
                    }
                    let total = uniques.len();
                    let job = EmbeddingsJob {
                        source_column: q.source_column,
                        new_column_name: q.new_column_name,
                        model_name: q.model_name,
                        num_dimensions: q.num_dimensions,
                        provider,
                        hide_new_column: q.hide_new_column,
                        row_texts,
                        uniques,
                        unique_index,
                        unique_embeddings: Vec::new(),
                        next_start: 0,
                        batch_size: 256,
                        total_uniques: total,
                    };
                    self.in_progress_embeddings = Some(job);
                    // Reset progress
                    self.busy_progress = 0.0;
                }
                // Process next batch if an embeddings job is active
                if let Some(done) = self.process_next_embeddings_batch()? {
                    if done {
                        // Finalize embeddings column
                        self.finalize_embeddings_job()?;
                        self.busy_active = false;
                        self.busy_message.clear();
                        self.busy_progress = 0.0;
                        // If we initiated from prompt flow, reopen the prompt dialog now
                        if let Some(pending) = self.pending_prompt_flow.take() {
                            let mapping = self.embedding_column_config_mapping.clone();
                            let initial = pending.embeddings_column_name.clone();
                            let mut dialog = crate::dialog::EmbeddingsPromptDialog::new_with_mapping(mapping, initial);
                            dialog.register_config_handler(self.config.clone())?;
                            // Restore prompt text and similarity new column name
                            dialog.new_column_input.insert_str(&pending.similarity_new_column);
                            dialog.new_column_name = dialog.new_column_input.lines().join("\n");
                            dialog.prompt_input.insert_str(&pending.prompt_text);
                            self.embeddings_prompt_dialog = Some(dialog);
                            self.embeddings_prompt_dialog_active = true;
                            return Ok(None);
                        }
                        return Ok(Some(Action::SaveWorkspaceState));
                    } else {
                        // Continue on next render
                        return Ok(None);
                    }
                }
                if let Some(p) = self.queued_pca.take() {
                    let res = self.execute_pca(&p.source_column, &p.new_column_name, p.k);
                    self.busy_active = false;
                    self.busy_message.clear();
                    self.busy_progress = 0.0;
                    match res {
                        Ok(_) => return Ok(Some(Action::SaveWorkspaceState)),
                        Err(e) => {
                            if let Some(dialog) = &mut self.column_operation_options_dialog {
                                dialog.mode = ColumnOperationOptionsMode::Error(format!("{e}"));
                            }
                            return Ok(None);
                        }
                    }
                }
                if let Some(cq) = self.queued_cluster.take() {
                    let res = self.execute_cluster(
                        &cq.source_column,
                        &cq.new_column_name,
                        cq.algorithm,
                        cq.kmeans,
                        cq.dbscan,
                    );
                    self.busy_active = false;
                    self.busy_message.clear();
                    self.busy_progress = 0.0;
                    match res {
                        Ok(_) => return Ok(Some(Action::SaveWorkspaceState)),
                        Err(e) => {
                            if let Some(dialog) = &mut self.column_operation_options_dialog {
                                dialog.mode = ColumnOperationOptionsMode::Error(format!("{e}"));
                            }
                            return Ok(None);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }
    
    /// Draw the DataTableContainer and its child widgets to the frame.
    ///
    /// This method lays out the viewing box, data table, instruction area, and any active dialogs as popups.
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Layout: [Viewing Box] [DataTable] [Instruction Area (optional)]
        let mut viewing_box_height: u16 = 3;
        let min_table_height = 5;
        
        // Calculate instruction area height based on wrapped lines (only if showing instructions)
        let instructions_height = if self.show_instructions {
            let instructions_text = self.get_instructions();
            let instructions_wrap_width = area.width.saturating_sub(4).max(1) as usize; // 2 for borders each side
            let wrapped_lines = wrap(&instructions_text, instructions_wrap_width);
            wrapped_lines.len() as u16 + 2 // +2 for border/title
        } else {
            0
        };

        // If auto-expand is enabled, compute dynamic height for viewing box based on wrapped content
        if self.auto_expand_value_display {
            // Inner width for wrapped content inside a bordered block
            let inner_width = area.width.saturating_sub(2).max(1) as usize;
            // Get raw selected cell text (unhighlighted) to measure wrapped lines
            let cell_value = self.selected_cell_json_value()?;
            let cell_text = match cell_value {
                Value::String(s) => s,
                v => v.to_string(),
            };
            let wrapped = wrap(&cell_text, inner_width);
            let required_height = (wrapped.len() as u16).saturating_add(2); // +2 for borders
            let max_view_height = area
                .height
                .saturating_sub(min_table_height)
                .saturating_sub(instructions_height);
            if max_view_height >= 3 {
                viewing_box_height = required_height.min(max_view_height).max(3);
            }
        }

        // Layout with conditional instruction area
        let constraints = if self.show_instructions {
            vec![
                Constraint::Length(viewing_box_height),
                Constraint::Min(min_table_height),
                Constraint::Length(instructions_height),
            ]
        } else {
            vec![
                Constraint::Length(viewing_box_height),
                Constraint::Min(min_table_height),
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        // Viewing box (top)
        let col_name = self.selected_column_name()?;
        let title = if !col_name.is_empty() {
            col_name
        } else {
            "Cell Value".to_string()
        };
        let viewing_block = Block::default()
            .title(title)
            .borders(Borders::ALL);
        let selected_cell_value = self.selected_cell_value_with_highlighting()?;
        let mut viewing_box = Paragraph::new(selected_cell_value)
            .block(viewing_block);
        if self.auto_expand_value_display {
            viewing_box = viewing_box.wrap(Wrap { trim: false });
        }
        frame.render_widget(viewing_box, chunks[0]);

        // DataTable (middle)
        self.datatable.draw(frame, chunks[1])?;

        // Instruction area (bottom, wrapped) - only if show_instructions is true
        if self.show_instructions {
            let instructions_text = self.get_instructions();
            let instructions = Paragraph::new(instructions_text)
                .block(Block::default().title("Instructions").borders(Borders::ALL))
                .wrap(Wrap { trim: true })
                .style(ratatui::style::Style::default().fg(Color::Yellow));
            Clear.render(chunks[2], frame.buffer_mut()   );
            frame.render_widget(instructions, chunks[2]);
        }
        let col_index = self.datatable.selection.col;
        let df = self.datatable.get_dataframe()?;
        let df = df.as_ref();
        let columns: Vec<String> = df
            .get_column_names_owned()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        self.sort_dialog.set_columns(columns.clone(), col_index);

        // Render SortDialog as a popup overlay only if active
        if self.sort_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            let max_rows = self.sort_dialog.render(popup_area, frame.buffer_mut());
            self.last_sort_dialog_area = Some(popup_area);
            self.last_sort_dialog_max_rows = Some(max_rows);
        }

        // Render FilterDialog as a popup overlay only if active
        if self.filter_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            let max_rows = self.filter_dialog.render(popup_area, frame.buffer_mut());
            self.last_filter_dialog_area = Some(popup_area);
            self.last_filter_dialog_max_rows = Some(max_rows);
        }

        // Render SqlDialog as a popup overlay only if active
        if self.sql_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            self.sql_dialog.render(popup_area, frame.buffer_mut());
            self.last_sql_dialog_area = Some(popup_area);
        }

        // Render JmesPathDialog as a popup overlay only if active
        if self.jmes_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            self.jmes_dialog.render(popup_area, frame.buffer_mut());
            self.last_jmes_dialog_area = Some(popup_area);
        }

        // Render ColumnOperationsDialog as a popup overlay only if active
        if self.column_operations_dialog_active {
            let popup_area = ratatui::layout::Rect {
                x: area.x + area.width / 8,
                y: area.y + area.height / 8,
                width: area.width - area.width / 4,
                height: area.height - area.height / 4,
            };
            self.column_operations_dialog.render(popup_area, frame.buffer_mut());
            self.last_column_operations_dialog_area = Some(popup_area);
        }

        // Render ColumnOperationOptionsDialog as a popup overlay only if active
        if self.column_operation_options_dialog_active
            && let Some(dialog) = &mut self.column_operation_options_dialog {
                let popup_area = ratatui::layout::Rect {
                    x: area.x + area.width / 8,
                    y: area.y + area.height / 8,
                    width: area.width - area.width / 4,
                    height: area.height - area.height / 4,
                };
                dialog.render(popup_area, frame.buffer_mut());
            }

        // Render LlmClientCreateDialog as a popup overlay only if active
        if self.llm_client_create_dialog_active
            && let Some(dialog) = &mut self.llm_client_create_dialog {
                let popup_area = ratatui::layout::Rect {
                    x: area.x + area.width / 8,
                    y: area.y + area.height / 8,
                    width: area.width - area.width / 4,
                    height: area.height - area.height / 4,
                };
                dialog.render(popup_area, frame.buffer_mut());
                self.last_llm_client_create_dialog_area = Some(popup_area);
            }

        // Render ColumnWidthDialog as a popup overlay only if active
        if self.column_width_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            let _max_rows = self.column_width_dialog.render(popup_area, frame.buffer_mut());
            self.last_column_width_dialog_area = Some(popup_area);
        }
        // Render EmbeddingsPromptDialog as a popup overlay only if active
        if self.embeddings_prompt_dialog_active
            && let Some(dialog) = &mut self.embeddings_prompt_dialog {
                let popup_area = ratatui::layout::Rect {
                    x: area.x + area.width / 8,
                    y: area.y + area.height / 8,
                    width: area.width - area.width / 4,
                    height: area.height - area.height / 4,
                };
                dialog.render(popup_area, frame.buffer_mut());
                self.last_embeddings_prompt_dialog_area = Some(popup_area);
            }
        // Render FindDialog as a popup overlay only if active
        if self.find_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            self.find_dialog.render(popup_area, frame.buffer_mut());
            self.last_find_dialog_area = Some(popup_area);
        }
        // Render FindAllResultsDialog as a popup overlay only if active
        if self.find_all_results_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            self.find_all_results_dialog.as_mut().unwrap().render(popup_area, frame.buffer_mut());
            self.last_find_all_results_dialog_area = Some(popup_area);
        }
        // DataExportDialog rendering moved to DataTabManagerDialog
        // Render DataFrameDetailsDialog as a popup overlay only if active
        if self.dataframe_details_dialog_active {
			let popup_area = ratatui::layout::Rect {
				x: area.x + area.width / 8,
				y: area.y + area.height / 8,
				width: area.width - area.width / 4,
				height: area.height - area.height / 4,
			};
            let max_rows = self.dataframe_details_dialog.render(popup_area, frame.buffer_mut());
            self.last_dataframe_details_dialog_area = Some(popup_area);
            self.last_dataframe_details_dialog_max_rows = Some(max_rows);
        }
        // Render busy/progress overlay if active (always on top)
        if self.busy_active {
            use ratatui::widgets::Gauge;
            use ratatui::style::Style as RtStyle;
            let popup_area = ratatui::layout::Rect {
                x: area.x + area.width / 4,
                y: area.y + area.height / 2 - 2,
                width: area.width / 2,
                height: 5,
            };
            // Clear the overlay region to avoid underlying artifacts
            Clear.render(popup_area, frame.buffer_mut());
            let gauge = Gauge::default()
                .block(Block::default().title(self.busy_message.clone()).borders(Borders::ALL))
                .ratio(self.busy_progress.clamp(0.0, 1.0))
                .style(RtStyle::default().fg(Color::Yellow))
                .label("Working...");
            gauge.render(popup_area, frame.buffer_mut());
        }
        Ok(())
    }
} 

#[derive(Debug, Clone)]
pub struct QueuedEmbeddings {
    pub source_column: String,
    pub new_column_name: String,
    pub model_name: String,
    pub num_dimensions: usize,
    pub selected_provider: Option<crate::dialog::LlmProvider>,
    pub hide_new_column: bool,
}

#[derive(Debug, Clone)]
pub struct EmbeddingsJob {
    pub source_column: String,
    pub new_column_name: String,
    pub model_name: String,
    pub num_dimensions: usize,
    pub provider: crate::dialog::LlmProvider,
    pub hide_new_column: bool,
    pub row_texts: Vec<Option<String>>,
    pub uniques: Vec<String>,
    pub unique_index: std::collections::HashMap<String, usize>,
    pub unique_embeddings: Vec<Vec<f32>>, // filled progressively, aligned with uniques
    pub next_start: usize,
    pub batch_size: usize,
    pub total_uniques: usize,
}

#[derive(Debug, Clone)]
pub struct QueuedPca {
    pub source_column: String,
    pub new_column_name: String,
    pub k: usize,
}

#[derive(Debug, Clone)]
pub struct QueuedCluster {
    pub source_column: String,
    pub new_column_name: String,
    pub algorithm: ClusterAlgorithm,
    pub kmeans: Option<KmeansOptions>,
    pub dbscan: Option<DbscanOptions>,
}
