use spacetimedb_lib::sats::{self, product, SatsString};
use std::fmt::Debug;
use std::hash::Hash;

pub const BENCH_PKEY_INDEX: u32 = 0;

// the following piece of code must remain synced with `modules/bencmarks/src/lib.rs`
// These are the schemas used for these database tables outside of the benchmark module.
// It needs to match the schemas used inside the benchmark .

// ---------- SYNCED CODE ----------
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Person {
    id: u32,
    name: String,
    age: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    id: u32,
    x: u64,
    y: u64,
}
// ---------- END SYNCED CODE ----------

pub trait BenchTable: Debug + Clone + PartialEq + Eq + Hash {
    /// PascalCase name. This is used to name tables.
    fn name_pascal_case() -> &'static str;
    /// snake_case name. This is used to look up reducers.
    fn name_snake_case() -> &'static str;

    /// Note: the first field will be used as the primary key, when using
    /// `TableStyle::Unique`. It should be a u32.
    fn product_type() -> sats::ProductType;
    /// MUST match product_type.
    fn into_product_value(self) -> sats::ProductValue;

    /// This should be a tuple like (u32, String, u32).
    /// Can be inserted with a prepared statement.
    /// Order must be the same as that used in `product_type`.
    type SqliteParams: rusqlite::Params;
    fn into_sqlite_params(self) -> Self::SqliteParams;
}

impl BenchTable for Person {
    fn name_pascal_case() -> &'static str {
        "Person"
    }
    fn name_snake_case() -> &'static str {
        "person"
    }

    fn product_type() -> sats::ProductType {
        [
            ("id", sats::AlgebraicType::U32),
            ("name", sats::AlgebraicType::String),
            ("age", sats::AlgebraicType::U64),
        ]
        .into()
    }
    fn into_product_value(self) -> sats::ProductValue {
        sats::product![self.id, SatsString::from_string(self.name), self.age]
    }

    type SqliteParams = (u32, String, u64);
    fn into_sqlite_params(self) -> Self::SqliteParams {
        (self.id, self.name, self.age)
    }
}

impl BenchTable for Location {
    fn name_pascal_case() -> &'static str {
        "Location"
    }
    fn name_snake_case() -> &'static str {
        "location"
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
        product![self.id, self.x, self.y]
    }

    type SqliteParams = (u32, u64, u64);
    fn into_sqlite_params(self) -> Self::SqliteParams {
        (self.id, self.x, self.x)
    }
}

/// How we configure the indexes for a table used in benchmarks.
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum IndexStrategy {
    /// Unique "id" field at index 0
    Unique,
    /// No unique field or indexes
    NonUnique,
    /// Non-unique index on all fields
    MultiIndex,
}

impl IndexStrategy {
    pub fn snake_case(&self) -> &'static str {
        match self {
            IndexStrategy::Unique => "unique",
            IndexStrategy::NonUnique => "non_unique",
            IndexStrategy::MultiIndex => "multi_index",
        }
    }
}

pub fn table_name<T: BenchTable>(style: IndexStrategy) -> String {
    let prefix = match style {
        IndexStrategy::Unique => "Unique",
        IndexStrategy::NonUnique => "NonUnique",
        IndexStrategy::MultiIndex => "MultiIndex",
    };
    let name = T::name_pascal_case();

    format!("{prefix}{name}")
}
pub fn snake_case_table_name<T: BenchTable>(style: IndexStrategy) -> String {
    let prefix = style.snake_case();
    let name = T::name_snake_case();

    format!("{prefix}_{name}")
}

// ---------- data synthesis ----------

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

impl RandomTable for Person {
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self {
        let name = nth_name(rng.gen() % buckets);
        let age = rng.gen() % buckets;
        Person { id, name, age }
    }
}

impl RandomTable for Location {
    fn gen(id: u32, rng: &mut XorShiftLite, buckets: u64) -> Self {
        let x = rng.gen() % buckets;
        let y = rng.gen() % buckets;
        Location { id, x, y }
    }
}

pub fn create_sequential<T: RandomTable>(seed: u64, count: u32, buckets: u64) -> Vec<T> {
    let mut rng = XorShiftLite(seed);
    (0..count).map(|id| T::gen(id, &mut rng, buckets)).collect()
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
    use super::{nth_name, XorShiftLite};

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
}
