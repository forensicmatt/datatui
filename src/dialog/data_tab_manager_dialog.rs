//! DataTabManagerDialog: Dialog for managing multiple data sources in tabs
//!
//! This dialog allows users to view and interact with multiple data sources simultaneously
//! by organizing them in tabs. Each tab contains a DataTableContainer for a specific
//! data source loaded from the DataManagementDialog.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap, Tabs};
use ratatui::text::Span;
use crate::action::Action;
use crate::config::Config;
use crate::tui::Event;
use color_eyre::Result;
use crossterm::event::{KeyEvent, MouseEvent, KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Size;
use tokio::sync::mpsc::UnboundedSender;
use polars::prelude::DataFrame;
use crate::components::Component;
use crate::components::datatable_container::DataTableContainer;
use crate::components::datatable::DataTable;
use crate::components::dialog_layout::split_dialog_area;
use crate::dataframe::manager::ManagedDataFrame;
use crate::dialog::data_management_dialog::{LoadedDataset, DataManagementDialog};
use crate::dialog::project_settings_dialog::{ProjectSettingsDialog, ProjectSettingsConfig};
use crate::style::StyleConfig;
use std::collections::HashMap;
use std::sync::Arc;
use crate::data_import_types::DataImportConfig;
use uuid::Uuid;
use std::fs::{File, create_dir_all};
use crate::workspace::WorkspaceState;


/// Represents a single tab in the DataTabManagerDialog
#[derive(Debug, Clone)]
pub struct DataTab {
    pub loaded_dataset: LoadedDataset,
    pub managed_dataframe: ManagedDataFrame,
    pub is_active: bool,
}

impl DataTab {
    /// Create a new DataTab
    pub fn new(
        loaded_dataset: LoadedDataset,
        managed_dataframe: ManagedDataFrame
    ) -> Self {
        Self {
            loaded_dataset,
            managed_dataframe,
            is_active: false,
        }
    }

    /// Get the ID for the tab
    pub fn id(&self) -> String {
        self.loaded_dataset.dataset.id.clone()
    }

    /// Get a display name for the tab
    pub fn display_name(&self) -> String {
        self.loaded_dataset.display_name()
    }
}

/// DataTabManagerDialog: Manages multiple data sources in tabs
#[derive(Debug)]
pub struct DataTabManagerDialog {
    pub tabs: Vec<DataTab>,
    pub active_tab_index: usize,
    pub containers: HashMap<String, DataTableContainer>,
    pub show_instructions: bool,
    pub style: StyleConfig,
    pub data_management_dialog: DataManagementDialog,
    pub show_data_management: bool,
    pub project_settings_dialog: ProjectSettingsDialog,
    pub show_project_settings: bool,
    pub tab_order: Vec<String>, // Maintains the order of tabs for reordering functionality
}

impl DataTabManagerDialog {
    /// Create a new DataTabManagerDialog
    pub fn new(style: StyleConfig) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_index: 0,
            containers: HashMap::new(),
            show_instructions: true,
            style,
            data_management_dialog: DataManagementDialog::new(),
            show_data_management: false,
            project_settings_dialog: ProjectSettingsDialog::new(ProjectSettingsConfig::default()),
            show_project_settings: false,
            tab_order: Vec::new(),
        }
    }

    /// Save the current workspace state (data sources and dialog states) to the workspace folder
    pub fn save_workspace_state(&self) -> color_eyre::Result<()> {
        let Some(workspace_path) = self.project_settings_dialog.config.workspace_path.as_ref() else {
            // No workspace configured; do not save
            return Ok(());
        };
        // ensure parquet folder
        let parquet_root = workspace_path.join(".datatui").join("tabs");
        create_dir_all(&parquet_root)?;

        // Build state and write parquet per tab if needed
        let mut state = WorkspaceState::from_dialogs(self)?;
        for tab in &mut state.tabs {
            if let Some(container) = self.containers.get(&tab.dataset_id) {
                if let Some(current) = container.datatable.dataframe.current_df.as_ref() {
                    let parquet_name = format!("{}.parquet", &tab.dataset_id);
                    let parquet_path = parquet_root.join(&parquet_name);
                    polars::prelude::ParquetWriter::new(File::create(&parquet_path)?)
                        .finish(&mut current.as_ref().clone())?;
                    tab.current_df_parquet = Some(parquet_name);
                } else {
                    tab.current_df_parquet = None;
                }
            }
        }

        state.save_to(workspace_path)
    }

    /// Load workspace state from the workspace folder (if present) and apply
    pub fn load_workspace_state(&mut self) -> color_eyre::Result<()> {
        let Some(workspace_path) = self.project_settings_dialog.config.workspace_path.as_ref() else {
            return Ok(());
        };
        if let Some(state) = WorkspaceState::load_from(workspace_path)? {
            state.apply_to(self)?;
            // Ensure containers see latest datasets and refresh table scroll so next draw reflects state
            let _ = self.update_all_containers_dataframes();
            if let Some(container) = self.get_active_container() {
                let _ = container.datatable.scroll_to_selection();
            }
        }
        Ok(())
    }

    /// Get all available DataFrames from the data management dialog
    pub fn get_available_datasets(&self) -> Result<HashMap<String, LoadedDataset>> {
        let mut available_datasets = HashMap::new();
        for tab in &self.tabs {
            available_datasets.insert(
                tab.display_name(),
                tab.loaded_dataset.clone()
            );
        }
        Ok(available_datasets)
    }

    /// Handle creation of a new dataset from SQL query results
    pub fn handle_new_dataset_creation(&mut self, dataset_name: String, dataframe: Arc<DataFrame>) -> Result<Option<Action>> {
        // Create a new dataset in the data management dialog
        let new_dataset = crate::dialog::data_management_dialog::Dataset {
            id: Uuid::new_v4().to_string(),
            name: dataset_name.clone(),
            alias: None,
            row_count: dataframe.height(),
            column_count: dataframe.width(),
            status: crate::dialog::data_management_dialog::DatasetStatus::Imported,
            error_message: None,
        };
        
        // Create a new data source for SQL-generated datasets
        let sql_source_id = self.data_management_dialog.data_sources.len();
        let sql_data_source = crate::dialog::data_management_dialog::DataSource {
            id: sql_source_id,
            name: "SQL Generated".to_string(),
            file_path: format!("sql://{dataset_name}"),
            import_type: "SQL Query".to_string(),
            datasets: vec![new_dataset.clone()],
            total_datasets: 1,
            imported_datasets: 1,
            failed_datasets: 0,
            data_import_config: DataImportConfig::Text(crate::data_import_types::TextImportConfig {
                file_path: std::path::PathBuf::from(format!("sql://{dataset_name}")),
                options: crate::dialog::csv_options_dialog::CsvImportOptions::default(),
            }),
        };

        // Add the data source
        self.data_management_dialog.data_sources.push(sql_data_source);
        
        // Sync tabs to include the new dataset
        self.sync_tabs_from_data_management()?;
        
        Ok(None)
    }

    /// Sync tabs with loaded DataFrames from DataManagementDialog
    pub fn sync_tabs_from_data_management(&mut self) -> Result<()> {
        // Preserve existing containers temporarily so we can retain dialog state (e.g., SQL text)
        let old_containers = std::mem::take(&mut self.containers);
        // Clear existing tabs and order
        self.tabs.clear();
        self.tab_order.clear();
        
        // Get all available DataFrames for SQL context
        let available_datasets = self.get_available_datasets()?;
        
        // Load all DataFrames from DataManagementDialog
        for data_source in &self.data_management_dialog.data_sources {
            let loaded_dataframes = data_source.load_dataframes()?;
            
            for (_dataset_id, loaded_dataset) in loaded_dataframes {
                // Convert Arc<DataFrame> to ManagedDataFrame
                let managed_df = ManagedDataFrame::from_arc(
                    loaded_dataset.dataframe.clone(),
                    loaded_dataset.dataset.name.clone(),
                    Some(format!("From {}", loaded_dataset.data_source.name)),
                    Some(loaded_dataset.data_source.file_path.clone().into()),
                );
                
                // Create DataTab
                let tab = DataTab::new(
                    loaded_dataset.clone(),
                    managed_df.clone(),
                );
                
                // Create DataTable and DataTableContainer for this tab
                let datatable = DataTable::new(managed_df, self.style.clone());
                let mut container = DataTableContainer::new_with_dataframes(
                    datatable, self.style.clone(), available_datasets.clone()
                );
                // If we had an existing container for this dataset, carry over transient UI state
                if let Some(prev) = old_containers.get(&loaded_dataset.dataset.id) {
                    // Preserve SQL textarea content if present
                    let prev_sql = prev.sql_dialog.textarea.lines().join("\n");
                    if !prev_sql.is_empty() {
                        container.sql_dialog.set_textarea_content(prev_sql);
                    }
                    // Preserve datatable state (current_df, sort, filter, widths, last_sql)
                    container.datatable.dataframe.current_df = prev.datatable.dataframe.current_df.clone();
                    container.datatable.dataframe.last_sort = prev.datatable.dataframe.last_sort.clone();
                    container.datatable.dataframe.filter = prev.datatable.dataframe.filter.clone();
                    // Ensure the FilterDialog reflects any existing filter
                    if let Some(f) = container.datatable.dataframe.filter.clone() {
                        container.set_filter_expression(f);
                    }
                    container.datatable.dataframe.column_width_config = prev.datatable.dataframe.column_width_config.clone();
                    container.datatable.dataframe.last_sql_query = prev.datatable.dataframe.last_sql_query.clone();
                    // Preserve selection/scroll so view doesn't jump
                    container.datatable.selection = prev.datatable.selection;
                    container.datatable.scroll = prev.datatable.scroll;
                    // Preserve JMES dialog state (body text and add_columns list)
                    let jmes_lines = prev.jmes_dialog.textarea.lines();
                    if !jmes_lines.is_empty() {
                        container.jmes_dialog.textarea = tui_textarea::TextArea::from(jmes_lines.to_vec());
                        container.jmes_dialog
                            .textarea
                            .set_line_number_style(ratatui::style::Style::default().bg(ratatui::style::Color::DarkGray));
                    }
                    if !prev.jmes_dialog.add_columns.is_empty() {
                        container.jmes_dialog.add_columns = prev.jmes_dialog.add_columns.clone();
                        container.jmes_dialog.selected_add_col = prev.jmes_dialog.selected_add_col.min(container.jmes_dialog.add_columns.len().saturating_sub(1));
                    }
                }
                
                // Add to tabs and maintain order
                self.tabs.push(tab);
                self.tab_order.push(loaded_dataset.dataset.id.clone());
                
                // Store the container
                self.containers.insert(loaded_dataset.dataset.id.clone(), container);
            }
        }
        
        // Set the first tab as active if we have any tabs
        if !self.tabs.is_empty() {
            self.active_tab_index = 0;
            if let Some(first_tab) = self.tabs.first_mut() {
                first_tab.is_active = true;
            }
        }
        
        Ok(())
    }

    /// Update all containers with the latest available dataframes
    pub fn update_all_containers_dataframes(&mut self) -> Result<()> {
        let latest = self.get_available_datasets()?;
        for container in self.containers.values_mut() {
            container.set_available_datasets(latest.clone());
        }
        Ok(())
    }

    /// Reorder tabs by moving a tab from one position to another
    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) -> Result<()> {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() {
            return Err(color_eyre::eyre::eyre!("Invalid tab indices for reordering"));
        }
        
        // Reorder the tabs vector
        let tab = self.tabs.remove(from_index);
        self.tabs.insert(to_index, tab);
        
        // Reorder the tab_order vector
        let tab_id = self.tab_order.remove(from_index);
        self.tab_order.insert(to_index, tab_id);
        
        // Update active tab index if necessary
        if self.active_tab_index == from_index {
            self.active_tab_index = to_index;
        } else if self.active_tab_index > from_index && self.active_tab_index <= to_index {
            self.active_tab_index -= 1;
        } else if self.active_tab_index < from_index && self.active_tab_index >= to_index {
            self.active_tab_index += 1;
        }
        
        // Update active state for all tabs
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            tab.is_active = i == self.active_tab_index;
        }
        
        Ok(())
    }

    /// Move a tab to the front (index 0)
    pub fn move_tab_to_front(&mut self, tab_index: usize) -> Result<()> {
        self.reorder_tab(tab_index, 0)
    }

    /// Move a tab to the back (last index)
    pub fn move_tab_to_back(&mut self, tab_index: usize) -> Result<()> {
        let new_index = self.tabs.len().saturating_sub(1);
        self.reorder_tab(tab_index, new_index)
    }

    /// Get tab by ID
    pub fn get_tab_by_id(&self, tab_id: &str) -> Option<&DataTab> {
        self.tabs.iter().find(|tab| tab.id() == tab_id)
    }

    /// Get tab by ID (mutable)
    pub fn get_tab_by_id_mut(&mut self, tab_id: &str) -> Option<&mut DataTab> {
        self.tabs.iter_mut().find(|tab| tab.id() == tab_id)
    }

    /// Get tab index by ID
    pub fn get_tab_index_by_id(&self, tab_id: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id() == tab_id)
    }

    /// Add a new tab with data from a data source
    pub fn add_tab(
        &mut self, loaded_dataset: LoadedDataset, managed_dataframe: ManagedDataFrame
    ) -> Result<()> {
        let tab = DataTab::new(
            loaded_dataset.clone(),
            managed_dataframe
        );

        // Create a DataTableContainer for this tab
        // Note: This is a simplified approach - in a real implementation, we'd need to handle lifetimes properly
        // For now, we'll create the container when needed during rendering
        let tab_id = tab.id();
        self.tabs.push(tab);
        self.tab_order.push(tab_id); // Add to order
        
        // If this is the first tab, make it active
        if self.tabs.len() == 1 {
            self.active_tab_index = 0;
            if let Some(first_tab) = self.tabs.first_mut() {
                first_tab.is_active = true;
            }
        }
        
        Ok(())
    }

    /// Remove a tab by index
    pub fn remove_tab(&mut self, index: usize) -> Result<()> {
        if index < self.tabs.len() {
            let tab_id = self.tabs[index].id();
            self.tabs.remove(index);
            self.containers.remove(&tab_id);
            self.tab_order.retain(|id| id != &tab_id); // Remove from order
            
            // Adjust active tab index
            if self.tabs.is_empty() {
                self.active_tab_index = 0;
            } else if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            }
            
            // Update active state for the new active tab
            if !self.tabs.is_empty() {
                for (i, tab) in self.tabs.iter_mut().enumerate() {
                    tab.is_active = i == self.active_tab_index;
                }
            }
        }
        Ok(())
    }

    /// Get the currently active tab
    pub fn active_tab(&self) -> Option<&DataTab> {
        self.tabs.get(self.active_tab_index)
    }

    /// Get a mutable reference to the currently active tab
    pub fn active_tab_mut(&mut self) -> Option<&mut DataTab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    /// Switch to a different tab
    pub fn switch_tab(&mut self, index: usize) -> Result<()> {
        if index < self.tabs.len() {
            // Clear previous active tab
            if let Some(prev_active) = self.tabs.get_mut(self.active_tab_index) {
                prev_active.is_active = false;
            }
            
            // Set new active tab
            self.active_tab_index = index;
            if let Some(new_active) = self.tabs.get_mut(index) {
                new_active.is_active = true;
            }

            let latest = self.get_available_datasets()?;
            if let Some(container) = self.get_active_container() {
                // Ensure container has the latest available DataFrames
                container.set_available_datasets(latest);
            }
        }
        Ok(())
    }

    /// Get the DataTableContainer for the active tab
    pub fn get_active_container(&mut self) -> Option<&mut DataTableContainer> {
        if let Some(active_tab) = self.active_tab() {
            let tab_id = active_tab.id();
            self.containers.get_mut(&tab_id)
        } else {
            None
        }
    }

    /// Create or get the DataTableContainer for a specific tab
    pub fn get_or_create_container(&mut self, tab_id: &str) -> Option<&mut DataTableContainer> {
        if !self.containers.contains_key(tab_id) {
            // Find the tab and create a container
            if let Some(_tab) = self.tabs.iter().find(|t| t.id() == tab_id) {
                // Create a new DataTableContainer
                // This is a simplified approach - in reality, we'd need to handle the lifetime properly
                // For now, we'll return None and handle this in the rendering logic
                return None;
            }
        }
        self.containers.get_mut(tab_id)
    }

    /// Render the dialog
    pub fn render(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if self.tabs.is_empty() {
            self.show_data_management = true;
        }

        if self.show_data_management {
            Clear.render(area, frame.buffer_mut());
            // Render the DataManagementDialog
            self.data_management_dialog.render(area, frame.buffer_mut());
            // Render ProjectSettings overlay if active
            if self.show_project_settings {
                let margin_x = (area.width as f32 * 0.10) as u16;
                let margin_y = (area.height as f32 * 0.10) as u16;
                let ps_area = Rect::new(
                    area.x + margin_x,
                    area.y + margin_y,
                    area.width.saturating_sub(margin_x * 2),
                    area.height.saturating_sub(margin_y * 2),
                );
                let _ = self.project_settings_dialog.render(ps_area, frame.buffer_mut());
            }
            Ok(())
        } else {
            // Render the main tab manager
            let instructions = "Ctrl+M: Data Management  Alt+S: Project Settings  Ctrl+S: Manual Sync Tabs  Ctrl+F: Move Tab to Front  Ctrl+B: Move Tab to Back  Ctrl+L: Move Tab Left  Ctrl+R: Move Tab Right  Ctrl+D: Delete Tab  Left/Right or h/l: Navigate Tabs  Esc: Close dialog";
            let show_instructions = self.tabs.is_empty();
            let layout = split_dialog_area(
                area, show_instructions, Some(instructions)
            );
            let content_area = layout.content_area;
            let instructions_area = layout.instructions_area;

            let block = Block::default()
                    .title("📊 DataTUI ")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL);

            // Calculate areas for tabs and content first
            let inner_area = block.inner(content_area);
            
            block.render(content_area, frame.buffer_mut());
            let tab_height = 1; // Height for tab bar
            let tab_area = inner_area;
            let content_area = Rect {
                x: inner_area.x,
                y: inner_area.y + tab_height,
                width: inner_area.width,
                height: inner_area.height.saturating_sub(tab_height),
            };

            // Render tabs
            self.render_tabs(tab_area, frame.buffer_mut());
            // Render content for active tab
            self.render_active_tab_content(frame, content_area)?;
            self.render_instructions(instructions, instructions_area, frame.buffer_mut());

            // Render ProjectSettings overlay if active
            if self.show_project_settings {
                let margin_x = (area.width as f32 * 0.10) as u16;
                let margin_y = (area.height as f32 * 0.10) as u16;
                let ps_area = Rect::new(
                    area.x + margin_x,
                    area.y + margin_y,
                    area.width.saturating_sub(margin_x * 2),
                    area.height.saturating_sub(margin_y * 2),
                );
                let _ = self.project_settings_dialog.render(ps_area, frame.buffer_mut());
            }

            Ok(())
        }
    }

    /// Render the tab bar
    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let tab_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        };
    
        if self.tabs.is_empty() {
            let message = "No data sources loaded.\nUse the Data Management dialog (Ctrl+M) to load data sources.";
            let paragraph = Paragraph::new(message)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::Gray));
            paragraph.render(tab_area, buf);
            return;
        }

        // Create tab titles using alias if available, otherwise source name
        let tab_titles: Vec<String> = self.tabs.iter().enumerate().map(|(index, tab)| {
            let is_active = index == self.active_tab_index;
            let _style = if is_active {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            };
            
            // Use alias if available, otherwise use source name
            tab.display_name().to_string()
            // Line::from(vec![Span::styled(title, style)])
        }).collect();

        // Calculate how many tabs can fit in the available width
        let available_width = tab_area.width as usize;
        let total_tabs = self.tabs.len();
        
        // Estimate tab width (including padding and dividers)
        let estimated_tab_width = 15; // Approximate width per tab
        let max_visible_tabs = available_width / estimated_tab_width;
        
        if max_visible_tabs >= total_tabs {
            // All tabs can fit, render normally
            let tabs = Tabs::new(tab_titles)
                .block(Block::default().borders(Borders::BOTTOM))
                .select(self.active_tab_index)
                .divider(" ")
                .highlight_style(Style::default().fg(Color::White).bg(Color::Yellow));

            tabs.render(tab_area, buf);
        } else {
            // Need to implement scrolling - show a subset of tabs around the active one
            let start_index = if self.active_tab_index >= max_visible_tabs {
                self.active_tab_index.saturating_sub(max_visible_tabs / 2)
            } else {
                0
            };
            
            let end_index = (start_index + max_visible_tabs).min(total_tabs);
            
            // Create visible tab titles
            let visible_titles: Vec<Line> = self.tabs.iter()
                .enumerate()
                .filter(|(index, _)| *index >= start_index && *index < end_index)
                .map(|(index, tab)| {
                    let is_active = index == self.active_tab_index;
                    let style = if is_active {
                        Style::default().fg(Color::White).bg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::White).bg(Color::White)
                    };
                    
                    let title = tab.display_name().to_string();
                    Line::from(vec![Span::styled(title, style)])
                })
                .collect();
            
            // Calculate the relative index for the active tab within visible tabs
            let relative_active_index = self.active_tab_index.saturating_sub(start_index);
            
            let tabs = Tabs::new(visible_titles)
                .block(Block::default().borders(Borders::BOTTOM))
                .select(relative_active_index)
                .divider(" ")
                .highlight_style(Style::default().fg(Color::White).bg(Color::Yellow));

            tabs.render(tab_area, buf);
            
            // Add scroll indicators if needed
            if start_index > 0 {
                // Show left scroll indicator
                let left_indicator = Span::styled("◀", Style::default().fg(Color::Yellow));
                let left_area = Rect {
                    x: tab_area.x,
                    y: tab_area.y,
                    width: 1,
                    height: tab_area.height,
                };
                left_indicator.render(left_area, buf);
            }
            
            if end_index < total_tabs {
                // Show right scroll indicator
                let right_indicator = Span::styled("▶", Style::default().fg(Color::Yellow));
                let right_area = Rect {
                    x: tab_area.x + tab_area.width.saturating_sub(1),
                    y: tab_area.y,
                    width: 1,
                    height: tab_area.height,
                };
                right_indicator.render(right_area, buf);
            }
        }
    }

    /// Render the content for the active tab
    fn render_active_tab_content(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if self.tabs.is_empty() {
            let message = "No tabs available. Use Ctrl+M to open Data Management, then close it to auto-sync tabs.";
            let paragraph = Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray));
            paragraph.render(area, frame.buffer_mut());
            return Ok(());
        }

        if let Some(active_tab) = self.tabs.get(self.active_tab_index)
            && let Some(container) = self.containers.get_mut(&active_tab.id()) {
                // Render the container in the content area
                // Sync auto expand option from project settings to container before draw
                container.auto_expand_value_display = self.project_settings_dialog
                    .config
                    .data_viewer
                    .auto_exapand_value_display;
                container.draw(frame, area)?;
        }

        Ok(())
    }

    /// Render instructions
    fn render_instructions(&self, instructions: &str, instructions_area: Option<Rect>, buf: &mut Buffer) {
        if self.show_instructions
            && let Some(instructions_area) = instructions_area {
                let instructions_paragraph = Paragraph::new(instructions)
                    .block(Block::default().borders(Borders::ALL).title("Instructions"))
                    .style(Style::default().fg(Color::Yellow))
                    .wrap(Wrap { trim: true });
                instructions_paragraph.render(instructions_area, buf);
        }
    }
}

impl Component for DataTabManagerDialog {
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    fn register_config_handler(&mut self, _config: Config) -> Result<()> {
        Ok(())
    }

    fn init(&mut self, _area: Size) -> Result<()> {
        Ok(())
    }

    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        if let Some(Event::Key(key)) = event {
            self.handle_key_event(key)
        } else {
            Ok(None)
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // Handle ProjectSettingsDialog first if active (overlay)
        if self.show_project_settings {
            if let Some(Event::Key(key_event)) = Some(Event::Key(key)) {
                let result = self.project_settings_dialog.handle_events(Some(Event::Key(key_event)))?;
                if let Some(action) = result.clone() {
                    match action {
                        Action::DialogClose => {
                            self.show_project_settings = false;
                            return Ok(None);
                        }
                        Action::ProjectSettingsApplied(_cfg) => {
                            // Apply and load workspace if valid
                            self.show_project_settings = false;
                            // If workspace path exists, try loading workspace state
                            if self.project_settings_dialog.config.workspace_path.as_ref().is_some_and(|p| p.is_dir()) {
                                let _ = self.load_workspace_state();
                            }
                            return Ok(result);
                        }
                        _ => {
                            return Ok(Some(action));
                        }
                    }
                }
                return Ok(None);
            }
            Ok(None)
        } else if self.show_data_management {
            // Handle events for DataManagementDialog
            // Allow opening ProjectSettings with Alt+S even while Data Management is open
            if key.kind == crossterm::event::KeyEventKind::Press
                && key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::ALT) {
                self.show_project_settings = true;
                return Ok(None);
            }
            if let Some(Event::Key(key_event)) = Some(Event::Key(key)) {
                let result = self.data_management_dialog.handle_events(Some(Event::Key(key_event)))?;
                
                // Check if the dialog should be closed
                if let Some(Action::CloseDataManagementDialog) = result {
                    self.show_data_management = false;
                    // Auto-sync tabs when DataManagementDialog is closed
                    if let Err(e) = self.sync_tabs_from_data_management() { return Ok(Some(Action::Error(format!("Failed to auto-sync tabs: {e}")))); }

                    // Update all containers with the latest available dataframes
                    let _ = self.update_all_containers_dataframes();

                    // Forward to active container if available
                    let latest = self.get_available_datasets()?;
                    if let Some(container) = self.get_active_container() {
                        // Ensure container has the latest available DataFrames
                        container.set_available_datasets(latest);
                        // Forward the key event to the active container
                        if let Some(action) = container.handle_key_event(key)? {
                            match action {
                                Action::SqlDialogAppliedNewDataset { dataset_name, dataframe } => {
                                    // Handle new dataset creation
                                    return self.handle_new_dataset_creation(dataset_name, dataframe);
                                }
                                _ => {
                                    // Forward other actions up
                                    return Ok(Some(action));
                                }
                            }
                        }
                    }
                    return Ok(None);
                }
                
                return Ok(result);
            }
            Ok(None)
        } else {
            // Handle events for main tab manager
            if key.kind == crossterm::event::KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Open ProjectSettingsDialog
                        self.show_project_settings = true;
                        Ok(None)
                    }
                    KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Open DataManagementDialog
                        self.show_data_management = true;
                        Ok(None)
                    }
                    KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Move current tab to front
                        if !self.tabs.is_empty()
                            && let Err(e) = self.move_tab_to_front(self.active_tab_index) {
                            return Ok(Some(Action::Error(format!("Failed to move tab to front: {e}"))));
                        }
                        Ok(None)
                    }
                    KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Move current tab to back
                        if !self.tabs.is_empty() && let Err(e) = self.move_tab_to_back(self.active_tab_index) {
                            return Ok(Some(Action::Error(format!("Failed to move tab to back: {e}"))));
                        }
                        Ok(None)
                    }
                    KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Move current tab left (swap with previous)
                        if !self.tabs.is_empty() && self.active_tab_index > 0 {
                            let new_index = self.active_tab_index - 1;
                            if let Err(e) = self.reorder_tab(self.active_tab_index, new_index) {
                                return Ok(Some(Action::Error(format!("Failed to move tab left: {e}"))));
                            }
                        }
                        Ok(None)
                    }
                    KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Move current tab right (swap with next)
                        if !self.tabs.is_empty() && self.active_tab_index < self.tabs.len() - 1 {
                            let new_index = self.active_tab_index + 1;
                            if let Err(e) = self.reorder_tab(self.active_tab_index, new_index) {
                                return Ok(Some(Action::Error(format!("Failed to move tab right: {e}"))));
                            }
                        }
                        Ok(None)
                    }
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Navigate to previous tab
                        if !self.tabs.is_empty() {
                            let new_index = if self.active_tab_index == 0 {
                                self.tabs.len() - 1
                            } else {
                                self.active_tab_index - 1
                            };
                            let _ = self.switch_tab(new_index);
                        }
                        Ok(None)
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                        // Navigate to next tab
                        if !self.tabs.is_empty() {
                            let new_index = if self.active_tab_index == self.tabs.len() - 1 {
                                0
                            } else {
                                self.active_tab_index + 1
                            };
                            let _ = self.switch_tab(new_index);
                        }
                        Ok(None)
                    }
                    _ => {
                        // Forward to active container if available
                        let latest = self.get_available_datasets()?;
                        if let Some(container) = self.get_active_container() {
                            // Ensure container has the latest available DataFrames
                            container.set_available_datasets(latest);
                            // Forward the key event to the active container
                            if let Some(action) = container.handle_key_event(key)? {
                                match action {
                                    Action::SqlDialogAppliedNewDataset { dataset_name, dataframe } => {
                                        // Handle new dataset creation
                                        return self.handle_new_dataset_creation(dataset_name, dataframe);
                                    }
                                    Action::SaveWorkspaceState => {
                                        // Ensure last SQL text is stored on the dataframe for capture
                                        if let Some(active_tab) = self.tabs.get(self.active_tab_index) {
                                            let tab_id = active_tab.id();
                                            if let Some(container) = self.containers.get_mut(&tab_id) {
                                                let sql_text = container.sql_dialog.textarea.lines().join("\n");
                                                container.datatable.dataframe.last_sql_query = Some(sql_text);
                                            }
                                        }
                                        if self.project_settings_dialog.config.workspace_path.as_ref().is_some_and(|p| p.is_dir()) {
                                            let _ = self.save_workspace_state();
                                        }
                                        return Ok(None);
                                    }
                                    _ => {
                                        // Forward other actions up
                                        return Ok(Some(action));
                                    }
                                }
                            }
                            Ok(None)
                        } else {
                            Ok(None)
                        }
                    }
                }
            } else {
                Ok(None)
            }
        }
    }

    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        // Forward updates (Tick, Render, etc.) to the active DataTableContainer
        if let Some(container) = self.get_active_container() && let Some(ret) = container.update(action)? {
            return Ok(Some(ret));
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // First render the basic structure (tabs, instructions, etc.)
        self.render(frame, area)?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataframe::manager::ManagedDataFrame;
    use crate::dialog::data_management_dialog::{DataSource, Dataset, DatasetStatus, LoadedDataset};
    use crate::data_import_types::DataImportConfig;
    use polars::prelude::*;

    fn create_test_dataframe() -> ManagedDataFrame {
        let s1 = Series::new("col1".into(), &[1, 2, 3]);
        let s2 = Series::new("col2".into(), &["a", "b", "c"]);
        let df = DataFrame::new(vec![s1.into(), s2.into()]).unwrap();
        ManagedDataFrame::new(df, "TestDF".to_string(), None, None)
    }

    fn create_test_loaded_dataset(id: &str, name: &str, alias: Option<String>) -> LoadedDataset {
        let csv_options = crate::dialog::csv_options_dialog::CsvImportOptions {
            delimiter: ',',
            has_header: true,
            quote_char: Some('"'),
            escape_char: Some('\\'),
        };
        let config = DataImportConfig::text(
            std::path::PathBuf::from("test.csv"),
            csv_options
        );
        
        let data_source = DataSource::from_import_config(0, &config);
        let dataset = Dataset {
            id: id.to_string(),
            name: name.to_string(),
            alias,
            row_count: 3,
            column_count: 2,
            status: DatasetStatus::Imported,
            error_message: None,
        };
        
        let s1 = Series::new("col1".into(), &[1, 2, 3]);
        let s2 = Series::new("col2".into(), &["a", "b", "c"]);
        let df = DataFrame::new(vec![s1.into(), s2.into()]).unwrap();
        
        LoadedDataset {
            data_source,
            dataset,
            dataframe: Arc::new(df),
        }
    }

    #[test]
    fn test_add_tab() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        let managed_df = create_test_dataframe();
        let loaded_dataset = create_test_loaded_dataset("1", "dataset1", None);
        
        assert!(dialog.add_tab(loaded_dataset, managed_df).is_ok());
        assert_eq!(dialog.tabs.len(), 1);
        assert_eq!(dialog.active_tab_index, 0);
    }

    #[test]
    fn test_switch_tab() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        let managed_df1 = create_test_dataframe();
        let managed_df2 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        
        dialog.add_tab(loaded_dataset1, managed_df1).unwrap();
        dialog.add_tab(loaded_dataset2, managed_df2).unwrap();
        
        dialog.switch_tab(1).unwrap();
        assert_eq!(dialog.active_tab_index, 1);
    }

    #[test]
    fn test_remove_tab() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        let managed_df = create_test_dataframe();
        let loaded_dataset = create_test_loaded_dataset("1", "dataset1", None);
        
        dialog.add_tab(loaded_dataset, managed_df).unwrap();
        assert_eq!(dialog.tabs.len(), 1);
        
        dialog.remove_tab(0).unwrap();
        assert_eq!(dialog.tabs.len(), 0);
    }

    #[test]
    fn test_reorder_tab() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add three tabs
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let df3 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        let loaded_dataset3 = create_test_loaded_dataset("3", "dataset3", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        dialog.add_tab(loaded_dataset3, df3).unwrap();
        
        assert_eq!(dialog.tabs.len(), 3);
        assert_eq!(dialog.tab_order.len(), 3);
        
        // Test reordering: move tab 0 to position 2
        dialog.reorder_tab(0, 2).unwrap();
        assert_eq!(dialog.tabs.len(), 3);
        assert_eq!(dialog.tab_order.len(), 3);
        
        // Verify the order changed
        let first_tab_id = dialog.tab_order[0].clone();
        let last_tab_id = dialog.tab_order[2].clone();
        assert_ne!(first_tab_id, last_tab_id);
    }

    #[test]
    fn test_move_tab_to_front() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add two tabs
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        
        let original_first_tab_id = dialog.tab_order[0].clone();
        let original_second_tab_id = dialog.tab_order[1].clone();
        
        // Move second tab to front
        dialog.move_tab_to_front(1).unwrap();
        
        // Verify the order changed
        assert_eq!(dialog.tab_order[0], original_second_tab_id);
        assert_eq!(dialog.tab_order[1], original_first_tab_id);
    }

    #[test]
    fn test_move_tab_to_back() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add two tabs
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        
        let original_first_tab_id = dialog.tab_order[0].clone();
        let original_second_tab_id = dialog.tab_order[1].clone();
        
        // Move first tab to back
        dialog.move_tab_to_back(0).unwrap();
        
        // Verify the order changed
        assert_eq!(dialog.tab_order[0], original_second_tab_id);
        assert_eq!(dialog.tab_order[1], original_first_tab_id);
    }

    #[test]
    fn test_sync_tabs_from_data_management() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Initially no tabs
        assert_eq!(dialog.tabs.len(), 0);
        assert_eq!(dialog.tab_order.len(), 0);
        
        // Sync tabs (should work even with no data sources)
        assert!(dialog.sync_tabs_from_data_management().is_ok());
        
        // Should still have no tabs since no data sources are loaded
        assert_eq!(dialog.tabs.len(), 0);
        assert_eq!(dialog.tab_order.len(), 0);
    }

    #[test]
    fn test_update_all_containers_dataframes() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add a few tabs
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let df3 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        let loaded_dataset3 = create_test_loaded_dataset("3", "dataset3", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        dialog.add_tab(loaded_dataset3, df3).unwrap();
        
        // Sync to populate containers
        dialog.sync_tabs_from_data_management().unwrap();
        
        // Update all containers
        let _ = dialog.update_all_containers_dataframes();
    }

    #[test]
    fn test_tab_navigation() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add three tabs
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let df3 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "dataset1", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "dataset2", None);
        let loaded_dataset3 = create_test_loaded_dataset("3", "dataset3", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        dialog.add_tab(loaded_dataset3, df3).unwrap();
        
        // Initially first tab should be active
        assert_eq!(dialog.active_tab_index, 0);
        assert!(dialog.tabs[0].is_active);
        assert!(!dialog.tabs[1].is_active);
        assert!(!dialog.tabs[2].is_active);
        
        // Navigate to next tab
        let _ = dialog.switch_tab(1);
        assert_eq!(dialog.active_tab_index, 1);
        assert!(!dialog.tabs[0].is_active);
        assert!(dialog.tabs[1].is_active);
        assert!(!dialog.tabs[2].is_active);
        
        // Navigate to last tab
        let _ = dialog.switch_tab(2);
        assert_eq!(dialog.active_tab_index, 2);
        assert!(!dialog.tabs[0].is_active);
        assert!(!dialog.tabs[1].is_active);
        assert!(dialog.tabs[2].is_active);
        
        // Navigate back to first tab
        let _ = dialog.switch_tab(0);
        assert_eq!(dialog.active_tab_index, 0);
        assert!(dialog.tabs[0].is_active);
        assert!(!dialog.tabs[1].is_active);
        assert!(!dialog.tabs[2].is_active);
    }

    #[test]
    fn test_tab_dataset_names() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add tabs with different dataset names
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let loaded_dataset1 = create_test_loaded_dataset("1", "sales_data", None);
        let loaded_dataset2 = create_test_loaded_dataset("2", "customer_data", None);
        
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        
        // Verify dataset names are stored correctly in the LoadedDataset
        assert_eq!(dialog.tabs[0].loaded_dataset.dataset.name, "sales_data");
        assert_eq!(dialog.tabs[1].loaded_dataset.dataset.name, "customer_data");
        
        // Verify display names show the dataset name when no alias is set
        // The LoadedDataset::display_name() returns dataset.name when no alias is set
        assert_eq!(dialog.tabs[0].display_name(), "sales_data");
        assert_eq!(dialog.tabs[1].display_name(), "customer_data");
    }

    #[test]
    fn test_tab_alias_display() {
        let style = StyleConfig::default();
        let mut dialog = DataTabManagerDialog::new(style);
        
        // Add tabs with different alias configurations
        let df1 = create_test_dataframe();
        let df2 = create_test_dataframe();
        let df3 = create_test_dataframe();
        
        // Tab without alias - should show source name
        let loaded_dataset1 = create_test_loaded_dataset("1", "sales_data", None);
        dialog.add_tab(loaded_dataset1, df1).unwrap();
        
        // Tab with alias - should show alias
        let loaded_dataset2 = create_test_loaded_dataset("2", "customer_data", Some("Customer Info".to_string()));
        dialog.add_tab(loaded_dataset2, df2).unwrap();
        
        // Tab with empty alias - should show source name
        let loaded_dataset3 = create_test_loaded_dataset("3", "product_data", Some("".to_string()));
        dialog.add_tab(loaded_dataset3, df3).unwrap();
        
        // Verify tab display names
        assert_eq!(dialog.tabs[0].display_name(), "sales_data"); // dataset name (no alias)
        assert_eq!(dialog.tabs[1].display_name(), "Customer Info"); // alias
        assert_eq!(dialog.tabs[2].display_name(), ""); // empty alias
        
        // Verify dataset_alias field values in the LoadedDataset
        assert_eq!(dialog.tabs[0].loaded_dataset.dataset.alias, None);
        assert_eq!(dialog.tabs[1].loaded_dataset.dataset.alias, Some("Customer Info".to_string()));
        assert_eq!(dialog.tabs[2].loaded_dataset.dataset.alias, Some("".to_string()));
    }
} 