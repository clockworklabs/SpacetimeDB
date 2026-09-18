//! Files containing resolved values are opened without following links and
//! checked through the opened handle. Directory ownership is checked before
//! any access; progress rewrites inherit the same private directory policy.
#[cfg(unix)]
use anyhow::{ensure, Result};
#[cfg(unix)]
use std::{
    fs::{self, File},
    path::Path,
};

#[cfg(unix)]
pub(super) fn create_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)?;
    directory(path)
}

#[cfg(unix)]
pub(super) fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(metadata.is_dir(), "publication directory must be a private directory");
    private_metadata(&metadata)
}

#[cfg(unix)]
fn private_metadata(metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        metadata.uid() == rustix::process::geteuid().as_raw() && metadata.mode() & 0o077 == 0,
        "publication storage must be owned by the current user and inaccessible to other users"
    );
    Ok(())
}

#[cfg(unix)]
pub(super) fn verify_file(file: &File) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1,
        "publication storage must be a regular file without additional links"
    );
    private_metadata(&metadata)
}

#[cfg(unix)]
pub(super) fn file(path: &Path, create: bool, write: bool) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = File::options()
        .read(true)
        .write(write)
        .create_new(create)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    verify_file(&file)?;
    Ok(file)
}

/// Reject ancestors through which another user could replace a directory
/// component between checks and access. Root-owned sticky temporary roots are
/// safe: entries below them are themselves required to be root/current-owned.
#[cfg(unix)]
pub(super) fn pin_parents(path: &Path) -> Result<Vec<File>> {
    use std::os::unix::fs::MetadataExt;
    let absolute = std::path::absolute(path)?;
    for component in absolute.ancestors() {
        let metadata = fs::symlink_metadata(component)?;
        ensure!(
            metadata.uid() == 0 || metadata.uid() == rustix::process::geteuid().as_raw(),
            "publication path has an untrusted owner"
        );
        if metadata.is_dir() {
            ensure!(
                metadata.mode() & 0o022 == 0 || (metadata.uid() == 0 && metadata.mode() & 0o1000 != 0),
                "publication path has an untrusted writable ancestor"
            );
        } else {
            ensure!(metadata.is_symlink(), "invalid publication directory ancestor");
            // Check the resolved target as well as the named ancestor chain.
            pin_parents(&component.canonicalize()?)?;
        }
    }
    Ok(Vec::new())
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::{create_directory, directory, file, pin_parents, temporary, verify_file};
