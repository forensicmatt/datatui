use color_eyre::Result;
use duckdb::Connection;

/// Initialize the global database schema
///
/// This database stores user-level configuration and history
pub fn init_global_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at BIGINT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS recent_files (
            path TEXT PRIMARY KEY,
            file_type TEXT NOT NULL,
            last_opened BIGINT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS query_history (
            id TEXT PRIMARY KEY,
            query TEXT NOT NULL,
            workspace_path TEXT,
            dataset_id TEXT,
            executed_at BIGINT NOT NULL,
            execution_time_ms BIGINT,
            row_count BIGINT,
            success BOOLEAN NOT NULL,
            error_message TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_query_history_time ON query_history(executed_at DESC);
        
        CREATE TABLE IF NOT EXISTS style_sets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            global BOOLEAN DEFAULT 1,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );
        
        CREATE TABLE IF NOT EXISTS style_rules (
            id TEXT PRIMARY KEY,
            style_set_id TEXT NOT NULL,
            priority INTEGER NOT NULL,
            condition_type TEXT NOT NULL,
            condition_value TEXT,
            scope_type TEXT NOT NULL,
            scope_value TEXT,
            foreground_color TEXT,
            background_color TEXT,
            modifiers TEXT,
            FOREIGN KEY (style_set_id) REFERENCES style_sets(id)
        );
        "#,
    )?;

    Ok(())
}

/// Initialize the session database schema
///
/// This database stores session-specific metadata and data tables for all imported datasets
pub fn init_session_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS datasets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_path TEXT,
            created_at BIGINT NOT NULL,
            last_modified BIGINT NOT NULL,
            row_count BIGINT,
            column_count INTEGER
        );
        
        CREATE TABLE IF NOT EXISTS dataset_filters (
            dataset_id TEXT PRIMARY KEY,
            filter_expr TEXT NOT NULL,
            applied_at BIGINT NOT NULL,
            FOREIGN KEY (dataset_id) REFERENCES datasets(id)
        );
        
        CREATE TABLE IF NOT EXISTS dataset_sorts (
            dataset_id TEXT PRIMARY KEY,
            sort_columns TEXT NOT NULL,
            applied_at BIGINT NOT NULL,
            FOREIGN KEY (dataset_id) REFERENCES datasets(id)
        );
        
        CREATE TABLE IF NOT EXISTS tabs (
            id TEXT PRIMARY KEY,
            dataset_id TEXT NOT NULL,
            display_order INTEGER NOT NULL,
            scroll_position_row INTEGER DEFAULT 0,
            scroll_position_col INTEGER DEFAULT 0,
            active BOOLEAN DEFAULT 0,
            FOREIGN KEY (dataset_id) REFERENCES datasets(id)
        );

        CREATE TABLE IF NOT EXISTS column_metadata (
            dataset_id TEXT NOT NULL,
            column_name TEXT NOT NULL,
            metadata_key TEXT NOT NULL,
            metadata_value TEXT NOT NULL,
            PRIMARY KEY (dataset_id, column_name, metadata_key),
            FOREIGN KEY (dataset_id) REFERENCES datasets(id)
        );

        CREATE TABLE IF NOT EXISTS sort_history (
            id TEXT PRIMARY KEY,
            dataset_id TEXT NOT NULL,
            source_column TEXT NOT NULL,
            prompt TEXT NOT NULL,
            provider TEXT NOT NULL,
            model_name TEXT NOT NULL,
            num_dimensions INTEGER NOT NULL,
            executed_at BIGINT NOT NULL,
            FOREIGN KEY (dataset_id) REFERENCES datasets(id)
        );
        "#,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_schema_initialization() {
        let conn = Connection::open_in_memory().unwrap();
        init_global_schema(&conn).unwrap();

        // Verify tables exist
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(table_count >= 4, "Expected at least 4 tables");
    }

    #[test]
    fn test_session_schema_initialization() {
        let conn = Connection::open_in_memory().unwrap();
        init_session_schema(&conn).unwrap();

        // Verify datasets table exists
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='datasets'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(exists, 1);
    }
}
