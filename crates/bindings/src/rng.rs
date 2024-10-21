use std::cell::UnsafeCell;
use std::marker::PhantomData;

use rand::distributions::{Distribution, Standard};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

use crate::ReducerContext;

impl ReducerContext {
    /// Generates a random value.
    ///
    /// Similar to [`rand::random()`], but using [`StdbRng`] instead.
    ///
    /// See also [`ReducerContext::rng()`].
    pub fn random<T>(&self) -> T
    where
        Standard: Distribution<T>,
    {
        Standard.sample(&mut self.rng())
    }

    /// Retrieve the random number generator for this reducer transaction,
    /// seeded by the timestamp of the reducer call.
    ///
    /// If you only need a single random value, you can use [`ReducerContext::random()`].
    ///
    /// # Examples
    ///
    /// ```
    /// # #[spacetimedb::reducer]
    /// # fn rng_demo(ctx: &spacetimedb::ReducerContext) {
    /// use rand::Rng;
    ///
    /// // Can be used in method chaining style:
    /// let digit = ctx.rng().gen_range(0..=9);
    ///
    /// // Or, cache locally for reuse:
    /// let mut rng = ctx.rng();
    /// let floats: Vec<f32> = rng.sample_iter(rand::distributions::Standard).collect();
    /// # }
    /// ```
    ///
    /// For more information, see [`StdbRng`] and [`rand::Rng`].
    pub fn rng(&self) -> &StdbRng {
        self.rng.get_or_init(|| StdbRng {
            rng: StdRng::seed_from_u64(self.timestamp.to_nanos_since_unix_epoch() as u64).into(),
            _marker: PhantomData,
        })
    }
}

/// A reference to the random number generator for this reducer call.
///
/// An instance can be obtained via [`ReducerContext::rng()`]. Import
/// [`rand::Rng`] in order to access many useful random algorithms.
///
/// `StdbRng` uses the same PRNG as `rand`'s [`StdRng`]. Note, however, that
/// because it is seeded from a publically-known timestamp, it is not
/// cryptographically secure.
///
/// You may be looking for a level of reproducibility that's finer-grained
/// than "if it happens at the exact same time, you get the same result" --
/// if so, just seed an [`StdRng`] directly, or use another rng like those
/// listed [here](https://rust-random.github.io/book/guide-rngs.html).
/// Just note that you must not store any state, including an rng, in a global
/// variable or any other in-WASM side channel. Any and all state persisted
/// across reducer calls _must_ be stored in the database.
pub struct StdbRng {
    // Comments in the rand crate claim RefCell can have an overhead of up to 15%,
    // and so they use an UnsafeCell instead:
    // <https://docs.rs/rand/0.8.5/src/rand/rngs/thread.rs.html#20-32>
    // This is safe as long as no method on `StdRngCell` is re-entrant.
    rng: UnsafeCell<StdRng>,

    // !Send + !Sync
    _marker: PhantomData<*mut ()>,
}

impl RngCore for StdbRng {
    fn next_u32(&mut self) -> u32 {
        (&*self).next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        (&*self).next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        (&*self).fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        (&*self).try_fill_bytes(dest)
    }
}

impl RngCore for &StdbRng {
    #[inline(always)]
    fn next_u32(&mut self) -> u32 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u32()
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.try_fill_bytes(dest)
    }
}
