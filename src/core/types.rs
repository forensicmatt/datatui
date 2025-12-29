use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Unique identifier for datasets
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatasetId(Uuid);

impl DatasetId {
    /// Create a new unique dataset ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the ID as a string slice
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for DatasetId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DatasetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DatasetId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::from_str(s).map_err(|e| e.to_string())?))
    }
}

/// Source type for imported data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    Csv,
    Parquet,
    Excel,
    Sqlite,
    Json,
    SqlQuery,
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Csv => write!(f, "csv"),
            Self::Parquet => write!(f, "parquet"),
            Self::Excel => write!(f, "excel"),
            Self::Sqlite => write!(f, "sqlite"),
            Self::Json => write!(f, "json"),
            Self::SqlQuery => write!(f, "sql_query"),
        }
    }
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Parquet => "parquet",
            Self::Excel => "excel",
            Self::Sqlite => "sqlite",
            Self::Json => "json",
            Self::SqlQuery => "sql_query",
        }
    }
}

impl FromStr for SourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "csv" => Ok(Self::Csv),
            "parquet" => Ok(Self::Parquet),
            "excel" => Ok(Self::Excel),
            "sqlite" => Ok(Self::Sqlite),
            "json" => Ok(Self::Json),
            "sql_query" => Ok(Self::SqlQuery),
            _ => Err(format!("Unknown source type: {}", s)),
        }
    }
}

/// CSV import options
#[derive(Debug, Clone)]
pub struct CsvImportOptions {
    pub has_header: bool,
    pub delimiter: char,
    pub quote_char: Option<char>,
}

impl Default for CsvImportOptions {
    fn default() -> Self {
        Self {
            has_header: true,
            delimiter: ',',
            quote_char: Some('"'),
        }
    }
}

/// Parquet import options
#[derive(Debug, Clone)]
pub struct ParquetImportOptions {
    // Parquet files are self-describing, minimal options needed
}

impl Default for ParquetImportOptions {
    fn default() -> Self {
        Self {}
    }
}

/// Import configuration enum
#[derive(Debug, Clone)]
pub enum ImportConfig {
    Csv(CsvImportOptions),
    Parquet(ParquetImportOptions),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_id_creation() {
        let id1 = DatasetId::new();
        let id2 = DatasetId::new();

        assert_ne!(id1, id2, "IDs should be unique");
        assert!(!id1.as_str().is_empty());
    }

    #[test]
    fn test_source_type_conversion() {
        assert_eq!(SourceType::from_str("csv").unwrap(), SourceType::Csv);
        assert_eq!(SourceType::Csv.as_str(), "csv");

        assert!(SourceType::from_str("invalid").is_err());
    }

    #[test]
    fn test_dataset_id_serialization() {
        let id = DatasetId::from_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        let restored: DatasetId = serde_json::from_str(&json).unwrap();

        assert_eq!(id, restored);
    }
}
