use datatui::core::JsonImportOptions;
use datatui::services::DataService;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing JSON import with Records attribute...\n");

    // Create a temp directory for the session
    let temp_dir = std::env::current_dir()?;
    let data_service = DataService::new(&temp_dir)?;

    // Test 1: Single file with Records attribute
    println!("Test 1: Loading single file with Records attribute");
    let options = JsonImportOptions::with_records_expr("Records");
    match data_service.import_json(PathBuf::from("testdata/records-001.json"), options.clone()) {
        Ok(dataset_id) => {
            let dataset = data_service.get_dataset(&dataset_id)?;
            println!(
                "✓ Success! Loaded {} rows, {} columns",
                dataset.row_count()?,
                dataset.column_count()?
            );
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
            return Err(e.into());
        }
    }

    // Test 2: Multiple files with glob pattern
    println!("\nTest 2: Loading multiple files with glob pattern");
    match data_service.import_json(PathBuf::from("testdata/records-*.json"), options) {
        Ok(dataset_id) => {
            let dataset = data_service.get_dataset(&dataset_id)?;
            println!(
                "✓ Success! Loaded {} rows from multiple files, {} columns",
                dataset.row_count()?,
                dataset.column_count()?
            );
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
            return Err(e.into());
        }
    }

    println!("\n✅ All tests passed!");
    Ok(())
}
