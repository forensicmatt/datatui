use std::fs::{File, create_dir_all};
use std::path::Path;
use serde::{Deserialize, Serialize};
use ratatui::style::{Color, Style};
use tui_textarea::TextArea;
use polars::prelude::SerReader;
use crate::dialog::sort_dialog::SortColumn;
use crate::dialog::data_management_dialog::DataSource;
use crate::dialog::project_settings_dialog::ProjectSettingsConfig;
use crate::dialog::filter_dialog::FilterExpr;
use crate::components::datatable_container::DataTableContainer;
use crate::dialog::data_tab_manager_dialog::DataTabManagerDialog;
use crate::dialog::column_width_dialog::ColumnWidthConfig;
use crate::dialog::jmes_dialog::JmesPathKeyValuePair;
use polars::prelude::ParquetReader;
use tracing::info;

// We surface errors via color-eyre; no custom error type needed.

/// Serializable snapshot of dialogs/state we want to persist for a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub project: ProjectSettingsConfig,
    pub data_sources: Vec<DataSource>,
    // DataTableContainer dialog states (per active tab) are captured by index order
    pub tabs: Vec<TabState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub dataset_id: String,
    // Stable identifiers to match tabs across sessions even if dataset_id changes
    pub source_file_path: Option<String>,
    pub dataset_name: Option<String>,
    pub sort: Vec<SortColumn>,
    pub filter: Option<FilterExpr>,
    pub column_widths: ColumnWidthConfig,
    pub sql_query: String,
    pub jmes_expression: String,
    pub jmes_add_columns: Vec<JmesPathKeyValuePair>,
    // If current_df is materialized, a parquet file name stored under workspace/.datatui/tabs
    pub current_df_parquet: Option<String>,
}

impl WorkspaceState {
    pub fn from_dialogs(manager: &DataTabManagerDialog) -> color_eyre::Result<Self> {
        // capture data_sources from data management dialog
        let data_sources: Vec<DataSource> = manager.data_management_dialog.data_sources.clone();

        // capture tabs + dialog states
        let mut tabs: Vec<TabState> = Vec::new();
        for tab in &manager.tabs {
            let tab_id = tab.loaded_dataset.dataset.id.clone();
            if let Some(container) = manager.containers.get(&tab_id) {
                tabs.push(Self::capture_tab_state(tab_id, container));
            } else {
                // fallback: minimal
                tabs.push(TabState{
                    dataset_id: tab_id,
                    source_file_path: None,
                    dataset_name: None,
                    sort: vec![],
                    filter: None,
                    column_widths: ColumnWidthConfig::default(),
                    sql_query: String::new(),
                    jmes_expression: String::new(),
                    jmes_add_columns: vec![],
                    current_df_parquet: None,
                });
            }
        }

        Ok(Self {
            project: manager.project_settings_dialog.config.clone(),
            data_sources,
            tabs,
        })
    }

    fn capture_tab_state(dataset_id: String, container: &DataTableContainer) -> TabState {
        // Sort
        let sort = container.datatable.dataframe.last_sort.clone().unwrap_or_default();
        // Filter
        let filter = container.datatable.dataframe.filter.clone();
        // Stable identifiers
        let source_file_path = container
            .datatable
            .dataframe
            .metadata
            .source_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let dataset_name = Some(container.datatable.dataframe.metadata.name.clone());
        // Column widths
        let column_widths = container.datatable.dataframe.column_width_config.clone();
        // SQL query (prefer recorded last_sql_query if available)
        let sql_query = container
            .datatable
            .dataframe
            .last_sql_query
            .clone()
            .unwrap_or_else(|| container.sql_dialog.textarea.lines().join("\n"));
        // JMES
        let (jmes_expression, jmes_add_columns) = {
            let _mode = &container.jmes_dialog.mode;
            (
                container.jmes_dialog.textarea.lines().join("\n"),
                container.jmes_dialog.add_columns.clone(),
            )
        };
        // Parquet name filled in by save_to when we have a workspace path
        TabState {
            dataset_id,
            source_file_path,
            dataset_name,
            sort,
            filter,
            column_widths,
            sql_query,
            jmes_expression,
            jmes_add_columns,
            current_df_parquet: None
        }
    }

    pub fn save_to(&self, workspace_path: &Path) -> color_eyre::Result<()> {
        if !workspace_path.is_dir() {
            return Err(color_eyre::eyre::eyre!("Workspace path is not a directory: {}", workspace_path.display()));
        }
        // ensure folders and just write state JSON; parquet writing is handled by caller with access to containers
        create_dir_all(workspace_path)?;
        let file = File::create(workspace_path.join("datatui_workspace_state.json"))?;
        serde_json::to_writer_pretty(file, &self)?;
        Ok(())
    }

    pub fn load_from(workspace_path: &Path) -> color_eyre::Result<Option<Self>> {
        let file_path = workspace_path.join("datatui_workspace_state.json");
        if !file_path.exists() {
            return Ok(None);
        }
        let file = File::open(file_path)?;
        let state: WorkspaceState = serde_json::from_reader(file)?;
        Ok(Some(state))
    }

    pub fn apply_to(self, manager: &mut DataTabManagerDialog) -> color_eyre::Result<()> {
        // Apply project settings (workspace already known)
        manager.project_settings_dialog.config = self.project;

        // Extend data sources using a function
        manager.data_management_dialog.extend_data_sources(self.data_sources);

        // Rebuild tabs/containers from data sources
        manager.sync_tabs_from_data_management()?;

        // Apply per-tab states
        let parquet_root = if let Some(path) = manager.project_settings_dialog.config.workspace_path.as_ref() {
            path.join(".datatui").join("tabs")
        } else {
            // No workspace configured; skip parquet restores
            return Ok(());
        };

        for tab_state in self.tabs.into_iter() {
            // Try direct match by dataset_id
            let mut container_key: Option<String> = if manager.containers.contains_key(&tab_state.dataset_id) {
                Some(tab_state.dataset_id.clone())
            } else {
                None
            };

            // If not found, attempt a stable match using source_file_path and dataset_name
            if container_key.is_none()
                && let (Some(saved_path), Some(saved_name)) = (&tab_state.source_file_path, &tab_state.dataset_name)
            {
                let found = manager
                    .containers
                    .iter()
                    .find_map(|(key, c)| {
                        let meta = &c.datatable.dataframe.metadata;
                        let meta_path = meta
                            .source_path
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_string());
                        if meta_path.as_deref() == Some(saved_path.as_str()) && meta.name == *saved_name {
                            Some(key.clone())
                        } else {
                            None
                        }
                    });
                container_key = found;
            }

            if let Some(key) = container_key
                && let Some(container) = manager.containers.get_mut(&key)
            {
                if !tab_state.sort.is_empty() {
                    container.datatable.dataframe.last_sort = Some(tab_state.sort);
                }

                // column widths
                container.datatable.set_column_width_config(tab_state.column_widths.clone());

                // sql
                container.sql_dialog.set_textarea_content(&tab_state.sql_query);

                // jmes dialog state
                container.jmes_dialog.add_columns = tab_state.jmes_add_columns.clone();
                let jmes_lines: Vec<String> = tab_state
                    .jmes_expression
                    .lines()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                container.jmes_dialog.textarea = TextArea::from(jmes_lines);
                container
                    .jmes_dialog
                    .textarea
                    .set_line_number_style(Style::default().bg(Color::DarkGray));

                // current_df parquet load if present (authoritative snapshot of current view)
                if let Some(fname) = &tab_state.current_df_parquet {
                    let path = parquet_root.join(fname);
                    if path.exists() {
                        info!("Loading parquet file: {}", path.display());
                        let file = File::open(&path)?;
                        if let Ok(df) = ParquetReader::new(file).finish() {
                            container.datatable.set_current_df(df);
                        }
                    }
                }

                if let Some(f) = tab_state.filter.clone() {
                    info!("Restoring filter: {:?}", f);
                    container.set_filter_expression(f);
                }
            }
        }
        // After applying state, refresh active container so UI reflects latest data
        if let Some(container) = manager.get_active_container() {
            let _ = container.datatable.scroll_to_selection();
        }
        Ok(())
    }
}


