use core::{hint::black_box, iter::repeat_with, time::Duration};
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement as _, WallTime},
    BenchmarkGroup, Criterion,
};
use foldhash::{HashSet, HashSetExt};
use itertools::Itertools as _;
use rand::{distributions::Standard, rngs::ThreadRng, Rng};
use spacetimedb_data_structures::map::{Entry, HashMap};
use spacetimedb_table::indexes::{PageIndex, PageOffset, RowPointer, Size, SquashedOffset};
use spacetimedb_table::table_index::unique_direct_index::UniqueDirectIndex;
use spacetimedb_table::table_index::uniquemap::UniqueMap;

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
fn monotonic_keys(n: u64) -> impl Clone + Iterator<Item = K> {
    (0 as K..).take(n as usize)
}

// Returns a set with `n` distinct random keys.
fn random_keys(n: u64) -> HashSet<K> {
    let desired_len = n as usize;
    let mut set = HashSet::with_capacity(desired_len);
    let mut iter = ThreadRng::default().sample_iter(Standard);
    while set.len() < desired_len {
        set.insert(iter.next().unwrap());
    }
    set
}

/// Times inserting `keys` to the index.
fn time_insertions<I: Index>(keys: impl Iterator<Item = K>) -> Duration {
    let mut index = I::new();
    keys.zip(gen_row_pointers())
        .map(black_box)
        .map(|(key, ptr)| time(|| index.insert(key, ptr)))
        .sum()
}

/// Times inserting monotonically increasing keys to the index.
///
/// The benchmark intentionally times N keys rather than the Nth key.
fn bench_insert_monotonic<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "insert_monotonic", |n| time_insertions::<I>(monotonic_keys(n)));
}

/// Times inserting random keys to the index.
///
/// The benchmark intentionally times N keys rather than the Nth key.
fn bench_insert_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "insert_random", |n| {
        time_insertions::<I>(random_keys(n).iter().copied())
    });
}

/// Times seeking `keys` in the index.
fn time_seeks<I: Index>(keys: impl Clone + Iterator<Item = K>) -> Duration {
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
fn bench_seek_monotonic<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "seek_monotonic", |n| time_seeks::<I>(monotonic_keys(n)));
}

/// Times seeking all keys in an index with random keys.
fn bench_seek_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "seek_random", |n| {
        let keys = random_keys(n);

        let keys2 = keys.iter().copied().sorted();

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
fn time_deletes<I: Index>(keys: impl Clone + Iterator<Item = K>) -> Duration {
    // Prepare index with the keys.
    let mut index = I::new();
    for (key, ptr) in keys.clone().zip(gen_row_pointers()) {
        index.insert(key, ptr).unwrap();
    }

    // Time deleting every K in keys.
    keys.map(black_box).map(|i| time(|| index.delete(i))).sum()
}

/// Times deleting for one key in an index with monotonically increasing keys.
fn bench_delete_monotonic<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "delete_monotonic", |n| time_deletes::<I>(monotonic_keys(n)));
}

/// Times seeking for one key in an index with monotonically increasing keys.
fn bench_delete_random<I: Index>(g: &mut BenchmarkGroup<'_, WallTime>) {
    bench_custom(g, "delete_random", |n| {
        time_deletes::<I>(random_keys(n).iter().copied())
    });
}

fn bench_index<I: Index>(c: &mut Criterion) {
    let mut g = c.benchmark_group(I::NAME);
    bench_insert_monotonic::<I>(&mut g);
    bench_insert_random::<I>(&mut g);
    bench_seek_monotonic::<I>(&mut g);
    bench_seek_random::<I>(&mut g);
    bench_delete_monotonic::<I>(&mut g);
    bench_delete_random::<I>(&mut g);
}

type K = u32;

trait Index: Clone {
    const NAME: &'static str;
    fn new() -> Self;
    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer>;
    fn seek(&self, key: K) -> impl Iterator<Item = RowPointer>;
    fn delete(&mut self, key: K) -> bool;
}

#[derive(Clone)]
struct IBTree(UniqueMap<K, RowPointer>);
impl Index for IBTree {
    const NAME: &'static str = "IBTree";
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        self.0.insert(key, val).map_err(|x| *x)
    }
    fn seek(&self, key: K) -> impl Iterator<Item = RowPointer> {
        self.0.values_in_range(&(key..=key)).copied()
    }
    fn delete(&mut self, key: K) -> bool {
        self.0.delete(&key)
    }
}

#[derive(Clone)]
struct IAHash(HashMap<K, RowPointer>);
impl Index for IAHash {
    const NAME: &'static str = "IAHash";
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        match self.0.entry(key) {
            Entry::Vacant(e) => {
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }
    fn seek(&self, key: K) -> impl Iterator<Item = RowPointer> {
        self.0.get(&key).into_iter().copied()
    }
    fn delete(&mut self, key: K) -> bool {
        self.0.remove(&key).is_some()
    }
}

#[derive(Clone)]
struct IFoldHash(HashMap<K, RowPointer, foldhash::fast::RandomState>);
impl Index for IFoldHash {
    const NAME: &'static str = "IFoldHash";
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        match self.0.entry(key) {
            Entry::Vacant(e) => {
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }
    fn seek(&self, key: K) -> impl Iterator<Item = RowPointer> {
        self.0.get(&key).into_iter().copied()
    }
    fn delete(&mut self, key: K) -> bool {
        self.0.remove(&key).is_some()
    }
}

#[derive(Clone)]
struct IDirectIndex(UniqueDirectIndex);
impl Index for IDirectIndex {
    const NAME: &'static str = "IDirectIndex";
    fn new() -> Self {
        Self(<_>::default())
    }
    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        self.0.insert(key as usize, val)
    }
    fn seek(&self, key: K) -> impl Iterator<Item = RowPointer> {
        self.0.seek_point(key as usize)
    }
    fn delete(&mut self, key: K) -> bool {
        self.0.delete(key as usize)
    }
}

criterion_group!(
    delete_table,
    bench_index::<IBTree>,
    bench_index::<IAHash>,
    bench_index::<IFoldHash>,
    bench_index::<IDirectIndex>,
);
criterion_main!(delete_table);
