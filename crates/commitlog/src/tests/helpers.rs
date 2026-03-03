use std::fmt::Debug;

use env_logger::Env;

use crate::{
    commitlog,
    repo::{self, Repo},
    Encode, Options,
};

pub fn mem_log<T: Encode>(max_segment_size: u64) -> commitlog::Generic<repo::Memory, T> {
    commitlog::Generic::open(
        repo::Memory::unlimited(),
        Options {
            max_segment_size,
            ..Options::default()
        },
    )
    .unwrap()
}

pub fn fill_log<R, T>(
    log: &mut commitlog::Generic<R, T>,
    num_commits: usize,
    txs_per_commit: impl Iterator<Item = usize>,
) -> usize
where
    R: Repo,
    T: Debug + Default + Encode,
{
    let mut offset = log.max_committed_offset().map(|x| x + 1).unwrap_or_default();
    for (_, n) in (0..num_commits).zip(txs_per_commit) {
        log.commit((0..n).map(|i| (offset + i as u64, T::default())))
            .unwrap_or_else(|e| panic!("failed to commit offset {offset}: {e:#}"));
        offset += n as u64;
        log.flush().expect("failed to flush commitlog");
        log.sync();
    }

    offset as usize
}

/// Put the `txes` into `log`.
///
/// Each TX from `txes` will be placed in its own commit within `log`.
pub fn fill_log_with<R, T>(log: &mut commitlog::Generic<R, T>, txes: impl IntoIterator<Item = T>)
where
    R: Repo,
    T: Debug + Encode,
{
    for (i, tx) in txes.into_iter().enumerate() {
        log.commit([(i as u64, tx)])
            .unwrap_or_else(|e| panic!("failed to commit offset {i}: {e:#}"));
    }
    log.flush().expect("failed to flush commitlog");
    log.sync();
}

pub fn enable_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
        .format_timestamp(None)
        .is_test(true)
        .try_init();
}
