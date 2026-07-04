use std::io::{ErrorKind, Write};
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
    atomic_write_bytes(file_path, data.as_bytes())
}

/// Atomically write `data` to `file_path`.
///
/// The bytes are written to a same-directory temporary file, fsynced, renamed
/// over `file_path`, and followed by a best-effort parent directory sync on
/// platforms that support opening directories as files.
pub fn atomic_write_bytes(file_path: &Path, data: &[u8]) -> anyhow::Result<()> {
    const ATOMIC_WRITE_MAX_TEMP_ATTEMPTS: u32 = 1024;

    let mut temp_path = file_path.to_path_buf();
    for _ in 0..ATOMIC_WRITE_MAX_TEMP_ATTEMPTS {
        temp_path.set_extension(format!(".tmp{}", rand::random::<u32>()));
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path);
        match opened {
            Ok(file) => {
                let mut temp_file = file;
                let write_result = (|| -> anyhow::Result<()> {
                    temp_file.write_all(data)?;
                    temp_file.sync_all()?;
                    drop(temp_file);
                    std::fs::rename(&temp_path, file_path)?;
                    if let Some(parent) = non_empty_parent(file_path) {
                        sync_dir(parent)?;
                    }
                    Ok(())
                })();
                if write_result.is_err() {
                    let _ = std::fs::remove_file(&temp_path);
                }
                return write_result;
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err.into()),
        }
    }
    anyhow::bail!(
        "failed to create a temporary file for atomic write after repeated name collisions: {}",
        file_path.display()
    )
}

fn non_empty_parent(path: &Path) -> Option<&Path> {
    path.parent().filter(|parent| !parent.as_os_str().is_empty())
}

/// Best-effort directory sync.
///
/// On Unix, opening and syncing a directory persists directory-entry updates
/// such as file creation and rename. On platforms where directories cannot be
/// opened this is a no-op, so callers still get durable file contents without
/// losing Windows compatibility.
pub fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Recursively create `path` and best-effort sync its parent directory.
pub fn create_dir_all_sync(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    if let Some(parent) = non_empty_parent(path) {
        sync_dir(parent)?;
    }
    Ok(())
}

/// Copy a file and fsync the destination file before returning.
pub fn copy_file_sync(src: &Path, dst: &Path) -> std::io::Result<u64> {
    if let Some(parent) = non_empty_parent(dst) {
        std::fs::create_dir_all(parent)?;
    }
    let copied = std::fs::copy(src, dst)?;
    std::fs::OpenOptions::new().write(true).open(dst)?.sync_all()?;
    if let Some(parent) = non_empty_parent(dst) {
        sync_dir(parent)?;
    }
    Ok(copied)
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
    create_dir_all_sync(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(entry.path(), &dst)?;
        } else {
            copy_file_sync(&entry.path(), &dst)
                .with_context(|| format!("copying {} to {}", entry.path().display(), dst.display()))?;
        }
    }
    sync_dir(dst).with_context(|| format!("syncing {}", dst.display()))?;
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

    #[test]
    fn atomic_write_bytes_replaces_file_without_leaving_temps() {
        let dir = tempdir::TempDir::new("atomic-write").unwrap();
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, b"old").unwrap();

        atomic_write_bytes(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let leftover_temp = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".tmp"));
        assert!(!leftover_temp, "atomic write left a temp file behind");
    }

    #[test]
    fn atomic_write_bytes_returns_error_for_missing_parent() {
        let dir = tempdir::TempDir::new("atomic-write").unwrap();
        let path = dir.path().join("missing/manifest.json");

        let err = atomic_write_bytes(&path, b"new").unwrap_err();

        let io_err = err
            .downcast_ref::<std::io::Error>()
            .expect("expected missing parent to return the original io error");
        assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
        assert!(!path.exists());
    }

    #[test]
    fn copy_dir_all_syncs_nested_file_contents() {
        let src = tempdir::TempDir::new("copy-src").unwrap();
        let dst = tempdir::TempDir::new("copy-dst").unwrap();
        std::fs::create_dir_all(src.path().join("nested")).unwrap();
        std::fs::write(src.path().join("nested/file"), b"contents").unwrap();

        copy_dir_all(src.path(), dst.path().join("out")).unwrap();

        assert_eq!(std::fs::read(dst.path().join("out/nested/file")).unwrap(), b"contents");
    }
}
