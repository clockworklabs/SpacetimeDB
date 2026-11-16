//! Websocket compression CPU policy. Callers retain the existing encode
//! executor and release database transaction locks before compressing.
use bytes::Bytes;
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};

const BROTLI_QUALITY: i32 = 2;
const BROTLI_MT_MIN_BYTES: usize = 512 * 1024;
const FOUR_THREADS_MIN_BYTES: usize = 64 * 1024 * 1024;
const ZSTD_LEVEL: i32 = -2;
const ZSTD_MT_MIN_BYTES: usize = 4 * 1024 * 1024;

// Share half the available CPUs across MT jobs, rather than allowing just one
// job or creating an independent allowance per connection. Count coordinators
// as well as workers; synchronous encode work remains on the existing executor.
static MT_BUDGET: OnceLock<MtBudget> = OnceLock::new();

struct MtBudget {
    available: AtomicUsize,
}

struct MtPermit<'a> {
    budget: &'a MtBudget,
    slots: usize,
}

impl MtBudget {
    const fn new(capacity: usize) -> Self {
        Self {
            available: AtomicUsize::new(capacity),
        }
    }

    /// Atomically reserve up to `desired` slots without waiting for capacity.
    /// Fewer than two slots cannot run an MT job, so leave them unclaimed.
    fn try_acquire(&self, desired: usize) -> Option<MtPermit<'_>> {
        self.available
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |free| {
                let slots = desired.min(free);
                (slots >= 2).then_some(free - slots)
            })
            .ok()
            .map(|free| MtPermit {
                budget: self,
                slots: desired.min(free),
            })
    }
}

impl Drop for MtPermit<'_> {
    fn drop(&mut self) {
        self.budget.available.fetch_add(self.slots, Ordering::Release);
    }
}

fn mt_cpu_budget(available: usize) -> usize {
    available / 2
}

fn mt_budget(available: usize) -> &'static MtBudget {
    // Initialize once from the CPU quota/affinity visible to the encode worker.
    // A restart is required to change this capacity after CPU limits change.
    MT_BUDGET.get_or_init(|| MtBudget::new(mt_cpu_budget(available)))
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
    let available = available_threads();
    let threads = thread_count(bytes.len(), BROTLI_MT_MIN_BYTES, available);
    if threads > 1
        && let Some(permit) = mt_budget(available).try_acquire(threads)
    {
        match brotli_parallel(bytes.clone(), permit.slots) {
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
    let available = available_threads();
    let workers = zstd_worker_count(bytes.len(), available);
    let permit = (workers > 0)
        .then(|| mt_budget(available).try_acquire(workers as usize + 1))
        .flatten();
    let workers = permit.as_ref().map_or(0, |permit| (permit.slots - 1) as u32);
    zstd_encode(bytes, out, workers);
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
        let budget = MtBudget::new(8);
        let other = budget.try_acquire(3).unwrap();
        let result = std::panic::catch_unwind(|| {
            let permit = budget.try_acquire(5).unwrap();
            assert_eq!(permit.slots, 5);
            assert!(budget.try_acquire(2).is_none());
            panic!("test permit cleanup");
        });
        assert!(result.is_err());
        assert_eq!(budget.available.load(Ordering::Relaxed), 5);
        drop(other);
        assert_eq!(budget.available.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn budget_uses_half_available_cpus_and_keeps_small_machines_synchronous() {
        for (cpus, slots) in [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (5, 2), (32, 16), (64, 32)] {
            assert_eq!(mt_cpu_budget(cpus), slots);
            let budget = MtBudget::new(slots);
            assert_eq!(budget.try_acquire(4).is_some(), slots >= 2);
        }
    }

    #[test]
    fn multiple_jobs_share_slots_and_can_take_partial_allocations() {
        let budget = MtBudget::new(mt_cpu_budget(32));
        let brotli = budget.try_acquire(4).unwrap();
        let zstd_a = budget.try_acquire(5).unwrap();
        let zstd_b = budget.try_acquire(5).unwrap();
        assert_eq!((brotli.slots, zstd_a.slots, zstd_b.slots), (4, 5, 5));
        let reduced_zstd = budget.try_acquire(5).unwrap();
        assert_eq!(reduced_zstd.slots, 2); // one worker plus the coordinator
        assert!(budget.try_acquire(2).is_none());
        drop(brotli);
        let replacement = budget.try_acquire(5).unwrap();
        assert_eq!(replacement.slots, 4);
        drop((zstd_a, zstd_b, reduced_zstd, replacement));
        assert_eq!(budget.available.load(Ordering::Relaxed), 16);

        let permits: Vec<_> = (0..8).map(|_| budget.try_acquire(2).unwrap()).collect();
        assert!(budget.try_acquire(2).is_none());
        drop(permits);
        assert_eq!(budget.available.load(Ordering::Relaxed), 16);
    }

    #[test]
    fn one_remaining_slot_is_not_consumed_by_an_mt_request() {
        let budget = MtBudget::new(3);
        let permit = budget.try_acquire(2).unwrap();
        assert!(budget.try_acquire(4).is_none());
        assert!(budget.try_acquire(1).is_none());
        assert!(budget.try_acquire(0).is_none());
        assert_eq!(budget.available.load(Ordering::Relaxed), 1);
        drop(permit);
        assert_eq!(budget.available.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn concurrent_reservations_never_exceed_capacity() {
        use std::sync::{Arc, Barrier};
        let budget = Arc::new(MtBudget::new(16));
        let start = Arc::new(Barrier::new(16));
        let active = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for i in 0..16 {
            let (budget, start, active) = (budget.clone(), start.clone(), active.clone());
            threads.push(std::thread::spawn(move || {
                start.wait();
                for _ in 0..100 {
                    if let Some(permit) = budget.try_acquire(2 + i % 4) {
                        assert!((2..=2 + i % 4).contains(&permit.slots));
                        let total = active.fetch_add(permit.slots, Ordering::SeqCst) + permit.slots;
                        assert!(total <= 16);
                        std::thread::yield_now();
                        active.fetch_sub(permit.slots, Ordering::SeqCst);
                    }
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(active.load(Ordering::Relaxed), 0);
        assert_eq!(budget.available.load(Ordering::Relaxed), 16);
    }

    #[test]
    fn brotli_sync_and_parallel_roundtrip() {
        let input = Bytes::from((0..1024 * 1024).map(|i| (i % 251) as u8).collect::<Vec<_>>());
        let mut sync = Vec::new();
        brotli_compress(&input, &mut sync);
        for frame in [
            sync,
            brotli_parallel(input.clone(), 2).unwrap(),
            brotli_parallel(input.clone(), 3).unwrap(),
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
        for workers in [0, 1, 2, 3, 4] {
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
