#![cfg(feature = "test-utils")]

use spacetimedb::spacetimedb_lib::{db::raw_def::v10::TableCachePolicy, RawModuleDef};
use spacetimedb::table;

#[table(accessor = memory_table)]
pub struct MemoryRow {
    id: u64,
}

#[table(accessor = disk_table, cache_policy = required_on_disk)]
pub struct DiskRow {
    id: u64,
}

#[table(accessor = object_table, cache_policy = no_requirement)]
pub struct ObjectRow {
    id: u64,
}

#[test]
fn declarations_emit_only_nondefault_cache_policies() {
    let RawModuleDef::V10(module) = spacetimedb::rt::module_def_for_tests() else {
        panic!("Rust bindings should emit V10 metadata");
    };
    let policies: Vec<_> = module
        .table_cache_policies()
        .map(|entry| (entry.table_name.as_ref(), entry.policy))
        .collect();
    assert_eq!(policies.len(), 2);
    assert!(policies.contains(&("disk_table", TableCachePolicy::RequiredOnDisk)));
    assert!(policies.contains(&("object_table", TableCachePolicy::NoRequirement)));
}
