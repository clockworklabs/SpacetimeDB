use core::{cmp::Ordering, iter::repeat_with, time::Duration};
use criterion::{
    black_box, criterion_group, criterion_main,
    measurement::{Measurement as _, WallTime},
    BenchmarkGroup, Criterion,
};
use itertools::Itertools;
use rand::{prelude::*, seq::SliceRandom};
use smallvec::SmallVec;
use spacetimedb::db::datastore::locking_tx_datastore::delete_table;
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_table::indexes::{PageIndex, PageOffset, RowPointer, Size, SquashedOffset};
use std::collections::BTreeSet;

fn time<R>(body: impl FnOnce() -> R) -> Duration {
    let start = WallTime.start();
    let ret = body();
    let end = WallTime.end(start);
    black_box(ret);
    end
}

const FIXED_ROW_SIZE: Size = Size(4 * 4);

fn gen_row_pointers(iters: u64) -> impl Iterator<Item = RowPointer> {
    let mut page_index = PageIndex(0);
    let mut page_offset = PageOffset(0);
    let iter = repeat_with(move || {
        page_offset += FIXED_ROW_SIZE;
        if page_offset >= PageOffset::PAGE_END {
            // Consumed the page, let's use a new page.
            page_index.0 += 1;
            page_offset = PageOffset(0);
        }

        black_box(RowPointer::new(
            false,
            page_index,
            page_offset,
            SquashedOffset::COMMITTED_STATE,
        ))
    });
    iter.take(iters as usize)
}

fn bench_custom(g: &mut BenchmarkGroup<'_, WallTime>, name: &str, run: impl Fn(u64) -> Duration) {
    g.bench_function(name, |b| b.iter_custom(&run));
}

fn bench_delete_table<DT: DeleteTable>(c: &mut Criterion) {
    let name = DT::NAME;
    let mut g = c.benchmark_group(name);
    let row_size = black_box(FIXED_ROW_SIZE);
    let new_dt = || DT::new(row_size);
    bench_custom(&mut g, "mixed", |i| {
        let mut dt = new_dt();
        gen_row_pointers(i)
            .map(|ptr| time(|| dt.contains(ptr)) + time(|| dt.insert(ptr)))
            .sum()
    });
    bench_custom(&mut g, "mixed_random", |i| {
        let mut dt = new_dt();
        let mut ptrs = gen_row_pointers(i).collect_vec();
        let mut rng = ThreadRng::default();
        ptrs.shuffle(&mut rng);
        ptrs.into_iter()
            .map(|ptr| time(|| dt.contains(ptr)) + time(|| dt.insert(ptr)))
            .sum()
    });
    bench_custom(&mut g, "insert", |i| {
        let mut dt = new_dt();
        gen_row_pointers(i).map(|ptr| time(|| dt.insert(ptr))).sum()
    });
    bench_custom(&mut g, "contains_for_half", |i| {
        let mut dt = new_dt();
        gen_row_pointers(i)
            .enumerate()
            .map(|(i, ptr)| {
                if i % 2 == 0 {
                    black_box(dt.insert(ptr));
                }
                time(|| dt.contains(ptr))
            })
            .sum()
    });
    bench_custom(&mut g, "contains_for_full", |i| {
        let mut dt = new_dt();
        gen_row_pointers(i)
            .map(|ptr| {
                black_box(dt.insert(ptr));
                time(|| dt.contains(ptr))
            })
            .sum()
    });
    bench_custom(&mut g, "remove", |i| {
        let mut dt = new_dt();
        for ptr in gen_row_pointers(i) {
            black_box(dt.insert(ptr));
        }
        gen_row_pointers(i).map(|ptr| time(|| dt.remove(ptr))).sum()
    });
    bench_custom(&mut g, "iter", |i| {
        let mut dt = new_dt();
        for ptr in gen_row_pointers(i) {
            black_box(dt.insert(ptr));
        }
        time(|| dt.iter().count())
    });
    g.finish();
}

trait DeleteTable {
    const NAME: &'static str;
    fn new(fixed_row_size: Size) -> Self;
    fn contains(&self, ptr: RowPointer) -> bool;
    fn insert(&mut self, ptr: RowPointer) -> bool;
    fn remove(&mut self, ptr: RowPointer) -> bool;
    fn iter(&self) -> impl Iterator<Item = RowPointer>;
    #[allow(unused)]
    fn len(&self) -> usize;
}

struct DTBTree(BTreeSet<RowPointer>);

impl DeleteTable for DTBTree {
    const NAME: &'static str = "DTBTree";
    fn new(_: Size) -> Self {
        Self(<_>::default())
    }
    fn contains(&self, ptr: RowPointer) -> bool {
        self.0.contains(&ptr)
    }
    fn insert(&mut self, ptr: RowPointer) -> bool {
        self.0.insert(ptr)
    }
    fn remove(&mut self, ptr: RowPointer) -> bool {
        self.0.remove(&ptr)
    }
    fn iter(&self) -> impl Iterator<Item = RowPointer> {
        self.0.iter().copied()
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

struct DTHashSet(HashSet<RowPointer>);

impl DeleteTable for DTHashSet {
    const NAME: &'static str = "DTHashSet";
    fn new(_: Size) -> Self {
        Self(<_>::default())
    }
    fn contains(&self, ptr: RowPointer) -> bool {
        self.0.contains(&ptr)
    }
    fn insert(&mut self, ptr: RowPointer) -> bool {
        self.0.insert(ptr)
    }
    fn remove(&mut self, ptr: RowPointer) -> bool {
        self.0.remove(&ptr)
    }
    fn iter(&self) -> impl Iterator<Item = RowPointer> {
        self.0.iter().copied()
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

struct DTHashSetFH(foldhash::HashSet<RowPointer>);

impl DeleteTable for DTHashSetFH {
    const NAME: &'static str = "DTHashSetFH";
    fn new(_: Size) -> Self {
        Self(<_>::default())
    }
    fn contains(&self, ptr: RowPointer) -> bool {
        self.0.contains(&ptr)
    }
    fn insert(&mut self, ptr: RowPointer) -> bool {
        self.0.insert(ptr)
    }
    fn remove(&mut self, ptr: RowPointer) -> bool {
        self.0.remove(&ptr)
    }
    fn iter(&self) -> impl Iterator<Item = RowPointer> {
        self.0.iter().copied()
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

type DTPageAndBitSet = delete_table::DeleteTable;

impl DeleteTable for DTPageAndBitSet {
    const NAME: &'static str = "DTPageAndBitSet";
    fn new(fixed_row_size: Size) -> Self {
        Self::new(fixed_row_size)
    }
    fn contains(&self, ptr: RowPointer) -> bool {
        self.contains(ptr)
    }
    fn insert(&mut self, ptr: RowPointer) -> bool {
        self.insert(ptr)
    }
    fn remove(&mut self, ptr: RowPointer) -> bool {
        self.remove(ptr)
    }
    fn iter(&self) -> impl Iterator<Item = RowPointer> {
        self.iter()
    }
    fn len(&self) -> usize {
        self.len()
    }
}

#[derive(Clone, Copy)]
struct OffsetRange {
    start: PageOffset,
    end: PageOffset,
}
impl OffsetRange {
    fn point(offset: PageOffset) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }
}
type OffsetRanges = SmallVec<[OffsetRange; 4]>;
struct DTPageAndOffsetRanges {
    deleted: Vec<OffsetRanges>,
    len: usize,
    fixed_row_size: Size,
}

fn cmp_start_end<T: Ord>(start: &T, end: &T, needle: &T) -> Ordering {
    match (start.cmp(needle), end.cmp(needle)) {
        // start = needle or start < offset <= end => we have a match.
        (Ordering::Less, Ordering::Equal | Ordering::Greater) | (Ordering::Equal, _) => Ordering::Equal,
        // start <= end < needle => no match.
        (Ordering::Less, Ordering::Less) => Ordering::Less,
        // start <= end > needle => no match.
        (Ordering::Greater, _) => Ordering::Greater,
    }
}

#[inline]
fn find_range_to_insert_offset(
    ranges: &OffsetRanges,
    offset: PageOffset,
    fixed_row_size: Size,
) -> Result<(bool, bool, usize), usize> {
    let mut extend_end = true;
    let mut exists = false;
    ranges
        .binary_search_by(|&OffsetRange { start, end }| {
            extend_end = true;
            exists = false;
            match end.cmp(&offset) {
                // `end + row_size = offset` => we can just extend `end = offset`.
                Ordering::Less if end.0 + fixed_row_size.0 == offset.0 => Ordering::Equal,
                // Cannot extend this range, so let's not find it.
                Ordering::Less => Ordering::Less,
                // `offset` is already covered, so don't do anything,
                // but `end = offset` is a no-op.
                Ordering::Equal => {
                    exists = true;
                    Ordering::Equal
                }
                // `end` is greater, but we may be covered by `start` instead.
                Ordering::Greater => match start.cmp(&offset) {
                    // `offset` is within the range, so don't do anything.
                    Ordering::Less | Ordering::Equal => {
                        exists = true;
                        Ordering::Equal
                    }
                    // `start - row_size = offset` => we can just extend `start = offset`.
                    Ordering::Greater if start.0 - fixed_row_size.0 == offset.0 => {
                        extend_end = false;
                        Ordering::Equal
                    }
                    // Range is entirely greater than offset.
                    Ordering::Greater => Ordering::Greater,
                },
            }
        })
        .map(|idx| (extend_end, exists, idx))
}

impl DeleteTable for DTPageAndOffsetRanges {
    const NAME: &'static str = "DTPageAndOffsetRanges";
    fn new(fixed_row_size: Size) -> Self {
        Self {
            deleted: <_>::default(),
            len: 0,
            fixed_row_size,
        }
    }
    fn contains(&self, ptr: RowPointer) -> bool {
        let page_idx = ptr.page_index().idx();
        let page_offset = ptr.page_offset();
        match self.deleted.get(page_idx) {
            Some(ranges) => ranges
                .binary_search_by(|r| cmp_start_end(&r.start, &r.end, &page_offset))
                .is_ok(),
            _ => false,
        }
    }
    fn insert(&mut self, ptr: RowPointer) -> bool {
        let fixed_row_size = self.fixed_row_size;
        let page_idx = ptr.page_index().idx();
        let page_offset = ptr.page_offset();

        let Some(ranges) = self.deleted.get_mut(page_idx) else {
            let pages = self.deleted.len();
            let after = 1 + page_idx;
            self.deleted.reserve(after - pages);
            for _ in pages..after {
                self.deleted.push(SmallVec::new());
            }
            self.deleted[page_idx].push(OffsetRange::point(page_offset));
            self.len += 1;
            return true;
        };

        let (extend_end, exists, range_idx) = match find_range_to_insert_offset(ranges, page_offset, fixed_row_size) {
            Err(range_idx) => {
                // Not found, so add a point range.
                ranges.insert(range_idx, OffsetRange::point(page_offset));
                self.len += 1;
                return true;
            }
            Ok(x) => x,
        };

        if extend_end {
            let next = range_idx + 1;
            let new_end = if let Some(r) = ranges
                .get(next)
                .copied()
                .filter(|r| r.start.0 - fixed_row_size.0 == page_offset.0)
            {
                ranges.remove(next);
                r.end
            } else {
                page_offset
            };
            ranges[range_idx].end = new_end;
        } else {
            let prev = range_idx.saturating_sub(1);
            if let Some(r) = ranges
                .get(prev)
                .copied()
                .filter(|r| r.end.0 + fixed_row_size.0 == page_offset.0)
            {
                ranges[range_idx].start = r.start;
                ranges.remove(prev);
            } else {
                ranges[range_idx].start = page_offset;
            };
        }

        let added = !exists;
        if added {
            self.len += 1;
        }
        added
    }
    fn remove(&mut self, ptr: RowPointer) -> bool {
        let fixed_row_size = self.fixed_row_size;
        let page_idx = ptr.page_index().idx();
        let page_offset = ptr.page_offset();

        let Some(ranges) = self.deleted.get_mut(page_idx) else {
            return false;
        };
        let Ok(idx) = ranges.binary_search_by(|r| cmp_start_end(&r.start, &r.end, &page_offset)) else {
            return false;
        };

        self.len -= 1;

        let range = &mut ranges[idx];
        let is_start = range.start == page_offset;
        let is_end = range.end == page_offset;
        match (is_start, is_end) {
            // Remove the point range.
            (true, true) => drop(ranges.remove(idx)),
            // Narrow the start.
            (true, false) => range.start += fixed_row_size,
            // Narrow the end.
            (false, true) => range.end -= fixed_row_size,
            // Split the range.
            (false, false) => {
                // Derive the second range, to the right of the hole.
                let end = range.end;
                let start = PageOffset(page_offset.0 + fixed_row_size.0);
                let new = OffsetRange { start, end };
                // Adjust the first range, to the left of the hole.
                range.end.0 = page_offset.0 - fixed_row_size.0;
                // Add the second range.
                ranges.insert(idx + 1, new);
            }
        }
        true
    }
    fn iter(&self) -> impl Iterator<Item = RowPointer> {
        (0..)
            .map(PageIndex)
            .zip(self.deleted.iter())
            .flat_map(move |(pi, ranges)| {
                ranges
                    .iter()
                    .flat_map(|range| (range.start.0..=range.end.0).step_by(self.fixed_row_size.0 as usize))
                    .map(PageOffset)
                    .map(move |po| RowPointer::new(false, pi, po, SquashedOffset::COMMITTED_STATE))
            })
    }
    fn len(&self) -> usize {
        self.len
    }
}

criterion_group!(
    delete_table,
    bench_delete_table::<DTBTree>,
    bench_delete_table::<DTHashSet>,
    bench_delete_table::<DTHashSetFH>,
    bench_delete_table::<DTPageAndBitSet>,
    bench_delete_table::<DTPageAndOffsetRanges>, // best so far.
);
criterion_main!(delete_table);
