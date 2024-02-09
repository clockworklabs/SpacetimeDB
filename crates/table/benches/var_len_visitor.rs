use core::slice;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use spacetimedb_sats::{AlgebraicType, ProductType};
use spacetimedb_table::row_type_visitor::{dump_visitor_program, row_type_visitor, VarLenVisitorProgram};
use spacetimedb_table::util::uninit_array;
use spacetimedb_table::var_len::{AlignedVarLenOffsets, NullVarLenVisitor, VarLenMembers, VarLenRef};
use std::mem::{self, MaybeUninit};

fn visit_count(row: &[MaybeUninit<u8>], visitor: &impl VarLenMembers) {
    black_box(unsafe { visitor.visit_var_len(row) }.count());
}

fn visitor_program(row_ty: impl Into<ProductType>) -> VarLenVisitorProgram {
    let row_ty: ProductType = row_ty.into();
    let visitor = row_type_visitor(&row_ty.into());

    eprintln!("Using visitor program:");
    dump_visitor_program(&visitor);
    eprintln!();

    visitor
}

fn visit_fixed_len(c: &mut C) {
    let row = &uninit_array::<u32, 1>();
    let row = row.as_ptr().cast::<MaybeUninit<u8>>();
    let row = unsafe { slice::from_raw_parts(row, mem::size_of::<u32>()) };

    let mut group = c.benchmark_group("visit_fixed_len/u64");

    let null_visitor = &NullVarLenVisitor;

    group.bench_function("NullVarLenVisitor", |b| {
        b.iter(|| visit_count(row, null_visitor));
    });

    let offsets_visitor = &AlignedVarLenOffsets::from_offsets(&[]);

    group.bench_function("AlignedVarLenOffsets", |b| {
        b.iter(|| visit_count(row, offsets_visitor));
    });

    let visitor = &visitor_program([AlgebraicType::U32]);

    group.bench_function("VarLenVisitorProgram", |b| {
        b.iter(|| visit_count(row, visitor));
    });
}

fn visit_var_len_product(c: &mut C) {
    let row = &uninit_array::<VarLenRef, 1>();
    let row = row.as_ptr().cast::<MaybeUninit<u8>>();
    let row = unsafe { slice::from_raw_parts(row, mem::size_of::<VarLenRef>()) };

    let mut group = c.benchmark_group("visit_var_len_product/VarLenRef");

    let offsets_visitor = &AlignedVarLenOffsets::from_offsets(&[0]);

    group.bench_function("AlignedVarLenOffsets", |b| {
        b.iter(|| visit_count(row, offsets_visitor));
    });

    let visitor = &visitor_program([AlgebraicType::String]);

    group.bench_function("VarLenVisitorProgram", |b| {
        b.iter(|| visit_count(row, visitor));
    });
}

fn visit_var_len_sum(c: &mut C) {
    let mut group = c.benchmark_group("visit_var_len_sum/opt_str");

    let visitor = &visitor_program([AlgebraicType::sum([AlgebraicType::String, AlgebraicType::unit()])]);

    let row = &mut uninit_array::<u16, 3>();
    let row = row.as_mut_ptr().cast::<MaybeUninit<u8>>();
    let row = unsafe { slice::from_raw_parts_mut(row, 6) };

    group.bench_function("none/VarLenVisitorProgram", |b| {
        // None
        row[4].write(1);

        b.iter(|| visit_count(row, visitor));
    });

    group.bench_function("some/VarLenVisitorProgram", |b| {
        // Some
        row[4].write(0);

        b.iter(|| visit_count(row, visitor));
    });
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod measurement {
    use criterion_perf_events::Perf;
    use perfcnt::linux::HardwareEventType as Hardware;
    use perfcnt::linux::PerfCounterBuilderLinux as Builder;

    pub type Measurement = Perf;
    pub fn get() -> Measurement {
        Perf::new(Builder::from_hardware_event(Hardware::Instructions))
    }
}
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
mod measurement {
    use criterion::measurement::WallTime;
    pub type Measurement = WallTime;
    pub fn get() -> Measurement {
        WallTime
    }
}

type C = Criterion<measurement::Measurement>;
fn config() -> C {
    Criterion::default().with_measurement(measurement::get())
}

criterion_group!(
    name = var_len_visitors;
    config = config();
    targets =
        visit_fixed_len,
        visit_var_len_product,
        visit_var_len_sum
);

criterion_main!(var_len_visitors);
