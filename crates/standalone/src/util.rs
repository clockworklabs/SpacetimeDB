use std::env;
use std::path::Path;

/// If `allow_create` is set to true and the directory is missing, create it.
/// Otherwise, if `allow_create` is set to false
/// and the directory is missing an error is returned.
/// Otherwise if the directory does exist, do nothing.
pub fn create_dir_or_err(path: &str) -> anyhow::Result<()> {
    if !Path::new(path).is_dir() {
        log::info!("Creating directory {}", path);
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// If `allow_create` is set to true and the file (and parent directory) is missing, create it with
/// `contents`. Otherwise if the file doesn't exist and `allow_create` is set to false, an error is
/// returned. Otherwise if the file does exist, do nothing.
pub fn create_file_with_contents(path: &str, contents: &str) -> anyhow::Result<()> {
    create_dir_or_err(Path::new(path).parent().unwrap().to_str().unwrap())?;
    if !Path::new(path).is_file() {
        log::info!("Creating file {}", path);
        std::fs::write(path, contents)?;
    }
    Ok(())
}

/// Returns the name of the current executable without the tail extension and the path.
pub fn get_exe_name() -> String {
    let exe_path = env::current_exe().expect("Failed to get executable path");
    let executable_name = Path::new(&exe_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    executable_name.to_string()
}
