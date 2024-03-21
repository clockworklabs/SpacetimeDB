use std::{num::NonZeroU16, sync::Arc, time::Instant};

use futures::{pin_mut, stream::FuturesOrdered, StreamExt as _};
use log::{debug, info};
use spacetimedb_commitlog::{io_uring::Commitlog, payload, Options};
use tempfile::tempdir_in;

use super::gen_payload;

#[test]
fn smoke() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(None)
        .is_test(true)
        .try_init();

    let n_txs: usize = 10_000;

    let root = tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (committed_offset, n) = tokio_uring::builder().start({
        let root = root.path().to_path_buf();
        async move {
            let clog = Commitlog::open(
                &root,
                Options {
                    max_segment_size: 8 * 1024,
                    max_records_in_commit: NonZeroU16::MIN,
                    ..Options::default()
                },
            )
            .await
            .map(Arc::new)
            .unwrap();

            let payload = gen_payload();

            let start = Instant::now();
            let tasks = (0..n_txs)
                .map(|_| {
                    let clog = clog.clone();
                    async move { clog.append_maybe_flush(payload).await }
                })
                .collect::<FuturesOrdered<_>>();
            pin_mut!(tasks);
            while let Some(res) = tasks.next().await {
                res.unwrap();
            }
            let committed_offset = clog.flush_and_sync().await.unwrap();

            let elapsed = start.elapsed();
            info!("wrote {} txs in {}ms", n_txs, elapsed.as_millis());

            let start = Instant::now();
            let mut n = 0;
            {
                let iter = clog.transactions_from(0, &payload::ArrayDecoder);
                pin_mut!(iter);
                while let Some(tx) = iter.next().await {
                    let _ = tx.unwrap();
                    n += 1;
                }
            }
            let elapsed = start.elapsed();
            info!("read {} txs in {}ms", n, elapsed.as_millis());

            Arc::into_inner(clog).unwrap().close().await.unwrap();

            (committed_offset, n as usize)
        }
    });

    debug!("committed-offset={committed_offset:?} n={n}");
    assert_eq!(n_txs - 1, committed_offset.unwrap() as usize);
    assert_eq!(n_txs, n);
}
