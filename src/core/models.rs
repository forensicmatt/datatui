use crate::core::types::{DatasetId, SourceType};
use chrono::{DateTime, Utc};
use color_eyre::Result;
use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Represents a dataset record in the workspace database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub id: DatasetId,
    pub name: String,
    pub source_type: SourceType,
    pub source_path: Option<String>,
    pub parquet_path: String,
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
        parquet_path: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            source_type,
            source_path,
            parquet_path,
            created_at: now,
            last_modified: now,
            row_count: None,
            column_count: None,
        }
    }

    /// Insert this record into the database
    pub fn insert(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "INSERT INTO datasets (id, name, source_type, source_path, parquet_path, created_at, last_modified, row_count, column_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                self.id.as_str(),
                &self.name,
                &self.source_type.to_string(),
                &self.source_path,
                &self.parquet_path,
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
            "SELECT id, name, source_type, source_path, parquet_path, created_at, last_modified, row_count, column_count
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
                parquet_path: row.get(4)?,
                created_at: DateTime::from_timestamp(row.get(5)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        5,
                        "created_at".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                last_modified: DateTime::from_timestamp(row.get(6)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        6,
                        "last_modified".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                row_count: row.get::<_, Option<i64>>(7)?.map(|n| n as u64),
                column_count: row.get::<_, Option<i32>>(8)?.map(|n| n as u32),
            })
        })?;

        Ok(record)
    }

    /// Load all dataset records
    pub fn load_all(conn: &Connection) -> Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, source_type, source_path, parquet_path, created_at, last_modified, row_count, column_count
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
                parquet_path: row.get(4)?,
                created_at: DateTime::from_timestamp(row.get(5)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        5,
                        "created_at".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                last_modified: DateTime::from_timestamp(row.get(6)?, 0).ok_or_else(|| {
                    duckdb::Error::InvalidColumnType(
                        6,
                        "last_modified".to_string(),
                        duckdb::types::Type::Null,
                    )
                })?,
                row_count: row.get::<_, Option<i64>>(7)?.map(|n| n as u64),
                column_count: row.get::<_, Option<i32>>(8)?.map(|n| n as u32),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::init_workspace_schema;

    #[test]
    fn test_dataset_record_insert_and_load() {
        let conn = Connection::open_in_memory().unwrap();
        init_workspace_schema(&conn).unwrap();

        let id = DatasetId::new();
        let record = DatasetRecord::new(
            id.clone(),
            "Test Dataset".to_string(),
            SourceType::Csv,
            Some("/path/to/test.csv".to_string()),
            "/path/to/test.parquet".to_string(),
        );

        record.insert(&conn).unwrap();

        let loaded = DatasetRecord::load(&conn, &id.as_str()).unwrap();
        assert_eq!(loaded.name, "Test Dataset");
        assert_eq!(loaded.source_type, SourceType::Csv);
    }

    #[test]
    fn test_dataset_record_update_stats() {
        let conn = Connection::open_in_memory().unwrap();
        init_workspace_schema(&conn).unwrap();

        let id = DatasetId::new();
        let mut record = DatasetRecord::new(
            id.clone(),
            "Test".to_string(),
            SourceType::Csv,
            None,
            "/test.parquet".to_string(),
        );

        record.insert(&conn).unwrap();
        record.update_stats(&conn, 1000, 10).unwrap();

        let loaded = DatasetRecord::load(&conn, &id.as_str()).unwrap();
        assert_eq!(loaded.row_count, Some(1000));
        assert_eq!(loaded.column_count, Some(10));
    }
}
