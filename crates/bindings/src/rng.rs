use std::cell::{OnceCell, UnsafeCell};
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use rand::distributions::{Distribution, Standard};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

use crate::Timestamp;

scoped_tls::scoped_thread_local! {
    static RNG: OnceCell<RngContext>
}

// this ensures that you can't stash an StdbRng somewhere and use it
// at a later point, breaking atomic reproducibility of transactions.
static RNG_GENERATION: AtomicU64 = AtomicU64::new(0);

struct RngContext {
    rng: StdCellRng,
    generation: u64,
}

impl RngContext {
    fn seed() -> Self {
        Self {
            rng: StdCellRng::new(StdRng::seed_from_u64(Timestamp::now().micros_since_epoch)),
            generation: RNG_GENERATION.fetch_add(1, Relaxed),
        }
    }
    fn get_rng(&self) -> StdbRng {
        StdbRng {
            generation: self.generation,
            _marker: PhantomData,
        }
    }
}

pub(crate) fn with_rng_set<R>(f: impl FnOnce() -> R) -> R {
    RNG.set(&OnceCell::new(), f)
}

/// Generates a random value.
///
/// Similar to [`rand::random()`], but using [`StdbRng`] instead.
///
/// See also [`spacetimedb::rng()`][rng()]
pub fn random<T>() -> T
where
    Standard: Distribution<T>,
{
    if !RNG.is_set() {
        panic!("cannot use `spacetimedb::random()` outside of a reducer");
    }
    Standard.sample(&mut rng())
}

/// Retrieve the random number generator for this reducer transaction,
/// seeded by the timestamp of the reducer call.
///
/// If you only need a single random value, use [`spacetimedb::random()`][random].
///
/// Can be used in method chaining style, e.g. with [`rand::Rng`]
/// imported: `spacetimedb::rng().gen_range(0..=10)`. Or, cache it locally
/// for reuse: `let mut rng = spacetimedb::rng();`.
///
/// For more information see [`StdbRng`].
pub fn rng() -> StdbRng {
    if !RNG.is_set() {
        panic!("cannot use `spacetimedb::rng()` outside of a reducer");
    }
    RNG.with(|r| r.get_or_init(RngContext::seed).get_rng())
}

/// A reference to the random number generator for this reducer call.
///
/// An instance can be obtained via [`spacetimedb::rng()`][rng()]. Import
/// [`rand::Rng`] to get access to useful methods
///
/// This handle
/// can only be used in the context of this reducer call; it cannot be
/// stashed in a global and used in a later reducer call, as that would break
/// the atomicity of transactions.
///
/// `StdbRng` uses the same PRNG as `rand`'s [`StdRng`], however, because it
/// is seeded from a timestamp, it is not cryptographically secure.
///
/// The type of reproducibility you're looking for be finer grained than
/// "if it happens at the exact same time, you get the same result" -- if
/// so, just seed an [`StdRng`] directly, or use another rng like those
/// listed [here](https://rust-random.github.io/book/guide-rngs.html).
/// Just note that you should never depend on state from outside of
/// the current reducer call (by e.g. storing an rng in a global variable),
/// as that can and will break things in the database.
pub struct StdbRng {
    // ensures atomicity of transactions - see comment on RNG_GENERATION
    generation: u64,
    // !Send + !Sync
    _marker: PhantomData<*mut ()>,
}

impl StdbRng {
    fn try_with<R>(&self, f: impl FnOnce(&StdCellRng) -> R) -> Result<R, RngError> {
        if !RNG.is_set() {
            return Err(RngError);
        }
        RNG.with(|r| {
            let r = r.get().filter(|r| r.generation == self.generation).ok_or(RngError)?;
            Ok(f(&r.rng))
        })
    }

    fn with<R>(&self, f: impl FnOnce(&StdCellRng) -> R) -> R {
        self.try_with(f).unwrap_or_else(
            #[cold]
            |e| panic!("{e}"),
        )
    }
}

impl RngCore for StdbRng {
    fn next_u32(&mut self) -> u32 {
        self.with(|rng| rng.next_u32())
    }

    fn next_u64(&mut self) -> u64 {
        self.with(|rng| rng.next_u64())
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.with(|rng| rng.fill_bytes(dest))
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.try_with(|rng| rng.try_fill_bytes(dest))
            .unwrap_or_else(|e| Err(rand::Error::new(e)))
    }
}

#[derive(Debug)]
struct RngError;

impl fmt::Display for RngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rng from previous reducer still in use")
    }
}

impl std::error::Error for RngError {}

// Comments in the rand crate claim RefCell can have an overhead of up to 15%,
// and so they use an UnsafeCell instead:
// <https://docs.rs/rand/0.8.5/src/rand/rngs/thread.rs.html#20-32>
struct StdCellRng {
    rng: UnsafeCell<StdRng>,
}

impl StdCellRng {
    fn new(rng: StdRng) -> Self {
        Self { rng: rng.into() }
    }
}

impl StdCellRng {
    #[inline(always)]
    fn next_u32(&self) -> u32 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u32()
    }

    #[inline(always)]
    fn next_u64(&self) -> u64 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u64()
    }

    fn fill_bytes(&self, dest: &mut [u8]) {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&self, dest: &mut [u8]) -> Result<(), rand::Error> {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.try_fill_bytes(dest)
    }
}
