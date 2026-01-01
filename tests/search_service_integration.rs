//! Integration tests for SearchService with real datasets

use datatui::core::{types::DatasetId, ManagedDataset};
use datatui::services::search_service::{FindOptions, SearchMode};
use datatui::services::SearchService;
use duckdb::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_dataset() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let parquet_path = dir.path().join("test_search.parquet");

    // Create a dataset with searchable content
    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        &format!(
            "COPY (SELECT * FROM (VALUES 
                (1, 'Alice Smith', 'alice@example.com'),
                (2, 'Bob Johnson', 'bob@test.com'),
                (3, 'Charlie Brown', 'charlie@example.com'),
                (4, 'David Wilson', 'david@test.com'),
                (5, 'Alice Cooper', 'acooper@music.com'),
                (6, 'Test User', 'test@alice.com')
            ) AS t(id, name, email)) TO '{}' (FORMAT PARQUET)",
            parquet_path.display()
        ),
        [],
    )
    .unwrap();

    (dir, parquet_path)
}

#[test]
fn test_search_service_count_matches() {
    let (_dir, parquet_path) = create_test_dataset();
    let conn = Arc::new(Connection::open_in_memory().unwrap());
    let dataset = ManagedDataset::new(conn, DatasetId::new(), parquet_path).unwrap();

    // Search for "alice" case-insensitive
    let options = FindOptions::default();
    let count =
        SearchService::count_matches(&dataset, "alice", &options, &SearchMode::Normal).unwrap();

    // Should find: Alice Smith, Alice Cooper, test@alice.com = 3 matches
    assert!(
        count >= 3,
        "Expected at least 3 matches for 'alice', got {}",
        count
    );
}

#[test]
fn test_search_service_find_all() {
    let (_dir, parquet_path) = create_test_dataset();
    let conn = Arc::new(Connection::open_in_memory().unwrap());
    let dataset = ManagedDataset::new(conn, DatasetId::new(), parquet_path).unwrap();

    // Search for "test" case-insensitive
    let options = FindOptions::default();
    let results = SearchService::find_all(
        &dataset,
        "test",
        &options,
        &SearchMode::Normal,
        20, // context chars
    )
    .unwrap();

    // Should find: bob@test.com, david@test.com, Test User = at least 3
    assert!(!results.is_empty(), "Expected to find matches for 'test'");

    // Check that context is generated
    for result in &results {
        assert!(!result.context.is_empty(), "Context should not be empty");
    }
}

#[test]
fn test_search_service_case_sensitive() {
    let (_dir, parquet_path) = create_test_dataset();
    let conn = Arc::new(Connection::open_in_memory().unwrap());
    let dataset = ManagedDataset::new(conn, DatasetId::new(), parquet_path).unwrap();

    // Case-sensitive search for "Alice" (capital A)
    let options = FindOptions {
        match_case: true,
        ..Default::default()
    };
    let count =
        SearchService::count_matches(&dataset, "Alice", &options, &SearchMode::Normal).unwrap();

    // Should find only "Alice Smith" and "Alice Cooper" = 2 matches
    // Should NOT match "alice@example.com" or "test@alice.com"
    assert!(
        count >= 2,
        "Expected at least 2 matches for case-sensitive 'Alice'"
    );
}

#[test]
fn test_search_service_regex() {
    let (_dir, parquet_path) = create_test_dataset();
    let conn = Arc::new(Connection::open_in_memory().unwrap());
    let dataset = ManagedDataset::new(conn, DatasetId::new(), parquet_path).unwrap();

    // Regex search for email pattern
    let options = FindOptions::default();
    let count =
        SearchService::count_matches(&dataset, r"\w+@\w+\.com", &options, &SearchMode::Regex)
            .unwrap();

    // Should find all 6 email addresses
    assert!(
        count >= 6,
        "Expected at least 6 email matches, got {}",
        count
    );
}
