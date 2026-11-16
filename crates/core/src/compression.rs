//! Websocket compression CPU policy. Callers retain the existing encode
//! executor and release database transaction locks before compressing.
use bytes::Bytes;
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

const BROTLI_QUALITY: i32 = 2;
const BROTLI_MT_MIN_BYTES: usize = 512 * 1024;
const FOUR_THREADS_MIN_BYTES: usize = 64 * 1024 * 1024;
const ZSTD_LEVEL: i32 = -2;
const ZSTD_MT_MIN_BYTES: usize = 4 * 1024 * 1024;

// A process-wide, nonblocking budget, not one pool per connection. Saturation
// falls back to the same codec synchronously; never wait on an encode worker.
static MT_BUDGET: MtBudget = MtBudget(AtomicBool::new(false));
struct MtBudget(AtomicBool);
struct MtPermit<'a>(&'a MtBudget);

impl MtBudget {
    fn try_acquire(&self) -> Option<MtPermit<'_>> {
        self.0
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MtPermit(self))
    }
}

impl Drop for MtPermit<'_> {
    fn drop(&mut self) {
        let MtBudget(busy) = self.0;
        busy.store(false, Ordering::Release);
    }
}

fn thread_count(len: usize, minimum: usize, available: usize) -> usize {
    if len < minimum {
        return 1;
    }
    let desired = if len >= FOUR_THREADS_MIN_BYTES { 4 } else { 2 };
    desired.min(available.max(1))
}

fn available_threads() -> usize {
    // Codec children inherit affinity. A pinned encode worker cannot safely
    // assume access to the process's other reserved CPUs.
    if cfg!(feature = "core-pinning") {
        return 1;
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

fn brotli_params() -> brotli::enc::BrotliEncoderParams {
    brotli::enc::BrotliEncoderParams {
        quality: BROTLI_QUALITY,
        ..Default::default()
    }
}

pub(crate) fn brotli_compress(bytes: &[u8], out: &mut impl io::Write) {
    brotli::BrotliCompress(&mut &bytes[..], out, &brotli_params()).expect("should be able to BrotliCompress");
}

struct SharedInput(Bytes);
impl brotli::SliceWrapper<u8> for SharedInput {
    fn slice(&self) -> &[u8] {
        &self.0
    }
}

fn brotli_parallel(bytes: Bytes, threads: usize) -> io::Result<Vec<u8>> {
    use brotli::enc::{
        compress_multi, BrotliEncoderMaxCompressedSizeMulti, Owned, SendAlloc, StandardAlloc, UnionHasher,
    };
    let bound = BrotliEncoderMaxCompressedSizeMulti(bytes.len(), threads);
    let mut out = Vec::new();
    out.try_reserve_exact(bound).map_err(io::Error::other)?;
    out.resize(bound, 0);
    let mut input = Owned::new(SharedInput(bytes));
    let mut allocators: Vec<_> = (0..threads)
        .map(|_| SendAlloc::new(StandardAlloc::default(), UnionHasher::Uninit))
        .collect();
    let len = compress_multi(&brotli_params(), &mut input, &mut out, &mut allocators)
        .map_err(|err| io::Error::other(format!("Brotli compression: {err:?}")))?;
    out.truncate(len);
    Ok(out)
}

pub(crate) fn brotli_compress_owned(bytes: Bytes, out: &mut impl io::Write) {
    let threads = thread_count(bytes.len(), BROTLI_MT_MIN_BYTES, available_threads());
    if threads > 1
        && let Some(_permit) = MT_BUDGET.try_acquire()
    {
        match brotli_parallel(bytes.clone(), threads) {
            Ok(compressed) => {
                // Do not retain the MT API's maximum-sized allocation in the
                // connection's reusable serialization buffer.
                out.write_all(&compressed)
                    .expect("should be able to write Brotli output");
                return;
            }
            Err(err) => log::warn!("Parallel Brotli failed; retrying synchronously: {err}"),
        }
    }
    brotli_compress(&bytes, out);
}

fn zstd_worker_count(len: usize, available: usize) -> u32 {
    if len < ZSTD_MT_MIN_BYTES {
        return 0;
    }
    // Zstd counts additional workers, whereas Brotli counts all lanes.
    let desired = if len >= FOUR_THREADS_MIN_BYTES { 4 } else { 2 };
    desired.min(available.saturating_sub(1)) as u32
}

fn zstd_encode(bytes: &[u8], out: &mut impl io::Write, workers: u32) {
    use io::Write;
    let mut encoder = zstd::stream::write::Encoder::new(out, ZSTD_LEVEL).expect("valid Zstd level");
    encoder.include_checksum(true).expect("enable Zstd checksum");
    if workers > 0
        && let Err(err) = encoder.multithread(workers)
    {
        log::warn!("Parallel Zstd unavailable; using synchronous compression: {err}");
    }
    encoder.write_all(bytes).expect("should be able to encode Zstd");
    encoder.finish().expect("should be able to finish Zstd frame");
}

pub(crate) fn zstd_compress(bytes: &[u8], out: &mut impl io::Write) {
    let workers = zstd_worker_count(bytes.len(), available_threads());
    let permit = (workers > 0).then(|| MT_BUDGET.try_acquire()).flatten();
    zstd_encode(bytes, out, if permit.is_some() { workers } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn parallel_thresholds_and_cpu_cap() {
        assert_eq!(thread_count(BROTLI_MT_MIN_BYTES - 1, BROTLI_MT_MIN_BYTES, 8), 1);
        assert_eq!(thread_count(BROTLI_MT_MIN_BYTES, BROTLI_MT_MIN_BYTES, 8), 2);
        assert_eq!(thread_count(FOUR_THREADS_MIN_BYTES, BROTLI_MT_MIN_BYTES, 8), 4);
        assert_eq!(thread_count(FOUR_THREADS_MIN_BYTES, BROTLI_MT_MIN_BYTES, 2), 2);
        assert_eq!(thread_count(FOUR_THREADS_MIN_BYTES, BROTLI_MT_MIN_BYTES, 1), 1);
    }

    #[test]
    fn budget_is_nonblocking_and_released_on_unwind() {
        let budget = MtBudget(AtomicBool::new(false));
        let result = std::panic::catch_unwind(|| {
            let _permit = budget.try_acquire().unwrap();
            assert!(budget.try_acquire().is_none());
            panic!("test permit cleanup");
        });
        assert!(result.is_err());
        assert!(budget.try_acquire().is_some());
    }

    #[test]
    fn brotli_sync_and_parallel_roundtrip() {
        let input = Bytes::from((0..1024 * 1024).map(|i| (i % 251) as u8).collect::<Vec<_>>());
        let mut sync = Vec::new();
        brotli_compress(&input, &mut sync);
        for frame in [
            sync,
            brotli_parallel(input.clone(), 2).unwrap(),
            brotli_parallel(input.clone(), 4).unwrap(),
        ] {
            let mut decoded = Vec::new();
            brotli::Decompressor::new(frame.as_slice(), 4096)
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, input);
        }
        assert!(input.is_unique(), "MT must release all shared input references");
    }

    #[test]
    fn zstd_workers_respect_size_and_cpu_budget() {
        assert_eq!(zstd_worker_count(ZSTD_MT_MIN_BYTES - 1, 8), 0);
        assert_eq!(zstd_worker_count(ZSTD_MT_MIN_BYTES, 8), 2);
        assert_eq!(zstd_worker_count(FOUR_THREADS_MIN_BYTES, 8), 4);
        assert_eq!(zstd_worker_count(FOUR_THREADS_MIN_BYTES, 2), 1);
        assert_eq!(zstd_worker_count(FOUR_THREADS_MIN_BYTES, 1), 0);
        assert_eq!(zstd_worker_count(FOUR_THREADS_MIN_BYTES, 0), 0);
    }

    #[cfg(feature = "core-pinning")]
    #[test]
    fn pinned_builds_keep_codec_workers_disabled() {
        assert_eq!(available_threads(), 1);
        assert_eq!(
            thread_count(FOUR_THREADS_MIN_BYTES, BROTLI_MT_MIN_BYTES, available_threads()),
            1
        );
        assert_eq!(zstd_worker_count(FOUR_THREADS_MIN_BYTES, available_threads()), 0);
    }

    #[test]
    fn zstd_sync_parallel_and_checksum_validation() {
        let input: Vec<_> = (0..5 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        for workers in [0, 2, 4] {
            let mut frame = Vec::new();
            zstd_encode(&input, &mut frame, workers);
            assert_eq!(zstd::stream::decode_all(frame.as_slice()).unwrap(), input);
            let end = frame.len() - 1;
            assert!(zstd::stream::decode_all(&frame[..end]).is_err());
            frame[end] ^= 1;
            assert!(zstd::stream::decode_all(frame.as_slice()).is_err());
        }
    }
}
