//! SQL Query Builder and Parser
//!
//! This module provides functionality to parse and build DuckDB SQL queries.
//! It serves as the foundation for the global query architecture where a single
//! SQL query is the source of truth for the dataset view.

use color_eyre::Result;
use serde::{Deserialize, Serialize};

/// Represents a column in an ORDER BY clause
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderByColumn {
    pub column: String,
    pub ascending: bool,
}

/// Query builder for constructing and parsing DuckDB SQL queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    /// Base table name (e.g., "dataset_123")
    base_table: String,

    /// SELECT columns (default: "*")
    select_columns: Vec<String>,

    /// WHERE clause (without "WHERE" keyword)
    where_clause: Option<String>,

    /// ORDER BY columns
    order_by: Vec<OrderByColumn>,

    /// Calculated columns (e.g., "(1 - list_cosine_similarity(emb, [1,2,3])) as score")
    calculated_columns: Vec<String>,

    /// LIMIT clause
    limit: Option<usize>,

    /// OFFSET clause
    offset: Option<usize>,
}

impl QueryBuilder {
    /// Create a new QueryBuilder with default settings
    pub fn new(base_table: impl Into<String>) -> Self {
        Self {
            base_table: base_table.into(),
            select_columns: vec!["*".to_string()],
            where_clause: None,
            order_by: Vec::new(),
            calculated_columns: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Get the base table name
    pub fn base_table(&self) -> &str {
        &self.base_table
    }

    /// Set SELECT columns
    pub fn select_columns(mut self, columns: Vec<String>) -> Self {
        self.select_columns = columns;
        self
    }

    /// Get SELECT columns
    pub fn get_select_columns(&self) -> &[String] {
        &self.select_columns
    }

    /// Set WHERE clause (without "WHERE" keyword)
    pub fn set_where(&mut self, clause: Option<String>) {
        self.where_clause = clause;
    }

    /// Get WHERE clause
    pub fn get_where(&self) -> Option<&str> {
        self.where_clause.as_deref()
    }

    /// Set ORDER BY columns
    pub fn set_order_by(&mut self, columns: Vec<OrderByColumn>) {
        self.order_by = columns;
    }

    /// Get ORDER BY columns
    pub fn get_order_by(&self) -> Vec<OrderByColumn> {
        self.order_by.clone()
    }

    /// Set LIMIT
    pub fn set_limit(&mut self, limit: Option<usize>) {
        self.limit = limit;
    }

    /// Get LIMIT
    pub fn get_limit(&self) -> Option<usize> {
        self.limit
    }

    /// Set OFFSET
    pub fn set_offset(&mut self, offset: Option<usize>) {
        self.offset = offset;
    }

    /// Get OFFSET
    pub fn get_offset(&self) -> Option<usize> {
        self.offset
    }

    /// Set calculated columns
    pub fn set_calculated_columns(&mut self, columns: Vec<String>) {
        self.calculated_columns = columns;
    }

    /// Add a calculated column
    pub fn add_calculated_column(&mut self, column: String) {
        self.calculated_columns.push(column);
    }

    /// Get calculated columns
    pub fn get_calculated_columns(&self) -> &[String] {
        &self.calculated_columns
    }

    /// Build the SQL query string
    pub fn to_sql(&self) -> String {
        let mut parts = Vec::new();

        // SELECT clause
        let mut select_parts = self.select_columns.clone();
        if select_parts.is_empty() {
            select_parts.push("*".to_string());
        }

        // Add calculated columns
        for calc in &self.calculated_columns {
            select_parts.push(calc.clone());
        }

        parts.push(format!("SELECT {}", select_parts.join(", ")));

        // FROM clause
        parts.push(format!("FROM \"{}\"", self.base_table));

        // WHERE clause
        if let Some(ref where_clause) = self.where_clause {
            parts.push(format!("WHERE {}", where_clause));
        }

        // ORDER BY clause
        if !self.order_by.is_empty() {
            let order_parts: Vec<String> = self
                .order_by
                .iter()
                .map(|col| {
                    let direction = if col.ascending { "ASC" } else { "DESC" };
                    format!("\"{}\" {}", col.column, direction)
                })
                .collect();

            // Always append rowid for stable sorting (DuckDB requirement)
            let mut order_with_rowid = order_parts;
            order_with_rowid.push("rowid ASC".to_string());

            parts.push(format!("ORDER BY {}", order_with_rowid.join(", ")));
        }

        // LIMIT clause
        if let Some(limit) = self.limit {
            parts.push(format!("LIMIT {}", limit));
        }

        // OFFSET clause
        if let Some(offset) = self.offset {
            parts.push(format!("OFFSET {}", offset));
        }

        parts.join(" ")
    }

    /// Parse a SQL query string into a QueryBuilder
    ///
    /// This is a simplified parser that handles basic SELECT queries.
    /// For complex queries (CTEs, subqueries, joins), it may not fully parse.
    pub fn parse(sql: &str, default_table: &str) -> Result<Self> {
        let sql = sql.trim();
        let sql_upper = sql.to_uppercase();

        let mut builder = Self::new(default_table);

        // Extract SELECT columns (simplified - just check if it's SELECT *)
        if sql_upper.starts_with("SELECT ") {
            let after_select = &sql[7..]; // Skip "SELECT "
            if let Some(from_pos) = after_select.to_uppercase().find(" FROM ") {
                let select_part = after_select[..from_pos].trim();
                if select_part != "*" {
                    // Parse individual columns (simplified)
                    builder.select_columns = select_part
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .collect();
                }
            }
        }

        // Extract WHERE clause
        if let Some(where_pos) = sql_upper.find(" WHERE ") {
            let after_where = &sql[where_pos + 7..]; // Skip " WHERE "

            // Find end of WHERE clause (before ORDER BY, LIMIT, or end of string)
            let end_pos = after_where
                .to_uppercase()
                .find(" ORDER BY ")
                .or_else(|| after_where.to_uppercase().find(" LIMIT "))
                .or_else(|| after_where.to_uppercase().find(" OFFSET "))
                .unwrap_or(after_where.len());

            let where_clause = after_where[..end_pos].trim().to_string();
            builder.where_clause = Some(where_clause);
        }

        // Extract ORDER BY clause
        if let Some(order_pos) = sql_upper.find(" ORDER BY ") {
            let after_order = &sql[order_pos + 10..]; // Skip " ORDER BY "

            // Find end of ORDER BY clause (before LIMIT, OFFSET, or end)
            let end_pos = after_order
                .to_uppercase()
                .find(" LIMIT ")
                .or_else(|| after_order.to_uppercase().find(" OFFSET "))
                .unwrap_or(after_order.len());

            let order_clause = after_order[..end_pos].trim();
            builder.order_by = parse_order_by_clause(order_clause)?;
        }

        // Extract LIMIT
        if let Some(limit_pos) = sql_upper.find(" LIMIT ") {
            let after_limit = &sql[limit_pos + 7..]; // Skip " LIMIT "

            // Find end of LIMIT (before OFFSET or end)
            let end_pos = after_limit
                .to_uppercase()
                .find(" OFFSET ")
                .unwrap_or_else(|| {
                    // Find first non-digit character
                    after_limit
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(after_limit.len())
                });

            let limit_str = after_limit[..end_pos].trim();
            if let Ok(limit) = limit_str.parse::<usize>() {
                builder.limit = Some(limit);
            }
        }

        // Extract OFFSET
        if let Some(offset_pos) = sql_upper.find(" OFFSET ") {
            let after_offset = &sql[offset_pos + 8..]; // Skip " OFFSET "

            // Find first non-digit character
            let end_pos = after_offset
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_offset.len());

            let offset_str = after_offset[..end_pos].trim();
            if let Ok(offset) = offset_str.parse::<usize>() {
                builder.offset = Some(offset);
            }
        }

        Ok(builder)
    }
}

/// Parse ORDER BY clause into OrderByColumn structs
fn parse_order_by_clause(clause: &str) -> Result<Vec<OrderByColumn>> {
    let mut columns = Vec::new();

    for part in clause.split(',') {
        let part = part.trim();
        if part.is_empty() || part.eq_ignore_ascii_case("rowid ASC") {
            continue; // Skip empty parts and the rowid tie-breaker
        }

        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        // Extract column name (remove quotes if present)
        let column = tokens[0].trim_matches('"').to_string();

        // Extract direction (default to ASC)
        let ascending = if tokens.len() > 1 {
            !tokens[1].eq_ignore_ascii_case("DESC")
        } else {
            true
        };

        columns.push(OrderByColumn { column, ascending });
    }

    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_simple_query() {
        let builder = QueryBuilder::new("dataset_123");
        let sql = builder.to_sql();
        assert_eq!(sql, "SELECT * FROM \"dataset_123\"");
    }

    #[test]
    fn test_build_query_with_order_by() {
        let mut builder = QueryBuilder::new("dataset_123");
        builder.set_order_by(vec![
            OrderByColumn {
                column: "name".to_string(),
                ascending: true,
            },
            OrderByColumn {
                column: "age".to_string(),
                ascending: false,
            },
        ]);

        let sql = builder.to_sql();
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("\"name\" ASC"));
        assert!(sql.contains("\"age\" DESC"));
        assert!(sql.contains("rowid ASC")); // Tie-breaker
    }

    #[test]
    fn test_build_query_with_where() {
        let mut builder = QueryBuilder::new("dataset_123");
        builder.set_where(Some("age > 18".to_string()));

        let sql = builder.to_sql();
        assert!(sql.contains("WHERE age > 18"));
    }

    #[test]
    fn test_build_query_with_limit_offset() {
        let mut builder = QueryBuilder::new("dataset_123");
        builder.set_limit(Some(100));
        builder.set_offset(Some(50));

        let sql = builder.to_sql();
        assert!(sql.contains("LIMIT 100"));
        assert!(sql.contains("OFFSET 50"));
    }

    #[test]
    fn test_parse_simple_query() {
        let sql = "SELECT * FROM dataset_123";
        let builder = QueryBuilder::parse(sql, "dataset_123").unwrap();

        assert_eq!(builder.base_table, "dataset_123");
        assert_eq!(builder.select_columns, vec!["*"]);
        assert!(builder.order_by.is_empty());
    }

    #[test]
    fn test_parse_query_with_order_by() {
        let sql = "SELECT * FROM dataset_123 ORDER BY \"name\" ASC, \"age\" DESC";
        let builder = QueryBuilder::parse(sql, "dataset_123").unwrap();

        assert_eq!(builder.order_by.len(), 2);
        assert_eq!(builder.order_by[0].column, "name");
        assert!(builder.order_by[0].ascending);
        assert_eq!(builder.order_by[1].column, "age");
        assert!(!builder.order_by[1].ascending);
    }

    #[test]
    fn test_parse_query_with_where() {
        let sql = "SELECT * FROM dataset_123 WHERE age > 18 ORDER BY name";
        let builder = QueryBuilder::parse(sql, "dataset_123").unwrap();

        assert_eq!(builder.where_clause, Some("age > 18".to_string()));
    }

    #[test]
    fn test_roundtrip_query() {
        let mut original = QueryBuilder::new("dataset_123");
        original.set_order_by(vec![OrderByColumn {
            column: "name".to_string(),
            ascending: true,
        }]);
        original.set_where(Some("age > 18".to_string()));
        original.set_limit(Some(100));

        let sql = original.to_sql();
        let parsed = QueryBuilder::parse(&sql, "dataset_123").unwrap();

        assert_eq!(parsed.order_by, original.order_by);
        assert_eq!(parsed.where_clause, original.where_clause);
        assert_eq!(parsed.limit, original.limit);
    }
}
