# Go Integration Test Module

This is a dedicated SpacetimeDB WASM module designed specifically for comprehensive integration testing of the Go bindings. It provides a complete set of tables and reducers to test all aspects of the Go database operations.

## Purpose

This module enables **real WASM integration testing** where:
- Go code calls actual WASM reducers
- Data is stored and retrieved through SpacetimeDB
- BSATN encoding/decoding is verified in practice
- All database managers (table, index, iterator, encoding) are tested with real WASM

## Tables

The module provides 6 specialized tables for different testing scenarios:

### 1. `test_table` - Basic CRUD Operations
```rust
pub struct TestTable {
    #[primary_key]
    pub id: u32,
    pub name: String,
    pub value: i32,
    pub active: bool,
    pub data: Vec<u8>,
}
```

### 2. `indexed_table` - Index Operations
```rust
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
```

### 3. `iteration_table` - Iterator and Batch Operations
```rust
pub struct IterationTable {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub batch_id: u32,
    pub sequence: u32,
    pub data_type: String,
    pub encoded_data: Vec<u8>,
}
```

### 4. `encoding_test` - BSATN Encoding Testing
```rust
pub struct EncodingTest {
    #[primary_key]
    pub id: u32,
    pub test_name: String,
    pub input_type: String,
    pub bsatn_data: Vec<u8>,
    pub compressed_data: Vec<u8>,
    pub metadata: String,
}
```

### 5. `complex_data` - Complex Data Structures
```rust
pub struct ComplexData {
    #[primary_key]
    pub id: u32,
    pub player_stats: PlayerStats,
    pub session_data: Vec<u8>,
    #[index(btree)]
    pub region: String,
}
```

### 6. `advanced_features` - Feature Configuration
```rust
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
```

## Reducers

The module provides 25+ reducers organized by functionality:

### Basic CRUD Operations
- `insert_test_record` - Insert test data
- `update_test_record` - Update existing record
- `delete_test_record` - Delete record by ID
- `get_test_record_count` - Get total record count

### Index Operations  
- `insert_indexed_data` - Insert data for index testing
- `query_by_category` - Test category index usage
- `query_by_score_range` - Test range queries on score index
- `query_by_level` - Test level-based queries

### Batch and Iterator Operations
- `insert_batch_data` - Insert batch of records for iteration testing
- `scan_batch_by_id` - Scan records by batch ID
- `stream_data_by_type` - Stream data by type with limits

### Encoding Operations
- `test_bsatn_encoding` - Test BSATN encoding for various types
- `verify_bsatn_roundtrip` - Verify round-trip encoding/decoding

### Complex Data Operations
- `insert_player_data` - Insert complex player structures
- `query_players_by_region` - Query complex data by region

### Advanced Features
- `configure_advanced_feature` - Configure feature settings
- `get_feature_statistics` - Get feature usage statistics

### Utility Functions
- `clear_all_test_data` - Clear all test data from all tables
- `get_database_statistics` - Get comprehensive database statistics

## Building the Module

To build this module:

```bash
cd SpacetimeDB
cargo build --release --target wasm32-unknown-unknown -p go-integration-test
```

The resulting WASM file will be at:
```
target/wasm32-unknown-unknown/release/go_integration_test.wasm
```

## Integration Test Usage

The module is used by the Go integration test at:
```
SpacetimeDB/crates/bindings-go/tests/go_integration_wasm_test.go
```

This test:
1. Loads the WASM module
2. Calls reducers to insert/update/delete data
3. Uses Go database managers to verify operations
4. Tests BSATN compatibility between Go and Rust
5. Validates all database manager functionality

## Test Coverage

The integration test covers:

- **✅ Table Operations**: CRUD operations, metadata, statistics
- **✅ Index Operations**: Creation, scanning, range queries, analysis
- **✅ Iterator Operations**: Table scans, batch processing, streaming
- **✅ Encoding Operations**: BSATN round-trips, compression
- **✅ Complex Data**: Nested structures, player stats
- **✅ Advanced Features**: Configuration, monitoring
- **✅ Error Handling**: Validation and edge cases

## Benefits

This integration testing approach provides:

1. **Real-World Validation**: Tests actual WASM ↔ Go interactions
2. **BSATN Compatibility**: Ensures Go and Rust BSATN are identical
3. **Complete Coverage**: Tests all database manager components
4. **Performance Verification**: Validates operations under real conditions
5. **Regression Prevention**: Catches issues early in development

## Example Usage

```go
// Run the integration test
func TestGoIntegration_WithDedicatedWASM(t *testing.T) {
    // Load the module
    wasmRuntime.LoadModule(ctx, wasmBytes)
    
    // Call reducers
    result, err := callWASMReducer(t, ctx, wasmRuntime, "insert_test_record", args)
    
    // Verify with Go managers
    stats, err := tableManager.GetTableStatistics(tableID)
    assert.Equal(t, uint64(1), stats.InsertCount)
}
```

This module is essential for ensuring the Go bindings work correctly with real SpacetimeDB WASM modules and provides confidence that the implementation is production-ready. 