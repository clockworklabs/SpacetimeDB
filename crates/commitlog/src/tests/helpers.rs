use std::fmt::Debug;

use env_logger::Env;

use crate::{
    commitlog,
    repo::{self, Repo},
    Encode, Options,
};

pub fn mem_log<T: Encode>(max_segment_size: u64) -> commitlog::Generic<repo::Memory, T> {
    commitlog::Generic::open(
        repo::Memory::new(),
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
    let mut total_txs = 0;
    for (_, n) in (0..num_commits).zip(txs_per_commit) {
        for _ in 0..n {
            log.append(T::default()).unwrap();
            total_txs += 1;
        }
        log.commit().unwrap();
    }

    total_txs
}

/// Put the `txes` into `log`.
///
/// Each TX from `txes` will be placed in its own commit within `log`.
pub fn fill_log_with<R, T>(log: &mut commitlog::Generic<R, T>, txes: impl IntoIterator<Item = T>)
where
    R: Repo,
    T: Debug + Encode,
{
    for tx in txes {
        log.append(tx).unwrap();
        log.commit().unwrap();
    }
}

pub fn enable_logging() {
    let _ = env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
        .format_timestamp(None)
        .is_test(true)
        .try_init();
}
