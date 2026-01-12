use serde::{Deserialize, Serialize};

/// JSON import options
///
/// Supports both standard JSON and NDJSON (newline-delimited JSON) formats.
/// Allows specifying a path expression to extract records from nested JSON structures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonImportOptions {
    /// If true, treat file as NDJSON (each line is a JSON object)
    /// If false, expect a standard JSON structure
    pub ndjson: bool,

    /// Path expression to extract the array of records from the JSON
    ///
    /// Supports dot-notation for nested paths or JSONPath syntax.
    ///
    /// Examples:
    /// - "`@`" - Use the root as records (default, for root-level arrays)
    /// - "`data`" - Extract from `{"data": [...]}`
    /// - "`results.items`" - Extract from `{"results": {"items": [...]}}`
    /// - "`$.Records`" - JSONPath syntax for `{"Records": [...]}`
    pub records_expr: String,
}

impl Default for JsonImportOptions {
    fn default() -> Self {
        Self {
            ndjson: false,
            records_expr: "@".to_string(),
        }
    }
}

impl JsonImportOptions {
    /// Create new options for NDJSON format
    pub fn ndjson() -> Self {
        Self {
            ndjson: true,
            records_expr: "@".to_string(),
        }
    }

    /// Create new options for JSON format with custom records expression
    pub fn with_records_expr(expr: impl Into<String>) -> Self {
        Self {
            ndjson: false,
            records_expr: expr.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = JsonImportOptions::default();
        assert!(!opts.ndjson);
        assert_eq!(opts.records_expr, "@");
    }

    #[test]
    fn test_ndjson_options() {
        let opts = JsonImportOptions::ndjson();
        assert!(opts.ndjson);
    }

    #[test]
    fn test_custom_records_expr() {
        let opts = JsonImportOptions::with_records_expr("data.items");
        assert!(!opts.ndjson);
        assert_eq!(opts.records_expr, "data.items");
    }
}
