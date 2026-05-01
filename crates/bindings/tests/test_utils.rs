use spacetimedb::spacetimedb_lib::RawModuleDef;
use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = test_utils_user, public)]
#[derive(Debug, PartialEq, Eq)]
pub struct TestUtilsUser {
    #[primary_key]
    id: u64,
    name: String,
}

#[reducer]
pub fn add_test_utils_user(_ctx: &ReducerContext, id: u64, name: String) {
    let _ = (id, name);
}

#[test]
fn module_def_includes_native_test_registrations() {
    let mut table_names = spacetimedb::test_utils::all_table_names();
    table_names.sort_unstable();
    assert!(table_names.contains(&"test_utils_user"));

    let RawModuleDef::V10(module) = spacetimedb::test_utils::module_def() else {
        panic!("test utils should return a v10 raw module def");
    };

    let tables = module.tables().expect("tables section should be present");
    assert!(tables
        .iter()
        .any(|table| table.source_name.as_ref() == "test_utils_user"));

    let reducers = module.reducers().expect("reducers section should be present");
    assert!(reducers
        .iter()
        .any(|reducer| reducer.source_name.as_ref() == "add_test_utils_user"));
}

#[test]
fn test_datastore_initializes_from_native_test_registrations() {
    let datastore = spacetimedb::test_utils::TestDatastore::from_module_def(spacetimedb::test_utils::module_def())
        .expect("test datastore should initialize");

    assert!(datastore.table_id("test_utils_user").is_ok());
}

#[test]
fn test_context_supports_basic_table_insert_and_iter() {
    let ctx = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let table = ctx.db.test_utils_user();

    let row = TestUtilsUser {
        id: 1,
        name: "Ada".to_owned(),
    };

    assert_eq!(table.count(), 0);
    assert_eq!(
        table.insert(row),
        TestUtilsUser {
            id: 1,
            name: "Ada".to_owned(),
        }
    );
    assert_eq!(table.count(), 1);
    assert_eq!(
        table.iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 1,
            name: "Ada".to_owned(),
        }]
    );
}
