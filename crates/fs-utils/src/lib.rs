use std::io::Write;
use std::path::{Component, Path, PathBuf};

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
    loop {
        temp_path.set_extension(format!(".tmp{}", rand::random::<u32>()));
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

/// Recursively compute the total size in bytes of all files under `path`.
pub fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut bytes = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            bytes += dir_size(&entry.path())?;
        } else {
            bytes += meta.len();
        }
    }
    Ok(bytes)
}

/// Recursively copy the directory `src` into `dst`, creating `dst` if necessary.
pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    use anyhow::Context as _;

    let src = src.as_ref();
    let dst = dst.as_ref();
    std::fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(entry.path(), &dst)?;
        } else {
            std::fs::copy(entry.path(), &dst)
                .with_context(|| format!("copying {} to {}", entry.path().display(), dst.display()))?;
        }
    }
    Ok(())
}

/// Normalize an absolute path that may not exist yet.
///
/// Canonicalizes the deepest existing ancestor of `path` — resolving any
/// symlinks in it — and re-appends the remaining, not-yet-existing components.
///
/// Errors if `path` is relative or contains `..` components, as those cannot
/// be resolved reliably for paths that do not exist yet.
pub fn normalize_absolute_path(path: &Path) -> anyhow::Result<PathBuf> {
    use anyhow::Context as _;

    anyhow::ensure!(path.is_absolute(), "path must be absolute: {}", path.display());
    anyhow::ensure!(
        !path.components().any(|c| matches!(c, Component::ParentDir)),
        "path must not contain `..` components: {}",
        path.display()
    );

    // Split off trailing components until we find an existing ancestor.
    // Use `symlink_metadata` so a dangling symlink counts as existing and
    // fails the subsequent `canonicalize` instead of being skipped over.
    let mut prefix = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while prefix.symlink_metadata().is_err() {
        match (prefix.file_name().map(|n| n.to_owned()), prefix.parent()) {
            (Some(name), Some(parent)) if parent != Path::new("") => {
                tail.push(name);
                prefix = parent.to_path_buf();
            }
            _ => anyhow::bail!("path has no existing ancestor: {}", path.display()),
        }
    }
    let mut normalized = prefix
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", prefix.display()))?;
    for name in tail.into_iter().rev() {
        normalized.push(name);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_absolute_path_rejects_relative_and_parent_components() {
        assert!(normalize_absolute_path(Path::new("relative/path")).is_err());
        let dir = tempdir::TempDir::new("normalize").unwrap();
        assert!(normalize_absolute_path(&dir.path().join("a/../b")).is_err());
    }

    #[test]
    fn normalize_absolute_path_resolves_nonexistent_suffix() {
        let dir = tempdir::TempDir::new("normalize").unwrap();
        let root = dir.path().canonicalize().unwrap();
        let normalized = normalize_absolute_path(&dir.path().join("does/not/exist")).unwrap();
        assert_eq!(normalized, root.join("does/not/exist"));
    }

    #[cfg(unix)]
    #[test]
    fn normalize_absolute_path_resolves_symlinks() {
        let dir = tempdir::TempDir::new("normalize").unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let normalized = normalize_absolute_path(&link.join("new")).unwrap();
        assert_eq!(normalized, target.canonicalize().unwrap().join("new"));
    }
}
