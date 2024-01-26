use core::hash::BuildHasher;
use core::mem::{self, MaybeUninit};
use core::time::Duration;
use criterion::measurement::{Measurement, WallTime};
use criterion::{black_box, criterion_group, criterion_main, Bencher, BenchmarkId, Criterion, Throughput};
use mem_arch_prototype::bflatn_from::serialize_row_from_page;
use mem_arch_prototype::bflatn_to::write_row_to_page;
use mem_arch_prototype::blob_store::NullBlobStore;
use mem_arch_prototype::eq::eq_row_in_page;
use mem_arch_prototype::indexes::{PageOffset, RowHash};
use mem_arch_prototype::layout::{row_size_for_type, RowTypeLayout};
use mem_arch_prototype::page::Page;
use mem_arch_prototype::row_hash::hash_row_in_page;
use mem_arch_prototype::row_type_visitor::{row_type_visitor, VarLenVisitorProgram};
use mem_arch_prototype::util;
use mem_arch_prototype::var_len::{AlignedVarLenOffsets, NullVarLenVisitor, VarLenGranule, VarLenMembers, VarLenRef};
use rand::distributions::OpenClosed01;
use rand::prelude::Distribution;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use spacetimedb_sats::algebraic_value::ser::ValueSerializer;
use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, ArrayValue, ProductType, ProductValue};

fn time<R>(acc: &mut Duration, body: impl FnOnce() -> R) -> R {
    let start = WallTime.start();
    let ret = body();
    let end = WallTime.end(start);
    *acc = WallTime.add(acc, &end);
    black_box(ret)
}

fn iter_time_with_page<P, B>(
    b: &mut Bencher,
    page: &mut Page,
    mut pre: impl FnMut(&mut Page) -> P,
    mut body: impl FnMut(P, u64, &mut Page) -> B,
) {
    b.iter_custom(|num_iters| {
        let mut elapsed = WallTime.zero();
        for i in 0..num_iters {
            let p = pre(page);
            black_box(&page);
            time(&mut elapsed, || body(p, i, page));
            black_box(&page);
        }
        elapsed
    })
}

fn clear_zero(page: &mut Page) {
    page.clear();
    unsafe { page.zero_data() };
}

fn as_bytes<T>(t: &T) -> &[MaybeUninit<u8>] {
    let ptr = (t as *const T).cast::<MaybeUninit<u8>>();
    unsafe { std::slice::from_raw_parts(ptr, mem::size_of::<T>()) }
}

#[allow(clippy::missing_safety_doc)] // It's a benchmark, clippy. Who cares.
unsafe trait Row {
    fn row_type() -> ProductType;

    fn var_len_visitor() -> VarLenVisitorProgram {
        row_type_visitor(&Self::row_type().into())
    }
}

#[allow(clippy::missing_safety_doc)] // It's a benchmark, clippy. Who cares.
unsafe trait FixedLenRow: Row + Sized {
    fn as_bytes(&self) -> &[MaybeUninit<u8>] {
        as_bytes(self)
    }

    unsafe fn from_bytes(bytes: &[MaybeUninit<u8>]) -> &Self {
        let ptr = bytes.as_ptr();
        debug_assert_eq!(ptr as usize % mem::align_of::<Self>(), 0);
        debug_assert_eq!(bytes.len(), mem::size_of::<Self>());
        unsafe { &*ptr.cast::<Self>() }
    }

    fn from_u64(u: u64) -> Self;
}

fn read_fixed_len<Row: FixedLenRow>(page: &Page, offset: PageOffset) -> &Row {
    let row = black_box(page).get_row_data(offset, row_size_for_type::<Row>());
    unsafe { Row::from_bytes(row) }
}

unsafe impl Row for u64 {
    fn row_type() -> ProductType {
        [AlgebraicType::U64].into()
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
}

unsafe impl FixedLenRow for U32x64 {
    fn from_u64(u: u64) -> Self {
        Self { vals: [u as u32; 64] }
    }
}

unsafe impl Row for VarLenRef {
    fn row_type() -> ProductType {
        [AlgebraicType::String].into()
    }
}

const fn rows_per_page<Row: FixedLenRow>() -> usize {
    PageOffset::PAGE_END.idx() / row_size_for_type::<Row>().len()
}

fn var_len_rows_per_page(data_size_in_bytes: usize) -> usize {
    let var_object_size = row_size_for_type::<VarLenRef>().len()
        + VarLenGranule::bytes_to_granules(data_size_in_bytes).0 * VarLenGranule::SIZE.len();
    PageOffset::PAGE_END.idx() / var_object_size
}

fn fill_with_fixed_len<Row: FixedLenRow>(page: &mut Page, val: Row, visitor: &impl VarLenMembers) {
    while black_box(&*page).has_space_for_row(row_size_for_type::<Row>(), 0) {
        let _ = black_box(unsafe {
            black_box(&mut *page).insert_row(val.as_bytes(), &[] as &[&[u8]], visitor, &mut NullBlobStore)
        });
    }
}

fn insert_fixed_len_with_visitor<Row: FixedLenRow, V: VarLenMembers>(
    c: &mut Criterion,
    visitor: &V,
    visitor_name: &str,
) {
    let group_name = format!("insert_fixed_len/{}/{}_bytes", visitor_name, mem::size_of::<Row>());

    let mut group = c.benchmark_group(&group_name);
    group.throughput(Throughput::Bytes(
        (rows_per_page::<Row>() * mem::size_of::<Row>()) as u64,
    ));

    group.bench_function("clean_page", |b| {
        let mut page = Page::new(row_size_for_type::<Row>());
        unsafe { page.zero_data() };
        iter_time_with_page(b, &mut page, clear_zero, |_, i, page| {
            fill_with_fixed_len(page, Row::from_u64(i), visitor)
        });
    });

    group.bench_function("dirty_page", |b| {
        // Setup: alloc and fill a page, so it's dirty.
        let mut page = Page::new(row_size_for_type::<Row>());
        let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);
        fill_with_fixed_len(&mut page, Row::from_u64(0), visitor);
        let pre = |page: &mut _| delete_all_shuffled::<Row>(page, &mut rng);
        iter_time_with_page(b, &mut page, pre, |_, i, page| {
            fill_with_fixed_len(page, Row::from_u64(i), visitor)
        });
    });
}

fn insert_fixed_len<Row: FixedLenRow>(c: &mut Criterion) {
    insert_fixed_len_with_visitor::<Row, _>(c, &NullVarLenVisitor, "NullVarLenVisitor");
    insert_fixed_len_with_visitor::<Row, _>(c, &AlignedVarLenOffsets::from_offsets(&[]), "AlignedVarLenOffsets");

    let visitor = Row::var_len_visitor();
    insert_fixed_len_with_visitor::<Row, _>(c, &visitor, "VarLenVisitorProgram");
}

fn delete_all_shuffled<R: Row>(page: &mut Page, rng: &mut impl Rng) {
    let mut row_offsets = page.iter_fixed_len(row_size_for_type::<R>()).collect::<Vec<_>>();
    // Asserts in here are fine; we don't time this function.
    assert_eq!(row_offsets.len(), page.num_rows());
    // Delete in a random order, so the freelist isn't in (reverse) order.
    row_offsets.shuffle(rng);

    let visitor = R::var_len_visitor();
    for row in row_offsets {
        unsafe { page.delete_row(row, row_size_for_type::<R>(), &visitor, &mut NullBlobStore) };
    }
    assert_eq!(page.num_rows(), 0);
}

fn fill_with_var_len(page: &mut Page, var_len: &[u8], visitor: &impl VarLenMembers) {
    while black_box(&*page).has_space_for_row_with_objects(row_size_for_type::<VarLenRef>(), &[var_len]) {
        let _ = black_box(unsafe {
            black_box(&mut *page).insert_row(as_bytes(&VarLenRef::NULL), &[var_len], visitor, &mut NullBlobStore)
        });
    }
}

const VL_SIZES: [usize; 6] = [
    1,
    VarLenGranule::DATA_SIZE,
    VarLenGranule::DATA_SIZE * 2,
    VarLenGranule::DATA_SIZE * 4,
    VarLenGranule::DATA_SIZE * 8,
    VarLenGranule::DATA_SIZE * 16,
];

fn insert_var_len_clean_page(c: &mut Criterion, visitor: &impl VarLenMembers, visitor_name: &str) {
    let mut group = c.benchmark_group(format!("insert_var_len/{}/clean_page", visitor_name));

    for len_in_bytes in VL_SIZES {
        group.throughput(Throughput::Bytes(
            (var_len_rows_per_page(len_in_bytes) * len_in_bytes) as u64,
        ));

        group.bench_with_input(
            BenchmarkId::new("fill_with_objs_of_size", len_in_bytes),
            &len_in_bytes,
            |b, &len_in_bytes| {
                let mut page = Page::new(row_size_for_type::<VarLenRef>());
                unsafe { page.zero_data() };
                let data = [0xa5u8].repeat(len_in_bytes);
                iter_time_with_page(b, &mut page, clear_zero, |_, _, page| {
                    fill_with_var_len(page, &data, visitor)
                });
            },
        );
    }
}

fn insert_var_len_dirty_page(c: &mut Criterion, visitor: &impl VarLenMembers, visitor_name: &str) {
    let mut group = c.benchmark_group(format!("insert_var_len/{}/dirty_page", visitor_name));

    for len_in_bytes in VL_SIZES {
        group.throughput(Throughput::Bytes(
            (var_len_rows_per_page(len_in_bytes) * len_in_bytes) as u64,
        ));

        group.bench_with_input(
            BenchmarkId::new("fill_with_objs_of_size", len_in_bytes),
            &len_in_bytes,
            |b, &len_in_bytes| {
                let mut page = Page::new(row_size_for_type::<VarLenRef>());
                let data = [0xa5u8].repeat(len_in_bytes);
                fill_with_var_len(&mut page, &data, visitor);

                let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);

                let pre = |page: &mut _| delete_all_shuffled::<VarLenRef>(page, &mut rng);
                iter_time_with_page(b, &mut page, pre, |_, _, page| fill_with_var_len(page, &data, visitor));
            },
        );
    }
}

fn insert_var_len(c: &mut Criterion) {
    let offset_visitor = AlignedVarLenOffsets::from_offsets(&[0]);
    insert_var_len_clean_page(c, &offset_visitor, "AlignedVarLenOffsets");
    insert_var_len_dirty_page(c, &offset_visitor, "AlignedVarLenOffsets");

    let program_visitor = VarLenRef::var_len_visitor();
    insert_var_len_clean_page(c, &program_visitor, "VarLenVisitorProgram");
    insert_var_len_dirty_page(c, &program_visitor, "VarLenVisitorProgram");
}

fn insert_opt_str(c: &mut Criterion) {
    let typ: ProductType = [AlgebraicType::sum([AlgebraicType::String, AlgebraicType::unit()])].into();
    let typ: RowTypeLayout = typ.into();

    let visitor = row_type_visitor(&typ);
    let fixed_row_size = typ.size();
    assert!(fixed_row_size.len() == 6);
    let mut clean_page_group = c.benchmark_group("insert_optional_str/clean_page");

    let mut variant_none = util::uninit_array::<u8, 6>();
    variant_none[4].write(1);

    let mut variant_some = util::uninit_array::<u8, 6>();
    variant_some[4].write(0);

    for some_ratio in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        for &data_length_in_bytes in if some_ratio == 0.0 { &[0][..] } else { &VL_SIZES } {
            let input = (some_ratio, data_length_in_bytes);
            let avg_row_useful_size = 1.0 + data_length_in_bytes as f64 * some_ratio;
            let granules_per_row = VarLenGranule::bytes_to_granules(data_length_in_bytes).0;
            let avg_row_stored_size =
                fixed_row_size.len() as f64 + (granules_per_row * VarLenGranule::SIZE.len()) as f64 * some_ratio;
            let rows_per_page = PageOffset::PAGE_END.idx() as f64 / avg_row_stored_size;

            clean_page_group.throughput(Throughput::Bytes((rows_per_page * avg_row_useful_size) as u64));

            let var_len_data = [0xa5].repeat(data_length_in_bytes);
            clean_page_group.bench_with_input(
                BenchmarkId::new(
                    "(some_ratio, length_in_bytes)",
                    format!("({}, {})", some_ratio, data_length_in_bytes),
                ),
                &input,
                |b, &(some_ratio, _)| {
                    let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);
                    let mut page = Page::new(fixed_row_size);
                    unsafe { page.zero_data() };

                    let body = |_, _, page: &mut Page| loop {
                        let insert_none = <OpenClosed01 as Distribution<f64>>::sample(&OpenClosed01, &mut rng);
                        if insert_none > some_ratio {
                            if !page.has_space_for_row(fixed_row_size, 0) {
                                break;
                            }
                            let _ = unsafe {
                                page.insert_row(&variant_none, &[] as &[&[u8]], &visitor, &mut NullBlobStore)
                            };
                        } else {
                            if !page.has_space_for_row_with_objects(fixed_row_size, &[&var_len_data]) {
                                break;
                            }
                            let _ = unsafe {
                                page.insert_row(&variant_some, &[&var_len_data], &visitor, &mut NullBlobStore)
                            };
                        }
                    };
                    iter_time_with_page(b, &mut page, clear_zero, body);
                },
            );
        }
    }
}

criterion_group!(
    insert,
    insert_fixed_len::<u64>,
    insert_fixed_len::<U32x8>,
    insert_fixed_len::<U32x64>,
    insert_var_len,
    insert_opt_str,
);

fn delete_to_approx_fullness_ratio<Row: FixedLenRow>(page: &mut Page, fullness: f64) {
    assert!(fullness > 0.0 && fullness < 1.0);
    let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);
    let row_offsets = page.iter_fixed_len(row_size_for_type::<Row>()).collect::<Vec<_>>();
    let visitor = Row::var_len_visitor();
    for row in row_offsets.into_iter() {
        let should_keep = <OpenClosed01 as Distribution<f64>>::sample(&OpenClosed01, &mut rng);
        if should_keep > fullness {
            unsafe { page.delete_row(row, row_size_for_type::<Row>(), &visitor, &mut NullBlobStore) };
        }
    }
}

fn iter_read_fixed_len_from_page<Row: FixedLenRow>(b: &mut Bencher, page: &Page) {
    b.iter(|| {
        for row_offset in black_box(page).iter_fixed_len(row_size_for_type::<Row>()) {
            black_box(read_fixed_len::<Row>(black_box(page), row_offset));
        }
    });
}

/// For various `fullness_ratio`s,
/// construct a page of `u64`s
/// which is `fullness_ratio` full,
/// then benchmark iterating over it and reading each row.
fn iter_read_fixed_len<Row: FixedLenRow>(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("iter_read_fixed_len {}_byte", mem::size_of::<Row>()));

    let visitor = Row::var_len_visitor();

    for fullness_ratio in [0.1, 0.25, 0.5, 0.75, 0.9] {
        // Construct a page which is approximately `fullness_ratio` full,
        // i.e. contains approximately `fullness_ratio * U64S_PER_PAGE` rows.
        let mut partial_page = Page::new(row_size_for_type::<Row>());
        fill_with_fixed_len::<Row>(&mut partial_page, Row::from_u64(0xa5a5a5a5_a5a5a5a5), &visitor);
        // `delete_u64s_to_approx_fullness_ratio` uses a seeded `StdRng`,
        // so this should be consistent-ish.
        // It is liable to change on different machines or when updating `rand`.
        // TODO: use `rand_chacha`?
        delete_to_approx_fullness_ratio::<Row>(&mut partial_page, fullness_ratio);

        // Throughput in rows visited per sample,
        // i.e. the number of rows in the page.
        group.throughput(Throughput::Bytes(
            (partial_page.num_rows() * mem::size_of::<Row>()) as u64,
        ));
        group.bench_with_input(
            BenchmarkId::new(
                format!("iter_read_fixed_len {}_byte", mem::size_of::<Row>()),
                fullness_ratio,
            ),
            &*partial_page,
            iter_read_fixed_len_from_page::<Row>,
        );
    }

    let mut full_page = Page::new(row_size_for_type::<Row>());
    fill_with_fixed_len(&mut full_page, Row::from_u64(0xa5a5a5a5_a5a5a5a5), &visitor);
    group.throughput(Throughput::Bytes((full_page.num_rows() * mem::size_of::<Row>()) as u64));
    group.bench_with_input(
        BenchmarkId::new(format!("iter_read_fixed_len {}_byte", mem::size_of::<Row>()), 1.0),
        &*full_page,
        iter_read_fixed_len_from_page::<Row>,
    );
}

criterion_group!(
    iter,
    iter_read_fixed_len::<u64>,
    iter_read_fixed_len::<U32x8>,
    iter_read_fixed_len::<U32x64>,
);

fn copy_filter_into_fixed_len_keep_ratio<Row: FixedLenRow>(b: &mut Bencher, keep_ratio: &f64) {
    let visitor = Row::var_len_visitor();

    let mut target_page = Page::new(row_size_for_type::<Row>());

    let mut src_page = Page::new(row_size_for_type::<Row>());
    fill_with_fixed_len::<Row>(&mut src_page, Row::from_u64(0xa5a5a5a5_a5a5a5a5), &visitor);

    let mut rng = StdRng::seed_from_u64(0xa5a5a5a5_a5a5a5a5);

    iter_time_with_page(b, &mut target_page, clear_zero, |_, _, target_page| {
        let _ = unsafe {
            black_box(&src_page).copy_filter_into(
                PageOffset(0),
                target_page,
                row_size_for_type::<Row>(),
                &visitor,
                &mut NullBlobStore,
                |_page, _row| {
                    let should_keep = <OpenClosed01 as Distribution<f64>>::sample(&OpenClosed01, &mut rng);
                    should_keep < *keep_ratio
                },
            )
        };
    });
}

fn copy_filter_into_fixed_len<Row: FixedLenRow>(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("copy_filter_into_fixed_len {}_byte", mem::size_of::<Row>()));

    for keep_ratio in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        group.throughput(if keep_ratio == 0.0 {
            Throughput::Elements(1)
        } else {
            Throughput::Bytes(((rows_per_page::<Row>() as f64) * keep_ratio) as u64 * mem::size_of::<Row>() as u64)
        });
        group.bench_with_input(
            BenchmarkId::new(
                format!("copy_filter_into_fixed_len {}_byte", mem::size_of::<Row>()),
                keep_ratio,
            ),
            &keep_ratio,
            copy_filter_into_fixed_len_keep_ratio::<Row>,
        );
    }
}

criterion_group!(
    copy_filter_into,
    copy_filter_into_fixed_len::<u64>,
    copy_filter_into_fixed_len::<U32x8>,
    copy_filter_into_fixed_len::<U32x64>,
);

fn u32x2_type() -> ProductType {
    [AlgebraicType::U32; 2].into()
}

fn u32x4_type() -> ProductType {
    [AlgebraicType::U32; 4].into()
}

fn string_row_type() -> ProductType {
    [AlgebraicType::String].into()
}

fn u32_array_row_type() -> ProductType {
    [AlgebraicType::array(AlgebraicType::U32)].into()
}

fn product_types<const N: usize>(types: [ProductType; N]) -> ProductType {
    types.map(AlgebraicType::from).into()
}

fn u32_array_value<const N: usize>(arr: [u32; N]) -> ProductValue {
    AlgebraicValue::Array(ArrayValue::U32(arr.into())).into()
}

fn u32_matrix_value<const N: usize, const M: usize>(matrix: [[u32; N]; M]) -> ProductValue {
    let elements = matrix
        .map(|inner| AlgebraicValue::product(inner.map(AlgebraicValue::U32).into()))
        .into();
    ProductValue { elements }
}

fn product_value_test_cases() -> impl Iterator<
    Item = (
        &'static str,
        ProductType,
        ProductValue,
        Option<NullVarLenVisitor>,
        Option<AlignedVarLenOffsets<'static>>,
    ),
> {
    [
        (
            "U32",
            [AlgebraicType::U32].into(),
            product![0xa5a5_a5a5u32],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "Option<U32>/None",
            [AlgebraicType::option(AlgebraicType::U32)].into(),
            product![AlgebraicValue::OptionNone()],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "Option<U32>/Some",
            [AlgebraicType::option(AlgebraicType::U32)].into(),
            product![AlgebraicValue::OptionSome(AlgebraicValue::U32(0xa5a5_a5a5))],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "U32x2",
            u32x2_type(),
            product![0u32, 1u32],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "U32x4",
            u32x4_type(),
            product![0u32, 1u32, 2u32, 3u32],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "U32x8",
            [AlgebraicType::U32; 8].into(),
            product![0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "String/0",
            string_row_type(),
            product!["".to_string()],
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "String/16",
            string_row_type(),
            product!["0123456789abcdef".to_string()],
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "String/128",
            string_row_type(),
            product!["0123456789abcdef".repeat(8)],
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "String/512",
            string_row_type(),
            product!["0123456789abcdef".repeat(32)],
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "Array<U32>/0",
            u32_array_row_type(),
            u32_array_value([]),
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "Array<U32>/1",
            u32_array_row_type(),
            u32_array_value([0]),
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "Array<U32>/2",
            u32_array_row_type(),
            u32_array_value([0, 1]),
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "Array<U32>/4",
            u32_array_row_type(),
            u32_array_value([0, 1, 2, 3]),
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "Array<U32>/8",
            u32_array_row_type(),
            u32_array_value([0, 1, 2, 3, 4, 5, 6, 7]),
            None,
            Some(AlignedVarLenOffsets::from_offsets(&[0])),
        ),
        (
            "U32x2x2",
            product_types([u32x2_type(), u32x2_type()]),
            u32_matrix_value([[0, 1], [2, 3]]),
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "U32x4x2",
            product_types([u32x4_type(), u32x4_type()]),
            u32_matrix_value([[0, 1, 2, 3], [4, 5, 6, 7]]),
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "U32x2x4",
            product_types([u32x2_type(), u32x2_type(), u32x2_type(), u32x2_type()]),
            u32_matrix_value([[0, 1], [2, 3], [4, 5], [6, 7]]),
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "Option<U32x2>/None",
            [AlgebraicType::option(u32x2_type().into())].into(),
            product![AlgebraicValue::OptionNone()],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
        (
            "Option<U32x2>/Some",
            [AlgebraicType::option(u32x2_type().into())].into(),
            product![AlgebraicValue::OptionSome(product![0u32, 1u32].into())],
            Some(NullVarLenVisitor),
            Some(AlignedVarLenOffsets::from_offsets(&[])),
        ),
    ]
    .into_iter()
}

fn time_insert_one(
    b: &mut Bencher,
    page: &mut Page,
    ty: &RowTypeLayout,
    value: &ProductValue,
    visitor: &impl VarLenMembers,
) {
    let pre = |page: &mut _| {
        clear_zero(page);
        value.clone()
    };
    iter_time_with_page(b, page, pre, |val, _, page| unsafe {
        let _ = black_box(write_row_to_page(page, &mut NullBlobStore, visitor, ty, &val));
    });
}

fn ty_page_visitor(ty: ProductType) -> (RowTypeLayout, Box<Page>, VarLenVisitorProgram) {
    let ty: RowTypeLayout = ty.into();
    let page = Page::new(ty.size());
    let vis = row_type_visitor(&ty);
    (ty, page, vis)
}

#[inline(never)]
fn insert_product_value_into_page(c: &mut Criterion) {
    for (name, ty, value, null_visitor, aligned_offsets_visitor) in product_value_test_cases() {
        let mut group = c.benchmark_group(format!("insert_product_value/{}", name));
        let (ty, mut page, program_visitor) = ty_page_visitor(ty);

        if let Some(null_visitor) = null_visitor {
            group.bench_function("NullVarLenVisitor", |b| {
                time_insert_one(b, &mut page, &ty, &value, &null_visitor);
            });
        }

        if let Some(aligned_offsets_visitor) = aligned_offsets_visitor {
            group.bench_function("AlignedVarLenOffsets", |b| {
                time_insert_one(b, &mut page, &ty, &value, &aligned_offsets_visitor);
            });
        }

        group.bench_function("VarLenVisitorProgram", |b| {
            time_insert_one(b, &mut page, &ty, &value, &program_visitor);
        });
    }
}

fn time_extract_one(b: &mut Bencher, page: &mut Page, offset: PageOffset, ty: &RowTypeLayout) {
    let body =
        |_, _, page: &mut _| unsafe { serialize_row_from_page(ValueSerializer, page, &NullBlobStore, offset, ty) };
    iter_time_with_page(b, page, |_| (), body);
}

#[inline(never)]
fn extract_product_value_from_page(c: &mut Criterion) {
    for (name, ty, value, _null_visitor, _aligned_offsets_visitor) in product_value_test_cases() {
        let mut group = c.benchmark_group("extract_product_value");
        let (ty, mut page, visitor) = ty_page_visitor(ty);
        let offset = unsafe { write_row_to_page(&mut page, &mut NullBlobStore, &visitor, &ty, &value) }.unwrap();
        group.bench_function(name, |b| time_extract_one(b, &mut page, offset, &ty));
    }
}

/// Puts two rows into a page which are equal,
/// then times how long it takes to use `eq_row_in_page`
/// to compare them.
/// One iteration = one comparison between two rows.
fn eq_in_page_same(c: &mut Criterion) {
    let mut group = c.benchmark_group("eq_in_page");
    for (name, ty, value, _null_visitor, _aligned_offsets_visitor) in product_value_test_cases() {
        let (ty, mut page, visitor) = ty_page_visitor(ty);

        let offset_0 = unsafe { write_row_to_page(&mut page, &mut NullBlobStore, &visitor, &ty, &value) }.unwrap();
        let offset_1 = unsafe { write_row_to_page(&mut page, &mut NullBlobStore, &visitor, &ty, &value) }.unwrap();

        group.bench_function(name, |b| {
            b.iter(|| {
                black_box(unsafe {
                    eq_row_in_page(
                        black_box(&page),
                        black_box(&page),
                        black_box(offset_0),
                        black_box(offset_1),
                        black_box(&ty),
                    )
                })
            });
        });
    }
}

/// Puts a row into a page,
/// then times how long it takes to use `hash_row_in_page`
/// to compute its hash.
/// One iteration = one row hashed.
fn hash_in_page(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_in_page");
    for (name, ty, value, _null_visitor, _aligned_offsets_visitor) in product_value_test_cases() {
        let (ty, mut page, visitor) = ty_page_visitor(ty);

        let offset = unsafe { write_row_to_page(&mut page, &mut NullBlobStore, &visitor, &ty, &value) }.unwrap();
        group.bench_function(name, |b| {
            let mut hasher = RowHash::hasher_builder().build_hasher();
            b.iter(|| {
                black_box(unsafe { hash_row_in_page(&mut hasher, black_box(&page), black_box(offset), black_box(&ty)) })
            });
        });
    }
}

criterion_group!(
    bflatn,
    insert_product_value_into_page,
    extract_product_value_from_page,
    eq_in_page_same,
    hash_in_page,
);

criterion_main!(insert, iter, copy_filter_into, bflatn);
