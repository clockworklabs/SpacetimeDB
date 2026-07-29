//! Workload value and migration generation for the engine DST driver.
//!
//! Read this file as policy first, mechanics second:
//! - `ValueGen::gen_insert_row` chooses the row-level insert shape: valid row,
//!   whole-row duplicate, arbitrary candidate, or targeted uniqueness conflict.
//! - `ValueGen::gen_value_for_case` chooses one generated column value. The
//!   type-specific helpers below it are mostly curated sample sets.
//! - `MigrationGen` only chooses accepted vs. rejected migration work; concrete
//!   schema rewrite rules live in `migrations.rs`.

use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::ArrayValue;

use super::migrations::{Migration, MigrationMode};
use super::model::Model;
use super::row::Row;
use crate::rng::{choice, Choice, WeightedChoice};
use crate::schema::{TablePlan, Type};

// Bound the valid-insert search: random generation may collide with existing
// unique constraints, but a failed search must not stall the workload.
const INSERT_CANDIDATE_ATTEMPTS: usize = 32;

/// Read-only generation context for one model state.
pub(crate) struct GenCtx<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> GenCtx<'a> {
    pub(crate) fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    pub(crate) fn gen_insert_row(&self, table: usize) -> Row {
        ValueGen::new(self.rng, self.model).gen_insert_row(table)
    }

    pub(crate) fn gen_migration(&self) -> Option<Migration> {
        MigrationGen::new(self.rng, self.model).choose()
    }
}

// Row-level insert cases choose what kind of insert operation to try.
#[derive(Clone, Copy)]
enum InsertRowCase {
    Valid,
    AnyCandidate,
    ExistingRow,
    UniqueConflict,
}

impl InsertRowCase {
    const CHOICES: [Choice<Self>; 4] = [
        choice(80, Self::Valid),
        choice(10, Self::AnyCandidate),
        choice(5, Self::ExistingRow),
        choice(5, Self::UniqueConflict),
    ];
}

impl WeightedChoice for InsertRowCase {}

// Column-level cases choose how to synthesize each non-sequence column value.
#[derive(Clone, Copy)]
enum ColumnValueCase {
    Random,
    Small,
    Edge,
    Weird,
    Existing,
    NearExisting,
}

impl ColumnValueCase {
    const CHOICES: [Choice<Self>; 6] = [
        choice(45, Self::Random),
        choice(15, Self::Small),
        choice(15, Self::Edge),
        choice(5, Self::Weird),
        choice(10, Self::Existing),
        choice(10, Self::NearExisting),
    ];
}

impl WeightedChoice for ColumnValueCase {}

#[derive(Clone, Copy)]
enum ExistingValueScope {
    SameColumn,
    SameTable,
    AnyTable,
}

impl ExistingValueScope {
    const CHOICES: [Choice<Self>; 3] = [
        choice(55, Self::SameColumn),
        choice(25, Self::SameTable),
        choice(20, Self::AnyTable),
    ];
}

impl WeightedChoice for ExistingValueScope {}

trait TypeValueGen {
    fn random(&self, rng: &Rng) -> AlgebraicValue;
    fn small(&self, rng: &Rng) -> AlgebraicValue;
    fn edge(&self, rng: &Rng) -> AlgebraicValue;
    fn weird(&self, rng: &Rng) -> AlgebraicValue;
    fn counter(&self, counter: u64) -> AlgebraicValue;
    fn near(&self, value: AlgebraicValue) -> AlgebraicValue;
    fn matches(&self, value: &AlgebraicValue) -> bool;
}

struct BoolGen;
struct I64Gen;
struct U64Gen;
struct StringGen;
struct BytesGen;
struct SumGen {
    variants: u8,
}

impl TypeValueGen for BoolGen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Bool(rng.next_u64().is_multiple_of(2))
    }

    fn small(&self, _rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Bool(false)
    }

    fn edge(&self, _rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Bool(true)
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        self.edge(rng)
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::Bool(counter.is_multiple_of(2))
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::Bool(value) => AlgebraicValue::Bool(!value),
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::Bool(_))
    }
}

impl TypeValueGen for I64Gen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::I64(rng.next_u64() as i64)
    }

    fn small(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::I64(sample(rng, &[-3, -2, -1, 0, 1, 2, 3]))
    }

    fn edge(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::I64(sample(rng, &[i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]))
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        self.edge(rng)
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::I64(counter as i64)
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::I64(value) => AlgebraicValue::I64(value.saturating_add(1)),
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::I64(_))
    }
}

impl TypeValueGen for U64Gen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::U64(rng.next_u64())
    }

    fn small(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::U64(sample(rng, &[0, 1, 2, 3, 4, 5]))
    }

    fn edge(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::U64(sample(rng, &[0, 1, 2, u64::MAX - 1, u64::MAX]))
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        self.edge(rng)
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::U64(counter)
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::U64(value) => AlgebraicValue::U64(value.saturating_add(1)),
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::U64(_))
    }
}

impl TypeValueGen for StringGen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::String(format!("v_{}", rng.next_u64()).into())
    }

    fn small(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::String(
            sample(rng, &["a", "aa", "ab", "b", "z", "v_0", "v_1"])
                .to_owned()
                .into(),
        )
    }

    fn edge(&self, _rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::String("x".repeat(128).into())
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        let value = match rng.index(100) {
            0..35 => sample(rng, &["quote'", "double\"quote", "back\\slash", "line\nbreak"]).to_owned(),
            35..60 => "nul\0byte".to_owned(),
            60..85 => sample(rng, &["a", "aa", "aaa", "ab", "aba", "abb", "b"]).to_owned(),
            _ => String::new(),
        };
        AlgebraicValue::String(value.into())
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::String(format!("fresh_{counter}").into())
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::String(value) => {
                let mut value = value.to_string();
                value.push('a');
                AlgebraicValue::String(value.into())
            }
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::String(_))
    }
}

impl TypeValueGen for BytesGen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        let len = (rng.next_u64() % 16) as usize;
        let value = (0..len).map(|_| rng.next_u64() as u8).collect::<Vec<_>>();
        AlgebraicValue::Array(ArrayValue::U8(value.into()))
    }

    fn small(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Array(ArrayValue::U8(
            sample(rng, &[&[][..], &[0][..], &[1][..], &[0, 255][..]])
                .to_vec()
                .into(),
        ))
    }

    fn edge(&self, _rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Array(ArrayValue::U8(vec![255; 32].into()))
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        let value = match rng.index(100) {
            0..25 => Vec::new(),
            25..45 => vec![0; 32],
            45..65 => vec![255; 32],
            65..85 => vec![0, 255, 0, 255, 0, 255],
            _ => sample(rng, &[&[][..], &[0][..], &[1][..], &[0, 255][..]]).to_vec(),
        };
        AlgebraicValue::Array(ArrayValue::U8(value.into()))
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::Array(ArrayValue::U8(counter.to_le_bytes().to_vec().into()))
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::Array(ArrayValue::U8(value)) => {
                let mut value = value.to_vec();
                value.push(0);
                AlgebraicValue::Array(ArrayValue::U8(value.into()))
            }
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::Array(ArrayValue::U8(_)))
    }
}

impl TypeValueGen for SumGen {
    fn random(&self, rng: &Rng) -> AlgebraicValue {
        let tag = rng.index(self.variants as usize) as u8;
        AlgebraicValue::sum(tag, AlgebraicValue::U8(rng.next_u64() as u8))
    }

    fn small(&self, rng: &Rng) -> AlgebraicValue {
        let tag = if self.variants <= 1 {
            0
        } else {
            rng.index(self.variants as usize) as u8
        };
        AlgebraicValue::sum(tag, AlgebraicValue::U8(0))
    }

    fn edge(&self, _rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::sum(self.variants.saturating_sub(1), AlgebraicValue::U8(u8::MAX))
    }

    fn weird(&self, rng: &Rng) -> AlgebraicValue {
        self.edge(rng)
    }

    fn counter(&self, counter: u64) -> AlgebraicValue {
        AlgebraicValue::sum(
            if self.variants == 0 {
                0
            } else {
                (counter % self.variants as u64) as u8
            },
            AlgebraicValue::U8(counter as u8),
        )
    }

    fn near(&self, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::Sum(sum) if self.variants > 0 => {
                AlgebraicValue::sum(sum.tag.wrapping_add(1) % self.variants, *sum.value)
            }
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::Sum(sum) if sum.tag < self.variants)
    }
}

fn generator_for(ty: Type) -> Box<dyn TypeValueGen> {
    match ty {
        Type::Bool => Box::new(BoolGen),
        Type::I64 => Box::new(I64Gen),
        Type::U64 => Box::new(U64Gen),
        Type::String => Box::new(StringGen),
        Type::Bytes => Box::new(BytesGen),
        Type::Sum { variants } => Box::new(SumGen { variants }),
    }
}

fn sample<T: Copy>(rng: &Rng, values: &[T]) -> T {
    values[rng.index(values.len())]
}

// Row generation deliberately mixes normal traffic with rows the engine should
// reject. The model is the oracle for whether a candidate is valid; this module
// only decides which kind of pressure to apply.
struct ValueGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> ValueGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    fn gen_insert_row(&self, table: usize) -> Row {
        // Start here when tuning insert behavior; everything below materializes
        // one arm of this match.
        match InsertRowCase::pick(self.rng, &InsertRowCase::CHOICES) {
            InsertRowCase::Valid => self.gen_valid_insert_row(table),
            InsertRowCase::AnyCandidate => self.gen_row_candidate(table),
            InsertRowCase::ExistingRow => self
                .existing_row(table)
                .unwrap_or_else(|| self.gen_valid_insert_row(table)),
            InsertRowCase::UniqueConflict => self
                .unique_conflict_row(table)
                .unwrap_or_else(|| self.gen_valid_insert_row(table)),
        }
    }

    fn gen_valid_insert_row(&self, table: usize) -> Row {
        // The valid path still begins with ordinary candidates so it samples the
        // same value domain as rejected paths. Counter rows are only the
        // progress fallback.
        for _ in 0..INSERT_CANDIDATE_ATTEMPTS {
            let row = self.gen_row_candidate(table);
            if !self.model.insert_would_violate_unique_constraint(table, &row) {
                return row;
            }
        }

        self.gen_counter_row(table, self.rng.next_u64())
    }

    fn gen_row_candidate(&self, table: usize) -> Row {
        self.table(table)
            .columns
            .iter()
            .enumerate()
            .map(|(column, column_plan)| self.gen_insert_value(table, column, column_plan.ty))
            .collect::<ProductValue>()
    }

    fn gen_counter_row(&self, table: usize, counter: u64) -> Row {
        self.table(table)
            .columns
            .iter()
            .enumerate()
            .map(|(column, column_plan)| {
                if self.is_sequence_column(table, column) {
                    sequence_placeholder(column_plan.ty)
                } else {
                    self.gen_counter_value(column_plan.ty, counter.wrapping_add(column as u64))
                }
            })
            .collect::<ProductValue>()
    }

    fn gen_insert_value(&self, table: usize, column: usize, ty: Type) -> AlgebraicValue {
        if self.is_sequence_column(table, column) {
            return sequence_placeholder(ty);
        }

        self.gen_value(table, column, ty)
    }

    fn gen_value(&self, table: usize, column: usize, ty: Type) -> AlgebraicValue {
        let case = ColumnValueCase::pick(self.rng, &ColumnValueCase::CHOICES);
        self.gen_value_for_case(table, column, ty, case).unwrap_or_else(|| {
            // Existing-value cases can fail when the model is empty or no
            // visible value matches the requested type.
            self.gen_value_for_case(table, column, ty, ColumnValueCase::Random)
                .expect("random value generation cannot fail")
        })
    }

    fn gen_value_for_case(
        &self,
        table: usize,
        column: usize,
        ty: Type,
        case: ColumnValueCase,
    ) -> Option<AlgebraicValue> {
        let type_gen = generator_for(ty);
        match case {
            ColumnValueCase::Random => Some(type_gen.random(self.rng)),
            ColumnValueCase::Small => Some(type_gen.small(self.rng)),
            ColumnValueCase::Edge => Some(type_gen.edge(self.rng)),
            ColumnValueCase::Weird => Some(type_gen.weird(self.rng)),
            ColumnValueCase::Existing => self.existing_value(table, column, ty),
            ColumnValueCase::NearExisting => self.near_existing_value(table, column, ty),
        }
    }

    fn existing_row(&self, table: usize) -> Option<Row> {
        let count = self.model.row_count(table);
        (count > 0).then(|| {
            self.model
                .row(table, self.rng.index(count))
                .expect("sampled row index is in bounds")
                .clone()
        })
    }

    fn unique_conflict_row(&self, table: usize) -> Option<Row> {
        // Target a specific unique constraint without turning the operation into
        // an exact-row duplicate: copy constrained columns from an existing row,
        // then perturb an unconstrained column if necessary.
        let constraints = &self.table(table).unique_constraints;
        if constraints.is_empty() || self.model.row_count(table) == 0 {
            return None;
        }

        let start = self.rng.index(constraints.len());
        for offset in 0..constraints.len() {
            let constraint = &constraints[(start + offset) % constraints.len()];
            let base = self.existing_row(table)?;
            let mut row = self.gen_row_candidate(table);

            for &column in &constraint.columns {
                row.elements[column] = base.elements[column].clone();
            }

            if row == base {
                let Some(column) =
                    (0..self.table(table).columns.len()).find(|column| !constraint.columns.contains(column))
                else {
                    continue;
                };
                row.elements[column] =
                    self.near_value(self.table(table).columns[column].ty, base.elements[column].clone());
            }

            if row != base && self.model.insert_would_violate_unique_constraint(table, &row) {
                return Some(row);
            }
        }

        None
    }

    fn existing_value(&self, table: usize, column: usize, ty: Type) -> Option<AlgebraicValue> {
        // Existing-value generation is runtime-value based, not column-domain
        // based. After migrations, the interesting reusable value may live in a
        // different column or table as long as its SATS shape still fits `ty`.
        let scope = ExistingValueScope::pick(self.rng, &ExistingValueScope::CHOICES);
        self.existing_value_in_scope(table, column, ty, scope)
            .or_else(|| self.existing_value_in_scope(table, column, ty, ExistingValueScope::SameColumn))
            .or_else(|| self.existing_value_in_scope(table, column, ty, ExistingValueScope::SameTable))
            .or_else(|| self.existing_value_in_scope(table, column, ty, ExistingValueScope::AnyTable))
    }

    fn existing_value_in_scope(
        &self,
        table: usize,
        column: usize,
        ty: Type,
        scope: ExistingValueScope,
    ) -> Option<AlgebraicValue> {
        match scope {
            ExistingValueScope::SameColumn => self.existing_column_value(table, column, ty),
            ExistingValueScope::SameTable => self.existing_table_value(table, ty),
            ExistingValueScope::AnyTable => self.existing_any_table_value(ty),
        }
    }

    fn existing_column_value(&self, table: usize, column: usize, ty: Type) -> Option<AlgebraicValue> {
        let count = self.model.row_count(table);
        if count == 0 {
            return None;
        }

        let start = self.rng.index(count);
        for offset in 0..count {
            let row = self.model.row(table, (start + offset) % count)?;
            let value = &row.elements[column];
            if value_matches_type(ty, value) {
                return Some(value.clone());
            }
        }

        None
    }

    fn existing_table_value(&self, table: usize, ty: Type) -> Option<AlgebraicValue> {
        let values = self.visible_values_in_tables(ty, table..table + 1);
        (!values.is_empty()).then(|| values[self.rng.index(values.len())].clone())
    }

    fn existing_any_table_value(&self, ty: Type) -> Option<AlgebraicValue> {
        let values = self.visible_values_in_tables(ty, 0..self.model.schema().tables.len());
        (!values.is_empty()).then(|| values[self.rng.index(values.len())].clone())
    }

    fn visible_values_in_tables(&self, ty: Type, tables: impl Iterator<Item = usize>) -> Vec<AlgebraicValue> {
        // Scan stored rows by runtime type compatibility. This is what lets
        // existing-value reuse survive schema rewrites and sum-variant changes.
        let mut values = Vec::new();
        for table in tables {
            let table_plan = self.table(table);
            for row_idx in 0..self.model.row_count(table) {
                let row = self.model.row(table, row_idx).expect("row index is in bounds");
                for (column, _column_plan) in table_plan.columns.iter().enumerate() {
                    let value = &row.elements[column];
                    if value_matches_type(ty, value) {
                        values.push(value.clone());
                    }
                }
            }
        }
        values
    }

    fn near_existing_value(&self, table: usize, column: usize, ty: Type) -> Option<AlgebraicValue> {
        self.existing_value(table, column, ty)
            .map(|value| self.near_value(ty, value))
    }

    fn near_value(&self, ty: Type, value: AlgebraicValue) -> AlgebraicValue {
        generator_for(ty).near(value)
    }

    fn gen_counter_value(&self, ty: Type, counter: u64) -> AlgebraicValue {
        generator_for(ty).counter(counter)
    }

    fn is_sequence_column(&self, table: usize, column: usize) -> bool {
        self.table(table)
            .sequences
            .iter()
            .any(|sequence| sequence.column == column)
    }

    fn table(&self, table: usize) -> &TablePlan {
        &self.model.schema().tables[table]
    }
}

// Sequence columns are engine-filled on insert. The command still needs a SATS
// value with the right shape, so this placeholder should be ignored downstream.
fn sequence_placeholder(ty: Type) -> AlgebraicValue {
    let value = generator_for(ty).counter(0);
    match value {
        AlgebraicValue::I64(_) | AlgebraicValue::U64(_) => value,
        _ => unreachable!("sequence columns are integral"),
    }
}

// Reuse checks the materialized value, not the current column definition. Sum
// values need an extra tag bound because migrations can narrow variant counts.
fn value_matches_type(ty: Type, value: &AlgebraicValue) -> bool {
    generator_for(ty).matches(value)
}

// Migration generation is intentionally shallow here: accepted migrations are
// short rewrite chains, while rejected migration construction lives in
// `migrations.rs` and falls back to accepted work when no rejection applies.
struct MigrationGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> MigrationGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    fn choose(&self) -> Option<Migration> {
        match MigrationMode::pick(self.rng, &MigrationMode::CHOICES) {
            MigrationMode::Accepted => self.choose_accepted(),
            MigrationMode::Rejected => {
                Migration::choose_rejected(self.rng, self.model.schema(), self.model).or_else(|| self.choose_accepted())
            }
        }
    }

    fn choose_accepted(&self) -> Option<Migration> {
        let original = self.model.schema();
        let mut schema = original.clone();
        let steps = 1 + self.rng.index(10);

        for _ in 0..steps {
            let Some(rewrite) = Migration::choose_rewrite(self.rng, &schema, self.model) else {
                break;
            };
            rewrite
                .apply_to(&mut schema)
                .expect("generated rewrite must be valid for the draft schema");
        }

        (schema != *original).then(|| Migration::from_schema(schema))
    }
}
