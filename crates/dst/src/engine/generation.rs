use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::ArrayValue;

use super::migrations::Migration;
use super::model::{ColumnDomain, Model};
use super::row::Row;
use crate::rng::{choice, Choice, WeightedChoice};
use crate::schema::Type;

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

#[derive(Clone, Copy)]
enum ValueCase {
    Random,
    Small,
    Edge,
    NearExisting,
    Existing,
    Weird,
}

impl WeightedChoice for ValueCase {
    const CHOICES: &'static [Choice<Self>] = &[
        choice(45, Self::Random),
        choice(15, Self::Small),
        choice(15, Self::Edge),
        choice(10, Self::NearExisting),
        choice(10, Self::Existing),
        choice(5, Self::Weird),
    ];
}

#[derive(Clone, Copy)]
enum FreshValueCase {
    Random,
    Small,
    Edge,
    Weird,
}

impl WeightedChoice for FreshValueCase {
    const CHOICES: &'static [Choice<Self>] = &[
        choice(50, Self::Random),
        choice(20, Self::Small),
        choice(20, Self::Edge),
        choice(10, Self::Weird),
    ];
}

#[derive(Clone, Copy)]
enum I64Case {
    Random,
    Small,
    Edge,
}

#[derive(Clone, Copy)]
enum U64Case {
    Random,
    Small,
    Edge,
}

#[derive(Clone, Copy)]
enum StringCase {
    RandomTagged,
    Empty,
    SmallAscii,
    OrderedPrefix,
    SqlEscaped,
    NullByte,
    Long,
}

impl StringCase {
    const WEIRD_CHOICES: &'static [Choice<Self>] = &[
        choice(35, Self::SqlEscaped),
        choice(25, Self::NullByte),
        choice(25, Self::OrderedPrefix),
        choice(15, Self::Empty),
    ];

    fn pick_weird(rng: &Rng) -> Self {
        crate::rng::pick_choice(rng, Self::WEIRD_CHOICES)
    }
}

#[derive(Clone, Copy)]
enum BytesCase {
    Random,
    Empty,
    Small,
    RepeatedZero,
    RepeatedMax,
    Alternating,
}

impl BytesCase {
    const WEIRD_CHOICES: &'static [Choice<Self>] = &[
        choice(25, Self::Empty),
        choice(20, Self::RepeatedZero),
        choice(20, Self::RepeatedMax),
        choice(20, Self::Alternating),
        choice(15, Self::Small),
    ];

    fn pick_weird(rng: &Rng) -> Self {
        crate::rng::pick_choice(rng, Self::WEIRD_CHOICES)
    }
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
        match ValueCase::pick(self.rng) {
            ValueCase::Random => self.gen_random_value(domain.ty),
            ValueCase::Small => self.gen_small_value(domain.ty),
            ValueCase::Edge => self.gen_edge_value(domain.ty),
            ValueCase::NearExisting => self
                .near_existing_value(domain)
                .unwrap_or_else(|| self.gen_random_value(domain.ty)),
            ValueCase::Existing => self
                .existing_value(domain)
                .unwrap_or_else(|| self.gen_random_value(domain.ty)),
            ValueCase::Weird => self.gen_weird_value(domain.ty),
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
        match FreshValueCase::pick(self.rng) {
            FreshValueCase::Random => self.gen_random_value(ty),
            FreshValueCase::Small => self.gen_small_value(ty),
            FreshValueCase::Edge => self.gen_edge_value(ty),
            FreshValueCase::Weird => self.gen_weird_value(ty),
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
            Type::I64 => AlgebraicValue::I64(self.gen_i64_value(I64Case::Random)),
            Type::U64 => AlgebraicValue::U64(self.gen_u64_value(U64Case::Random)),
            Type::String => AlgebraicValue::String(self.gen_string_value(StringCase::RandomTagged).into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(self.gen_bytes_value(BytesCase::Random).into())),
            Type::Sum { variants } => {
                let tag = self.rng.index(variants as usize) as u8;
                AlgebraicValue::sum(tag, AlgebraicValue::U8(self.rng.next_u64() as u8))
            }
        }
    }

    fn gen_small_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(false),
            Type::I64 => AlgebraicValue::I64(self.gen_i64_value(I64Case::Small)),
            Type::U64 => AlgebraicValue::U64(self.gen_u64_value(U64Case::Small)),
            Type::String => AlgebraicValue::String(self.gen_string_value(StringCase::SmallAscii).into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(self.gen_bytes_value(BytesCase::Small).into())),
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
            Type::I64 => AlgebraicValue::I64(self.gen_i64_value(I64Case::Edge)),
            Type::U64 => AlgebraicValue::U64(self.gen_u64_value(U64Case::Edge)),
            Type::String => AlgebraicValue::String(self.gen_string_value(StringCase::Long).into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(self.gen_bytes_value(BytesCase::RepeatedMax).into())),
            Type::Sum { variants } => AlgebraicValue::sum(variants.saturating_sub(1), AlgebraicValue::U8(u8::MAX)),
        }
    }

    fn gen_weird_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::String => {
                let case = StringCase::pick_weird(self.rng);
                AlgebraicValue::String(self.gen_string_value(case).into())
            }
            Type::Bytes => {
                let case = BytesCase::pick_weird(self.rng);
                AlgebraicValue::Array(ArrayValue::U8(self.gen_bytes_value(case).into()))
            }
            _ => self.gen_edge_value(ty),
        }
    }

    fn gen_i64_value(&self, case: I64Case) -> i64 {
        match case {
            I64Case::Random => self.rng.next_u64() as i64,
            I64Case::Small => self.sample(&[-3, -2, -1, 0, 1, 2, 3]),
            I64Case::Edge => self.sample(&[i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]),
        }
    }

    fn gen_u64_value(&self, case: U64Case) -> u64 {
        match case {
            U64Case::Random => self.rng.next_u64(),
            U64Case::Small => self.sample(&[0, 1, 2, 3, 4, 5]),
            U64Case::Edge => self.sample(&[0, 1, 2, u64::MAX - 1, u64::MAX]),
        }
    }

    fn gen_string_value(&self, case: StringCase) -> String {
        match case {
            StringCase::RandomTagged => format!("v_{}", self.rng.next_u64()),
            StringCase::Empty => String::new(),
            StringCase::SmallAscii => self.sample(&["a", "aa", "ab", "b", "z", "v_0", "v_1"]).to_owned(),
            StringCase::OrderedPrefix => self.sample(&["a", "aa", "aaa", "ab", "aba", "abb", "b"]).to_owned(),
            StringCase::SqlEscaped => self
                .sample(&["quote'", "double\"quote", "back\\slash", "line\nbreak"])
                .to_owned(),
            StringCase::NullByte => "nul\0byte".to_owned(),
            StringCase::Long => "x".repeat(128),
        }
    }

    fn gen_bytes_value(&self, case: BytesCase) -> Vec<u8> {
        match case {
            BytesCase::Random => {
                let len = (self.rng.next_u64() % 16) as usize;
                (0..len).map(|_| self.rng.next_u64() as u8).collect()
            }
            BytesCase::Empty => Vec::new(),
            BytesCase::Small => self.sample(&[&[][..], &[0][..], &[1][..], &[0, 255][..]]).to_vec(),
            BytesCase::RepeatedZero => vec![0; 32],
            BytesCase::RepeatedMax => vec![255; 32],
            BytesCase::Alternating => vec![0, 255, 0, 255, 0, 255],
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

    fn sample<T: Copy>(&self, values: &[T]) -> T {
        values[self.rng.index(values.len())]
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
