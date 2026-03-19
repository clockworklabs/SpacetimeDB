use core::{any::type_name, hash::BuildHasherDefault, hint::black_box, iter::repeat_with, mem, time::Duration};
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement as _, WallTime},
    BenchmarkGroup, Criterion,
};
use foldhash::{HashSet, HashSetExt};
use hashbrown::{hash_map::Entry, HashMap};
use itertools::Itertools as _;
use rand::{
    distr::{Distribution, StandardUniform},
    Rng,
};
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_sats::{layout::Size, product, u256};
use spacetimedb_table::indexes::{PageIndex, PageOffset, RowPointer, SquashedOffset};
use spacetimedb_table::table_index::uniquemap::UniqueMap;
use spacetimedb_table::table_index::Index as _;
use spacetimedb_table::table_index::{
    unique_direct_index::{ToFromUsize, UniqueDirectIndex},
    KeySize,
};
use std::hash::Hash;

fn time<R>(body: impl FnOnce() -> R) -> Duration {
    let start = WallTime.start();
    let ret = body();
    let end = WallTime.end(start);
    black_box(ret);
    end
}

const FIXED_ROW_SIZE: Size = Size(4 * 4);

fn gen_row_pointers() -> impl Iterator<Item = RowPointer> {
    let mut page_index = PageIndex(0);
    let mut page_offset = PageOffset(0);
    repeat_with(move || {
        if page_offset.0 as usize + FIXED_ROW_SIZE.0 as usize >= PageOffset::PAGE_END.0 as usize {
            // Consumed the page, let's use a new page.
            page_index.0 += 1;
            page_offset = PageOffset(0);
        } else {
            page_offset += FIXED_ROW_SIZE;
        }

        black_box(RowPointer::new(
            false,
            page_index,
            page_offset,
            SquashedOffset::COMMITTED_STATE,
        ))
    })
}

fn bench_custom(g: &mut BenchmarkGroup<'_, WallTime>, name: &str, run: impl Fn(u64) -> Duration) {
    g.bench_function(name, |b| b.iter_custom(&run));
}

/// Returns an iterator with the keys `0..n`.
fn monotonic_keys(n: u64) -> impl Clone + Iterator<Item = MonoKey> {
    (0 as MonoKey..).take(n as usize)
}

// Returns a set with `n` distinct random keys.
fn random_keys<K: Eq + Hash>(n: u64) -> HashSet<K>
where
    StandardUniform: Distribution<K>,
{
    let desired_len = n as usize;
    let mut set = HashSet::with_capacity(desired_len);
    let mut iter = rand::random_iter();
    while set.len() < desired_len {
        set.insert(iter.next().unwrap());
    }
    set
}

/// Times inserting `keys` to the index.
fn time_insertions<I: Index>(keys: impl Iterator<Item = I::K>) -> Duration {
    let mut index = I::new();
    keys.zip(gen_row_pointers())
        .map(black_box)
        .map(|(key, ptr)| time(|| index.insert(key, ptr)))
        .sum()
}

/// Times inserting monotonically increasing keys to the index.
///
/// The benchmark intentionally times N keys rather than the Nth key.
fn bench_insert_monotonic<I: Index<K = MonoKey>>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "insert_monotonic", |n| time_insertions::<I>(monotonic_keys(n)));
}

/// Times inserting random keys to the index.
///
/// The benchmark intentionally times N keys rather than the Nth key.
fn bench_insert_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>)
where
    StandardUniform: Distribution<I::K>,
{
    bench_custom(g, "insert_random", |n| time_insertions::<I>(random_keys(n).into_iter()));
}

/// Times seeking `keys` in the index.
fn time_seeks<I: Index>(keys: impl Clone + Iterator<Item = I::K>) -> Duration {
    // Prepare index with the keys.
    let mut index = I::new();
    for (key, ptr) in keys.clone().zip(gen_row_pointers()) {
        index.insert(key, ptr).unwrap();
    }

    // Time seeking every K in keys.
    keys.map(black_box)
        .map(|i| time(|| black_box(index.seek(i)).next()))
        .sum()
}

/// Times seeking all keys index with monotonically increasing keys.
fn bench_seek_monotonic<I: Index<K = MonoKey>>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "seek_monotonic", |n| time_seeks::<I>(monotonic_keys(n)));
}

/// Times seeking all keys in an index with random keys.
fn bench_seek_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>)
where
    StandardUniform: Distribution<I::K>,
{
    bench_custom(g, "seek_random", |n| {
        let keys = random_keys(n);

        let keys2 = keys.iter().cloned().sorted();

        // Prepare index with the keys.
        let mut index = I::new();
        for (key, ptr) in keys2.clone().zip(gen_row_pointers()) {
            index.insert(key, ptr).unwrap();
        }

        // Time seeking every K in keys.
        keys.into_iter()
            .map(black_box)
            .map(|i| time(|| black_box(index.seek(i)).next()))
            .sum()
    });
}

/// Times deleting `keys` in the index.
fn time_deletes<I: Index>(keys: impl Clone + IntoIterator<Item = I::K>) -> Duration {
    // Prepare index with the keys.
    let mut index = I::new();
    for (key, ptr) in keys.clone().into_iter().zip(gen_row_pointers()) {
        index.insert(key, ptr).unwrap();
    }

    // Time deleting every K in keys.
    keys.into_iter().map(black_box).map(|i| time(|| index.delete(i))).sum()
}

/// Times deleting for one key in an index with monotonically increasing keys.
fn bench_delete_monotonic<I: Index<K = MonoKey>>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "delete_monotonic", |n| time_deletes::<I>(monotonic_keys(n)));
}

/// Times seeking for one key in an index with monotonically increasing keys.
fn bench_delete_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>)
where
    StandardUniform: Distribution<I::K>,
{
    bench_custom(g, "delete_random", |n| time_deletes::<I>(random_keys(n)));
}

fn bench_index_mono<I: Index<K = MonoKey>>(c: &mut Criterion) {
    let mut g = c.benchmark_group(type_name::<I>());
    bench_insert_monotonic::<I>(&mut g);
    bench_insert_random::<I>(&mut g);
    bench_seek_monotonic::<I>(&mut g);
    bench_seek_random::<I>(&mut g);
    bench_delete_monotonic::<I>(&mut g);
    bench_delete_random::<I>(&mut g);
}

fn bench_index_random<I: Index>(c: &mut Criterion)
where
    StandardUniform: Distribution<I::K>,
{
    let mut g = c.benchmark_group(type_name::<I>());
    bench_insert_random::<I>(&mut g);
    bench_seek_random::<I>(&mut g);
    bench_delete_random::<I>(&mut g);
}

type MonoKey = u32;

trait Index: Clone {
    type K: Eq + Hash + Ord + Clone;
    fn new() -> Self;
    fn insert(&mut self, key: Self::K, val: RowPointer) -> Result<(), RowPointer>;
    fn seek(&self, key: Self::K) -> impl Iterator<Item = RowPointer>;
    fn delete(&mut self, key: Self::K) -> bool;
}

#[derive(Clone)]
struct IBTree<K: KeySize<MemoStorage: Clone + Default>>(UniqueMap<K>);
impl<K: KeySize<MemoStorage: Clone + Default> + Clone + Eq + Hash + Ord> Index for IBTree<K> {
    type K = K;
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: Self::K, val: RowPointer) -> Result<(), RowPointer> {
        self.0.insert(key, val)
    }
    fn seek(&self, key: Self::K) -> impl Iterator<Item = RowPointer> {
        self.0.seek_point(&key)
    }
    fn delete(&mut self, key: Self::K) -> bool {
        self.0.delete(&key, RowPointer(0))
    }
}

#[derive(Clone)]
struct IAHash<K>(HashMap<K, RowPointer, BuildHasherDefault<ahash::AHasher>>);
impl<K: Clone + Eq + Hash + Ord> Index for IAHash<K> {
    type K = K;
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: Self::K, val: RowPointer) -> Result<(), RowPointer> {
        match self.0.entry(key) {
            Entry::Vacant(e) => {
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }
    fn seek(&self, key: Self::K) -> impl Iterator<Item = RowPointer> {
        self.0.get(&key).into_iter().copied()
    }
    fn delete(&mut self, key: Self::K) -> bool {
        self.0.remove(&key).is_some()
    }
}

#[derive(Clone)]
struct IFoldHash<K>(HashMap<K, RowPointer, foldhash::fast::RandomState>);
impl<K: Clone + Eq + Hash + Ord> Index for IFoldHash<K> {
    type K = K;
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: Self::K, val: RowPointer) -> Result<(), RowPointer> {
        match self.0.entry(key) {
            Entry::Vacant(e) => {
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }
    fn seek(&self, key: Self::K) -> impl Iterator<Item = RowPointer> {
        self.0.get(&key).into_iter().copied()
    }
    fn delete(&mut self, key: Self::K) -> bool {
        self.0.remove(&key).is_some()
    }
}

#[derive(Clone)]
struct IDirectIndex<K>(UniqueDirectIndex<K>);
impl<K: KeySize + ToFromUsize + Eq + Hash + Ord> Index for IDirectIndex<K> {
    type K = K;
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: Self::K, val: RowPointer) -> Result<(), RowPointer> {
        self.0.insert(key, val)
    }
    fn seek(&self, key: Self::K) -> impl Iterator<Item = RowPointer> {
        self.0.seek_point(&key)
    }
    fn delete(&mut self, key: Self::K) -> bool {
        self.0.delete(&key, RowPointer(0))
    }
}

/* Complex keys */

#[derive(Clone, Copy)]
struct U256(u256);

impl Distribution<U256> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> U256 {
        let (hi, lo) = self.sample(rng);
        U256(u256::from_words(hi, lo))
    }
}

impl From<U256> for AlgebraicValue {
    fn from(value: U256) -> AlgebraicValue {
        AlgebraicValue::U256(Box::new(value.0))
    }
}

impl U256 {
    fn to_le_bytes(self) -> [u8; 32] {
        self.0.to_le_bytes()
    }
}

macro_rules! av_enc_type {
    ($name_av:ident, $name_enc:ident, $sample_ty:ty, ($($sample_pat:ident),* $(,)?)) => {
        #[allow(non_camel_case_types)]
        #[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
        struct $name_av(AlgebraicValue);
        #[allow(non_camel_case_types)]
        #[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
        struct $name_enc([u8; mem::size_of::<$sample_ty>()]);

        impl KeySize for $name_av { type MemoStorage = (); }
        impl KeySize for $name_enc { type MemoStorage = (); }

        impl Distribution<$name_av> for StandardUniform {
            fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> $name_av {
                let ($($sample_pat,)*): $sample_ty = self.sample(rng);
                $name_av(product![$($sample_pat),*].into())
            }
        }

        impl Distribution<$name_enc> for StandardUniform {
            #[allow(unused_assignments)]
            fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> $name_enc {
                let ($($sample_pat,)*): $sample_ty = self.sample(rng);

                let mut enc = [0; _];
                let mut start = 0;

                $(
                    let size = mem::size_of_val(&$sample_pat);
                    enc[start..start + size].copy_from_slice(&$sample_pat.to_le_bytes());
                    start += size;
                )*

                $name_enc(enc)
            }
        }
    };
}

av_enc_type!(
    U16xU256xU32xU256_AV,
    U16xU256xU32xU256_Enc,
    (u16, U256, u32, U256),
    (a, b, c, d)
);
av_enc_type!(U16xU256xU32_AV, U16xU256xU32_Enc, (u16, U256, u32), (a, b, c));

av_enc_type!(U256xU32xBool_AV, U256xU32xBool_Enc, (U256, u32, u8), (a, b, c));
av_enc_type!(U256xU32_AV, U256xU32_Enc, (U256, u32), (a, b));

// This is the index x_z_chunk_index from
// https://github.com/clockworklabs/BitCraft/blob/f8177345df1e4ec54b91acee94146e75a0525548/BitCraftServer/packages/game/src/messages/components.rs#L224C18-L224C33
av_enc_type!(I32xI32xU64_AV, I32xI32xU64_Enc, (i32, i32, u64), (a, b, c));

criterion_group!(
    delete_table,
    // Primitive types, both random and monotonic keys:
    bench_index_mono::<IBTree<u32>>,
    bench_index_mono::<IAHash<u32>>,
    bench_index_mono::<IFoldHash<u32>>,
    bench_index_mono::<IDirectIndex<u32>>,
    // Complex keys, hash index:
    bench_index_random::<IFoldHash<U16xU256xU32xU256_AV>>,
    bench_index_random::<IFoldHash<U16xU256xU32xU256_Enc>>,
    bench_index_random::<IFoldHash<U16xU256xU32_AV>>,
    bench_index_random::<IFoldHash<U16xU256xU32_Enc>>,
    bench_index_random::<IFoldHash<U256xU32xBool_AV>>,
    bench_index_random::<IFoldHash<U256xU32xBool_Enc>>,
    bench_index_random::<IFoldHash<U256xU32_AV>>,
    bench_index_random::<IFoldHash<U256xU32_Enc>>,
    bench_index_random::<IFoldHash<I32xI32xU64_AV>>,
    bench_index_random::<IFoldHash<I32xI32xU64_Enc>>,
    // Complex keys, btree index:
    bench_index_random::<IBTree<U16xU256xU32xU256_AV>>,
    bench_index_random::<IBTree<U16xU256xU32xU256_Enc>>,
    bench_index_random::<IBTree<U16xU256xU32_AV>>,
    bench_index_random::<IBTree<U16xU256xU32_Enc>>,
    bench_index_random::<IBTree<U256xU32xBool_AV>>,
    bench_index_random::<IBTree<U256xU32xBool_Enc>>,
    bench_index_random::<IBTree<U256xU32_AV>>,
    bench_index_random::<IBTree<U256xU32_Enc>>,
    bench_index_random::<IBTree<I32xI32xU64_AV>>,
    bench_index_random::<IBTree<I32xI32xU64_Enc>>,
);
criterion_main!(delete_table);
