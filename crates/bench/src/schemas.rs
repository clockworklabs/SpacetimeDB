#![allow(non_camel_case_types)]

use serde::Deserialize;
use spacetimedb_lib::de::Deserialize as SatsDeserializer;
use spacetimedb_lib::sats;
use std::fmt::Debug;
use std::hash::Hash;

pub const BENCH_PKEY_INDEX: u32 = 0;

// the following piece of code must remain synced with `modules/benchmarks/src/synthetic.rs`
// These are the schemas used for these database tables outside of the benchmark module.
// It needs to match the schemas used inside the benchmark.

// ---------- SYNCED CODE ----------
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, SatsDeserializer)]
pub struct u32_u64_str {
    // column 0
    id: u32,
    // column 1
    age: u64,
    // column 2
    name: Box<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, SatsDeserializer)]
pub struct u32_u64_u64 {
    // column 0
    id: u32,
    // column 1
    x: u64,
    // column 2
    y: u64,
}
// ---------- END SYNCED CODE ----------

/// This is a duplicate of [`u32_u64_u64`] with the fields shuffled to minimize interior padding,
/// used to compare the effects of interior padding on BFLATN -> BSATN serialization.
///
/// This type *should not* be used for any benchmarks except `special::serialize_benchmarks`,
/// as it doesn't have proper implementations in modules or Sqlite.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, SatsDeserializer)]
pub struct u64_u64_u32 {
    x: u64,
    y: u64,
    id: u32,
}

/// A schema used in the benchmarks.
/// Schemas should convert to a `ProductType` / `ProductValue` in a canonical way.
/// We require that, when converted, to a ProductValue:
/// - column 0 is a u32 (used in many places)
/// - and column 1 is a u64 (used in `update_bulk`).
pub trait BenchTable: Debug + Clone + PartialEq + Eq + Hash {
    /// PascalCase name. This is used to name tables.
    fn name() -> &'static str;

    fn product_type() -> sats::ProductType;
    /// MUST match product_type.
    fn into_product_value(self) -> sats::ProductValue;

    /// This should be a tuple like (u32, String, u32).
    /// Can be inserted with a prepared statement.
    /// Order must be the same as that used in `product_type`.
    type SqliteParams: rusqlite::Params;
    fn into_sqlite_params(self) -> Self::SqliteParams;
}

impl BenchTable for u32_u64_str {
    fn name() -> &'static str {
        "u32_u64_str"
    }

    fn product_type() -> sats::ProductType {
        [
            ("id", sats::AlgebraicType::U32),
            ("age", sats::AlgebraicType::U64),
            ("name", sats::AlgebraicType::String),
        ]
        .into()
    }
    fn into_product_value(self) -> sats::ProductValue {
        sats::product![self.id, self.age, self.name,]
    }

    type SqliteParams = (u32, u64, Box<str>);
    fn into_sqlite_params(self) -> Self::SqliteParams {
        (self.id, self.age, self.name)
    }
}

impl BenchTable for u32_u64_u64 {
    fn name() -> &'static str {
        "u32_u64_u64"
    }

    fn product_type() -> sats::ProductType {
        [
            ("id", sats::AlgebraicType::U32),
            ("x", sats::AlgebraicType::U64),
            ("y", sats::AlgebraicType::U64),
        ]
        .into()
    }
    fn into_product_value(self) -> sats::ProductValue {
        sats::product![self.id, self.x, self.y]
    }

    type SqliteParams = (u32, u64, u64);
    fn into_sqlite_params(self) -> Self::SqliteParams {
        (self.id, self.x, self.y)
    }
}

impl BenchTable for u64_u64_u32 {
    fn name() -> &'static str {
        "u64_u64_u32"
    }
    fn product_type() -> sats::ProductType {
        [
            ("x", sats::AlgebraicType::U64),
            ("y", sats::AlgebraicType::U64),
            ("id", sats::AlgebraicType::U32),
        ]
        .into()
    }
    fn into_product_value(self) -> sats::ProductValue {
        sats::product![self.x, self.y, self.id]
    }

    type SqliteParams = ();
    fn into_sqlite_params(self) -> Self::SqliteParams {
        unimplemented!()
    }
}

/// How we configure the indexes for a table used in benchmarks.
/// TODO(jgilles): this should be more general, but for that we'll need to dynamically generate the modules...
#[derive(PartialEq, Copy, Clone, Debug, Deserialize)]
pub enum IndexStrategy {
    /// Unique "id" field at index 0
    #[serde(alias = "unique_0")]
    Unique0,
    /// No unique field or indexes
    #[serde(alias = "no_index")]
    NoIndex,
    /// Non-unique index on all fields
    #[serde(alias = "btree_each_column")]
    BTreeEachColumn,
}

impl IndexStrategy {
    pub fn name(&self) -> &'static str {
        match self {
            IndexStrategy::Unique0 => "unique_0",
            IndexStrategy::NoIndex => "no_index",
            IndexStrategy::BTreeEachColumn => "btree_each_column",
        }
    }
}

pub fn table_name<T: BenchTable>(style: IndexStrategy) -> String {
    let prefix = style.name();
    let name = T::name();

    format!("{prefix}_{name}")
}

// ---------- data synthesis ----------
#[derive(Clone)]
pub struct XorShiftLite(pub u64);
impl XorShiftLite {
    fn gen(&mut self) -> u64 {
        let old = self.0;
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        old
    }
}

pub trait RandomTable {
    /// Generate an instance of this table.
    ///
    /// `buckets` counts the number of buckets non-unique attributes are intended to fall into.
    /// e.g. the number of possible names a person can have, or the number of x positions a location can have.
    ///
    /// Then in the filter benchmarks, `mean_result_count = table_size / buckets`.
    ///
    /// Currently the same number of buckets is used for all attributes.
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self;
}

impl RandomTable for u32_u64_str {
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self {
        let name = nth_name(rng.gen() % buckets).into();
        let age = rng.gen() % buckets;
        u32_u64_str { id, name, age }
    }
}

impl RandomTable for u32_u64_u64 {
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self {
        let x = rng.gen() % buckets;
        let y = rng.gen() % buckets;
        u32_u64_u64 { id, x, y }
    }
}

impl RandomTable for u64_u64_u32 {
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self {
        let x = rng.gen() % buckets;
        let y = rng.gen() % buckets;
        u64_u64_u32 { x, y, id }
    }
}

pub fn create_sequential<T: RandomTable>(seed: u64, count: u32, buckets: u64) -> Vec<T> {
    let mut rng = XorShiftLite(seed);
    (0..count).map(|id| T::gen(id, &mut rng, buckets)).collect()
}

/// Create a table whose first `identical` rows are identical except for their `id` column.
/// The remainder of the rows in the table are random. The total size of the table is `total`.
/// Intended for filter benchmarks.
pub fn create_partly_identical<T: RandomTable>(seed: u64, identical: u64, total: u64) -> Vec<T> {
    // large to avoid duplicates.
    // technically, overlaps with the identical part of the table can still occur;
    // in a given row+column, this happens with probability 1 / 2^32, which is negligible.
    // We use u32::max because sqlite sometimes chokes on large u64s.
    let buckets = u32::MAX as u64;

    let mut rng = XorShiftLite(seed);
    let mut result = Vec::with_capacity(total as usize);
    let mut id = 0;
    for _ in 0..identical {
        // clone to preserve rng state
        let mut rng_ = rng.clone();
        result.push(T::gen(id as u32, &mut rng_, buckets));
        id += 1;
    }
    // advance rng
    drop(T::gen(id as u32, &mut rng, buckets));
    for _ in identical..total {
        result.push(T::gen(id as u32, &mut rng, buckets));
        id += 1;
    }
    result
}

/// May contain repeated IDs!
pub fn create_random<T: RandomTable>(seed: u64, count: u32, buckets: u64) -> Vec<T> {
    let mut rng = XorShiftLite(seed);
    (0..count)
        .map(|_| {
            let id = (rng.gen() % (u32::MAX as u64)) as u32;
            T::gen(id, &mut rng, buckets)
        })
        .collect()
}

const FIRST_NAMES: [&str; 32] = [
    "Anthony",
    "Tony",
    "Antonio",
    "Barbara",
    "Charles",
    "Daniel",
    "Danyl",
    "Darkholder Fleshbane",
    "Dan",
    "David",
    "Droog",
    "Elizabeth",
    "Liz",
    "James",
    "Jim",
    "Jimmy",
    "Jennifer",
    "Jen",
    "John",
    "Linda",
    "Lindy",
    "Margaret",
    "Marge",
    "Mary",
    "Michael",
    "Nutmeg",
    "Richard",
    "Dick",
    "Robert",
    "Thomas",
    "Tom",
    "Zanzibar",
];

const LAST_NAMES: [&str; 32] = [
    "Anderson",
    "Brown",
    "Carter",
    "Cook",
    "Davis",
    "Frogson",
    "Garcia",
    "Green",
    "Hall",
    "Harris",
    "Hill",
    "Hunch",
    "Jackson",
    "Johnson",
    "Jones",
    "Lewis",
    "Martin",
    "Miller",
    "Moore",
    "Morgan",
    "Robinson",
    "Sanchez",
    "Smith",
    "Taylor",
    "The Destroyer",
    "Thomas",
    "Thompson",
    "Walker",
    "White",
    "Williams",
    "Wilson",
    "Wood",
];

/// An injection (input-total one-to-one relation) from u64s to short strings.
/// Provides some variation in length.
pub fn nth_name(n: u64) -> String {
    let n = n as usize;
    let first = n % FIRST_NAMES.len();
    let last = (n / FIRST_NAMES.len()) % LAST_NAMES.len();
    let remaining = n / (FIRST_NAMES.len() * LAST_NAMES.len());
    assert_eq!(
        n,
        first + last * FIRST_NAMES.len() + remaining * FIRST_NAMES.len() * LAST_NAMES.len()
    );

    let first = FIRST_NAMES[first];
    let last = LAST_NAMES[last];
    format!("{last}, {first} [{remaining}]")
}

#[cfg(test)]
mod tests {
    use super::{create_partly_identical, nth_name, XorShiftLite};

    #[test]
    fn test_nth_name() {
        let mut rng = XorShiftLite(0xdeadbeef);
        for n in 0..1000 {
            let name = nth_name(n);
            assert_eq!(name, nth_name(n), "name gen deterministic");
            if n == 0 {
                continue;
            }
            // sample some earlier names to make sure we haven't overlapped
            for _ in 0..30 {
                let prev = rng.gen() % n;
                assert!(
                    name != nth_name(prev),
                    "names should not repeat, but {}->{} and {}->{}",
                    n,
                    name,
                    prev,
                    nth_name(prev)
                );
            }
        }
    }

    #[test]
    fn test_partly_identical() {
        use crate::schemas::u32_u64_str;

        let identical = 100;
        let total = 2000;

        let data = create_partly_identical::<u32_u64_str>(0xdeadbeef, identical, total);
        let p1 = data[0].clone();

        for item in data.iter().take(identical as usize).skip(1) {
            assert_ne!(p1.id, item.id, "identical part should still have distinct ids");
            assert_eq!(p1.name, item.name, "names should be identical");
            assert_eq!(p1.age, item.age, "ages should be identical");
        }
        for item in data.iter().take(total as usize).skip(identical as usize) {
            assert_ne!(p1.id, item.id, "identical part should still have distinct ids");
            assert_ne!(p1.name, item.name, "names should not be identical");
            assert_ne!(p1.age, item.age, "ages should not be identical");
        }
    }
}
