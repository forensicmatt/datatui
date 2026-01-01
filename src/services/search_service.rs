//! Search functionality using DuckDB SQL queries
//!
//! This module provides memory-efficient search operations by leveraging
//! DuckDB's SQL engine instead of loading entire datasets into memory.

use crate::core::ManagedDataset;
use color_eyre::Result;
use serde::{Deserialize, Serialize};

/// Options for find/search operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindOptions {
    /// Search backward from current position
    pub backward: bool,
    /// Match whole words only
    pub whole_word: bool,
    /// Case-sensitive matching
    pub match_case: bool,
    /// Wrap around when reaching end/start
    pub wrap_around: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            backward: false,
            whole_word: false,
            match_case: false,
            wrap_around: true,
        }
    }
}

/// Search mode (normal substring or regex)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    /// Normal substring matching
    Normal,
    /// Regular expression matching
    Regex,
}

/// Result of a search operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Row index (0-based)
    pub row: usize,
    /// Column name
    pub column: String,
}

/// Result for "Find All" with context
#[derive(Debug, Clone)]
pub struct FindAllResult {
    /// Row index (0-based)
    pub row: usize,
    /// Column name
    pub column: String,
    /// Context string (cell value with surrounding context)
    pub context: String,
}

/// Service for search operations
pub struct SearchService;

impl SearchService {
    /// Find the next occurrence of a pattern starting from a given position
    ///
    /// Returns the position (row, column) of the next match, or None if not found.
    pub fn find_next(
        dataset: &ManagedDataset,
        pattern: &str,
        options: &FindOptions,
        mode: &SearchMode,
        start_row: usize,
        start_col: usize,
    ) -> Result<Option<SearchResult>> {
        if pattern.is_empty() {
            return Ok(None);
        }

        let column_names = dataset.column_names()?;
        if column_names.is_empty() {
            return Ok(None);
        }

        let row_count = dataset.row_count()?;
        if row_count == 0 {
            return Ok(None);
        }

        // Build the query with filtering - returns (query, params)
        let (query, params) =
            Self::build_find_query_with_filter(&column_names, pattern, options, mode)?;

        // Convert String parameters to &dyn ToSql for DuckDB
        let params_refs: Vec<&dyn duckdb::ToSql> =
            params.iter().map(|s| s as &dyn duckdb::ToSql).collect();

        // Execute the query with parameters
        let batch = match dataset.execute_query_with_params(&query, &params_refs) {
            Ok(b) => b,
            Err(e) => {
                return Err(color_eyre::eyre::eyre!(
                    "Search query failed: {}\nSQL: {}",
                    e,
                    query
                ));
            }
        };

        // Parse results and find the next match
        let matches = Self::parse_search_results(&batch)?;

        // Apply forward/backward logic and wrap-around
        Self::find_next_match(&matches, &column_names, start_row, start_col, options)
    }

    /// Count total matches in the dataset
    pub fn count_matches(
        dataset: &ManagedDataset,
        pattern: &str,
        options: &FindOptions,
        mode: &SearchMode,
    ) -> Result<usize> {
        if pattern.is_empty() {
            return Ok(0);
        }

        let column_names = dataset.column_names()?;
        if column_names.is_empty() {
            return Ok(0);
        }

        // Build COUNT query that sums matches across all columns - returns (query, params)
        let (count_query, params) = Self::build_count_query(&column_names, pattern, options, mode)?;

        // Convert String parameters to &dyn ToSql for DuckDB
        let params_refs: Vec<&dyn duckdb::ToSql> =
            params.iter().map(|s| s as &dyn duckdb::ToSql).collect();

        // Execute query with parameters
        let batch = dataset.execute_query_with_params(&count_query, &params_refs)?;

        // Parse the count from the result
        Self::parse_count_result(&batch)
    }

    /// Find all occurrences with context strings
    pub fn find_all(
        dataset: &ManagedDataset,
        pattern: &str,
        options: &FindOptions,
        mode: &SearchMode,
        context_chars: usize,
    ) -> Result<Vec<FindAllResult>> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }

        let column_names = dataset.column_names()?;
        if column_names.is_empty() {
            return Ok(Vec::new());
        }

        // Build query to get all matches with their values - returns (query, params)
        let (query, params) =
            Self::build_find_query_with_filter(&column_names, pattern, options, mode)?;

        // Convert String parameters to &dyn ToSql for DuckDB
        let params_refs: Vec<&dyn duckdb::ToSql> =
            params.iter().map(|s| s as &dyn duckdb::ToSql).collect();

        // Execute query with parameters
        let batch = dataset.execute_query_with_params(&query, &params_refs)?;

        // Parse results and generate context strings
        Self::parse_find_all_results(&batch, pattern, context_chars, options, mode)
    }

    /// Build query with WHERE clause filtering (used internally after getting structure)
    /// Returns (query_string, parameters_vector) for use with prepared statements
    fn build_find_query_with_filter(
        column_names: &[String],
        pattern: &str,
        options: &FindOptions,
        mode: &SearchMode,
    ) -> Result<(String, Vec<String>)> {
        let mut queries = Vec::new();
        let mut all_params = Vec::new();

        for col_name in column_names {
            // Build the condition for this specific column using ? placeholders
            let (condition, param_value) = match mode {
                SearchMode::Normal => {
                    if options.whole_word {
                        if options.match_case {
                            ("value = ?".to_string(), pattern.to_string())
                        } else {
                            ("LOWER(value) = LOWER(?)".to_string(), pattern.to_string())
                        }
                    } else {
                        if options.match_case {
                            ("value LIKE ?".to_string(), format!("%{}%", pattern))
                        } else {
                            (
                                "LOWER(value) LIKE LOWER(?)".to_string(),
                                format!("%{}%", pattern),
                            )
                        }
                    }
                }
                SearchMode::Regex => {
                    let regex_pattern = if options.whole_word {
                        format!("^{}$", pattern)
                    } else {
                        pattern.to_string()
                    };

                    let regex_pattern = if options.match_case {
                        regex_pattern
                    } else {
                        format!("(?i){}", regex_pattern)
                    };

                    ("regexp_matches(value, ?)".to_string(), regex_pattern)
                }
            };

            let query = format!(
                "SELECT row_num, '{}' as col_name, value FROM (SELECT (ROW_NUMBER() OVER () - 1) as row_num, CAST({} AS VARCHAR) as value FROM {{table}}) sub WHERE value IS NOT NULL AND {}",
                col_name,
                Self::quote_identifier(col_name),
                condition
            );
            queries.push(query);
            all_params.push(param_value);
        }

        if queries.is_empty() {
            return Ok((
                "SELECT 0 as row_num, '' as col_name, '' as value WHERE 1=0".to_string(),
                vec![],
            ));
        }

        Ok((queries.join(" UNION ALL "), all_params))
    }

    /// Build COUNT query
    /// Returns (query_string, parameters_vector) for use with prepared statements
    fn build_count_query(
        column_names: &[String],
        pattern: &str,
        options: &FindOptions,
        mode: &SearchMode,
    ) -> Result<(String, Vec<String>)> {
        let mut case_statements = Vec::new();
        let mut param_values = Vec::new();

        for col_name in column_names {
            let col_expr = format!("CAST({} AS VARCHAR)", Self::quote_identifier(col_name));

            // Build the condition for this specific column using ? placeholders
            let (condition, param_value) = match mode {
                SearchMode::Normal => {
                    if options.whole_word {
                        if options.match_case {
                            (format!("{} = ?", col_expr), pattern.to_string())
                        } else {
                            (
                                format!("LOWER({}) = LOWER(?)", col_expr),
                                pattern.to_string(),
                            )
                        }
                    } else {
                        if options.match_case {
                            (format!("{} LIKE ?", col_expr), format!("%{}%", pattern))
                        } else {
                            (
                                format!("LOWER({}) LIKE LOWER(?)", col_expr),
                                format!("%{}%", pattern),
                            )
                        }
                    }
                }
                SearchMode::Regex => {
                    let regex_pattern = if options.whole_word {
                        format!("^{}$", pattern)
                    } else {
                        pattern.to_string()
                    };

                    let regex_pattern = if options.match_case {
                        regex_pattern
                    } else {
                        format!("(?i){}", regex_pattern)
                    };

                    (format!("regexp_matches({}, ?)", col_expr), regex_pattern)
                }
            };

            case_statements.push(format!("SUM(CASE WHEN {} THEN 1 ELSE 0 END)", condition));
            param_values.push(param_value);
        }

        Ok((
            format!(
                "SELECT {} as total FROM {{table}}",
                case_statements.join(" + ")
            ),
            param_values,
        ))
    }

    /// Parse search results from RecordBatch
    fn parse_search_results(
        batch: &duckdb::arrow::array::RecordBatch,
    ) -> Result<Vec<SearchResult>> {
        use duckdb::arrow::array::{Array, Int64Array, StringArray};

        let mut results = Vec::new();

        if batch.num_rows() == 0 {
            return Ok(results);
        }

        // Get row_num and col_name columns
        let row_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| color_eyre::eyre::eyre!("Expected Int64Array for row_num"))?;
        let col_name_col = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| color_eyre::eyre::eyre!("Expected StringArray for col_name"))?;

        for i in 0..batch.num_rows() {
            if !row_col.is_null(i) && !col_name_col.is_null(i) {
                results.push(SearchResult {
                    row: row_col.value(i) as usize,
                    column: col_name_col.value(i).to_string(),
                });
            }
        }

        Ok(results)
    }

    /// Find the next match considering direction and wrap-around
    fn find_next_match(
        matches: &[SearchResult],
        column_names: &[String],
        start_row: usize,
        start_col: usize,
        options: &FindOptions,
    ) -> Result<Option<SearchResult>> {
        if matches.is_empty() {
            return Ok(None);
        }

        // Convert column name to index for comparison
        let start_col_name = column_names
            .get(start_col)
            .ok_or_else(|| color_eyre::eyre::eyre!("Invalid column index"))?;

        // TODO: Implement forward/backward search with wrap-around
        // For now, just return the first match after the current position
        for result in matches {
            let col_idx = column_names
                .iter()
                .position(|c| c == &result.column)
                .unwrap_or(0);

            if options.backward {
                // Backward search: find matches before current position
                if result.row < start_row || (result.row == start_row && col_idx < start_col) {
                    return Ok(Some(result.clone()));
                }
            } else {
                // Forward search: find matches after current position
                if result.row > start_row || (result.row == start_row && col_idx > start_col) {
                    return Ok(Some(result.clone()));
                }
            }
        }

        // Wrap around if enabled
        if options.wrap_around && !matches.is_empty() {
            return Ok(Some(matches[0].clone()));
        }

        Ok(None)
    }

    /// Parse count result from RecordBatch
    fn parse_count_result(batch: &duckdb::arrow::array::RecordBatch) -> Result<usize> {
        use duckdb::arrow::array::{Array, Decimal128Array, Float64Array, Int64Array, UInt64Array};

        if batch.num_rows() == 0 {
            return Ok(0);
        }

        let count_col = batch.column(0);

        // Try different numeric types since SUM can return different types
        if count_col.is_null(0) {
            return Ok(0);
        }

        // Try Int64 first
        if let Some(int64_arr) = count_col.as_any().downcast_ref::<Int64Array>() {
            return Ok(int64_arr.value(0) as usize);
        }

        // Try UInt64
        if let Some(uint64_arr) = count_col.as_any().downcast_ref::<UInt64Array>() {
            return Ok(uint64_arr.value(0) as usize);
        }

        // Try Float64
        if let Some(float64_arr) = count_col.as_any().downcast_ref::<Float64Array>() {
            return Ok(float64_arr.value(0) as usize);
        }

        // Try Decimal128 (common for SUM operations)
        if let Some(decimal_arr) = count_col.as_any().downcast_ref::<Decimal128Array>() {
            return Ok(decimal_arr.value(0) as usize);
        }

        Err(color_eyre::eyre::eyre!(
            "Unexpected count column type: {:?}",
            count_col.data_type()
        ))
    }

    /// Parse find all results with context generation
    fn parse_find_all_results(
        batch: &duckdb::arrow::array::RecordBatch,
        pattern: &str,
        context_chars: usize,
        options: &FindOptions,
        mode: &SearchMode,
    ) -> Result<Vec<FindAllResult>> {
        use duckdb::arrow::array::{Array, Int64Array, StringArray};

        let mut results = Vec::new();

        if batch.num_rows() == 0 {
            return Ok(results);
        }

        let row_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| color_eyre::eyre::eyre!("Expected Int64Array for row_num"))?;
        let col_name_col = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| color_eyre::eyre::eyre!("Expected StringArray for col_name"))?;
        let value_col = batch
            .column(2)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| color_eyre::eyre::eyre!("Expected StringArray for value"))?;

        for i in 0..batch.num_rows() {
            if !row_col.is_null(i) && !col_name_col.is_null(i) && !value_col.is_null(i) {
                let value = value_col.value(i);
                let context = Self::generate_context(value, pattern, context_chars, options, mode);

                results.push(FindAllResult {
                    row: row_col.value(i) as usize,
                    column: col_name_col.value(i).to_string(),
                    context,
                });
            }
        }

        Ok(results)
    }

    /// Generate context string with ellipsis around the match
    fn generate_context(
        value: &str,
        pattern: &str,
        context_chars: usize,
        options: &FindOptions,
        mode: &SearchMode,
    ) -> String {
        // Find the match position in the value
        let match_pos = match mode {
            SearchMode::Normal => {
                let search_value = if options.match_case {
                    value.to_string()
                } else {
                    value.to_lowercase()
                };
                let search_pattern = if options.match_case {
                    pattern.to_string()
                } else {
                    pattern.to_lowercase()
                };

                search_value.find(&search_pattern)
            }
            SearchMode::Regex => {
                // Try to compile regex and find match
                let regex_pattern = if options.match_case {
                    pattern.to_string()
                } else {
                    format!("(?i){}", pattern)
                };

                if let Ok(re) = regex::Regex::new(&regex_pattern) {
                    re.find(value).map(|m| m.start())
                } else {
                    None
                }
            }
        };

        if let Some(pos) = match_pos {
            let start = pos.saturating_sub(context_chars);
            let end = (pos + pattern.len() + context_chars).min(value.len());

            let mut context = String::new();
            if start > 0 {
                context.push_str("...");
            }
            context.push_str(&value[start..end]);
            if end < value.len() {
                context.push_str("...");
            }
            context
        } else {
            // Fallback: return the full value if we can't find the match position
            value.to_string()
        }
    }

    /// Quote SQL identifier (column name)
    fn quote_identifier(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_options_default() {
        let opts = FindOptions::default();
        assert!(!opts.backward);
        assert!(!opts.whole_word);
        assert!(!opts.match_case);
        assert!(opts.wrap_around);
    }

    #[test]
    fn test_search_mode_variants() {
        let normal = SearchMode::Normal;
        let regex = SearchMode::Regex;
        assert_ne!(normal, regex);
    }

    #[test]
    fn test_quote_identifier() {
        assert_eq!(SearchService::quote_identifier("column"), "\"column\"");
        assert_eq!(
            SearchService::quote_identifier("col\"umn"),
            "\"col\"\"umn\""
        );
    }

    #[test]
    fn test_generate_context() {
        let value = "The quick brown fox jumps over the lazy dog";
        let pattern = "fox";
        let context = SearchService::generate_context(
            value,
            pattern,
            10,
            &FindOptions::default(),
            &SearchMode::Normal,
        );

        assert!(context.contains("fox"));
        assert!(context.contains("...")); // Should have ellipsis
    }
}
