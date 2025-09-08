use polars::prelude::*;
use polars_lazy::frame::IntoLazy;
use std::collections::BTreeMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use std::fmt;
use std::sync::Arc;
// info! used only in tests below; call via fully qualified path there

use crate::dialog::sort_dialog::SortColumn;
use crate::dialog::filter_dialog::FilterExpr;
use crate::dialog::column_width_dialog::ColumnWidthConfig;

/// Metadata for a managed DataFrame.
#[derive(Debug, Clone)]
pub struct DataFrameMetadata {
    pub name: String,
    pub description: Option<String>,
    pub source_path: Option<PathBuf>,
    pub creation_time: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

/// A managed DataFrame with metadata and state.
#[derive(Clone)]
pub struct ManagedDataFrame {
    /// Base dataset as a lazy query plan
    pub df: LazyFrame,
    /// Materialized view for display; None means not yet collected
    pub current_df: Option<Arc<DataFrame>>,
    pub metadata: DataFrameMetadata,
    pub last_sort: Option<Vec<SortColumn>>,
    pub filter: Option<FilterExpr>,
    pub last_sql_query: Option<String>,
    pub column_width_config: ColumnWidthConfig,
}

impl ManagedDataFrame {

    /// Reset the current DataFrame to the base lazy frame
    pub fn reset_current_df(&mut self) {
        self.current_df = None;
    }

    /// Set the column width configuration
    pub fn set_column_width_config(&mut self, config: ColumnWidthConfig) {
        self.column_width_config = config;
    }

    /// Returns the number of rows in the DataFrame.
    pub fn row_count(&self) -> usize {
        if let Some(df) = self.current_df.as_ref() {
            df.height()
        } else {
            // Fallback: get row count from the lazy frame if not materialized
            // Note: This will trigger collection, so only use if necessary
            match self.df.clone().collect() {
                Ok(df) => df.height(),
                Err(_) => 0,
            }
        }
    }
    /// Returns the number of columns in the DataFrame.
    pub fn column_count(&self) -> usize {
        if let Some(df) = self.current_df.as_ref() {
            df.width()
        } else {
            // Fallback: get column count from the lazy frame if not materialized
            match self.df.clone().collect() {
                Ok(df) => df.width(),
                Err(_) => 0,
            }
        }
    }
    /// Returns a Vec of (column name, DataType) for all columns.
    pub fn column_types(&self) -> Vec<(String, DataType)> {
        if let Some(df) = &self.current_df {
            df.get_columns()
                .iter()
                .map(|s| (s.name().to_string(), s.dtype().clone()))
                .collect()
        } else {
            // Fallback: get column types from the lazy frame if not materialized
            match self.df.clone().collect() {
                Ok(df) => df
                    .get_columns()
                    .iter()
                    .map(|s| (s.name().to_string(), s.dtype().clone()))
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
    }
    
    /// Returns a summary string with row/column count and column types.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Rows: {}, Columns: {}\n",
            self.row_count(),
            self.column_count()
        );
        s.push_str("Column Types:\n");
        for (name, dtype) in self.column_types() {
            s.push_str(&format!("  {name}: {dtype:?}\n"));
        }
        s
    }

    pub fn get_dataframe(&self) ->  color_eyre::Result<Arc<DataFrame>> {
        if self.current_df.is_none() {
            // Populate current_df from the base lazy frame
            let collected = Arc::new(self.collect_base_df()?);
            // We need to mutate self.dataframe to set current_df
            // But self is &self, so we can't mutate here.
            // Instead, return the collected DataFrame directly.
            return Ok(collected);
        }
        Ok(self.current_df.as_ref().unwrap().clone())
    }

    pub fn new(df: DataFrame, name: String, description: Option<String>, source_path: Option<PathBuf>) -> Self {
        let now = chrono::Utc::now();
        let metadata = DataFrameMetadata {
            name,
            description,
            source_path,
            creation_time: now,
            last_modified: now,
        };
        let lazy = df.clone().lazy();
        Self {
            df: lazy,
            metadata,
            last_sort: None,
            filter: None,
            last_sql_query: None,
            current_df: None,
            column_width_config: ColumnWidthConfig::default(),
        }
    }

    /// Set the current DataFrame
    pub fn set_current_df(&mut self, df: DataFrame) {
        self.current_df = Some(Arc::new(df));
    }
    
    /// Create a new ManagedDataFrame from an Arc<DataFrame>
    pub fn from_arc(df: Arc<DataFrame>, name: String, description: Option<String>, source_path: Option<PathBuf>) -> Self {
        let now = chrono::Utc::now();
        let metadata = DataFrameMetadata {
            name,
            description,
            source_path,
            creation_time: now,
            last_modified: now,
        };
        let lazy = df.as_ref().clone().lazy();
        Self {
            df: lazy,
            metadata,
            last_sort: None,
            filter: None,
            last_sql_query: None,
            current_df: None,
            column_width_config: ColumnWidthConfig::default(),
        }
    }

    /// Collect the base lazy frame into a DataFrame.
    pub fn collect_base_df(&self) -> color_eyre::Result<DataFrame> {
        self.df
            .clone()
            .collect()
            .map_err(|e| color_eyre::eyre::eyre!("Collect error: {}", e))
    }

    /// Ensure `current_df` is populated by collecting from base if needed.
    pub fn ensure_current_df(&mut self) -> color_eyre::Result<Arc<DataFrame>> {
        if let Some(df) = &self.current_df {
            return Ok(df.clone());
        }
        let collected = Arc::new(self.collect_base_df()?);
        self.current_df = Some(collected.clone());
        Ok(collected)
    }

    /// Reorder columns in the DataFrame according to the provided column names
    pub fn reorder_columns(&mut self, column_order: &[String]) -> color_eyre::Result<()> {
        // Validate that all provided column names exist in the DataFrame
        let current = self.ensure_current_df()?;
        let existing_columns: std::collections::HashSet<_> = current
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        
        for col_name in column_order {
            if !existing_columns.contains(col_name) {
                return Err(color_eyre::eyre::eyre!("Column '{}' not found in DataFrame", col_name));
            }
        }
        
        // Select columns in the new order
        let columns: Vec<String> = column_order.to_vec();
        let new_df = current.select(&columns)?;
        self.current_df = Some(Arc::new(new_df));
        
        Ok(())
    }

    /// Cast a column to a new DataType in the current DataFrame view
    pub fn cast_column(&mut self, column: &str, dtype: &DataType) -> color_eyre::Result<()> {
        use polars::prelude::Column;
        let current = self.ensure_current_df()?;
        let s = current.column(column)?;
        let casted = s.cast(dtype)
            .map_err(|e| color_eyre::eyre::eyre!("Cast error on '{}': {}", column, e))?;
        let mut cols: Vec<Column> = Vec::with_capacity(current.width());
        for c in current.get_columns() {
            if c.name().as_str() == column {
                let mut out = casted.clone();
                out.rename(polars::prelude::PlSmallStr::from_str(column));
                cols.push(out);
            } else {
                cols.push(c.clone());
            }
        }
        let new_df = DataFrame::new(cols)
            .map_err(|e| color_eyre::eyre::eyre!("Rebuild error after cast: {}", e))?;
        self.current_df = Some(Arc::new(new_df));
        Ok(())
    }
}

impl fmt::Debug for ManagedDataFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let current_shape = self
            .current_df
            .as_ref()
            .map(|df| df.shape());
        f.debug_struct("ManagedDataFrame")
            .field("metadata", &self.metadata)
            .field("last_sort", &self.last_sort)
            .field("filter", &self.filter)
            .field("column_width_config", &self.column_width_config)
            .field("current_shape", &current_shape)
            .finish()
    }
}

impl fmt::Display for DataFrameMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(desc) = &self.description {
            write!(f, " - {desc}")?;
        }
        if let Some(path) = &self.source_path {
            write!(f, "\nSource: {}", path.display())?;
        }
        write!(f, "\nCreated: {}\nModified: {}", self.creation_time, self.last_modified)
    }
}

impl fmt::Display for ManagedDataFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.metadata)?;
        write!(f, "{}", self.summary())
    }
}

/// Trait for managing multiple DataFrames.
pub trait DataFrameManager {
    /// Add a new DataFrame to the manager.
    fn add_dataframe(&mut self, df: DataFrame, name: String, description: Option<String>, source: Option<PathBuf>) -> usize;
    /// Get a reference to a managed DataFrame by ID.
    fn get_dataframe(&self, id: usize) -> Option<&ManagedDataFrame>;
    /// Get a mutable reference to a managed DataFrame by ID.
    fn get_dataframe_mut(&mut self, id: usize) -> Option<&mut ManagedDataFrame>;
    /// Remove a DataFrame by ID.
    fn remove_dataframe(&mut self, id: usize) -> bool;
    /// List all DataFrames' metadata.
    fn list_dataframes(&self) -> Vec<(usize, &DataFrameMetadata)>;
    // Add filter/sort methods as needed
}

/// Trait for DataFrames that can be sorted by multiple columns
pub trait SortableDataFrame {
    /// Sort the DataFrame by the given columns and directions.
    fn sort_by_columns(&mut self, columns: &[SortColumn]) -> color_eyre::Result<()>;
}

impl SortableDataFrame for ManagedDataFrame {
    fn sort_by_columns(&mut self, columns: &[SortColumn]) -> color_eyre::Result<()> {
        if columns.is_empty() {
            return Ok(());
        }
        let by: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
        let reverse: Vec<bool> = columns.iter().map(|c: &SortColumn| !c.ascending).collect();
        let nulls_last: Vec<bool> = columns.iter().map(|c: &SortColumn| !c.ascending).collect();
        let options = SortMultipleOptions::default()
            .with_order_descending_multi(reverse)
            .with_nulls_last_multi(nulls_last);
        let current = self.ensure_current_df()?;
        let sorted = current.sort(&by, options)?;
        self.current_df = Some(Arc::new(sorted));
        self.last_sort = Some(columns.to_vec());
        Ok(())
    }
}

impl ManagedDataFrame {
    /// Toggle sorting for a single column. First press sorts ascending.
    /// Pressing again on the same column reverses the direction.
    pub fn sort_toggle_for_column(&mut self, column: &str) -> color_eyre::Result<()> {
        let col_name = column.to_string();
        let mut ascending = true;
        if let Some(ref last) = self.last_sort
            && last.len() == 1 && last[0].name == col_name {
            ascending = !last[0].ascending;
        }
        let source: Arc<DataFrame> = self.ensure_current_df()?;
        let by = vec![col_name.clone()];
        let reverse = vec![!ascending];
        let options = SortMultipleOptions::default()
            .with_order_descending_multi(reverse.clone())
            .with_nulls_last_multi(reverse);
        let sorted = source.sort(&by, options)?;
        self.current_df = Some(Arc::new(sorted));
        self.last_sort = Some(vec![SortColumn { name: col_name, ascending }]);
        Ok(())
    }
}

pub trait FilterableDataFrame {
    fn apply_filter(&mut self, filter: FilterExpr) -> color_eyre::Result<()>;
    fn clear_filter(&mut self);
}

impl FilterableDataFrame for ManagedDataFrame {
    fn apply_filter(&mut self, filter: FilterExpr) -> color_eyre::Result<()> {
        let base_df = self.collect_base_df()?;
        let mask = filter.create_mask(&base_df)?;
        let new_df = base_df.filter(&mask)?;
        self.current_df = Some(Arc::new(new_df));
        Ok(())
    }
    fn clear_filter(&mut self) {
        match self.collect_base_df() {
            Ok(df) => {
                self.current_df = Some(Arc::new(df));
                self.filter = None;
            }
            Err(_) => {
                // If collect fails, just clear to None
                self.current_df = None;
                self.filter = None;
            }
        }
    }
}

/// Concrete implementation of DataFrameManager using a BTreeMap.
pub struct DataFrameManagerImpl {
    dataframes: BTreeMap<usize, ManagedDataFrame>,
    next_id: usize,
}

impl Default for DataFrameManagerImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl DataFrameManagerImpl {
    /// Create a new DataFrameManagerImpl.
    pub fn new() -> Self {
        Self {
            dataframes: BTreeMap::new(),
            next_id: 0,
        }
    }
}

impl DataFrameManager for DataFrameManagerImpl {
    fn add_dataframe(&mut self, df: DataFrame, name: String, description: Option<String>, source: Option<PathBuf>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let now = Utc::now();
        let metadata = DataFrameMetadata {
            name,
            description,
            source_path: source,
            creation_time: now,
            last_modified: now,
        };
        let managed = ManagedDataFrame {
            df: df.clone().lazy(),
            metadata,
            last_sort: None,
            filter: None,
            last_sql_query: None,
            current_df: Some(Arc::new(df)),
            column_width_config: ColumnWidthConfig::default(),
        };
        self.dataframes.insert(id, managed);
        id
    }

    fn get_dataframe(&self, id: usize) -> Option<&ManagedDataFrame> {
        self.dataframes.get(&id)
    }

    fn get_dataframe_mut(&mut self, id: usize) -> Option<&mut ManagedDataFrame> {
        self.dataframes.get_mut(&id)
    }

    fn remove_dataframe(&mut self, id: usize) -> bool {
        self.dataframes.remove(&id).is_some()
    }

    fn list_dataframes(&self) -> Vec<(usize, &DataFrameMetadata)> {
        self.dataframes.iter().map(|(id, managed)| (*id, &managed.metadata)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_df() -> DataFrame {
        let s1 = Series::new("a".into(), [1i32, 2, 3]);
        let s2 = Series::new("b".into(), ["x", "y", "z"]);
        DataFrame::new(vec![s1.into(), s2.into()]).unwrap()
    }

    #[test]
    fn test_add_and_get_dataframe() {
        let mut manager = DataFrameManagerImpl::new();
        let df = sample_df();
        let id = manager.add_dataframe(df.clone(), "TestDF".to_string(), Some("desc".to_string()), None);
        let retrieved = manager.get_dataframe(id).unwrap();
        assert_eq!(retrieved.metadata.name, "TestDF");
        assert_eq!(retrieved.metadata.description.as_deref(), Some("desc"));
        assert_eq!(retrieved.column_count(), 2);
        assert_eq!(retrieved.row_count(), 3);
        // Compare shape and column names for equality
        let cur = retrieved.current_df.as_ref().unwrap();
        assert_eq!(cur.shape(), df.shape());
        assert_eq!(cur.get_column_names(), df.get_column_names());
        // Test column_types and summary
        let col_types = retrieved.column_types();
        assert_eq!(col_types.len(), 2);
        assert_eq!(col_types[0].0, "a");
        assert_eq!(col_types[1].0, "b");
        let summary = retrieved.summary();
        assert!(summary.contains("Rows: 3"));
        assert!(summary.contains("Columns: 2"));
        assert!(summary.contains("a: Int32"));
        // Accept either "b: Utf8" or "b: String" or "b: Str" for the string column
        assert!(summary.contains("b: Utf8") || summary.contains("b: String") || summary.contains("b: Str"), "Summary did not contain expected string column type. Actual summary: {summary}");
        // Test Display impls
        let meta_str = format!("{}", retrieved.metadata);
        let df_str = format!("{retrieved}");
        assert!(meta_str.contains("TestDF"));
        assert!(df_str.contains("Rows: 3"));
    }

    #[test]
    fn test_list_and_remove_dataframes() {
        let mut manager = DataFrameManagerImpl::new();
        let id1 = manager.add_dataframe(sample_df(), "DF1".to_string(), None, None);
        let id2 = manager.add_dataframe(sample_df(), "DF2".to_string(), None, None);
        let list = manager.list_dataframes();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|(id, meta)| *id == id1 && meta.name == "DF1"));
        assert!(list.iter().any(|(id, meta)| *id == id2 && meta.name == "DF2"));
        assert!(manager.remove_dataframe(id1));
        assert!(manager.get_dataframe(id1).is_none());
        assert_eq!(manager.list_dataframes().len(), 1);
    }
} 