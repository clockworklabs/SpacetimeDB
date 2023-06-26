pub use spacetimedb_sats::buffer;
pub mod address;
pub mod data_key;
pub mod filter;
pub mod identity;
pub use spacetimedb_sats::de;
pub mod error;
pub mod hash;
#[cfg(feature = "serde")]
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
#[cfg(feature = "serde")]
pub mod recovery;
pub mod table;
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
use spacetimedb_sats::de::{Deserializer, Error};
use spacetimedb_sats::ser::Serializer;

pub const MODULE_ABI_VERSION: VersionTuple = VersionTuple::new(2, 0);

// if it ends up we need more fields in the future, we can split one of them in two
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
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

/// Describe the visibility of the table
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum StAccess {
    /// Visible to all
    Public,
    /// Visible only to the owner
    Private,
}

impl StAccess {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    /// Select the appropriated [Self] for the name.
    ///
    /// A name that start with '_' like '_sample' is [Self::Private]
    pub fn for_name(of: &str) -> Self {
        if of.starts_with('_') {
            Self::Private
        } else {
            Self::Public
        }
    }
}

impl<'a> TryFrom<&'a str> for StAccess {
    type Error = &'a str;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(match value {
            "public" => Self::Public,
            "private" => Self::Private,
            x => return Err(x),
        })
    }
}

impl ser::Serialize for StAccess {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> de::Deserialize<'de> for StAccess {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = deserializer.deserialize_str_slice()?;
        StAccess::try_from(value).map_err(|x| {
            Error::custom(format!(
                "DecodeError for StAccess: `{x}`. Expected `public` | 'private'"
            ))
        })
    }
}

/// Describe is the table is a `system table` or not.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum StTableType {
    /// Created by the system
    ///
    /// System tables are `StAccess::Public` by default
    System,
    /// Created by the User
    User,
}

impl StTableType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

impl<'a> TryFrom<&'a str> for StTableType {
    type Error = &'a str;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(match value {
            "system" => Self::System,
            "user" => Self::User,
            x => return Err(x),
        })
    }
}

impl ser::Serialize for StTableType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> de::Deserialize<'de> for StTableType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = deserializer.deserialize_str_slice()?;
        StTableType::try_from(value).map_err(|x| {
            Error::custom(format!(
                "DecodeError for StTableType: `{x}`. Expected 'system' | 'user'"
            ))
        })
    }
}

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

    pub fn serialize_args<'a>(ty: sats::TypeInSpace<'a, Self>, value: &'a ProductValue) -> impl ser::Serialize + 'a {
        ReducerArgsWithSchema { value, ty }
    }

    pub fn deserialize(
        ty: sats::TypeInSpace<'_, Self>,
    ) -> impl for<'de> de::DeserializeSeed<'de, Output = ProductValue> + '_ {
        ReducerDeserialize(ty)
    }
}

struct ReducerDeserialize<'a>(sats::TypeInSpace<'a, ReducerDef>);

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
    ty: sats::TypeInSpace<'a, ReducerDef>,
}

impl ser::Serialize for ReducerArgsWithSchema<'_> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use itertools::Itertools;
        use ser::SerializeSeqProduct;
        let mut seq = serializer.serialize_seq_product(self.value.elements.len())?;
        for (value, elem) in self.value.elements.iter().zip_eq(&self.ty.ty().args) {
            seq.serialize_element(&self.ty.with(&elem.algebraic_type).with_value(value))?;
        }
        seq.end()
    }
}

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

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, de::Deserialize, ser::Serialize)]
pub enum ColumnIndexAttribute {
    #[default]
    UnSet = 0,
    /// Unique + AutoInc
    Identity = 1,
    /// Index unique
    Unique = 2,
    ///  Index no unique
    Indexed = 3,
    /// Generate the next [Sequence]
    AutoInc = 4,
    /// Primary key column (implies Unique)
    PrimaryKey = 5,
    /// PrimaryKey + AutoInc
    PrimaryKeyAuto = 6,
}

impl ColumnIndexAttribute {
    pub const fn is_unique(self) -> bool {
        matches!(
            self,
            Self::Identity | Self::Unique | Self::PrimaryKey | Self::PrimaryKeyAuto
        )
    }
    pub const fn is_autoinc(self) -> bool {
        matches!(self, Self::Identity | Self::AutoInc | Self::PrimaryKeyAuto)
    }
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::PrimaryKey | Self::PrimaryKeyAuto)
    }
}

impl TryFrom<u8> for ColumnIndexAttribute {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::UnSet),
            1 => Ok(Self::Identity),
            2 => Ok(Self::Unique),
            3 => Ok(Self::Indexed),
            4 => Ok(Self::AutoInc),
            5 => Ok(Self::PrimaryKey),
            6 => Ok(Self::PrimaryKeyAuto),
            _ => Err(()),
        }
    }
}

// use std::fmt;
//
// #[cfg(feature = "serde")]
// use serde::de::Expected as SerdeExpected;
// #[cfg(not(feature = "serde"))]
// use Sized as SerdeExpected;
// fn fmt_fn(f: impl Fn(&mut fmt::Formatter) -> fmt::Result) -> impl fmt::Display + fmt::Debug + SerdeExpected {
//     struct FDisplay<F>(F);
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Debug for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     #[cfg(feature = "serde")]
//     impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> serde::de::Expected for FDisplay<F> {
//         fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             (self.0)(f)
//         }
//     }
//     FDisplay(f)
// }
