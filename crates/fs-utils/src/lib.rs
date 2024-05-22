use anyhow::Context;
use std::path::Path;

pub mod lockfile;

pub fn create_parent_dir(file: &Path) -> anyhow::Result<()> {
    let parent = file
        .parent()
        .with_context(|| format!("Cannot find the parent directory of path {file:?}"))?;

    // If the `file` path is a relative path with no directory component,
    // `parent` will be the empty path.
    // In this case, do not attempt to create a directory.
    if parent != Path::new("") {
        // If the `file` path has a directory component,
        // do `create_dir_all` to ensure it exists.
        // If `parent` already exists as a directory, this is a no-op.
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory structure {parent:?} to contain {file:?}"))?;
    }
    Ok(())
}
