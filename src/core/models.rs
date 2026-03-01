use crate::core::types::{DatasetId, SourceType};
use chrono::{DateTime, Utc};
use color_eyre::Result;
use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Represents a dataset record in the session database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub id: DatasetId,
    pub name: String,
    pub source_type: SourceType,
    pub source_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub row_count: Option<u64>,
    pub column_count: Option<u32>,
}

impl DatasetRecord {
    /// Create a new dataset record
    pub fn new(
        id: DatasetId,
        name: String,
        source_type: SourceType,
        source_path: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            source_type,
            source_path,
            created_at: now,
            last_modified: now,
            row_count: None,
            column_count: None,
        }
    }

    /// Insert this record into the database
    pub fn insert(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "INSERT INTO datasets (id, name, source_type, source_path, created_at, last_modified, row_count, column_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                self.id.as_str(),
                &self.name,
                &self.source_type.to_string(),
                &self.source_path,
                self.created_at.timestamp(),
                self.last_modified.timestamp(),
                self.row_count.map(|n| n as i64),
                self.column_count.map(|n| n as i32),
            ]
        )?;
        Ok(())
    }

    /// Load a dataset record by ID
    pub fn load(conn: &Connection, id: &str) -> Result<Self> {
        let mut stmt = conn.prepare(
            "SELECT id, name, source_type, source_path, created_at, last_modified, row_count, column_count
             FROM datasets WHERE id = ?"
        )?;

        let record = stmt.query_row([id], |row| {
            Ok(Self {
                id: DatasetId::from_str(&row.get::<_, String>(0)?).map_err(|e| {
                    duckdb::Error::FromSqlConversionFailure(
                        0,
                        duckdb::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?,
                name: row.get(1)?,
                source_type: SourceType::from_str(&row.get::<_, String>(2)?).map_err(|e| {
                    duckdb::Error::FromSqlConversionFailure(
                        2,
                        duckdb::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?,
                source_path: row.get(3)?,
                created_at: DateTime::from_timestamp(row.get(4)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        4,
                        "created_at".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                last_modified: DateTime::from_timestamp(row.get(5)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        5,
                        "last_modified".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                row_count: row.get::<_, Option<i64>>(6)?.map(|n| n as u64),
                column_count: row.get::<_, Option<i32>>(7)?.map(|n| n as u32),
            })
        })?;

        Ok(record)
    }

    /// Load all dataset records
    pub fn load_all(conn: &Connection) -> Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, source_type, source_path, created_at, last_modified, row_count, column_count
             FROM datasets ORDER BY created_at DESC"
        )?;

        let records = stmt.query_map([], |row| {
            Ok(Self {
                id: DatasetId::from_str(&row.get::<_, String>(0)?).map_err(|e| {
                    duckdb::Error::FromSqlConversionFailure(
                        0,
                        duckdb::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?,
                name: row.get(1)?,
                source_type: SourceType::from_str(&row.get::<_, String>(2)?).map_err(|e| {
                    duckdb::Error::FromSqlConversionFailure(
                        2,
                        duckdb::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?,
                source_path: row.get(3)?,
                created_at: DateTime::from_timestamp(row.get(4)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        4,
                        "created_at".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                last_modified: DateTime::from_timestamp(row.get(5)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        5,
                        "last_modified".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                row_count: row.get::<_, Option<i64>>(6)?.map(|n| n as u64),
                column_count: row.get::<_, Option<i32>>(7)?.map(|n| n as u32),
            })
        })?;

        records.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Update row and column counts
    pub fn update_stats(&mut self, conn: &Connection, rows: u64, cols: u32) -> Result<()> {
        self.row_count = Some(rows);
        self.column_count = Some(cols);
        self.last_modified = Utc::now();

        conn.execute(
            "UPDATE datasets SET row_count = ?, column_count = ?, last_modified = ? WHERE id = ?",
            params![
                rows as i64,
                cols as i32,
                self.last_modified.timestamp(),
                self.id.as_str()
            ],
        )?;
        Ok(())
    }
}

/// Configuration used to generate an embedding column
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingColumnConfig {
    pub provider: crate::core::LlmProvider,
    pub model_name: String,
    pub num_dimensions: usize,
    pub source_column: String,
}

impl EmbeddingColumnConfig {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

/// Information about a column including its type and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub embedding_config: Option<EmbeddingColumnConfig>,
}

/// A record in the sort history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortHistoryRecord {
    pub id: String,
    pub dataset_id: DatasetId,
    pub source_column: String,
    pub prompt: String,
    pub provider: String,
    pub model_name: String,
    pub num_dimensions: usize,
    pub executed_at: DateTime<Utc>,
}

impl SortHistoryRecord {
    pub fn new(
        dataset_id: DatasetId,
        source_column: String,
        prompt: String,
        provider: String,
        model_name: String,
        num_dimensions: usize,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            dataset_id,
            source_column,
            prompt,
            provider,
            model_name,
            num_dimensions,
            executed_at: Utc::now(),
        }
    }

    pub fn insert(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "INSERT INTO sort_history (id, dataset_id, source_column, prompt, provider, model_name, num_dimensions, executed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                self.id,
                self.dataset_id.as_str(),
                self.source_column,
                self.prompt,
                self.provider,
                self.model_name,
                self.num_dimensions as i64,
                self.executed_at.timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn load_for_dataset(conn: &Connection, dataset_id: &DatasetId) -> Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, source_column, prompt, provider, model_name, num_dimensions, executed_at
             FROM sort_history WHERE dataset_id = ? ORDER BY executed_at DESC"
        )?;

        let rows = stmt.query_map([dataset_id.as_str()], |row| {
            Ok(Self {
                id: row.get(0)?,
                dataset_id: DatasetId::from_str(&row.get::<_, String>(1)?).map_err(|e| {
                    duckdb::Error::FromSqlConversionFailure(
                        1,
                        duckdb::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?,
                source_column: row.get(2)?,
                prompt: row.get(3)?,
                provider: row.get(4)?,
                model_name: row.get(5)?,
                num_dimensions: row.get::<_, i64>(6)? as usize,
                executed_at: DateTime::from_timestamp(row.get(7)?, 0).unwrap_or_else(Utc::now),
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::init_session_schema;

    #[test]
    fn test_dataset_record_insert_and_load() {
        let conn = Connection::open_in_memory().unwrap();
        init_session_schema(&conn).unwrap();

        let id = DatasetId::new();
        let record = DatasetRecord::new(
            id.clone(),
            "Test Dataset".to_string(),
            SourceType::Csv,
            Some("/path/to/test.csv".to_string()),
        );

        record.insert(&conn).unwrap();

        let loaded = DatasetRecord::load(&conn, &id.as_str()).unwrap();
        assert_eq!(loaded.name, "Test Dataset");
        assert_eq!(loaded.source_type, SourceType::Csv);
    }

    #[test]
    fn test_dataset_record_update_stats() {
        let conn = Connection::open_in_memory().unwrap();
        init_session_schema(&conn).unwrap();

        let id = DatasetId::new();
        let mut record = DatasetRecord::new(id.clone(), "Test".to_string(), SourceType::Csv, None);

        record.insert(&conn).unwrap();
        record.update_stats(&conn, 1000, 10).unwrap();

        let loaded = DatasetRecord::load(&conn, &id.as_str()).unwrap();
        assert_eq!(loaded.row_count, Some(1000));
        assert_eq!(loaded.column_count, Some(10));
    }
}
