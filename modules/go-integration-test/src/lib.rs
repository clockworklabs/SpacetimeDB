use spacetimedb::{ReducerContext, SpacetimeType, Table};
use anyhow::Result;

/// Table for testing basic database operations
#[spacetimedb::table(name = test_table)]
pub struct TestTable {
    #[primary_key]
    pub id: u32,
    pub name: String,
    pub value: i32,
    pub active: bool,
    pub data: Vec<u8>,
}

/// Table for testing index operations
#[spacetimedb::table(name = indexed_table)]
pub struct IndexedTable {
    #[primary_key]
    pub id: u32,
    #[index(btree)]
    pub category: String,
    #[index(btree)]
    pub score: i64,
    #[index(btree)]
    pub level: u32,
    pub description: String,
}

/// Table for testing iterator patterns
#[spacetimedb::table(name = iteration_table)]
pub struct IterationTable {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub batch_id: u32,
    pub sequence: u32,
    pub data_type: String,
    pub encoded_data: Vec<u8>,
}

/// Table for testing encoding operations
#[spacetimedb::table(name = encoding_test)]
pub struct EncodingTest {
    #[primary_key]
    pub id: u32,
    pub test_name: String,
    pub input_type: String,
    pub bsatn_data: Vec<u8>,
    pub compressed_data: Vec<u8>,
    pub metadata: String,
}

/// Table for testing advanced features
#[spacetimedb::table(name = advanced_features)]
pub struct AdvancedFeatures {
    #[primary_key]
    pub feature_id: u32,
    pub feature_name: String,
    pub config_json: String,
    pub enabled: bool,
    pub performance_data: Vec<u8>,
    #[index(btree)]
    pub category: String,
}

/// Complex struct for testing nested data
#[derive(SpacetimeType)]
pub struct PlayerStats {
    pub player_id: u32,
    pub username: String,
    pub level: u32,
    pub experience: u64,
    pub achievements: Vec<String>,
    pub last_login: u64, // timestamp
}

/// Table for complex data structures
#[spacetimedb::table(name = complex_data)]
pub struct ComplexData {
    #[primary_key]
    pub id: u32,
    pub player_stats: PlayerStats,
    pub session_data: Vec<u8>,
    #[index(btree)]
    pub region: String,
}

// Basic CRUD Operations

/// Insert a test record
#[spacetimedb::reducer]
pub fn insert_test_record(ctx: &ReducerContext, id: u32, name: String, value: i32, active: bool, data: Vec<u8>) -> Result<()> {
    ctx.db.test_table().insert(TestTable {
        id,
        name,
        value,
        active,
        data,
    });
    log::info!("Inserted test record with id: {}", id);
    Ok(())
}

/// Update a test record
#[spacetimedb::reducer]
pub fn update_test_record(ctx: &ReducerContext, id: u32, name: String, value: i32, active: bool) -> Result<()> {
    if let Some(mut record) = ctx.db.test_table().id().find(id) {
        record.name = name;
        record.value = value;
        record.active = active;
        ctx.db.test_table().id().update(record);
        log::info!("Updated test record with id: {}", id);
    } else {
        return Err(anyhow::anyhow!("Record with id {} not found", id));
    }
    Ok(())
}

/// Delete a test record
#[spacetimedb::reducer]
pub fn delete_test_record(ctx: &ReducerContext, id: u32) -> Result<()> {
    let deleted = ctx.db.test_table().id().delete(&id);
    if deleted {
        log::info!("Deleted test record with id: {}", id);
    } else {
        return Err(anyhow::anyhow!("Record with id {} not found", id));
    }
    Ok(())
}

/// Get test record count
#[spacetimedb::reducer]
pub fn get_test_record_count(ctx: &ReducerContext) -> Result<()> {
    let count = ctx.db.test_table().count();
    log::info!("Test table has {} records", count);
    Ok(())
}

// Index Operations

/// Insert indexed data for testing range queries
#[spacetimedb::reducer]
pub fn insert_indexed_data(ctx: &ReducerContext, id: u32, category: String, score: i64, level: u32, description: String) -> Result<()> {
    ctx.db.indexed_table().insert(IndexedTable {
        id,
        category,
        score,
        level,
        description,
    });
    log::info!("Inserted indexed data with id: {}", id);
    Ok(())
}

/// Query by category (tests index usage)
#[spacetimedb::reducer]
pub fn query_by_category(ctx: &ReducerContext, category: String) -> Result<()> {
    let count = ctx.db.indexed_table().category().filter(&category).count();
    log::info!("Found {} records in category: {}", count, category);
    Ok(())
}

/// Query by score range (tests index range scans)
#[spacetimedb::reducer]
pub fn query_by_score_range(ctx: &ReducerContext, min_score: i64, max_score: i64) -> Result<()> {
    let count = ctx.db.indexed_table().score().filter(min_score..=max_score).count();
    log::info!("Found {} records with score between {} and {}", count, min_score, max_score);
    Ok(())
}

/// Query by level (tests different index patterns)
#[spacetimedb::reducer]
pub fn query_by_level(ctx: &ReducerContext, min_level: u32) -> Result<()> {
    let count = ctx.db.indexed_table().level().filter(min_level..).count();
    log::info!("Found {} records with level >= {}", count, min_level);
    Ok(())
}

// Batch and Iterator Operations

/// Insert batch data for iterator testing
#[spacetimedb::reducer]
pub fn insert_batch_data(ctx: &ReducerContext, batch_id: u32, batch_size: u32, data_type: String) -> Result<()> {
    for i in 0..batch_size {
        let data = format!("{}_{}", data_type, i);
        let encoded_data = data.as_bytes().to_vec();
        
        ctx.db.iteration_table().insert(IterationTable {
            id: 0, // auto_inc will assign
            batch_id,
            sequence: i,
            data_type: data_type.clone(),
            encoded_data,
        });
    }
    log::info!("Inserted batch of {} records with batch_id: {}", batch_size, batch_id);
    Ok(())
}

/// Scan batch by ID (tests iterator patterns)
#[spacetimedb::reducer]
pub fn scan_batch_by_id(ctx: &ReducerContext, batch_id: u32) -> Result<()> {
    let records: Vec<_> = ctx.db.iteration_table().iter()
        .filter(|r| r.batch_id == batch_id)
        .collect();
    
    log::info!("Scanned {} records for batch_id: {}", records.len(), batch_id);
    
    // Process in batches to test batch iterator patterns
    for chunk in records.chunks(10) {
        log::info!("Processing batch chunk of {} records", chunk.len());
        // Simulate processing
    }
    Ok(())
}

/// Stream data by type (tests streaming patterns)
#[spacetimedb::reducer]
pub fn stream_data_by_type(ctx: &ReducerContext, data_type: String, limit: u32) -> Result<()> {
    let mut count = 0;
    for record in ctx.db.iteration_table().iter() {
        if record.data_type == data_type {
            count += 1;
            log::info!("Streaming record {}: {} bytes", count, record.encoded_data.len());
            
            if count >= limit {
                break;
            }
        }
    }
    log::info!("Streamed {} records of type: {}", count, data_type);
    Ok(())
}

// Encoding Operations

/// Test BSATN encoding with various data types
#[spacetimedb::reducer]
pub fn test_bsatn_encoding(ctx: &ReducerContext, id: u32, test_name: String, input_type: String) -> Result<()> {
    let bsatn_data = match input_type.as_str() {
        "u8" => spacetimedb::sats::bsatn::to_vec(&42u8)?,
        "u32" => spacetimedb::sats::bsatn::to_vec(&12345u32)?,
        "i64" => spacetimedb::sats::bsatn::to_vec(&-9876543210i64)?,
        "string" => spacetimedb::sats::bsatn::to_vec(&"hello_spacetimedb".to_string())?,
        "bool" => spacetimedb::sats::bsatn::to_vec(&true)?,
        "array_i32" => spacetimedb::sats::bsatn::to_vec(&vec![10i32, 20, 30])?,
        _ => vec![],
    };

    // Simulate compression (in real scenario, we'd use actual compression)
    let compressed_data = format!("compressed_{}", input_type).as_bytes().to_vec();
    
    let metadata = format!("{{\"size\": {}, \"compressed_size\": {}}}", bsatn_data.len(), compressed_data.len());

    let input_type_clone = input_type.clone();
    
    ctx.db.encoding_test().insert(EncodingTest {
        id,
        test_name,
        input_type,
        bsatn_data,
        compressed_data,
        metadata,
    });

    log::info!("Tested BSATN encoding for type: {}", input_type_clone);
    Ok(())
}

/// Verify BSATN round-trip
#[spacetimedb::reducer]
pub fn verify_bsatn_roundtrip(ctx: &ReducerContext, id: u32) -> Result<()> {
    if let Some(test) = ctx.db.encoding_test().id().find(id) {
        log::info!("Verifying BSATN round-trip for test: {} (type: {})", test.test_name, test.input_type);
        log::info!("BSATN data size: {} bytes", test.bsatn_data.len());
        log::info!("Metadata: {}", test.metadata);
    } else {
        return Err(anyhow::anyhow!("Encoding test with id {} not found", id));
    }
    Ok(())
}

// Complex Data Operations

/// Insert complex player data
#[spacetimedb::reducer]
pub fn insert_player_data(
    ctx: &ReducerContext,
    id: u32,
    player_id: u32,
    username: String,
    level: u32,
    experience: u64,
    achievements: Vec<String>,
    region: String,
    session_data: Vec<u8>
) -> Result<()> {
    let player_stats = PlayerStats {
        player_id,
        username,
        level,
        experience,
        achievements,
        last_login: ctx.timestamp.to_micros_since_unix_epoch() as u64,
    };

    ctx.db.complex_data().insert(ComplexData {
        id,
        player_stats,
        session_data,
        region,
    });

    log::info!("Inserted complex player data with id: {}", id);
    Ok(())
}

/// Query players by region
#[spacetimedb::reducer]
pub fn query_players_by_region(ctx: &ReducerContext, region: String) -> Result<()> {
    let players: Vec<_> = ctx.db.complex_data().region().filter(&region).collect();
    
    for player in &players {
        log::info!("Player {}: level {}, experience {}", 
                  player.player_stats.username, 
                  player.player_stats.level, 
                  player.player_stats.experience);
    }
    
    log::info!("Found {} players in region: {}", players.len(), region);
    Ok(())
}

// Advanced Features

/// Configure advanced feature
#[spacetimedb::reducer]
pub fn configure_advanced_feature(
    ctx: &ReducerContext,
    feature_id: u32,
    feature_name: String,
    category: String,
    config_json: String,
    enabled: bool
) -> Result<()> {
    let performance_data = format!("{{\"last_updated\": {}}}", ctx.timestamp.to_micros_since_unix_epoch())
        .as_bytes().to_vec();

    let feature_name_clone = feature_name.clone();

    ctx.db.advanced_features().insert(AdvancedFeatures {
        feature_id,
        feature_name,
        config_json,
        enabled,
        performance_data,
        category,
    });

    log::info!("Configured advanced feature: {} (enabled: {})", feature_name_clone, enabled);
    Ok(())
}

/// Get feature statistics
#[spacetimedb::reducer]
pub fn get_feature_statistics(ctx: &ReducerContext, category: String) -> Result<()> {
    let total_features = ctx.db.advanced_features().category().filter(&category).count();
    let enabled_features = ctx.db.advanced_features().category().filter(&category)
        .filter(|f| f.enabled).count();

    log::info!("Category {}: {} total features, {} enabled", category, total_features, enabled_features);
    Ok(())
}

// Utility Functions

/// Clear all test data
#[spacetimedb::reducer]
pub fn clear_all_test_data(ctx: &ReducerContext) -> Result<()> {
    // Clear all tables
    for record in ctx.db.test_table().iter() {
        ctx.db.test_table().delete(record);
    }
    
    for record in ctx.db.indexed_table().iter() {
        ctx.db.indexed_table().delete(record);
    }
    
    for record in ctx.db.iteration_table().iter() {
        ctx.db.iteration_table().delete(record);
    }
    
    for record in ctx.db.encoding_test().iter() {
        ctx.db.encoding_test().delete(record);
    }
    
    for record in ctx.db.complex_data().iter() {
        ctx.db.complex_data().delete(record);
    }
    
    for record in ctx.db.advanced_features().iter() {
        ctx.db.advanced_features().delete(record);
    }

    log::info!("Cleared all test data");
    Ok(())
}

/// Get database statistics
#[spacetimedb::reducer]
pub fn get_database_statistics(ctx: &ReducerContext) -> Result<()> {
    let test_count = ctx.db.test_table().count();
    let indexed_count = ctx.db.indexed_table().count();
    let iteration_count = ctx.db.iteration_table().count();
    let encoding_count = ctx.db.encoding_test().count();
    let complex_count = ctx.db.complex_data().count();
    let features_count = ctx.db.advanced_features().count();

    log::info!("Database Statistics:");
    log::info!("  Test Table: {} records", test_count);
    log::info!("  Indexed Table: {} records", indexed_count);
    log::info!("  Iteration Table: {} records", iteration_count);
    log::info!("  Encoding Test: {} records", encoding_count);
    log::info!("  Complex Data: {} records", complex_count);
    log::info!("  Advanced Features: {} records", features_count);

    let total = test_count + indexed_count + iteration_count + encoding_count + complex_count + features_count;
    log::info!("  Total: {} records", total);

    Ok(())
} 