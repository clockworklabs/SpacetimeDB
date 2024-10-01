use crate::db::raw_def::v9::RawModuleDefV9Builder;
use crate::db::raw_def::RawTableDefV8;
use anyhow::Context;
use sats::typespace::TypespaceBuilder;
use spacetimedb_sats::{impl_serialize, WithTypespace};
use std::any::TypeId;
use std::collections::{btree_map, BTreeMap};

pub mod address;
pub mod db;
pub mod error;
pub mod filter;
pub mod auth;
pub mod identity;
pub mod operator;
pub mod relation;
pub mod scheduler;
pub mod version;

pub mod type_def {
    pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue, ProductValue};
}

pub use address::Address;
pub use identity::Identity;
pub use scheduler::ScheduleAt;
pub use spacetimedb_sats::hash::{self, hash_bytes, Hash};
pub use spacetimedb_sats::SpacetimeType;
pub use spacetimedb_sats::__make_register_reftype;
pub use spacetimedb_sats::{self as sats, bsatn, buffer, de, ser};
pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
pub use spacetimedb_sats::{AlgebraicValue, ProductValue};

pub const MODULE_ABI_MAJOR_VERSION: u16 = 10;

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
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub struct TableDesc {
    pub schema: RawTableDefV8,
    /// data should always point to a ProductType in the typespace
    pub data: sats::AlgebraicTypeRef,
}

impl TableDesc {
    pub fn into_table_def(table: WithTypespace<'_, TableDesc>) -> anyhow::Result<RawTableDefV8> {
        let schema = table
            .map(|t| &t.data)
            .resolve_refs()
            .context("recursive types not yet supported")?;
        let schema = schema.into_product().ok().context("table not a product type?")?;
        let table = table.ty();
        anyhow::ensure!(
            table.schema.columns.len() == schema.elements.len(),
            "mismatched number of columns"
        );

        Ok(table.schema.clone())
    }
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
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

//WARNING: Change this structure (or any of their members) is an ABI change.
#[derive(Debug, Clone, Default, SpacetimeType)]
#[sats(crate = crate)]
pub struct RawModuleDefV8 {
    pub typespace: sats::Typespace,
    pub tables: Vec<TableDesc>,
    pub reducers: Vec<ReducerDef>,
    pub misc_exports: Vec<MiscModuleExport>,
}

impl RawModuleDefV8 {
    pub fn builder() -> ModuleDefBuilder {
        ModuleDefBuilder::default()
    }

    pub fn with_builder(f: impl FnOnce(&mut ModuleDefBuilder)) -> Self {
        let mut builder = Self::builder();
        f(&mut builder);
        builder.finish()
    }
}

/// A versioned raw module definition.
///
/// This is what is actually returned by the module when `__describe_module__` is called, serialized to BSATN.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[non_exhaustive]
pub enum RawModuleDef {
    V8BackCompat(RawModuleDefV8),
    V9(db::raw_def::v9::RawModuleDefV9),
    // TODO(jgilles): It would be nice to have a custom error message if this fails with an unknown variant,
    // but I'm not sure if that can be done via the Deserialize trait.
}

/// A builder for a [`ModuleDef`].
#[derive(Default)]
pub struct ModuleDefBuilder {
    /// The module definition.
    module: RawModuleDefV8,
    /// The type map from `T: 'static` Rust types to sats types.
    type_map: BTreeMap<TypeId, sats::AlgebraicTypeRef>,
}

impl ModuleDefBuilder {
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Add a type that may not correspond to a Rust type.
    /// Used only in tests.
    #[cfg(feature = "test")]
    pub fn add_type_for_tests(&mut self, name: &str, ty: AlgebraicType) -> spacetimedb_sats::AlgebraicTypeRef {
        let slot_ref = self.module.typespace.add(ty);
        self.module.misc_exports.push(MiscModuleExport::TypeAlias(TypeAlias {
            name: name.to_owned(),
            ty: slot_ref,
        }));
        slot_ref
    }

    /// Add a table that may not correspond to a Rust type.
    /// Wraps it in a `TableDesc` and generates a corresponding `ProductType` in the typespace.
    /// Used only in tests.
    /// Returns the `AlgebraicTypeRef` of the generated `ProductType`.
    #[cfg(feature = "test")]
    pub fn add_table_for_tests(&mut self, schema: RawTableDefV8) -> spacetimedb_sats::AlgebraicTypeRef {
        let ty: ProductType = schema
            .columns
            .iter()
            .map(|c| ProductTypeElement {
                name: Some(c.col_name.clone()),
                algebraic_type: c.col_type.clone(),
            })
            .collect();
        let data = self.module.typespace.add(ty.into());
        self.add_type_alias(TypeAlias {
            name: schema.table_name.clone().into(),
            ty: data,
        });
        self.add_table(TableDesc { schema, data });
        data
    }

    pub fn add_table(&mut self, table: TableDesc) {
        self.module.tables.push(table)
    }

    pub fn add_reducer(&mut self, reducer: ReducerDef) {
        self.module.reducers.push(reducer)
    }

    #[cfg(feature = "test")]
    pub fn add_reducer_for_tests(&mut self, name: impl Into<Box<str>>, args: ProductType) {
        self.add_reducer(ReducerDef {
            name: name.into(),
            args: args.elements.to_vec(),
        });
    }

    pub fn add_misc_export(&mut self, misc_export: MiscModuleExport) {
        self.module.misc_exports.push(misc_export)
    }

    pub fn add_type_alias(&mut self, type_alias: TypeAlias) {
        self.add_misc_export(MiscModuleExport::TypeAlias(type_alias))
    }

    pub fn typespace(&self) -> &sats::Typespace {
        &self.module.typespace
    }

    pub fn finish(self) -> RawModuleDefV8 {
        self.module
    }
}

impl TypespaceBuilder for ModuleDefBuilder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        let r = match self.type_map.entry(typeid) {
            btree_map::Entry::Occupied(o) => *o.get(),
            btree_map::Entry::Vacant(v) => {
                // Bind a fresh alias to the unit type.
                let slot_ref = self.module.typespace.add(AlgebraicType::unit());
                // Relate `typeid -> fresh alias`.
                v.insert(slot_ref);

                // Alias provided? Relate `name -> slot_ref`.
                if let Some(name) = name {
                    self.module.misc_exports.push(MiscModuleExport::TypeAlias(TypeAlias {
                        name: name.to_owned(),
                        ty: slot_ref,
                    }));
                }

                // Borrow of `v` has ended here, so we can now convince the borrow checker.
                let ty = make_ty(self);
                self.module.typespace[slot_ref] = ty;
                slot_ref
            }
        };
        AlgebraicType::Ref(r)
    }
}

// an enum to keep it extensible without breaking abi
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub enum MiscModuleExport {
    TypeAlias(TypeAlias),
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct TypeAlias {
    pub name: String,
    pub ty: sats::AlgebraicTypeRef,
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
    let hex = if hex.starts_with(b"0x") {
        &hex[2..]
    } else if hex.starts_with(b"X'") {
        &hex[2..hex.len()]
    } else {
        hex
    };
    hex::FromHex::from_hex(hex)
}

/// Returns a resolved `AlgebraicType` (containing no `AlgebraicTypeRefs`) for a given `SpacetimeType`,
/// using the v9 moduledef infrastructure.
/// Panics if the type is recursive.
///
/// TODO: we could implement something like this in `sats` itself, but would need a lightweight `TypespaceBuilder` implementation there.
pub fn resolved_type_via_v9<T: SpacetimeType>() -> AlgebraicType {
    let mut builder = RawModuleDefV9Builder::new();
    let ty = T::make_type(&mut builder);
    let module = builder.finish();

    WithTypespace::new(&module.typespace, &ty)
        .resolve_refs()
        .expect("recursive types not supported")
}
