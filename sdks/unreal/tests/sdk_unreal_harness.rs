use spacetimedb_testing::sdk::Test;
use std::env;
use std::path::{Path, PathBuf};

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

/// Reads the UE_ROOT_PATH environment variable
fn ue_root() -> PathBuf {
    let root = env::var("UE_ROOT_PATH")
        .expect("UE_ROOT_PATH not set — set to Unreal Engine root directory (no trailing slash)");
    PathBuf::from(root.replace('\\', "/"))
}

pub struct TestSuite {
    pub module: &'static str,
    pub client_root: &'static str,
    pub unreal_module: &'static str,
    pub uproject_file: &'static str,
}

pub fn make_test_with_suite(suite: &TestSuite, test_name: &str) -> Test {
    let build_script = ue_build_script();
    let editor_exe = ue_editor_exe();

    assert_existing_file("Unreal build script", &build_script);
    assert_existing_file("Unreal Editor executable", &editor_exe);

    let client_root = normalize_path(PathBuf::from(suite.client_root));
    let uproject_path = normalize_path(PathBuf::from(format!("{}/{}", client_root, suite.uproject_file)));
    assert_existing_file("uproject", &uproject_path);

    // Headless compile (no cook)
    let compile_command = if cfg!(target_os = "windows") {
        format!(
            "\"{}\" {}Editor Win64 Development \"{}\" -waitmutex -skipbuildengine",
            build_script, suite.unreal_module, uproject_path
        )
    } else {
        format!(
            "\"{}\" {}Editor Linux Development \"{}\" -skipbuildengine",
            build_script, suite.unreal_module, uproject_path
        )
    };

    // Run automation test
    let run_command = format!(
        "\"{}\" \"{}\" -NullRHI -Unattended -NoSound -nop4 -NoSplash -NoZen -ddc=InstalledNoZenLocalFallback -nocore -ExecCmds=\"Automation RunTests SpacetimeDB.{}.{}; Quit\"",
        editor_exe, uproject_path, suite.unreal_module, test_name
    );

    Test::builder()
        .with_name(test_name)
        .with_module(suite.module)
        .with_client(&client_root)
        .with_language(LANGUAGE)
        .with_unreal_module(suite.unreal_module)
        .with_compile_command(compile_command)
        .with_run_command(run_command)
        .build()
}
