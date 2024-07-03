use db::raw_def::RawDatabaseDefV1;
use spacetimedb_sats::impl_serialize;

pub mod address;
pub mod db;
pub mod filter;
pub mod identity;
pub mod operator;
pub mod type_def {
    pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue, ProductValue};
}
pub mod error;
pub mod relation;
pub mod version;

pub use address::Address;
pub use identity::Identity;
pub use spacetimedb_sats::hash::{self, hash_bytes, Hash};
pub use spacetimedb_sats::{self as sats, bsatn, buffer, de, ser};
pub use type_def::*;
pub use type_value::{AlgebraicValue, ProductValue};

pub const MODULE_ABI_MAJOR_VERSION: u16 = 8;

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

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub struct ReducerDef {
    pub name: Box<str>,
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

#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
pub enum ModuleDef {
    V1(ModuleDefV1),
}

impl Default for ModuleDef {
    fn default() -> Self {
        Self::V1(ModuleDefV1::default())
    }
}

//WARNING: Change this structure(or any of their members) is an ABI change.
#[derive(Debug, Clone, Default, de::Deserialize, ser::Serialize)]
pub struct ModuleDefV1 {
    pub database_def: RawDatabaseDefV1,
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

impl ModuleDefV1 {
    pub fn validate_reducers(&self) -> Result<(), ModuleValidationError> {
        for reducer in &self.reducers {
            match &*reducer.name {
                // in the future, these should maybe be flagged as lifecycle reducers by a MiscModuleExport
                //  or something, rather than by magic names
                "__init__" => {}
                "__identity_connected__" | "__identity_disconnected__" | "__update__" | "__migrate__" => {
                    if !reducer.args.is_empty() {
                        return Err(ModuleValidationError::InvalidLifecycleReducer {
                            reducer: reducer.name.clone(),
                        });
                    }
                }
                name if name.starts_with("__") && name.ends_with("__") => {
                    return Err(ModuleValidationError::UnknownDunderscore)
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ModuleValidationError {
    #[error("lifecycle reducer {reducer:?} has invalid signature")]
    InvalidLifecycleReducer { reducer: Box<str> },
    #[error("reducers with double-underscores at the start and end of their names are not allowed")]
    UnknownDunderscore,
}

/// Converts a hexadecimal string reference to a byte array.
///
/// This function takes a reference to a hexadecimal string and attempts to convert it into a byte array.
///
/// If the hexadecimal string starts with "0x", these characters are ignored.
pub fn from_hex_pad<R: hex::FromHex<Error = hex::FromHexError>, T: AsRef<[u8]>>(
    hex: T,
) -> Result<R, hex::FromHexError> {
    let hex = hex.as_ref();
    let hex = if hex.starts_with(b"0x") { &hex[2..] } else { hex };
    hex::FromHex::from_hex(hex)
}
