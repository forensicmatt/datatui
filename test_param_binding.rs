// Quick test to understand DuckDB parameter binding with UNION ALL
use duckdb::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open_in_memory()?;

    // Create test table
    conn.execute(
        "CREATE TABLE test AS VALUES (1, 'apple'), (2, 'banana'), (3, 'cherry')",
        [],
    )?;

    println!("Test 1: Single query with ?");
    let mut stmt = conn.prepare("SELECT * FROM test WHERE column1 LIKE ?")?;
    let result = stmt.query_map([&"%a%"], |row| {
        let val: String = row.get(1)?;
        Ok(val)
    })?;
    for r in result {
        println!("  {}", r?);
    }

    println!("\nTest 2: UNION ALL with multiple ?");
    let query =
        "SELECT * FROM test WHERE column1 LIKE ? UNION ALL SELECT * FROM test WHERE column1 LIKE ?";
    let mut stmt = conn.prepare(query)?;
    let result = stmt.query_map([&"%a%", &"%e%"], |row| {
        let val: String = row.get(1)?;
        Ok(val)
    })?;
    for r in result {
        println!("  {}", r?);
    }

    println!("\nTest 3: UNION ALL with $1, $2");
    let query = "SELECT * FROM test WHERE column1 LIKE $1 UNION ALL SELECT * FROM test WHERE column1 LIKE $2";
    let mut stmt = conn.prepare(query)?;
    let result = stmt.query_map([&"%a%", &"%e%"], |row| {
        let val: String = row.get(1)?;
        Ok(val)
    })?;
    for r in result {
        println!("  {}", r?);
    }

    Ok(())
}
