use serial_test::serial;
use spacetimedb_testing::sdk::Test;
use std::env;
use std::path::{Path, PathBuf};

const MODULE: &str = "sdk-test"; // Spacetime module name in SpacetimeDB/modules
const UNREAL_MODULE: &str = "TestClient"; // Unreal C++ module target for codegen
const CLIENT_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/TestClient");
const UPROJECT_FILE: &str = "TestClient.uproject";
const LANGUAGE: &str = "unrealcpp"; // Language for SpacetimeDB codegen

/// Panics if the file does not exist
fn assert_existing_file<P: AsRef<Path>>(label: &str, path: P) {
    let path = path.as_ref();
    match path.try_exists() {
        Ok(true) => {
            if !path.is_file() {
                panic!("{} exists but is not a file: {}", label, path.display());
            }
        }
        Ok(false) => panic!("{} does not exist: {}", label, path.display()),
        Err(e) => panic!("Failed to check {} ({}): {}", label, path.display(), e),
    }
}

/// Converts a PathBuf to a forward-slash string
fn normalize_path(path: PathBuf) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Returns full path to Unreal Editor executable
fn ue_editor_exe() -> String {
    let root = ue_root();
    let path = if cfg!(target_os = "windows") {
        root.join("Engine/Binaries/Win64/UnrealEditor.exe")
    } else {
        root.join("Engine/Binaries/Linux/UnrealEditor")
    };
    normalize_path(path)
}

/// Returns full path to Unreal Build script (Build.bat or Build.sh)
fn ue_build_script() -> String {
    let root = ue_root();
    let path = if cfg!(target_os = "windows") {
        root.join("Engine/Build/BatchFiles/Build.bat")
    } else {
        root.join("Engine/Build/BatchFiles/Linux/Build.sh")
    };
    normalize_path(path)
}

/// Reads the UE_ROOT_PATH environment variable to find the Unreal Editor directory.
///
/// Errors if UE_ROOT_PATH is not set — must point to the full path of Editor direcory path.
/// Example: "C:/Program Files/Epic Games/UE_5.6"
fn ue_root() -> PathBuf {
    let root = env::var("UE_ROOT_PATH")
        .expect("UE_ROOT_PATH not set — set to Unreal Engine root directory (no trailing slash)");
    PathBuf::from(root.replace('\\', "/"))
}

fn make_test(test_name: &str) -> Test {
    let build_script = ue_build_script();
    let editor_exe = ue_editor_exe();

    assert_existing_file("Unreal build script", &build_script);
    assert_existing_file("Unreal Editor executable", &editor_exe);

    let client_root = normalize_path(PathBuf::from(CLIENT_ROOT));
    let uproject_path = normalize_path(PathBuf::from(format!("{client_root}/{UPROJECT_FILE}")));
    assert_existing_file("uproject", &uproject_path);

    // Headless compile (no cook)
    let compile_command = if cfg!(target_os = "windows") {
        format!(
            "\"{build_script}\" {UNREAL_MODULE}Editor Win64 Development \"{uproject_path}\" -waitmutex -skipbuildengine"
        )
    } else {
        format!("\"{build_script}\" {UNREAL_MODULE}Editor Linux Development \"{uproject_path}\" -skipbuildengine")
    };

    // Run automation test

    let run_command = format!(
		// Updated to -NoZen and -dcc=InstalledNoZenLocalFallback to stop Unreal from trying to install Zen Server in CI
		// This is failing during tests as each test tries to install Zen and create a race condition where two tests try to handle this at the same time
		// Zen Server and the Derived Cache seem like a good idea during tests but they were not designed with mutli-threaded tests in mind, it is suggested to allow each test to run in isolation
        "\"{editor_exe}\" \"{uproject_path}\" -NullRHI -Unattended -NoSound -nop4 -NoSplash -NoZen -ddc=InstalledNoZenLocalFallback -nocore -ExecCmds=\"Automation RunTests SpacetimeDB.TestClient.{test_name}; Quit\""
    );

    Test::builder()
        .with_name(test_name)
        // Spacetime DB module
        .with_module(MODULE)
        // For unrealcpp this is the .uproject root folder
        .with_client(&client_root)
        .with_language(LANGUAGE)
        // Unreal-only: required for spacetime generate --module-name
        .with_unreal_module(UNREAL_MODULE)
        .with_compile_command(compile_command)
        .with_run_command(run_command)
        .build()
}

// Below shows examples on how to use the serial and parallel attributes for tests

//#[test]
//#[serial]
//fn test_serial_one() {
//  // Do things
//}

//#[test]
//#[serial(something)]
//fn test_serial_one() {
//  // Do things
//}

//#[test]
//#[parallel]
//fn test_parallel_another() {
//  // Do parallel things
//}

//48 tests → 6 groups → 8 per group (where possible)
//Gouping added for performace
// ---------------- GROUP 1 ----------------

#[test]
#[serial(Group1)]
fn unreal_insert_primitive() {
    make_test("InsertPrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscribe_and_cancel() {
    make_test("SubscribeAndCancelTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscribe_and_unsubscribe() {
    make_test("SubscribeAndUnsubscribeTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_subscription_error_smoke_test() {
    make_test("SubscriptionErrorSmokeTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_delete_primitive() {
    make_test("DeletePrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_update_primitive() {
    make_test("UpdatePrimitiveTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_insert_identity() {
    make_test("InsertIdentityTest").run();
}

#[test]
#[serial(Group1)]
fn unreal_insert_caller_identity() {
    make_test("InsertCallerIdentityTest").run();
}

// ---------------- GROUP 2 ----------------
#[test]
#[serial(Group2)]
fn unreal_delete_identity() {
    make_test("DeleteIdentityTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_update_identity() {
    make_test("UpdateIdentityTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_connection_id() {
    make_test("InsertConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_caller_connection_id() {
    make_test("InsertCallerConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_delete_connection_id() {
    make_test("DeleteConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_update_connection_id() {
    make_test("UpdateConnectionIdTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_timestamp() {
    make_test("InsertTimestampTest").run();
}

#[test]
#[serial(Group2)]
fn unreal_insert_call_timestamp() {
    make_test("InsertCallTimestampTest").run();
}

// ---------------- GROUP 3 ----------------
#[test]
#[serial(Group3)]
fn unreal_on_reducer() {
    make_test("OnReducerTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_fail_reducer() {
    make_test("FailReducerTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_vec() {
    make_test("InsertVecTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_option_some() {
    make_test("InsertOptionSomeTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_option_none() {
    make_test("InsertOptionNoneTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_struct() {
    make_test("InsertStructTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_simple_enum() {
    make_test("InsertSimpleEnumTest").run();
}

#[test]
#[serial(Group3)]
fn unreal_insert_enum_with_payload() {
    make_test("InsertEnumWithPayloadTest").run();
}

// ---------------- GROUP 4 ----------------
#[test]
#[serial(Group4)]
fn unreal_insert_delete_large_table() {
    make_test("InsertDeleteLargeTableTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_insert_primitives_as_strings() {
    make_test("InsertPrimitivesAsStringsTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_reauth() {
    make_test("ReauthPart1Test").run();
    make_test("ReauthPart2Test").run();
}

#[test]
#[should_panic]
#[serial(Group4)]
fn unreal_should_fail() {
    make_test("ShouldFailTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_caller_always_notified() {
    make_test("CallerAlwaysNotifiedTest").run();
}

#[test]
#[serial(Group4)]
fn unreal_subscribe_all_select_star() {
    make_test("SubscribeAllSelectStarTest").run();
}

// ---------------- GROUP 5 ----------------
#[test]
#[serial(Group5)]
fn unreal_row_deduplication() {
    make_test("RowDeduplicationTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_row_deduplication_join_r_and_s() {
    make_test("RowDeduplicationJoinRAndSTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_row_deduplication_r_join_s_and_r_joint() {
    make_test("RowDeduplicationRJoinSAndRJoinTTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_lhs_join_update() {
    make_test("LhsJoinUpdateTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_lhs_join_update_disjoint_queries() {
    make_test("LhsJoinUpdateDisjointQueriesTest").run();
}

#[test]
#[serial(Group5)]
fn unreal_test_intra_query_bag_semantics_for_join() {
    make_test("IntraQueryBagSemanticsForJoinTest").run();
}

// ---------------- GROUP 6 ----------------
#[test]
#[serial(Group6)]
fn unreal_test_parameterized_subscription() {
    make_test("ParameterizedSubscriptionTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_test_rls_subscription() {
    make_test("RlsSubscriptionTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_pk_simple_enum() {
    make_test("PkSimpleEnumTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_indexed_simple_enum() {
    make_test("IndexedSimpleEnumTest").run();
}

#[test]
#[serial(Group6)]
fn unreal_overlapping_subscriptions() {
    make_test("OverlappingSubscriptionsTest").run();
}
