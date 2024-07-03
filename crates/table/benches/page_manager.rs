use core::iter;
use core::mem;
use core::time::Duration;
use criterion::measurement::{Measurement, WallTime};
use criterion::{
    black_box, criterion_group, criterion_main, Bencher, BenchmarkGroup, BenchmarkId, Criterion, Throughput,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use spacetimedb_lib::db::def::{TableDef, TableSchema};
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_table::blob_store::NullBlobStore;
use spacetimedb_table::indexes::Byte;
use spacetimedb_table::indexes::{Bytes, PageOffset, RowPointer, Size, SquashedOffset, PAGE_DATA_SIZE};
use spacetimedb_table::layout::{row_size_for_bytes, row_size_for_type};
use spacetimedb_table::pages::Pages;
use spacetimedb_table::row_type_visitor::{row_type_visitor, VarLenVisitorProgram};
use spacetimedb_table::table::Table;
use spacetimedb_table::var_len::{NullVarLenVisitor, VarLenGranule, VarLenMembers, VarLenRef};

fn time<R>(acc: &mut Duration, body: impl FnOnce() -> R) -> R {
    let start = WallTime.start();
    let ret = body();
    let end = WallTime.end(start);
    *acc = WallTime.add(acc, &end);
    black_box(ret)
}

fn iter_time_with<P, B, X>(
    b: &mut Bencher,
    x: &mut X,
    mut pre: impl FnMut(u64, &mut X) -> P,
    mut body: impl FnMut(P, u64, &mut X) -> B,
) {
    b.iter_custom(|num_iters| {
        let mut elapsed = WallTime.zero();
        for i in 0..num_iters {
            let p = black_box(pre(i, x));
            black_box(&x);
            time(&mut elapsed, || body(p, i, x));
            black_box(&x);
        }
        elapsed
    })
}

// Strictly this would be unsafe,
// since it causes UB when applied to types that contain padding/`poison`,
// but it's a benchmark so who cares.
fn as_bytes<T>(t: &T) -> &Bytes {
    let ptr = (t as *const T).cast::<Byte>();
    unsafe { std::slice::from_raw_parts(ptr, mem::size_of::<T>()) }
}

#[allow(clippy::missing_safety_doc)] // It's a benchmark, clippy. Who cares.
unsafe trait Row {
    fn row_type() -> ProductType;

    fn var_len_visitor() -> VarLenVisitorProgram {
        row_type_visitor(&Self::row_type().into())
    }

    fn to_product(self) -> ProductValue;
}

#[allow(clippy::missing_safety_doc)] // It's a benchmark, clippy. Who cares.
/// Apply only to types which:
/// - Contain no padding bytes.
/// - Contain no members which are stored BFLATN as var-len.
unsafe trait FixedLenRow: Row + Sized {
    fn as_bytes(&self) -> &Bytes {
        as_bytes(self)
    }

    fn from_u64(u: u64) -> Self;
}

unsafe impl Row for u64 {
    fn row_type() -> ProductType {
        [AlgebraicType::U64].into()
    }

    fn to_product(self) -> ProductValue {
        AlgebraicValue::U64(self).into()
    }
}

unsafe impl FixedLenRow for u64 {
    fn from_u64(u: u64) -> Self {
        u
    }
}

#[repr(C)]
struct U32x8 {
    vals: [u32; 8],
}

unsafe impl Row for U32x8 {
    fn row_type() -> ProductType {
        [AlgebraicType::U32; 8].into()
    }

    fn to_product(self) -> ProductValue {
        self.vals.map(AlgebraicValue::U32).into()
    }
}

unsafe impl FixedLenRow for U32x8 {
    fn from_u64(u: u64) -> Self {
        Self { vals: [u as u32; 8] }
    }
}

#[repr(C)]
struct U32x64 {
    vals: [u32; 64],
}

unsafe impl Row for U32x64 {
    fn row_type() -> ProductType {
        [AlgebraicType::U32; 64].into()
    }

    fn to_product(self) -> ProductValue {
        self.vals.map(AlgebraicValue::U32).into()
    }
}

unsafe impl FixedLenRow for U32x64 {
    fn from_u64(u: u64) -> Self {
        Self { vals: [u as u32; 64] }
    }
}

unsafe impl Row for Box<str> {
    fn row_type() -> ProductType {
        [AlgebraicType::String].into()
    }

    fn to_product(self) -> ProductValue {
        AlgebraicValue::String(self).into()
    }
}

const fn rows_per_page<Row: FixedLenRow>() -> usize {
    PageOffset::PAGE_END.idx() / row_size_for_type::<Row>().len()
}

#[allow(unused)]
fn var_len_rows_per_page(data_size_in_bytes: usize) -> usize {
    let var_object_size = row_size_for_type::<VarLenRef>().len()
        + VarLenGranule::bytes_to_granules(data_size_in_bytes).0 * VarLenGranule::SIZE.len();
    PageOffset::PAGE_END.idx() / var_object_size
}

fn reserve_empty_page(c: &mut Criterion) {
    const RESERVE_SIZE: Size = row_size_for_bytes(8);

    let mut group = c.benchmark_group("reserve_empty_page");
    group.throughput(Throughput::Bytes(PAGE_DATA_SIZE as _));
    group.bench_function("leave_uninit", |b| {
        let mut pages = Pages::default();
        b.iter(|| {
            let _ = black_box(pages.reserve_empty_page(RESERVE_SIZE));
        });
    });

    let fill_with_zeros = |_, _, pages: &mut Pages| {
        let page = pages.reserve_empty_page(RESERVE_SIZE).unwrap();
        let page = pages.get_page_mut(page);
        unsafe { page.zero_data() };
    };
    group.bench_function("fill_with_zeros", |b| {
        iter_time_with(b, &mut Pages::default(), |_, _| (), fill_with_zeros)
    });
}

fn insert_one_page_worth_fixed_len<R: FixedLenRow>(pages: &mut Pages, visitor: &impl VarLenMembers, val: &R) {
    let size = row_size_for_type::<R>();
    for _ in 0..rows_per_page::<R>() {
        let _ = black_box(unsafe {
            black_box(&mut *pages).insert_row(visitor, size, val.as_bytes(), &[], &mut NullBlobStore)
        });
    }
}

type Group<'a, 'b> = &'a mut BenchmarkGroup<'b, WallTime>;

// time to insert a whole bunch of rows
fn insert_one_page_fixed_len(c: &mut Criterion) {
    fn bench_insert_one_page_fixed_len<R: FixedLenRow>(group: Group<'_, '_>, visitor: &impl VarLenMembers, name: &str) {
        group.throughput(Throughput::Bytes(
            rows_per_page::<R>() as u64 * mem::size_of::<R>() as u64,
        ));
        group.bench_function(name, |b| {
            let mut pages = Pages::default();
            // `0xa5` is the alternating bit pattern, which makes incorrect accesses obvious.
            insert_one_page_worth_fixed_len(&mut pages, visitor, &R::from_u64(0xa5a5a5a5_a5a5a5a5));
            let pre = |_, pages: &mut Pages| pages.clear();
            iter_time_with(b, &mut pages, pre, |_, _, pages| {
                insert_one_page_worth_fixed_len(pages, visitor, &R::from_u64(0xdeadbeef_0badbeef))
            });
        });
    }

    let mut group = c.benchmark_group("insert_one_page_fixed_len");
    bench_insert_one_page_fixed_len::<u64>(&mut group, &NullVarLenVisitor, "u64/NullVarLenVisitor");
    bench_insert_one_page_fixed_len::<u64>(&mut group, &u64::var_len_visitor(), "u64/VarLenVisitorProgram");

    bench_insert_one_page_fixed_len::<U32x8>(&mut group, &NullVarLenVisitor, "U32x8/NullVarLenVisitor");
    bench_insert_one_page_fixed_len::<U32x8>(&mut group, &U32x8::var_len_visitor(), "U32x8/VarLenVisitorProgram");

    bench_insert_one_page_fixed_len::<U32x64>(&mut group, &NullVarLenVisitor, "U32x64/NullVarLenVisitor");
    bench_insert_one_page_fixed_len::<U32x64>(&mut group, &U32x64::var_len_visitor(), "U32x64/VarLenVisitorProgram");
}

fn fill_page_with_fixed_len_collect_row_pointers<R: FixedLenRow>(
    pages: &mut Pages,
    visitor: &impl VarLenMembers,
    val: &R,
) -> Vec<RowPointer> {
    let mut ptrs = Vec::with_capacity(rows_per_page::<R>());
    for _ in 0..rows_per_page::<R>() {
        let (page, offset) = unsafe {
            pages.insert_row(
                visitor,
                row_size_for_type::<R>(),
                val.as_bytes(),
                &[],
                &mut NullBlobStore,
            )
        }
        .unwrap();
        let ptr = RowPointer::new(false, page, offset, SquashedOffset::COMMITTED_STATE);
        ptrs.push(ptr);
    }
    ptrs
}

// insert a whole bunch of rows, then time to delete them all
fn delete_one_page_fixed_len(c: &mut Criterion) {
    fn bench_delete_one_page_fixed_len<R: FixedLenRow>(group: Group<'_, '_>, visitor: &impl VarLenMembers, name: &str) {
        let rows_per_page = rows_per_page::<R>();

        group.throughput(Throughput::Bytes(rows_per_page as u64 * mem::size_of::<R>() as u64));

        group.bench_function(name, |b| {
            let pre = |i, pages: &mut _| {
                let val = R::from_u64(i);
                fill_page_with_fixed_len_collect_row_pointers::<R>(pages, visitor, &val)
            };
            iter_time_with(b, &mut Pages::default(), pre, |ptrs, _, pages| {
                for ptr in ptrs {
                    unsafe { pages.delete_row(visitor, row_size_for_type::<R>(), black_box(ptr), &mut NullBlobStore) };
                }
            });
        });
    }

    let mut group = c.benchmark_group("delete_one_page_fixed_len");

    bench_delete_one_page_fixed_len::<u64>(&mut group, &NullVarLenVisitor, "u64/NullVarLenVisitor");
    bench_delete_one_page_fixed_len::<u64>(&mut group, &u64::var_len_visitor(), "u64/VarLenVisitorProgram");

    bench_delete_one_page_fixed_len::<U32x8>(&mut group, &NullVarLenVisitor, "U32x8/NullVarLenVisitor");
    bench_delete_one_page_fixed_len::<U32x8>(&mut group, &U32x8::var_len_visitor(), "U32x8/VarLenVisitorProgram");

    bench_delete_one_page_fixed_len::<U32x64>(&mut group, &NullVarLenVisitor, "U32x64/NullVarLenVisitor");
    bench_delete_one_page_fixed_len::<U32x64>(&mut group, &U32x64::var_len_visitor(), "U32x64/VarLenVisitorProgram");
}

// insert a whole bunch of rows, then time to access them
fn retrieve_one_page_fixed_len(c: &mut Criterion) {
    fn bench_retrieve_one_page<R: FixedLenRow>(group: Group<'_, '_>, visitor: &impl VarLenMembers, name: &str) {
        let rows_per_page = rows_per_page::<R>();
        group.throughput(Throughput::Bytes(rows_per_page as u64 * mem::size_of::<R>() as u64));

        group.bench_function(name, |b| {
            let mut pages = Pages::default();

            let ptrs =
                fill_page_with_fixed_len_collect_row_pointers(&mut pages, visitor, &R::from_u64(0xdeadbeef_0badbeef));

            b.iter(|| {
                for &ptr in &ptrs {
                    let bytes = black_box(&pages).get_fixed_len_row(ptr, row_size_for_type::<R>());
                    let val: *const R = bytes.as_ptr().cast();
                    let val: R = unsafe { std::ptr::read(val) };
                    black_box(val);
                }
            });
        });
    }

    let mut group = c.benchmark_group("retrieve_one_page_fixed_len");

    bench_retrieve_one_page::<u64>(&mut group, &NullVarLenVisitor, "u64/NullVarLenVisitor");
    bench_retrieve_one_page::<u64>(&mut group, &u64::var_len_visitor(), "u64/VarLenVisitorProgram");

    bench_retrieve_one_page::<U32x8>(&mut group, &NullVarLenVisitor, "U32x8/NullVarLenVisitor");
    bench_retrieve_one_page::<U32x8>(&mut group, &U32x8::var_len_visitor(), "U32x8/VarLenVisitorProgram");

    bench_retrieve_one_page::<U32x64>(&mut group, &NullVarLenVisitor, "U32x64/NullVarLenVisitor");
    bench_retrieve_one_page::<U32x64>(&mut group, &U32x64::var_len_visitor(), "U32x64/VarLenVisitorProgram");
}

// insert a bunch of rows,
// delete some fraction of them to create holes in multiple pages,
// then time to insert into those holes
fn insert_with_holes_fixed_len(c: &mut Criterion) {
    fn bench_insert_with_holes<R: FixedLenRow>(c: &mut Criterion, var_len_visitor: &impl VarLenMembers, name: &str) {
        let mut group = c.benchmark_group(format!("insert_with_holes_fixed_len/{}", name));
        let val = R::from_u64(0xdeadbeef_0badbeef);
        for delete_ratio in [0.1f64, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let num_pages = 16;
            let total_num_rows = rows_per_page::<R>() * num_pages;
            let num_to_delete = (total_num_rows as f64 * delete_ratio) as usize;

            let num_to_delete_in_bytes = num_to_delete * mem::size_of::<R>();

            let row_size = row_size_for_type::<R>();

            group.throughput(Throughput::Bytes(num_to_delete_in_bytes as u64));

            group.bench_function(delete_ratio.to_string(), |b| {
                let mut pages = Pages::default();

                let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);

                for _ in 0..num_pages {
                    let page = pages.reserve_empty_page(row_size).unwrap();
                    let page = pages.get_page_mut(page);

                    unsafe { page.zero_data() };
                }

                let pre = |_, pages: &mut Pages| {
                    pages.clear();
                    let mut ptrs_to_delete = Vec::with_capacity(num_to_delete);
                    for _ in 0..total_num_rows {
                        let (page_idx, offset) = unsafe {
                            pages.insert_row(var_len_visitor, row_size, val.as_bytes(), &[], &mut NullBlobStore)
                        }
                        .unwrap();

                        if rng.gen_bool(delete_ratio) {
                            ptrs_to_delete.push(RowPointer::new(
                                false,
                                page_idx,
                                offset,
                                SquashedOffset::COMMITTED_STATE,
                            ));
                        }
                    }
                    let actual_num_deleted = ptrs_to_delete.len();
                    for ptr in ptrs_to_delete {
                        unsafe {
                            pages.delete_row(var_len_visitor, row_size, ptr, &mut NullBlobStore);
                        }
                    }
                    actual_num_deleted
                };
                let body = |actual_num_deleted, _, pages: &mut Pages| {
                    for _ in 0..actual_num_deleted {
                        let _ = black_box(unsafe {
                            pages.insert_row(var_len_visitor, row_size, val.as_bytes(), &[], &mut NullBlobStore)
                        });
                    }
                };
                iter_time_with(b, &mut pages, pre, body);
            });
        }
    }

    bench_insert_with_holes::<u64>(c, &NullVarLenVisitor, "u64/NullVarLenVisitor");
    bench_insert_with_holes::<u64>(c, &u64::var_len_visitor(), "u64/VarLenVisitorProgram");

    bench_insert_with_holes::<U32x8>(c, &NullVarLenVisitor, "U32x8/NullVarLenVisitor");
    bench_insert_with_holes::<U32x8>(c, &U32x8::var_len_visitor(), "U32x8/VarLenVisitorProgram");

    bench_insert_with_holes::<U32x64>(c, &NullVarLenVisitor, "U32x64/NullVarLenVisitor");
    bench_insert_with_holes::<U32x64>(c, &U32x64::var_len_visitor(), "U32x64/VarLenVisitorProgram");
}

// insert a whole bunch of rows, then time to copy_filter materialize a view

fn copy_filter_fixed_len(c: &mut Criterion) {
    fn bench_copy_filter<R: FixedLenRow>(c: &mut Criterion, name: &str) {
        let mut group = c.benchmark_group(format!("copy_filter_fixed_len/{}", name));
        let row_size = black_box(row_size_for_type::<R>());

        let val = R::from_u64(0xdeadbeef_0badbeef);
        for keep_ratio in [0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let visitor = &NullVarLenVisitor;
            let mut pages = Pages::default();

            let num_pages = 16;
            let total_num_rows = rows_per_page::<R>() * num_pages;

            for _ in 0..total_num_rows {
                unsafe { pages.insert_row(visitor, row_size, val.as_bytes(), &[], &mut NullBlobStore) }.unwrap();
            }

            let num_to_keep = (total_num_rows as f64 * keep_ratio) as usize;
            let num_to_keep_bytes = num_to_keep * mem::size_of::<R>();

            group.throughput(Throughput::Bytes(num_to_keep_bytes as u64));

            let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);

            // To avoid advancing RNG in the benchmark,
            // precompute a big vec of bools, with one bool for each value that we may or may not keep.
            let keep_seq: Vec<bool> = (0..total_num_rows).map(|_| rng.gen_bool(keep_ratio)).collect();

            group.bench_function(keep_ratio.to_string(), |b| {
                b.iter_with_large_drop(|| unsafe {
                    let mut keep_iter = keep_seq.iter().copied();
                    black_box(&pages).copy_filter(visitor, row_size, &mut NullBlobStore, |_, _| {
                        black_box(keep_iter.next().unwrap_or_default())
                    })
                });
            });
        }
    }

    bench_copy_filter::<u64>(c, "u64");
    bench_copy_filter::<U32x8>(c, "U32x8");
    bench_copy_filter::<U32x64>(c, "U32x64");
}

// TODO(bench):
// - Duplicate above benchmarks with var-len rows of various sizes
//   - In the insert-with-holes benchmark, randomize size of each row to simulate fragmentation.
// - Extend above benchmarks to go through `Table` with `AlgebraicValue`.

criterion_group!(
    pages,
    reserve_empty_page,
    insert_one_page_fixed_len,
    delete_one_page_fixed_len,
    retrieve_one_page_fixed_len,
    insert_with_holes_fixed_len,
    copy_filter_fixed_len,
);

fn schema_from_ty(ty: ProductType, name: &str) -> TableSchema {
    TableSchema::from_def(TableId(0), TableDef::from_product(name, ty))
}

fn make_table(c: &mut Criterion) {
    fn bench_make_table<R: Row>(group: Group<'_, '_>, name: &str) {
        let ty = R::row_type();
        let schema = schema_from_ty(ty.clone(), name);
        group.bench_function(name, |b| {
            b.iter_custom(|num_iters| {
                let schemas = vec![schema.clone(); num_iters as usize];
                let mut tables = Vec::with_capacity(num_iters as usize);
                let start = WallTime.start();
                for schema in schemas {
                    tables.push(Table::new(schema.into(), SquashedOffset::COMMITTED_STATE));
                }
                let elapsed = WallTime.end(start);
                black_box(tables);
                elapsed
            });
        });
    }

    let mut group = c.benchmark_group("make_table");

    bench_make_table::<u64>(&mut group, "u64");
    bench_make_table::<U32x8>(&mut group, "U32x8");
    bench_make_table::<U32x64>(&mut group, "U32x64");
}

fn make_table_for_row_type<R: Row>(name: &str) -> Table {
    let ty = R::row_type();
    let schema = schema_from_ty(ty.clone(), name);
    Table::new(schema.into(), SquashedOffset::COMMITTED_STATE)
}

fn use_type_throughput<T>(group: &mut BenchmarkGroup<'_, impl Measurement>) {
    group.throughput(Throughput::Bytes(mem::size_of::<T>() as u64));
}

fn table_insert_one_row(c: &mut Criterion) {
    fn bench_insert_row<R: Row>(group: Group<'_, '_>, val: R, name: &str) {
        let table = make_table_for_row_type::<R>(name);
        let val = black_box(val.to_product());

        // Insert before benching to alloc and fault in a page.
        let mut ctx = (table, NullBlobStore);
        let ptr = ctx.0.insert(&mut ctx.1, &val).unwrap().1.pointer();
        let pre = |_, (table, bs): &mut (Table, NullBlobStore)| {
            table.delete(bs, ptr, |_| ()).unwrap();
        };
        group.bench_function(name, |b| {
            iter_time_with(b, &mut ctx, pre, |_, _, (table, bs)| {
                table.insert(bs, &val).map(|r| r.1.pointer())
            });
        });
    }

    {
        let mut group = c.benchmark_group("table_insert_one_row/fixed_len");

        use_type_throughput::<u64>(&mut group);
        bench_insert_row(&mut group, 0xdeadbeef_0badbabeu64, "u64");

        use_type_throughput::<U32x8>(&mut group);
        bench_insert_row(&mut group, U32x8::from_u64(0xdeadbeef_0badbabe), "U32x8");

        use_type_throughput::<U32x64>(&mut group);
        bench_insert_row(&mut group, U32x64::from_u64(0xdeadbeef_0badbabe), "U32x64");
    }

    let mut group = c.benchmark_group("table_insert_one_row/String");

    group.throughput(Throughput::Elements(1));
    bench_insert_row(&mut group, Box::from(""), "0");

    group.throughput(Throughput::Bytes(1));
    bench_insert_row(&mut group, Box::from("a"), "1");

    for num_granules in [1, 2, 4, 8, 16] {
        let num_bytes = VarLenGranule::DATA_SIZE * num_granules;
        group.throughput(Throughput::Bytes(num_bytes as u64));
        bench_insert_row(
            &mut group,
            "a".repeat(num_bytes).into_boxed_str(),
            &num_bytes.to_string(),
        );
    }
}

fn table_delete_one_row(c: &mut Criterion) {
    fn bench_delete_row<R: Row>(group: Group<'_, '_>, val: R, name: &str) {
        let table = make_table_for_row_type::<R>(name);
        let val = val.to_product();

        // Insert before benching to alloc and fault in a page.
        let mut ctx = (table, NullBlobStore);
        let insert = |_: u64, (table, bs): &mut (Table, NullBlobStore)| table.insert(bs, &val).unwrap().1.pointer();

        group.bench_function(name, |b| {
            iter_time_with(b, &mut ctx, insert, |row, _, (table, bs)| table.delete(bs, row, |_| ()));
        });
    }

    {
        let mut group = c.benchmark_group("table_delete_one_row/fixed_len");

        use_type_throughput::<u64>(&mut group);
        bench_delete_row(&mut group, 0xdeadbeef_0badbabeu64, "u64");

        use_type_throughput::<U32x8>(&mut group);
        bench_delete_row(&mut group, U32x8::from_u64(0xdeadbeef_0badbabe), "U32x8");

        use_type_throughput::<U32x64>(&mut group);
        bench_delete_row(&mut group, U32x64::from_u64(0xdeadbeef_0badbabe), "U32x64");
    }

    let mut group = c.benchmark_group("table_delete_one_row/String");

    group.throughput(Throughput::Elements(1));
    bench_delete_row(&mut group, Box::from(""), "0");

    group.throughput(Throughput::Bytes(1));
    bench_delete_row(&mut group, Box::from("a"), "1");

    for num_granules in [1, 2, 4, 8, 16] {
        let num_bytes = VarLenGranule::DATA_SIZE * num_granules;
        group.throughput(Throughput::Bytes(num_bytes as u64));
        bench_delete_row(
            &mut group,
            "a".repeat(num_bytes).into_boxed_str(),
            &num_bytes.to_string(),
        );
    }
}

fn table_extract_one_row(c: &mut Criterion) {
    fn bench_extract_row<R: Row>(group: Group<'_, '_>, val: R, name: &str) {
        let mut table = make_table_for_row_type::<R>(name);
        let val = val.to_product();

        let mut blob_store = NullBlobStore;
        let row = black_box(table.insert(&mut blob_store, &val).unwrap().1);
        group.bench_function(name, |b| {
            b.iter_with_large_drop(|| black_box(row.to_product_value()));
        });
    }

    {
        let mut group = c.benchmark_group("table_extract_one_row/fixed_len");

        use_type_throughput::<u64>(&mut group);
        bench_extract_row(&mut group, 0xdeadbeef_0badbabeu64, "u64");

        use_type_throughput::<U32x8>(&mut group);
        bench_extract_row(&mut group, U32x8::from_u64(0xdeadbeef_0badbabe), "U32x8");

        use_type_throughput::<U32x64>(&mut group);
        bench_extract_row(&mut group, U32x64::from_u64(0xdeadbeef_0badbabe), "U32x64");
    }

    let mut group = c.benchmark_group("table_extract_one_row/String");

    group.throughput(Throughput::Elements(1));
    bench_extract_row(&mut group, Box::from(""), "0");

    group.throughput(Throughput::Bytes(1));
    bench_extract_row(&mut group, Box::from("a"), "1");

    for num_granules in [1, 2, 4, 8, 16] {
        let num_bytes = VarLenGranule::DATA_SIZE * num_granules;
        group.throughput(Throughput::Bytes(num_bytes as u64));
        bench_extract_row(
            &mut group,
            "a".repeat(num_bytes).into_boxed_str(),
            &num_bytes.to_string(),
        );
    }
}

// TODO(bench):
// - table insert_with_holes benchmark
// - table copy_filter benchmark
// - index benchmarks: for a variety of table sizes,
//  - insert a row into a table with an index
//  - delete a row from a table with an index
//  - seek a row in an index

criterion_group!(
    table,
    make_table,
    table_insert_one_row,
    table_delete_one_row,
    table_extract_one_row,
);

trait IndexedRow: Row + Sized {
    fn indexed_columns() -> ColList {
        0.into()
    }
    fn make_table_def() -> TableDef {
        TableDef::from_product(std::any::type_name::<Self>(), Self::row_type())
            .with_column_index(Self::indexed_columns(), false)
    }
    fn make_schema() -> TableSchema {
        TableSchema::from_def(TableId(0), Self::make_table_def())
    }
    fn throughput() -> Throughput {
        Throughput::Bytes(mem::size_of::<Self>() as u64)
    }
    fn column_value_from_u64(u: u64) -> AlgebraicValue;
}

impl IndexedRow for u64 {
    fn column_value_from_u64(u: u64) -> AlgebraicValue {
        AlgebraicValue::U64(u)
    }
}

impl IndexedRow for U32x8 {
    fn column_value_from_u64(u: u64) -> AlgebraicValue {
        AlgebraicValue::U32(u as _)
    }
}

impl IndexedRow for U32x64 {
    fn column_value_from_u64(u: u64) -> AlgebraicValue {
        AlgebraicValue::U32(u as _)
    }
}

impl IndexedRow for Box<str> {
    fn column_value_from_u64(u: u64) -> AlgebraicValue {
        AlgebraicValue::String(u.to_string().into())
    }
    fn throughput() -> Throughput {
        // I'm too lazy to come up with an interface that computes the length of a string
        // and passes it to throughput.
        Throughput::Elements(1)
    }
}

fn make_table_with_indexes<R: IndexedRow>() -> Table {
    let schema = R::make_schema();
    let mut tbl = Table::new(schema.into(), SquashedOffset::COMMITTED_STATE);

    let cols = R::indexed_columns();
    let idx = tbl.new_index(IndexId(0), &cols, false).unwrap();
    tbl.insert_index(&NullBlobStore, cols, idx);

    tbl
}

#[cfg(not(debug_assertions))]
const TABLE_SIZE_POWERS: [u64; 7] = [0, 3, 5, 8, 11, 14, 17];
#[cfg(debug_assertions)]
const TABLE_SIZE_POWERS: [u64; 6] = [0, 3, 5, 8, 11, 14];

fn powers<const N: usize>(ps: [u64; N]) -> [u64; N] {
    ps.map(|n| 1 << n)
}

fn insert_num_same<R: IndexedRow>(
    tbl: &mut Table,
    mut make_row: impl FnMut() -> R,
    num_same: usize,
) -> Option<RowPointer> {
    iter::repeat(make_row().to_product())
        .take(num_same)
        .zip(0u32..)
        .map(|(mut row, n)| {
            if let Some(slot) = row.elements.get_mut(1) {
                *slot = n.into();
            }
            tbl.insert(&mut NullBlobStore, &row).map(|(_, row)| row.pointer()).ok()
        })
        .last()
        .flatten()
}

fn clear_all_same<R: IndexedRow>(tbl: &mut Table, val_same: u64) {
    let ptrs = tbl
        .index_seek(
            &NullBlobStore,
            &R::indexed_columns(),
            &R::column_value_from_u64(val_same),
        )
        .unwrap()
        .map(|r| r.pointer())
        .collect::<Vec<_>>();
    for ptr in ptrs {
        tbl.delete(&mut NullBlobStore, ptr, |_| ()).unwrap();
    }
}

fn bench_id_for_index(name: &str, num_rows: u64, same_ratio: f64, num_same: usize) -> BenchmarkId {
    BenchmarkId::new(
        name,
        format_args!("(rows = {num_rows}, sratio = {same_ratio}, snum = {num_same})"),
    )
}

fn make_table_with_same_ratio<R: IndexedRow>(
    mut make_row: impl FnMut(u64) -> R,
    num_rows: u64,
    same_ratio: f64,
) -> (Table, usize, u64) {
    let mut tbl = make_table_with_indexes::<R>();

    let num_same = (num_rows as f64 * same_ratio) as usize;
    let num_same = num_same.max(1);
    let num_diff = num_rows / num_same as u64;

    for i in 0..num_diff {
        insert_num_same(&mut tbl, || make_row(i), num_same);
    }

    (tbl, num_same, num_diff)
}

fn index_insert(c: &mut Criterion) {
    fn bench_index_insert<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        num_rows: u64,
        same_ratio: f64,
    ) {
        let make_row_move = &mut make_row;
        let (tbl, num_same, _) = make_table_with_same_ratio::<R>(make_row_move, num_rows, same_ratio);
        let mut ctx = (tbl, NullBlobStore);

        group.bench_with_input(
            bench_id_for_index(name, num_rows, same_ratio, num_same),
            &num_rows,
            |b, &num_rows| {
                let pre = |_, (tbl, _): &mut (Table, NullBlobStore)| {
                    clear_all_same::<R>(tbl, num_rows);
                    insert_num_same(tbl, || make_row(num_rows), num_same - 1);
                    make_row(num_rows).to_product()
                };
                iter_time_with(b, &mut ctx, pre, |row, _, (tbl, bs)| {
                    tbl.insert(bs, &row).map(|r| r.1.pointer())
                });
            },
        );
    }
    fn bench_many_table_sizes<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        same_ratio: f64,
    ) {
        group.throughput(R::throughput());
        for num_rows in powers(TABLE_SIZE_POWERS) {
            bench_index_insert(&mut make_row, group, name, num_rows, same_ratio);
        }
    }

    let mut group = c.benchmark_group("index_insert");

    bench_many_table_sizes::<u64>(FixedLenRow::from_u64, &mut group, "u64", 0.0);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.00);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.01);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.05);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.10);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.25);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.50);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 1.00);
    bench_many_table_sizes::<U32x64>(FixedLenRow::from_u64, &mut group, "U32x64", 0.0);
    bench_many_table_sizes::<Box<str>>(|i| i.to_string().into(), &mut group, "String", 0.0);
}

fn index_seek(c: &mut Criterion) {
    fn bench_index_seek<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        num_rows: u64,
        same_ratio: f64,
    ) {
        let make_row_move = &mut make_row;
        let (tbl, num_same, num_diff) = make_table_with_same_ratio::<R>(make_row_move, num_rows, same_ratio);

        group.bench_with_input(
            bench_id_for_index(name, num_rows, same_ratio, num_same),
            &num_diff,
            |b, &num_diff| {
                let col_to_seek = black_box(R::column_value_from_u64(num_diff / 2));
                let col_ids = black_box(R::indexed_columns());
                b.iter_custom(|num_iters| {
                    let mut elapsed = WallTime.zero();
                    for _ in 0..num_iters {
                        let (row, none) = time(&mut elapsed, || {
                            let mut iter = black_box(&tbl)
                                .index_seek(&NullBlobStore, &col_ids, &col_to_seek)
                                .unwrap();
                            (iter.next(), iter.next())
                        });
                        assert!(
                            num_same > 1 || none.is_none(),
                            "Found a second row at {:?}: {:?} (first row is {:?})",
                            none,
                            none.unwrap().to_product_value(),
                            row.unwrap().to_product_value(),
                        );
                    }
                    elapsed
                });
            },
        );
    }
    fn bench_many_table_sizes<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        same_ratio: f64,
    ) {
        group.throughput(Throughput::Elements(1));
        for num_rows in powers(TABLE_SIZE_POWERS) {
            bench_index_seek(&mut make_row, group, name, num_rows, same_ratio);
        }
    }

    let mut group = c.benchmark_group("index_seek");

    bench_many_table_sizes::<u64>(FixedLenRow::from_u64, &mut group, "u64", 0.0);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.00);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.01);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.05);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.10);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.25);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.50);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 1.00);
    bench_many_table_sizes::<U32x64>(FixedLenRow::from_u64, &mut group, "U32x64", 0.0);
    bench_many_table_sizes::<Box<str>>(|i| i.to_string().into(), &mut group, "String", 0.0);
}

fn index_delete(c: &mut Criterion) {
    fn bench_index_delete<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        num_rows: u64,
        same_ratio: f64,
    ) {
        let make_row_move = &mut make_row;
        let (mut tbl, num_same, _) = make_table_with_same_ratio::<R>(make_row_move, num_rows, same_ratio);

        group.bench_with_input(
            bench_id_for_index(name, num_rows, same_ratio, num_same),
            &num_rows,
            |b, &num_rows| {
                let pre = |_, tbl: &mut Table| {
                    clear_all_same::<R>(tbl, num_rows);
                    insert_num_same(tbl, || make_row(num_rows), num_same).unwrap()
                };
                iter_time_with(b, &mut tbl, pre, |ptr, _, tbl| {
                    tbl.delete(&mut NullBlobStore, ptr, |_| ())
                });
            },
        );
    }
    fn bench_many_table_sizes<R: IndexedRow>(
        mut make_row: impl FnMut(u64) -> R,
        group: Group<'_, '_>,
        name: &str,
        same_ratio: f64,
    ) {
        group.throughput(R::throughput());
        for num_rows in powers(TABLE_SIZE_POWERS) {
            bench_index_delete(&mut make_row, group, name, num_rows, same_ratio);
        }
    }

    let mut group = c.benchmark_group("index_delete");

    bench_many_table_sizes::<u64>(FixedLenRow::from_u64, &mut group, "u64", 0.0);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.0);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.01);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.05);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.10);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.25);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 0.50);
    bench_many_table_sizes::<U32x8>(FixedLenRow::from_u64, &mut group, "U32x8", 1.00);
    bench_many_table_sizes::<U32x64>(FixedLenRow::from_u64, &mut group, "U32x64", 0.0);
    bench_many_table_sizes::<Box<str>>(|i| i.to_string().into(), &mut group, "String", 0.0);
}

criterion_group!(index, index_insert, index_seek, index_delete);

criterion_main!(pages, table, index);
