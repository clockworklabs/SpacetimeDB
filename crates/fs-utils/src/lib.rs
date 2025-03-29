use rand::Rng;
use std::io::Write;
use std::path::Path;

pub mod compression;
pub mod dir_trie;
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
    if parent == Path::new("") {
        return Ok(());
    }

    // If the `file` path has a directory component,
    // do `create_dir_all` to ensure it exists.
    // If `parent` already exists as a directory, this is a no-op.
    std::fs::create_dir_all(parent)
}

pub fn atomic_write(file_path: &Path, data: String) -> anyhow::Result<()> {
    let mut temp_path = file_path.to_path_buf();
    let mut temp_file: std::fs::File;
    let mut rng = rand::thread_rng();
    loop {
        temp_path.set_extension(format!(".tmp{}", rng.gen::<u32>()));
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path);
        if let Ok(file) = opened {
            temp_file = file;
            break;
        }
    }
    temp_file.write_all(data.as_bytes())?;
    std::fs::rename(&temp_path, file_path)?;
    Ok(())
}
