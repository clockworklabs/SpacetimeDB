//! Demonstrates the crash behaviour of `spacetimedb_durability::Local`
//! if the `fallocate` feature is enabled and when there is not enough disk
//! space to pre-allocate commitlog segments.
//!
//! Requires `target_os = "linux"`.
//!
//! The setup involves mounting a file as a loop device. For this, it invokes
//! the `mount`, `umount` and `chmod` commands via `sudo`. The caller must
//! ensure that they have the appropriate entries in `sudoers(5)` to do that
//! without `sudo` prompting for a password. For example:
//!
//! ```ignore
//! %sudo   ALL=(ALL)   NOPASSWD:    /usr/bin/mount, /usr/bin/umount, /usr/bin/chmod
//! ```
//!
//! The `fallocate` feature is not enabled by default. To run, use:
//!
//! ```ignore
//! cargo test --features fallocate
//! ```
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context as _};
use log::{error, info};
use scopeguard::ScopeGuard;
use spacetimedb_commitlog::{
    payload::txdata::{Mutations, Ops},
    repo::{self, OnNewSegmentFn, Repo},
    segment,
    tests::helpers::enable_logging,
};
use spacetimedb_durability::{Durability, Txdata};
use spacetimedb_paths::{
    server::{CommitLogDir, ReplicaDir},
    FromPathUnchecked,
};
use tempfile::{NamedTempFile, TempDir};
use tokio::{sync::watch, time::sleep};

const MB: u64 = 1024 * 1024;

#[tokio::test]
async fn local_durability_cannot_be_created_if_not_enough_space() -> anyhow::Result<()> {
    enable_logging();

    let Tmp {
        device_file,
        mountpoint,
    } = Tmp::create()?;
    {
        let file_path = device_file.path();
        let mountpoint = mountpoint.path();

        let _guard = mount(file_path, mountpoint, 512 * MB)?;
        let replica_dir = ReplicaDir::from_path_unchecked(mountpoint);

        match local_durability(replica_dir.commit_log(), 1024 * MB, None).await {
            Err(e) if e.kind() == io::ErrorKind::StorageFull => Ok(()),
            Err(e) => Err(e).context("unexpected error"),
            Ok(durability) => {
                durability.close().await?;
                Err(anyhow!("unexpected success"))
            }
        }
    }
}

// NOTE: This test is set up to proceed more or less sequentially.
// In reality, `append_tx` will fail at some point in the future.
// I.e. transactions can be lost when the host runs out of disk space.
#[tokio::test]
#[should_panic = "durability actor crashed"]
async fn local_durability_crashes_on_new_segment_if_not_enough_space() {
    enable_logging();

    // Inner run fn to allow the use of `?`,
    // `should_panic` tests must return unit.
    async fn run() -> anyhow::Result<()> {
        let Tmp {
            device_file,
            mountpoint,
        } = Tmp::create()?;
        {
            let _guard = mount(device_file.path(), mountpoint.path(), 512 * MB)?;
            let replica_dir = ReplicaDir::from_path_unchecked(mountpoint.path());

            let (new_segment_tx, mut new_segment_rx) = watch::channel(());
            let on_new_segment = Arc::new(move || {
                new_segment_tx.send_replace(());
            });
            let durability = local_durability(replica_dir.commit_log(), 256 * MB, Some(on_new_segment)).await?;
            let txdata = txdata();

            // Mark initial segment as seen.
            new_segment_rx.borrow_and_update();
            // Write past available space.
            for _ in 0..256 {
                durability.append_tx(txdata.clone());
            }
            // Ensure new segment is created.
            new_segment_rx.changed().await?;
            // Yield to give fallocate a chance to run (and fail).
            sleep(Duration::from_millis(5)).await;
            // Durability actor should have crashed, so this should panic.
            info!("trying append on crashed durability");
            durability.append_tx(txdata.clone());
        }

        Ok(())
    }

    run().await.unwrap()
}

/// Approximates the case where a commitlog has segments that were created
/// without `fallocate`.
///
/// Resuming a segment when there is insufficient space should fail.
#[tokio::test]
async fn local_durability_crashes_on_resume_with_insuffient_space() -> anyhow::Result<()> {
    enable_logging();

    let Tmp {
        device_file,
        mountpoint,
    } = Tmp::create()?;
    {
        let _guard = mount(device_file.path(), mountpoint.path(), 512 * MB)?;
        let replica_dir = ReplicaDir::from_path_unchecked(mountpoint.path());

        // Write a segment with only a header and no `fallocate` reservation.
        {
            let repo = repo::Fs::new(replica_dir.commit_log(), None)?;
            let mut segment = repo.create_segment(0)?;
            segment::Header::default().write(&mut segment)?;
            segment.sync_data()?;
        }

        // Try to open local durability with a 1GiB segment size,
        // which is larger than the available disk space.
        match local_durability(replica_dir.commit_log(), 1024 * MB, None).await {
            Err(e) if e.kind() == io::ErrorKind::StorageFull => Ok(()),
            Err(e) => Err(e).context("unexpected error"),
            Ok(durability) => {
                durability.close().await?;
                Err(anyhow!("unexpected success"))
            }
        }
    }
}

async fn local_durability(
    dir: CommitLogDir,
    max_segment_size: u64,
    on_new_segment: Option<Arc<OnNewSegmentFn>>,
) -> io::Result<spacetimedb_durability::Local<[u8; 1024 * 1024]>> {
    spacetimedb_durability::Local::open(
        dir,
        tokio::runtime::Handle::current(),
        spacetimedb_durability::local::Options {
            commitlog: spacetimedb_commitlog::Options {
                max_segment_size,
                max_records_in_commit: 1.try_into().unwrap(),
                preallocate_segments: true,
                ..<_>::default()
            },
            ..<_>::default()
        },
        on_new_segment,
    )
}

fn txdata() -> Txdata<[u8; 1024 * 1024]> {
    Txdata {
        inputs: None,
        outputs: None,
        mutations: Some(Mutations {
            inserts: [Ops {
                table_id: 8000.into(),
                rowdata: Arc::new([[42u8; 1024 * 1024]]),
            }]
            .into(),
            deletes: [].into(),
            truncates: [].into(),
        }),
    }
}

struct Tmp {
    device_file: NamedTempFile,
    mountpoint: TempDir,
}

impl Tmp {
    fn create() -> io::Result<Self> {
        let device_file = tempfile::Builder::new().prefix("disk-").tempfile()?;
        let mountpoint = tempfile::Builder::new().prefix("mnt-").tempdir()?;

        Ok(Self {
            device_file,
            mountpoint,
        })
    }
}

fn mount(device_file: &Path, mountpoint: &Path, len: u64) -> anyhow::Result<ScopeGuard<PathBuf, impl FnOnce(PathBuf)>> {
    info!("creating empty file at {} with len {}", device_file.display(), len);
    {
        let file = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(device_file)?;
        file.set_len(len)?;
        file.sync_data()?;
    }

    info!("creating filesystem");
    process::Command::new("mkfs")
        .args(["-t", "ext4"])
        .arg(device_file)
        .status()
        .success()?;

    info!("mounting {} at {}", device_file.display(), mountpoint.display());
    sudo(|cmd| {
        cmd.args(["mount", "-t", "ext4", "-o", "loop"])
            .arg(device_file)
            .arg(mountpoint)
            .status()
    })
    .success()?;

    let guard = scopeguard::guard(mountpoint.to_path_buf(), |mountpoint| {
        if let Err(e) = umount(&mountpoint) {
            error!("failed to umount {}: {}", mountpoint.display(), e)
        }
    });

    sudo(|cmd| cmd.args(["chmod", "-R", "777"]).arg(mountpoint).status()).success()?;

    Ok(guard)
}

fn umount(mountpoint: &Path) -> io::Result<()> {
    sudo(|cmd| cmd.arg("umount").arg(mountpoint).status()).success()
}

fn sudo<T>(f: impl FnOnce(&mut process::Command) -> T) -> T {
    f(process::Command::new("sudo").arg("--non-interactive"))
}

trait ExitStatusExt {
    fn success(self) -> io::Result<()>;
}

impl ExitStatusExt for io::Result<process::ExitStatus> {
    fn success(self) -> io::Result<()> {
        let status = self?;
        match status.success() {
            true => Ok(()),
            false => Err(io::Error::from_raw_os_error(status.code().unwrap())),
        }
    }
}
