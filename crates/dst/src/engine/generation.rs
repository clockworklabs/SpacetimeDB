use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::ArrayValue;

use super::migrations::Migration;
use super::model::{ColumnDomain, Model};
use super::workload::Row;
use crate::schema::{SchemaDecisions, Type};

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

pub(crate) fn pick_weighted(rng: &Rng, weights: &[u64]) -> usize {
    let total: u64 = weights.iter().sum();

    assert!(total > 0, "at least one interaction weight must be non-zero");

    let mut selected = rng.next_u64() % total;

    for (idx, weight) in weights.iter().copied().enumerate() {
        if selected < weight {
            return idx;
        }

        selected -= weight;
    }

    unreachable!("selected value is always inside total weight")
}

struct ValueGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> ValueGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    fn gen_insert_row(&self, table: usize) -> Row {
        self.model.schema().tables[table]
            .columns
            .iter()
            .enumerate()
            .map(|(column, _)| {
                let domain = self.model.column_domain(table, column);
                self.gen_insert_value(&domain)
            })
            .collect::<ProductValue>()
    }

    fn gen_insert_value(&self, domain: &ColumnDomain) -> AlgebraicValue {
        if domain.sequenced {
            return sequence_placeholder(domain.ty);
        }

        if domain.unique {
            return self.gen_fresh_value(domain);
        }

        self.gen_value(domain)
    }

    fn gen_value(&self, domain: &ColumnDomain) -> AlgebraicValue {
        match pick_weighted(self.rng, &[45, 15, 15, 10, 10, 5]) {
            0 => self.gen_random_value(domain.ty),
            1 => self.gen_small_value(domain.ty),
            2 => self.gen_edge_value(domain.ty),
            3 => self
                .near_existing_value(domain)
                .unwrap_or_else(|| self.gen_random_value(domain.ty)),
            4 => self
                .existing_value(domain)
                .unwrap_or_else(|| self.gen_random_value(domain.ty)),
            5 => self.gen_weird_value(domain.ty),
            _ => unreachable!(),
        }
    }

    fn gen_fresh_value(&self, domain: &ColumnDomain) -> AlgebraicValue {
        for _ in 0..32 {
            let value = self.gen_fresh_candidate(domain.ty);
            if !domain.values.contains(&value) {
                return value;
            }
        }

        self.gen_counter_value(domain.ty, self.rng.next_u64())
    }

    fn gen_fresh_candidate(&self, ty: Type) -> AlgebraicValue {
        match pick_weighted(self.rng, &[50, 20, 20, 10]) {
            0 => self.gen_random_value(ty),
            1 => self.gen_small_value(ty),
            2 => self.gen_edge_value(ty),
            3 => self.gen_weird_value(ty),
            _ => unreachable!(),
        }
    }

    fn existing_value(&self, domain: &ColumnDomain) -> Option<AlgebraicValue> {
        (!domain.values.is_empty()).then(|| domain.values[self.rng.index(domain.values.len())].clone())
    }

    fn near_existing_value(&self, domain: &ColumnDomain) -> Option<AlgebraicValue> {
        let existing = self.existing_value(domain)?;
        Some(match (domain.ty, existing) {
            (Type::Bool, AlgebraicValue::Bool(value)) => AlgebraicValue::Bool(!value),
            (Type::I64, AlgebraicValue::I64(value)) => AlgebraicValue::I64(value.saturating_add(1)),
            (Type::U64, AlgebraicValue::U64(value)) => AlgebraicValue::U64(value.saturating_add(1)),
            (Type::String, AlgebraicValue::String(value)) => {
                let mut value = value.to_string();
                value.push('a');
                AlgebraicValue::String(value.into())
            }
            (Type::Bytes, AlgebraicValue::Array(ArrayValue::U8(value))) => {
                let mut value = value.to_vec();
                value.push(0);
                AlgebraicValue::Array(ArrayValue::U8(value.into()))
            }
            (Type::Sum { .. }, value) => value,
            (_, value) => value,
        })
    }

    fn gen_random_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(self.rng.next_u64().is_multiple_of(2)),
            Type::I64 => AlgebraicValue::I64(self.rng.next_u64() as i64),
            Type::U64 => AlgebraicValue::U64(self.rng.next_u64()),
            Type::String => AlgebraicValue::String(format!("v_{}", self.rng.next_u64()).into()),
            Type::Bytes => {
                let len = (self.rng.next_u64() % 16) as usize;
                let bytes: Vec<u8> = (0..len).map(|_| self.rng.next_u64() as u8).collect();
                AlgebraicValue::Array(ArrayValue::U8(bytes.into()))
            }
            Type::Sum { variants } => {
                let tag = self.rng.index(variants as usize) as u8;
                AlgebraicValue::sum(tag, AlgebraicValue::U8(self.rng.next_u64() as u8))
            }
        }
    }

    fn gen_small_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(false),
            Type::I64 => {
                const VALUES: &[i64] = &[-1, 0, 1, 2, 3];
                AlgebraicValue::I64(VALUES[self.rng.index(VALUES.len())])
            }
            Type::U64 => {
                const VALUES: &[u64] = &[0, 1, 2, 3];
                AlgebraicValue::U64(VALUES[self.rng.index(VALUES.len())])
            }
            Type::String => {
                const VALUES: &[&str] = &["", "a", "aa", "ab", "b", "v_0"];
                AlgebraicValue::String(VALUES[self.rng.index(VALUES.len())].into())
            }
            Type::Bytes => {
                const VALUES: &[&[u8]] = &[&[], &[0], &[1], &[0, 255]];
                AlgebraicValue::Array(ArrayValue::U8(VALUES[self.rng.index(VALUES.len())].to_vec().into()))
            }
            Type::Sum { variants } => {
                let tag = if variants <= 1 {
                    0
                } else {
                    self.rng.index(variants as usize) as u8
                };
                AlgebraicValue::sum(tag, AlgebraicValue::U8(0))
            }
        }
    }

    fn gen_edge_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(true),
            Type::I64 => {
                const VALUES: &[i64] = &[i64::MIN, i64::MIN + 1, i64::MAX - 1, i64::MAX];
                AlgebraicValue::I64(VALUES[self.rng.index(VALUES.len())])
            }
            Type::U64 => {
                const VALUES: &[u64] = &[0, 1, u64::MAX - 1, u64::MAX];
                AlgebraicValue::U64(VALUES[self.rng.index(VALUES.len())])
            }
            Type::String => AlgebraicValue::String("x".repeat(128).into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(vec![255; 32].into())),
            Type::Sum { variants } => AlgebraicValue::sum(variants.saturating_sub(1), AlgebraicValue::U8(u8::MAX)),
        }
    }

    fn gen_weird_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::String => {
                const VALUES: &[&str] = &["quote'", "double\"quote", "back\\slash", "line\nbreak", "\0"];
                AlgebraicValue::String(VALUES[self.rng.index(VALUES.len())].into())
            }
            Type::Bytes => {
                const VALUES: &[&[u8]] = &[&[0], &[0, 0, 0], &[255], &[0, 255, 0, 255]];
                AlgebraicValue::Array(ArrayValue::U8(VALUES[self.rng.index(VALUES.len())].to_vec().into()))
            }
            _ => self.gen_edge_value(ty),
        }
    }

    fn gen_counter_value(&self, ty: Type, counter: u64) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(counter.is_multiple_of(2)),
            Type::I64 => AlgebraicValue::I64(counter as i64),
            Type::U64 => AlgebraicValue::U64(counter),
            Type::String => AlgebraicValue::String(format!("fresh_{counter}").into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(counter.to_le_bytes().to_vec().into())),
            Type::Sum { variants } => AlgebraicValue::sum(
                if variants == 0 {
                    0
                } else {
                    (counter % variants as u64) as u8
                },
                AlgebraicValue::U8(counter as u8),
            ),
        }
    }
}

fn sequence_placeholder(ty: Type) -> AlgebraicValue {
    match ty {
        Type::I64 => AlgebraicValue::I64(0),
        Type::U64 => AlgebraicValue::U64(0),
        _ => unreachable!("sequence columns are integral"),
    }
}

struct MigrationGen<'a> {
    rng: &'a Rng,
    model: &'a Model,
}

impl<'a> MigrationGen<'a> {
    fn new(rng: &'a Rng, model: &'a Model) -> Self {
        Self { rng, model }
    }

    fn choose(&self) -> Option<Migration> {
        let candidates = Migration::candidates(self.model);
        SchemaDecisions::choose_index(self.rng, candidates.len()).map(|idx| candidates[idx].clone())
    }
}
