use std::env;
use std::path::Path;

/// Returns the name of the current executable without the tail extension and the path.
pub fn get_exe_name() -> String {
    let exe_path = env::current_exe().expect("Failed to get executable path");
    let executable_name = Path::new(&exe_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    executable_name.to_string()
}
