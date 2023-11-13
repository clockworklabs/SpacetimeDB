use anyhow::Context;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::db::attr::ColumnAttribute;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::{ColumnDef, IndexDef, IndexType, AUTO_TABLE_ID};
use spacetimedb_sats::{impl_serialize, WithTypespace};

pub mod address;
pub mod filter;
pub mod identity;
pub mod name;
pub mod operator;
pub mod primary_key;
pub mod type_def {
    pub use spacetimedb_sats::{AlgebraicType, ProductType, ProductTypeElement, SumType};
}
pub mod type_value {
    pub use spacetimedb_sats::{AlgebraicValue, ProductValue};
}

pub mod error;
#[cfg(feature = "serde")]
pub mod recovery;
#[cfg(feature = "cli")]
pub mod util;
pub mod version;

pub use address::Address;
pub use identity::Identity;
pub use primary_key::PrimaryKey;
pub use spacetimedb_sats::hash::{self, hash_bytes, Hash};
pub use spacetimedb_sats::relation;
pub use spacetimedb_sats::DataKey;
pub use spacetimedb_sats::{self as sats, bsatn, buffer, de, ser};
pub use type_def::*;
pub use type_value::{AlgebraicValue, ProductValue};

pub const MODULE_ABI_MAJOR_VERSION: u16 = 7;

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
#[sats(crate = crate)]
pub struct TableDef {
    pub name: String,
    /// data should always point to a ProductType in the typespace
    pub data: sats::AlgebraicTypeRef,
    pub column_attrs: Vec<ColumnAttribute>,
    pub indexes: Vec<IndexDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl TableDef {
    pub fn into_table_def(table: WithTypespace<'_, TableDef>) -> anyhow::Result<spacetimedb_sats::db::def::TableDef> {
        let schema = table
            .map(|t| &t.data)
            .resolve_refs()
            .context("recursive types not yet supported")?;
        let schema = schema.into_product().ok().context("table not a product type?")?;
        let table = table.ty();
        anyhow::ensure!(
            table.column_attrs.len() == schema.elements.len(),
            "mismatched number of columns"
        );

        // Build single-column index definitions, determining `is_unique` from
        // their respective column attributes.
        let mut columns = Vec::with_capacity(schema.elements.len());
        let mut indexes = Vec::new();
        for (col_id, (ty, col_attr)) in std::iter::zip(&schema.elements, &table.column_attrs).enumerate() {
            let col = ColumnDef {
                col_name: ty.name.clone().context("column without name")?,
                col_type: ty.algebraic_type.clone(),
                is_autoinc: col_attr.has_autoinc(),
            };

            let index_for_column = table.indexes.iter().find(|index| {
                // Ignore multi-column indexes
                index.cols.tail.is_empty() && index.cols.head.idx() == col_id
            });

            // If there's an index defined for this column already, use it,
            // making sure that it is unique if the column has a unique constraint
            let index_info = if let Some(index) = index_for_column {
                Some((index.name.clone(), index.index_type))
            } else if col_attr.has_unique() {
                // If you didn't find an index, but the column is unique then create a unique btree index
                // anyway.
                Some((format!("{}_{}_unique", table.name, col.col_name), IndexType::BTree))
            } else {
                None
            };
            if let Some((name, ty)) = index_info {
                match ty {
                    IndexType::BTree => {}
                    // TODO
                    IndexType::Hash => anyhow::bail!("hash indexes not yet supported"),
                }
                indexes.push(spacetimedb_sats::db::def::IndexDef::new(
                    name,
                    AUTO_TABLE_ID,
                    ColId(col_id as u32),
                    col_attr.has_unique(),
                ))
            }
            columns.push(col);
        }

        // Multi-column indexes cannot be unique (yet), so just add them.
        let multi_col_indexes = table.indexes.iter().filter_map(|index| {
            (index.cols.len() > 1).then(|| {
                spacetimedb_sats::db::def::IndexDef::new_cols(
                    index.name.clone(),
                    AUTO_TABLE_ID,
                    false,
                    index.cols.clone(),
                )
            })
        });
        indexes.extend(multi_col_indexes);

        Ok(spacetimedb_sats::db::def::TableDef {
            table_name: table.name.clone(),
            columns,
            indexes,
            table_type: table.table_type,
            table_access: table.table_access,
        })
    }
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
