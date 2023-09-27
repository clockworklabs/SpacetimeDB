use auth::StAccess;
use auth::StTableType;
use sats::impl_serialize;
pub use spacetimedb_sats::buffer;
pub mod address;
pub mod data_key;
pub mod filter;
pub mod identity;
pub use spacetimedb_sats::de;
pub mod error;
pub mod hash;
pub mod name;
pub mod operator;
pub mod primary_key;
pub use spacetimedb_sats::ser;
pub mod type_def {
    pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue, ProductValue};
}
pub mod auth;
#[cfg(feature = "serde")]
pub mod recovery;
pub mod relation;
pub mod table;
#[cfg(feature = "cli")]
pub mod util;
pub mod version;

pub use spacetimedb_sats::bsatn;

pub use address::Address;
pub use data_key::DataKey;
pub use hash::Hash;
pub use identity::Identity;
pub use primary_key::PrimaryKey;
pub use type_def::*;
pub use type_value::{AlgebraicValue, ProductValue};

pub use spacetimedb_sats as sats;

pub const MODULE_ABI_MAJOR_VERSION: u16 = 6;

// if it ends up we need more fields in the future, we can split one of them in two
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct VersionTuple {
    /// Breaking change; different major versions are not at all compatible with each other.
    pub major: u16,
    /// Non-breaking change; a host can run a module that requests an older minor version than the
    /// host implements, but not the other way around
    pub minor: u16,
}

impl VersionTuple {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    #[inline]
    pub const fn eq(self, other: Self) -> bool {
        self.major == other.major && self.minor == other.minor
    }

    /// Checks if a host implementing this version can run a module that expects `module_version`
    #[inline]
    pub const fn supports(self, module_version: VersionTuple) -> bool {
        self.major == module_version.major && self.minor >= module_version.minor
    }

    #[inline]
    pub const fn from_u32(v: u32) -> Self {
        let major = (v >> 16) as u16;
        let minor = (v & 0xFF) as u16;
        Self { major, minor }
    }

    #[inline]
    pub const fn to_u32(self) -> u32 {
        (self.major as u32) << 16 | self.minor as u32
    }
}

impl std::fmt::Display for VersionTuple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { major, minor } = *self;
        write!(f, "{major}.{minor}")
    }
}

extern crate self as spacetimedb_lib;

//WARNING: Change this structure(or any of their members) is an ABI change.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, de::Deserialize, ser::Serialize)]
pub struct TableDef {
    pub name: String,
    /// data should always point to a ProductType in the typespace
    pub data: sats::AlgebraicTypeRef,
    pub column_attrs: Vec<ColumnIndexAttribute>,
    pub indexes: Vec<IndexDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub struct ReducerDef {
    pub name: String,
    pub args: Vec<ProductTypeElement>,
}

impl ReducerDef {
    pub fn encode(&self, writer: &mut impl buffer::BufWriter) {
        bsatn::to_writer(writer, self).unwrap()
    }

    pub fn serialize_args<'a>(ty: sats::WithTypespace<'a, Self>, value: &'a ProductValue) -> impl ser::Serialize + 'a {
        ReducerArgsWithSchema { value, ty }
    }

    pub fn deserialize(
        ty: sats::WithTypespace<'_, Self>,
    ) -> impl for<'de> de::DeserializeSeed<'de, Output = ProductValue> + '_ {
        ReducerDeserialize(ty)
    }
}

struct ReducerDeserialize<'a>(sats::WithTypespace<'a, ReducerDef>);

impl<'de> de::DeserializeSeed<'de> for ReducerDeserialize<'_> {
    type Output = ProductValue;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Output, D::Error> {
        deserializer.deserialize_product(self)
    }
}

impl<'de> de::ProductVisitor<'de> for ReducerDeserialize<'_> {
    type Output = ProductValue;

    fn product_name(&self) -> Option<&str> {
        Some(&self.0.ty().name)
    }
    fn product_len(&self) -> usize {
        self.0.ty().args.len()
    }
    fn product_kind(&self) -> de::ProductKind {
        de::ProductKind::ReducerArgs
    }

    fn visit_seq_product<A: de::SeqProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_seq_product(self.0.map(|r| &*r.args), &self, tup)
    }

    fn visit_named_product<A: de::NamedProductAccess<'de>>(self, tup: A) -> Result<Self::Output, A::Error> {
        de::visit_named_product(self.0.map(|r| &*r.args), &self, tup)
    }
}

struct ReducerArgsWithSchema<'a> {
    value: &'a ProductValue,
    ty: sats::WithTypespace<'a, ReducerDef>,
}
impl_serialize!([] ReducerArgsWithSchema<'_>, (self, ser) => {
    use itertools::Itertools;
    use ser::SerializeSeqProduct;
    let mut seq = ser.serialize_seq_product(self.value.elements.len())?;
    for (value, elem) in self.value.elements.iter().zip_eq(&self.ty.ty().args) {
        seq.serialize_element(&self.ty.with(&elem.algebraic_type).with_value(value))?;
    }
    seq.end()
});

//WARNING: Change this structure(or any of their members) is an ABI change.
#[derive(Debug, Clone, Default, de::Deserialize, ser::Serialize)]
pub struct ModuleDef {
    pub typespace: sats::Typespace,
    pub tables: Vec<TableDef>,
    pub reducers: Vec<ReducerDef>,
    pub misc_exports: Vec<MiscModuleExport>,
}

// an enum to keep it extensible without breaking abi
#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub enum MiscModuleExport {
    TypeAlias(TypeAlias),
}

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub struct TypeAlias {
    pub name: String,
    pub ty: sats::AlgebraicTypeRef,
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, de::Deserialize, ser::Serialize)]
pub struct IndexDef {
    pub name: String,
    pub ty: IndexType,
    pub col_ids: Vec<u8>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, de::Deserialize, ser::Serialize)]
pub enum IndexType {
    BTree,
    Hash,
}

// NOTE: Duplicated in `crates/bindings-macro/src/lib.rs`
bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
    pub struct ColumnIndexAttribute: u8 {
        const UNSET = Self::empty().bits();
        ///  Index no unique
        const INDEXED = 0b0001;
        /// Generate the next [Sequence]
        const AUTO_INC = 0b0010;
        /// Index unique
        const UNIQUE = Self::INDEXED.bits() | 0b0100;
        /// Unique + AutoInc
        const IDENTITY = Self::UNIQUE.bits() | Self::AUTO_INC.bits();
        /// Primary key column (implies Unique)
        const PRIMARY_KEY = Self::UNIQUE.bits() | 0b1000;
        /// PrimaryKey + AutoInc
        const PRIMARY_KEY_AUTO = Self::PRIMARY_KEY.bits() | Self::AUTO_INC.bits();
    }
}

impl ColumnIndexAttribute {
    pub const fn is_unique(self) -> bool {
        self.contains(Self::UNIQUE)
    }
    pub const fn is_indexed(self) -> bool {
        self.contains(Self::INDEXED)
    }
    pub const fn is_autoinc(self) -> bool {
        self.contains(Self::AUTO_INC)
    }
    pub const fn is_primary(self) -> bool {
        self.contains(Self::PRIMARY_KEY)
    }
}

impl TryFrom<u8> for ColumnIndexAttribute {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Self::from_bits(v).ok_or(())
    }
}

impl<'de> de::Deserialize<'de> for ColumnIndexAttribute {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_bits(deserializer.deserialize_u8()?)
            .ok_or_else(|| de::Error::custom("invalid bitflags for ColumnIndexAttribute"))
    }
}

impl ser::Serialize for ColumnIndexAttribute {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.bits())
    }
}
