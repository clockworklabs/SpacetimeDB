//! Workload value and migration generation for the engine DST driver.
//!
//! Read this file as policy first, mechanics second:
//! - `ValueGen::gen_insert_row` chooses the row-level insert shape: valid row,
//!   whole-row duplicate, arbitrary candidate, or targeted uniqueness conflict.
//! - `ValueGen::gen_value_for_case` chooses one generated column value: fresh
//!   random, sampled from the accumulated pool, or near an accumulated pool value.
//! - `MigrationGen` only chooses accepted vs. rejected migration work; concrete
//!   schema rewrite rules live in `migrations.rs`.

use std::collections::BTreeMap;

use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::ArrayValue;

use super::migrations::Migration;
use super::model::Model;
use super::row::Row;
use crate::rng::{choice, Choice, WeightedChoice};
use crate::schema::{SchemaPlan, TablePlan, Type};

// Bound the valid-insert search: random generation may collide with existing
// unique constraints, but a failed search must not stall the workload.
const INSERT_CANDIDATE_ATTEMPTS: usize = 32;
const VALUE_POOL_VALUES_PER_TYPE: usize = 4096;

/// Generation context for one model state plus accumulated generator memory.
pub(crate) struct GenCtx<'a> {
    rng: &'a Rng,
    model: &'a Model,
    generation: &'a mut GenerationState,
}

impl<'a> GenCtx<'a> {
    pub(crate) fn new(rng: &'a Rng, model: &'a Model, generation: &'a mut GenerationState) -> Self {
        Self { rng, model, generation }
    }

    pub(crate) fn gen_insert_row(&mut self, table: usize) -> Row {
        ValueGen::new(self.rng, self.model, self.generation).gen_insert_row(table)
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
    Pooled,
    Near,
}

impl ColumnValueCase {
    const CHOICES: [Choice<Self>; 3] = [
        choice(50, Self::Random),
        choice(30, Self::Pooled),
        choice(20, Self::Near),
    ];
}

impl WeightedChoice for ColumnValueCase {}

pub(crate) struct GenerationState {
    generators: BTreeMap<TypeKey, Box<dyn TypeValueGen>>,
}

impl GenerationState {
    pub(crate) fn seeded(schema: &SchemaPlan) -> Self {
        let mut state = Self {
            generators: BTreeMap::new(),
        };
        state.seed_schema(schema);
        state
    }

    pub(crate) fn seed_schema(&mut self, schema: &SchemaPlan) {
        for table in &schema.tables {
            for column in &table.columns {
                self.generator_mut(column.ty);
            }
        }
    }

    pub(crate) fn observe_row(&mut self, table: &TablePlan, row: &Row) {
        for (column, value) in table.columns.iter().zip(row.elements.iter()) {
            self.generator_mut(column.ty).observe(value.clone());
        }
    }

    fn generator_mut(&mut self, ty: Type) -> &mut dyn TypeValueGen {
        self.generators
            .entry(TypeKey::from(ty))
            .or_insert_with(|| new_generator_for(ty))
            .as_mut()
    }
}

#[derive(Default)]
struct ValueBucket {
    values: Vec<AlgebraicValue>,
    observed: usize,
}

impl ValueBucket {
    fn contains(&self, value: &AlgebraicValue) -> bool {
        self.values.contains(value)
    }

    fn observe(&mut self, value: AlgebraicValue) {
        if self.values.len() < VALUE_POOL_VALUES_PER_TYPE {
            self.values.push(value);
        } else {
            let slot = self.observed % VALUE_POOL_VALUES_PER_TYPE;
            self.values[slot] = value;
        }
        self.observed = self.observed.wrapping_add(1);
    }

    fn sample(&self, rng: &Rng) -> Option<AlgebraicValue> {
        (!self.values.is_empty()).then(|| self.values[rng.index(self.values.len())].clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TypeKey {
    Bool,
    I64,
    U64,
    String,
    Bytes,
    Sum { variants: u8 },
}

impl From<Type> for TypeKey {
    fn from(ty: Type) -> Self {
        match ty {
            Type::Bool => Self::Bool,
            Type::I64 => Self::I64,
            Type::U64 => Self::U64,
            Type::String => Self::String,
            Type::Bytes => Self::Bytes,
            Type::Sum { variants } => Self::Sum { variants },
        }
    }
}

/// Stateful value generator for one exact DST column type.
///
/// Each implementation owns its accumulated value pool. `seeds` supplies the
/// stable starting corpus installed when the generator is first stored;
/// `observe` appends runtime values; `near` samples the pool itself and applies
/// the type-local perturbation. Keeping the pool behind this trait makes pooled
/// and near-value generation follow the same exact type key.
trait TypeValueGen {
    fn pool(&self) -> &ValueBucket;
    fn pool_mut(&mut self) -> &mut ValueBucket;

    /// Stable boundary values installed once when this exact type first appears.
    fn seeds(&self) -> Vec<AlgebraicValue>;

    /// Produce a fresh valid value without consulting accumulated history.
    fn random(&self, rng: &Rng) -> AlgebraicValue;

    /// Make a type-valid nearby value from an already selected value.
    /// Returning `value` unchanged is allowed when no sensible nearby value
    /// exists or the input does not match this generator's type.
    fn near_from(&self, rng: &Rng, value: AlgebraicValue) -> AlgebraicValue;

    /// Check whether a materialized value is valid for this exact type.
    fn matches(&self, value: &AlgebraicValue) -> bool;

    fn observe(&mut self, value: AlgebraicValue) {
        if self.matches(&value) {
            self.pool_mut().observe(value);
        }
    }

    fn pooled(&self, rng: &Rng) -> Option<AlgebraicValue> {
        self.pool().sample(rng)
    }

    fn near(&self, rng: &Rng) -> Option<AlgebraicValue> {
        let value = self.pooled(rng)?;
        let original = value.clone();
        let near = self.near_from(rng, value);
        Some(if self.matches(&near) { near } else { original })
    }
}

fn seed_generator(generator: &mut dyn TypeValueGen) {
    for value in generator.seeds() {
        let should_seed = generator.matches(&value) && !generator.pool().contains(&value);
        if should_seed {
            generator.pool_mut().observe(value);
        }
    }
}

macro_rules! value_pool_methods {
    () => {
        fn pool(&self) -> &ValueBucket {
            &self.pool
        }

        fn pool_mut(&mut self) -> &mut ValueBucket {
            &mut self.pool
        }
    };
}

#[derive(Default)]
struct BoolGen {
    pool: ValueBucket,
}

#[derive(Default)]
struct I64Gen {
    pool: ValueBucket,
}

#[derive(Default)]
struct U64Gen {
    pool: ValueBucket,
}

#[derive(Default)]
struct StringGen {
    pool: ValueBucket,
}

#[derive(Default)]
struct BytesGen {
    pool: ValueBucket,
}

struct SumGen {
    variants: u8,
    pool: ValueBucket,
}

impl TypeValueGen for BoolGen {
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        vec![AlgebraicValue::Bool(false), AlgebraicValue::Bool(true)]
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::Bool(rng.next_u64().is_multiple_of(2))
    }

    fn near_from(&self, _rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
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
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        [-3, -2, -1, 0, 1, 2, 3, i64::MIN, i64::MIN + 1, i64::MAX - 1, i64::MAX]
            .into_iter()
            .map(AlgebraicValue::I64)
            .collect()
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::I64(rng.next_u64() as i64)
    }

    fn near_from(&self, rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::I64(value) if rng.next_u64().is_multiple_of(2) => {
                AlgebraicValue::I64(value.wrapping_add(1))
            }
            AlgebraicValue::I64(value) => AlgebraicValue::I64(value.wrapping_sub(1)),
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::I64(_))
    }
}

impl TypeValueGen for U64Gen {
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        [0, 1, 2, 3, 4, 5, u64::MAX - 1, u64::MAX]
            .into_iter()
            .map(AlgebraicValue::U64)
            .collect()
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        AlgebraicValue::U64(rng.next_u64())
    }

    fn near_from(&self, rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::U64(value) if rng.next_u64().is_multiple_of(2) => {
                AlgebraicValue::U64(value.wrapping_add(1))
            }
            AlgebraicValue::U64(value) => AlgebraicValue::U64(value.wrapping_sub(1)),
            other => other,
        }
    }

    fn matches(&self, value: &AlgebraicValue) -> bool {
        matches!(value, AlgebraicValue::U64(_))
    }
}

impl TypeValueGen for StringGen {
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        [
            "",
            "a",
            "aa",
            "aaa",
            "ab",
            "aba",
            "abb",
            "b",
            "z",
            "v_0",
            "v_1",
            "quote'",
            "double\"quote",
            "back\\slash",
            "line\nbreak",
            "nul\0byte",
        ]
        .into_iter()
        .map(|value| AlgebraicValue::String(value.into()))
        .chain(std::iter::once(AlgebraicValue::String("x".repeat(128).into())))
        .collect()
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        let mut state = rng.next_u64();
        let len = match state % 100 {
            0..=9 => 0,
            10..=79 => (state as usize >> 8) % 32,
            80..=97 => 32 + ((state as usize >> 8) % 224),
            _ => 256 + ((state as usize >> 8) % 768),
        };
        let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-'\"\\\n\t\0 %.,:;()[]{}";
        let value = (0..len)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(state as usize >> 32) % alphabet.len()] as char
            })
            .collect::<String>();
        AlgebraicValue::String(value.into())
    }

    fn near_from(&self, rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::String(value) => {
                let mut value = value.into_string();
                if !value.is_empty() && rng.next_u64().is_multiple_of(4) {
                    value.pop();
                } else {
                    value.push((b'a' + rng.index(26) as u8) as char);
                }
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
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        vec![
            AlgebraicValue::Array(ArrayValue::U8(Vec::new().into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![0].into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![1].into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![0, 255].into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![0; 32].into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![255; 32].into())),
            AlgebraicValue::Array(ArrayValue::U8(vec![0, 255, 0, 255, 0, 255].into())),
        ]
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        let len = (rng.next_u64() % 16) as usize;
        let value = (0..len).map(|_| rng.next_u64() as u8).collect::<Vec<_>>();
        AlgebraicValue::Array(ArrayValue::U8(value.into()))
    }

    fn near_from(&self, rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
        match value {
            AlgebraicValue::Array(ArrayValue::U8(value)) => {
                let mut value = value.to_vec();
                if value.is_empty() {
                    value.push(rng.next_u64() as u8);
                } else {
                    let edits = 1 + rng.index(3);
                    for _ in 0..edits {
                        let index = rng.index(value.len());
                        let delta = match rng.index(4) {
                            0 => 1,
                            1 => u8::MAX,
                            2 => 0x80,
                            _ => 0x7f,
                        };
                        value[index] = value[index].wrapping_add(delta);
                    }
                }
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
    value_pool_methods!();

    fn seeds(&self) -> Vec<AlgebraicValue> {
        let mut values = vec![AlgebraicValue::sum(0, AlgebraicValue::U8(0))];
        if self.variants > 1 {
            values.push(AlgebraicValue::sum(self.variants - 1, AlgebraicValue::U8(u8::MAX)));
        }
        values
    }

    fn random(&self, rng: &Rng) -> AlgebraicValue {
        debug_assert!(self.variants > 0);
        let tag = rng.index(self.variants as usize) as u8;
        AlgebraicValue::sum(tag, AlgebraicValue::U8(rng.next_u64() as u8))
    }

    fn near_from(&self, _rng: &Rng, value: AlgebraicValue) -> AlgebraicValue {
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

fn new_generator_for(ty: Type) -> Box<dyn TypeValueGen> {
    let mut generator: Box<dyn TypeValueGen> = match ty {
        Type::Bool => Box::<BoolGen>::default(),
        Type::I64 => Box::<I64Gen>::default(),
        Type::U64 => Box::<U64Gen>::default(),
        Type::String => Box::<StringGen>::default(),
        Type::Bytes => Box::<BytesGen>::default(),
        Type::Sum { variants } => Box::new(SumGen {
            variants,
            pool: ValueBucket::default(),
        }),
    };
    seed_generator(generator.as_mut());
    generator
}

// Row generation deliberately mixes normal traffic with rows the engine should
// reject. The model is the oracle for whether a candidate is valid; this module
// only decides which kind of pressure to apply.
struct ValueGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
    generation: &'a mut GenerationState,
}

impl<'a> ValueGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model, generation: &'a mut GenerationState) -> Self {
        Self { rng, model, generation }
    }

    fn gen_insert_row(&mut self, table: usize) -> Row {
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

    fn gen_valid_insert_row(&mut self, table: usize) -> Row {
        // The valid path samples the same value domain as rejected paths. After
        // bounded attempts, prefer an existing row as an accepted no-op over a
        // hand-built per-type escape value.
        for _ in 0..INSERT_CANDIDATE_ATTEMPTS {
            let row = self.gen_row_candidate(table);
            if !self.model.insert_would_violate_unique_constraint(table, &row) {
                return row;
            }
        }

        self.existing_row(table)
            .unwrap_or_else(|| self.gen_row_candidate(table))
    }

    fn gen_row_candidate(&mut self, table: usize) -> Row {
        let column_types = self
            .table(table)
            .columns
            .iter()
            .map(|column| column.ty)
            .collect::<Vec<_>>();
        column_types
            .into_iter()
            .enumerate()
            .map(|(column, ty)| self.gen_insert_value(table, column, ty))
            .collect::<ProductValue>()
    }

    fn gen_insert_value(&mut self, table: usize, column: usize, ty: Type) -> AlgebraicValue {
        if self.is_sequence_column(table, column) {
            return sequence_placeholder(ty);
        }

        self.gen_value(ty)
    }

    fn gen_value(&mut self, ty: Type) -> AlgebraicValue {
        let case = ColumnValueCase::pick(self.rng, &ColumnValueCase::CHOICES);
        if let Some(value) = self.gen_value_for_case(ty, case) {
            return value;
        }

        self.gen_value_for_case(ty, ColumnValueCase::Random)
            .expect("random value generation cannot fail")
    }

    fn gen_value_for_case(&mut self, ty: Type, case: ColumnValueCase) -> Option<AlgebraicValue> {
        match case {
            ColumnValueCase::Random => Some(self.generation.generator_mut(ty).random(self.rng)),
            ColumnValueCase::Pooled => self.pooled_value(ty),
            ColumnValueCase::Near => Some(self.near_value(ty)),
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

    fn unique_conflict_row(&mut self, table: usize) -> Option<Row> {
        // Target a specific unique constraint without turning the operation into
        // an exact-row duplicate: copy constrained columns from an existing row,
        // then perturb an unconstrained column if necessary.
        let constraints = self.table(table).unique_constraints.clone();
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
                let ty = self.table(table).columns[column].ty;
                row.elements[column] = self.near_value(ty);
            }

            if row != base && self.model.insert_would_violate_unique_constraint(table, &row) {
                return Some(row);
            }
        }

        None
    }

    fn pooled_value(&mut self, ty: Type) -> Option<AlgebraicValue> {
        self.generation.generator_mut(ty).pooled(self.rng)
    }

    fn near_value(&mut self, ty: Type) -> AlgebraicValue {
        let type_gen = self.generation.generator_mut(ty);
        type_gen.near(self.rng).unwrap_or_else(|| type_gen.random(self.rng))
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
    match ty {
        Type::I64 => AlgebraicValue::I64(0),
        Type::U64 => AlgebraicValue::U64(0),
        _ => unreachable!("sequence columns are integral"),
    }
}

#[derive(Debug, Clone, Copy)]
enum MigrationCase {
    Accepted,
    Rejected,
}

impl MigrationCase {
    const CHOICES: [Choice<Self>; 2] = [choice(95, Self::Accepted), choice(5, Self::Rejected)];
}

impl WeightedChoice for MigrationCase {}

// Migration generation is intentionally shallow here: accepted migrations are
// short rewrite chains, while rejected migrations are one invalidated accepted
// rule composed with otherwise valid accepted cases.
struct MigrationGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> MigrationGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    fn choose(&self) -> Option<Migration> {
        match MigrationCase::pick(self.rng, &MigrationCase::CHOICES) {
            MigrationCase::Accepted => self.choose_accepted(),
            MigrationCase::Rejected => {
                Migration::choose_rejected(self.rng, self.model.schema(), self.model).or_else(|| self.choose_accepted())
            }
        }
    }

    fn choose_accepted(&self) -> Option<Migration> {
        Migration::choose_accepted(self.rng, self.model.schema(), self.model)
    }
}
