use spacetimedb::{ReducerContext, Table};

/// Table to store the results of BSATN echo operations
#[spacetimedb::table(name = bsatn_test_result)]
pub struct BsatnTestResult {
    /// Unique identifier for the test result
    #[primary_key]
    pub id: u32,
    /// The test name/type
    pub test_name: String,
    /// The original input data (for verification)
    pub input_data: String,
    /// The BSATN-serialized data as bytes
    pub bsatn_data: Vec<u8>,
}

/// Reducer that takes a u8 value, serializes it to BSATN, and stores the result
#[spacetimedb::reducer]
pub fn echo_u8(ctx: &ReducerContext, id: u32, value: u8) {
    // Serialize the u8 value using BSATN
    let bsatn_data = spacetimedb::sats::bsatn::to_vec(&value).unwrap();
    
    // Store the result in the table
    ctx.db.bsatn_test_result().insert(BsatnTestResult {
        id,
        test_name: "echo_u8".to_string(),
        input_data: value.to_string(),
        bsatn_data,
    });
}

/// Reducer that takes two i32 values, serializes them as an array to BSATN, and stores the result
#[spacetimedb::reducer]
pub fn echo_vec2(ctx: &ReducerContext, id: u32, x: i32, y: i32) {
    // Create array and serialize using BSATN
    let array = [x, y];
    let bsatn_data = spacetimedb::sats::bsatn::to_vec(&array).unwrap();
    
    // Store the result in the table
    ctx.db.bsatn_test_result().insert(BsatnTestResult {
        id,
        test_name: "echo_vec2".to_string(), 
        input_data: format!("[{}, {}]", x, y),
        bsatn_data,
    });
}

/// Helper reducer to clear all test results (useful for test cleanup)
#[spacetimedb::reducer]
pub fn clear_results(ctx: &ReducerContext) {
    // Delete all entries from the table
    for result in ctx.db.bsatn_test_result().iter() {
        ctx.db.bsatn_test_result().delete(result);
    }
}

