use std::path::Path;

pub mod lockfile;

pub fn create_parent_dir(file: &Path) -> Result<(), std::io::Error> {
    // If the path doesn't have a parent,
    // i.e. is a single-component path with just a root or is empty,
    // do nothing.
    let Some(parent) = file.parent() else {
        return Ok(());
    };

    // If the `file` path is a relative path with no directory component,
    // `parent` will be the empty path.
    // In this case, do not attempt to create a directory.
    if parent != Path::new("") {
        // If the `file` path has a directory component,
        // do `create_dir_all` to ensure it exists.
        // If `parent` already exists as a directory, this is a no-op.
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
