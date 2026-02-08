//! SQL Suggestions Module
//!
//! Provides autocomplete suggestions for DuckDB SQL queries, similar to the command bar.
//! Suggests keywords, column names, and functions based on cursor position and context.

use crate::tui::components::sql_dialog::SuggestionItem;

/// Get SQL autocomplete suggestions based on input and cursor position
///
/// # Arguments
/// * `input` - The current SQL query text
/// * `cursor` - Cursor position in the input (0-indexed)
/// * `columns` - Available column names for the current dataset
///
/// # Returns
/// Vector of suggestion items with metadata
pub fn get_sql_suggestions(input: &str, cursor: usize, columns: &[String]) -> Vec<SuggestionItem> {
    // Ensure cursor is within bounds
    let cursor = cursor.min(input.len());

    // Get the text up to the cursor
    let text_before_cursor = &input[..cursor];

    // Split into tokens (whitespace-separated)
    let tokens: Vec<&str> = text_before_cursor.split_whitespace().collect();

    // Determine if we're at the end of a word or starting a new one
    let is_new_word = text_before_cursor.ends_with(char::is_whitespace);

    // Get the current word being typed (empty if starting new word)
    let current_word = if is_new_word {
        ""
    } else {
        tokens.last().copied().unwrap_or("")
    };

    // Determine context and provide suggestions
    if tokens.is_empty() || (tokens.len() == 1 && !is_new_word) {
        // Beginning of query - suggest SELECT
        return filter_keyword_suggestions(&["SELECT"], current_word);
    }

    // Check the last complete keyword to determine context
    let last_keyword = find_last_keyword(&tokens, is_new_word);

    match last_keyword {
        Some("SELECT") => {
            // After SELECT - suggest columns, *, DISTINCT, or functions
            let mut suggestions =
                filter_keyword_suggestions(&["*", "DISTINCT", "FROM"], current_word);
            suggestions.extend(filter_function_suggestions(DUCKDB_FUNCTIONS, current_word));
            suggestions.extend(filter_column_suggestions(columns, current_word));
            suggestions
        }
        Some("FROM") => {
            // Check if we are immediately after FROM (expecting table)
            // or if we have already typed a table/alias (expecting WHERE, etc.)
            let last_token_is_from = tokens
                .last()
                .map(|t| t.to_uppercase() == "FROM")
                .unwrap_or(false);

            if last_token_is_from {
                // Immediately after FROM - suggest table placeholder
                vec![SuggestionItem::field("{table}".to_string())]
            } else {
                // We have a table, treat as general context to suggest WHERE, ORDER, etc.
                get_general_context_suggestions(input, current_word)
            }
        }
        Some("WHERE") => {
            // After WHERE - suggest columns and comparison operators
            let mut result = filter_column_suggestions(columns, current_word);
            result.extend(filter_keyword_suggestions(
                COMPARISON_OPERATORS,
                current_word,
            ));
            result
        }
        Some("ORDER") => {
            // Expecting "BY" after "ORDER"
            filter_keyword_suggestions(&["BY"], current_word)
        }
        Some("BY") => {
            // After ORDER BY or GROUP BY - suggest columns
            filter_column_suggestions(columns, current_word)
        }
        Some("GROUP") => {
            // Expecting "BY" after "GROUP"
            filter_keyword_suggestions(&["BY"], current_word)
        }
        Some("LIMIT") | Some("OFFSET") => {
            // After LIMIT/OFFSET - no suggestions (expecting numbers)
            Vec::new()
        }
        _ => {
            // General context - suggest next clause keywords
            get_general_context_suggestions(input, current_word)
        }
    }
}

/// Helper to get general context suggestions (next clauses)
fn get_general_context_suggestions(input: &str, current_word: &str) -> Vec<SuggestionItem> {
    let mut keyword_list = Vec::new();

    // Determine what keywords make sense based on what we've seen
    let input_upper = input.to_uppercase();

    if !input_upper.contains(" FROM ") {
        keyword_list.push("FROM");
    }
    if input_upper.contains(" FROM ") && !input_upper.contains(" WHERE ") {
        keyword_list.push("WHERE");
    }
    if input_upper.contains(" FROM ") && !input_upper.contains(" GROUP BY ") {
        keyword_list.push("GROUP");
    }
    if input_upper.contains(" FROM ") && !input_upper.contains(" ORDER BY ") {
        keyword_list.push("ORDER");
    }
    if input_upper.contains(" FROM ") && !input_upper.contains(" LIMIT ") {
        keyword_list.push("LIMIT");
    }
    if input_upper.contains(" LIMIT ") && !input_upper.contains(" OFFSET ") {
        keyword_list.push("OFFSET");
    }

    // Also suggest AND/OR if we're in a WHERE clause
    if input_upper.contains(" WHERE ") && !input_upper.contains(" ORDER BY ") {
        keyword_list.push("AND");
        keyword_list.push("OR");
    }

    filter_keyword_suggestions(&keyword_list, current_word)
}

/// Find the last SQL keyword in the tokens
fn find_last_keyword<'a>(tokens: &'a [&'a str], is_new_word: bool) -> Option<&'a str> {
    // If we're starting a new word, the last token is the keyword
    // Otherwise, look at the second-to-last token
    let check_tokens = if is_new_word {
        tokens
    } else if tokens.len() > 1 {
        &tokens[..tokens.len() - 1]
    } else {
        return None;
    };

    // Search backwards for a keyword
    for token in check_tokens.iter().rev() {
        let upper = token.to_uppercase();
        if SQL_KEYWORDS.contains(&upper.as_str()) {
            // Return a static string from SQL_KEYWORDS instead of leaking
            for &keyword in SQL_KEYWORDS {
                if keyword == upper.as_str() {
                    return Some(keyword);
                }
            }
        }
    }

    None
}

/// Filter keyword suggestions based on current word prefix
fn filter_keyword_suggestions(suggestions: &[&str], prefix: &str) -> Vec<SuggestionItem> {
    let prefix_upper = prefix.to_uppercase();
    suggestions
        .iter()
        .filter(|s| s.to_uppercase().starts_with(&prefix_upper))
        .map(|s| SuggestionItem::field(s.to_string()))
        .collect()
}

/// Filter function suggestions with descriptions
fn filter_function_suggestions(functions: &[(&str, &str)], prefix: &str) -> Vec<SuggestionItem> {
    let prefix_upper = prefix.to_uppercase();
    functions
        .iter()
        .filter(|(name, _)| name.to_uppercase().starts_with(&prefix_upper))
        .map(|(name, desc)| SuggestionItem::function(name.to_string(), desc.to_string()))
        .collect()
}

/// Filter column suggestions based on current word prefix
fn filter_column_suggestions(columns: &[String], prefix: &str) -> Vec<SuggestionItem> {
    let prefix_lower = prefix.to_lowercase();
    columns
        .iter()
        .filter(|c| c.to_lowercase().starts_with(&prefix_lower))
        .map(|c| SuggestionItem::field(c.clone()))
        .collect()
}

/// Common DuckDB SQL keywords
const SQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "ORDER",
    "BY",
    "GROUP",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "DISTINCT",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IN",
    "LIKE",
    "BETWEEN",
    "IS",
    "NULL",
    "ASC",
    "DESC",
    "INNER",
    "LEFT",
    "RIGHT",
    "OUTER",
    "JOIN",
    "ON",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
];

/// Common DuckDB functions with signatures
const DUCKDB_FUNCTIONS: &[(&str, &str)] = &[
    ("COUNT", "(col) → INTEGER"),
    ("SUM", "(col) → NUMERIC"),
    ("AVG", "(col) → NUMERIC"),
    ("MIN", "(col) → ANY"),
    ("MAX", "(col) → ANY"),
    ("CAST", "(expr AS type) → type"),
    ("COALESCE", "(val1, val2, ...) → ANY"),
    ("NULLIF", "(val1, val2) → ANY"),
    ("LENGTH", "(str) → INTEGER"),
    ("UPPER", "(str) → VARCHAR"),
    ("LOWER", "(str) → VARCHAR"),
    ("TRIM", "(str) → VARCHAR"),
    ("SUBSTRING", "(str, start, len) → VARCHAR"),
    ("CONCAT", "(str1, str2, ...) → VARCHAR"),
    ("REPLACE", "(str, from, to) → VARCHAR"),
    ("NOW", "() → TIMESTAMP"),
    ("CURRENT_DATE", "() → DATE"),
    ("CURRENT_TIME", "() → TIME"),
    ("CURRENT_TIMESTAMP", "() → TIMESTAMP"),
    ("YEAR", "(date) → INTEGER"),
    ("MONTH", "(date) → INTEGER"),
    ("DAY", "(date) → INTEGER"),
    ("HOUR", "(time) → INTEGER"),
    ("MINUTE", "(time) → INTEGER"),
    ("SECOND", "(time) → INTEGER"),
    ("ABS", "(num) → NUMERIC"),
    ("ROUND", "(num, decimals) → NUMERIC"),
    ("FLOOR", "(num) → NUMERIC"),
    ("CEIL", "(num) → NUMERIC"),
];

/// Comparison operators
const COMPARISON_OPERATORS: &[&str] = &[
    "=",
    "<>",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "LIKE",
    "IN",
    "BETWEEN",
    "IS NULL",
    "IS NOT NULL",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_select_at_start() {
        let suggestions = get_sql_suggestions("", 0, &[]);
        assert!(suggestions.iter().any(|s| s.name == "SELECT"));
    }

    #[test]
    fn test_suggest_columns_after_select() {
        let columns = vec!["name".to_string(), "age".to_string(), "email".to_string()];
        let suggestions = get_sql_suggestions("SELECT ", 7, &columns);

        assert!(suggestions.iter().any(|s| s.name == "*"));
        assert!(suggestions.iter().any(|s| s.name == "name"));
        assert!(suggestions.iter().any(|s| s.name == "age"));
    }

    #[test]
    fn test_filter_columns_by_prefix() {
        let columns = vec!["name".to_string(), "age".to_string(), "email".to_string()];
        let suggestions = get_sql_suggestions("SELECT n", 8, &columns);

        assert!(suggestions.iter().any(|s| s.name == "name"));
        assert!(!suggestions.iter().any(|s| s.name == "age"));
    }

    #[test]
    fn test_suggest_from_after_columns() {
        let suggestions = get_sql_suggestions("SELECT * ", 9, &[]);
        assert!(suggestions.iter().any(|s| s.name == "FROM"));
    }

    #[test]
    fn test_suggest_where_after_from() {
        let suggestions = get_sql_suggestions("SELECT * FROM {table} ", 23, &[]);
        assert!(suggestions.iter().any(|s| s.name == "WHERE"));
        assert!(suggestions.iter().any(|s| s.name == "ORDER"));
    }

    #[test]
    fn test_suggest_columns_after_where() {
        let columns = vec!["name".to_string(), "age".to_string()];
        let suggestions = get_sql_suggestions("SELECT * FROM {table} WHERE ", 29, &columns);

        assert!(suggestions.iter().any(|s| s.name == "name"));
        assert!(suggestions.iter().any(|s| s.name == "age"));
    }

    #[test]
    fn test_suggest_order_by() {
        let suggestions = get_sql_suggestions("SELECT * FROM {table} ORDER ", 29, &[]);
        assert!(suggestions.iter().any(|s| s.name == "BY"));
    }

    #[test]
    fn test_suggest_columns_after_order_by() {
        let columns = vec!["name".to_string(), "age".to_string()];
        let suggestions = get_sql_suggestions("SELECT * FROM {table} ORDER BY ", 32, &columns);

        assert!(suggestions.iter().any(|s| s.name == "name"));
        assert!(suggestions.iter().any(|s| s.name == "age"));
    }

    #[test]
    fn test_case_insensitive_keyword_matching() {
        let suggestions = get_sql_suggestions("select ", 7, &[]);
        assert!(suggestions.iter().any(|s| s.name == "*"));
    }

    #[test]
    fn test_partial_keyword_completion() {
        let suggestions = get_sql_suggestions("SEL", 3, &[]);
        assert!(suggestions.iter().any(|s| s.name == "SELECT"));
    }
}
