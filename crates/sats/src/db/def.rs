use derive_more::Display;
use nonempty::NonEmpty;
use spacetimedb_primitives::*;

use crate::db::attr::{AttributeKind, ColumnIndexAttribute};
use crate::db::auth::{StAccess, StTableType};
use crate::de::BasicVecVisitor;
use crate::product_value::InvalidFieldError;
use crate::relation::{Column, DbTable, FieldName, FieldOnly, Header, TableField};
use crate::ser::SerializeArray;
use crate::{de, ser, AlgebraicValue, ProductValue};
use crate::{AlgebraicType, ProductType, ProductTypeElement};

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
    pub struct ConstraintFlags: u8 {
        const UNSET = Self::empty().bits();
        ///  Index no unique
        const INDEXED = 0b0001;
        /// Index unique
        const UNIQUE = Self::INDEXED.bits() | 0b0010;
        /// Unique + AutoInc
        const IDENTITY = Self::UNIQUE.bits() | 0b0100;
        /// Primary key column (implies Unique)
        const PRIMARY_KEY = Self::UNIQUE.bits() | 0b1000;
        /// PrimaryKey + AutoInc
        const PRIMARY_KEY_AUTO = Self::PRIMARY_KEY.bits() | 0b10000;
        /// PrimaryKey + Identity
        const PRIMARY_KEY_IDENTITY = Self::PRIMARY_KEY.bits() | Self::IDENTITY.bits();
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum ConstraintKind {
    UNSET,
    ///  Index no unique
    INDEXED,
    /// Index unique
    UNIQUE,
    /// Unique + AutoInc
    IDENTITY,
    /// Primary key column (implies Unique)
    PRIMARY_KEY,
    /// PrimaryKey + AutoInc
    PRIMARY_KEY_AUTO,
    /// PrimaryKey + Identity
    PRIMARY_KEY_IDENTITY,
}

/// Represents `constraints` for a database `table`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct Constraints {
    pub attr: ConstraintFlags,
}

impl Constraints {
    /// Creates a new `Constraints` instance with no constraints set.
    pub const fn unset() -> Self {
        Constraints {
            attr: ConstraintFlags::UNSET,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintFlags::INDEXED] set.
    pub const fn indexed() -> Self {
        Constraints {
            attr: ConstraintFlags::INDEXED,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintAttribute::UNIQUE' constraint set.
    pub const fn unique() -> Self {
        Constraints {
            attr: ConstraintFlags::UNIQUE,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintFlags::IDENTITY] set.
    pub const fn identity() -> Self {
        Constraints {
            attr: ConstraintFlags::IDENTITY,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintFlags::PRIMARY_KEY] set.
    pub const fn primary_key() -> Self {
        Constraints {
            attr: ConstraintFlags::PRIMARY_KEY,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintFlags::PRIMARY_KEY_AUTO] set.
    pub const fn primary_key_auto() -> Self {
        Constraints {
            attr: ConstraintFlags::PRIMARY_KEY_AUTO,
        }
    }

    /// Creates a new `Constraints` instance with [ConstraintFlags::PRIMARY_KEY_IDENTITY] set.
    pub const fn primary_key_identity() -> Self {
        Constraints {
            attr: ConstraintFlags::PRIMARY_KEY_IDENTITY,
        }
    }

    /// Adds a constraint to the existing constraints.
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb_sats::db::def::*;
    ///
    /// let constraints = Constraints::unset().push(ConstraintFlags::INDEXED);
    /// assert!(constraints.has_indexed());
    /// ```
    pub fn push(self, attr: ConstraintFlags) -> Self {
        Constraints { attr: self.attr | attr }
    }

    /// Returns the bits representing the constraints.
    pub const fn bits(&self) -> u8 {
        self.attr.bits()
    }

    /// Returns the [ConstraintKind] of constraints as an enum variant.
    ///
    /// NOTE: This represent the higher possible representation of a constraints, so for example
    /// `IDENTITY` imply that is `INDEXED, UNIQUE`
    pub fn kind(&self) -> ConstraintKind {
        match self {
            x if x.attr == ConstraintFlags::UNSET => ConstraintKind::UNSET,
            x if x.attr == ConstraintFlags::INDEXED => ConstraintKind::INDEXED,
            x if x.attr == ConstraintFlags::UNIQUE => ConstraintKind::UNIQUE,
            x if x.attr == ConstraintFlags::IDENTITY => ConstraintKind::IDENTITY,
            x if x.attr == ConstraintFlags::PRIMARY_KEY => ConstraintKind::PRIMARY_KEY,
            x if x.attr == ConstraintFlags::PRIMARY_KEY_AUTO => ConstraintKind::PRIMARY_KEY_AUTO,
            x if x.attr == ConstraintFlags::PRIMARY_KEY_IDENTITY => ConstraintKind::PRIMARY_KEY_IDENTITY,
            x => unreachable!("Unexpected value {x:?}"),
        }
    }

    /// Checks if the 'UNIQUE' constraint is set.
    pub const fn has_unique(&self) -> bool {
        self.attr.contains(ConstraintFlags::UNIQUE)
    }

    /// Checks if the 'INDEXED' constraint is set.
    pub const fn has_indexed(&self) -> bool {
        self.attr.contains(ConstraintFlags::INDEXED)
    }

    /// Checks if either 'IDENTITY' or 'PRIMARY_KEY_AUTO' constraints are set because the imply the use of
    /// auto increment sequence.
    pub const fn has_autoinc(&self) -> bool {
        self.attr.contains(ConstraintFlags::IDENTITY) || self.attr.contains(ConstraintFlags::PRIMARY_KEY_AUTO)
    }

    /// Checks if the 'PRIMARY_KEY' constraint is set.
    pub const fn has_pk(&self) -> bool {
        self.attr.contains(ConstraintFlags::PRIMARY_KEY)
    }
}

impl From<ConstraintFlags> for Constraints {
    fn from(attr: ConstraintFlags) -> Self {
        Constraints { attr }
    }
}

impl TryFrom<u8> for Constraints {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(Constraints {
            attr: ConstraintFlags::from_bits(v).ok_or(())?,
        })
    }
}

impl TryFrom<ColumnIndexAttribute> for Constraints {
    type Error = ();

    fn try_from(value: ColumnIndexAttribute) -> Result<Self, Self::Error> {
        Ok(match value.kind() {
            AttributeKind::UNSET => Constraints::unset(),
            AttributeKind::INDEXED => Constraints::indexed(),
            AttributeKind::UNIQUE => Constraints::unique(),
            AttributeKind::IDENTITY => Constraints::identity(),
            AttributeKind::PRIMARY_KEY => Constraints::primary_key(),
            AttributeKind::PRIMARY_KEY_AUTO => Constraints::primary_key_auto(),
            AttributeKind::PRIMARY_KEY_IDENTITY => Constraints::primary_key_identity(),
            AttributeKind::AUTO_INC => return Err(()),
        })
    }
}

impl<'de> de::Deserialize<'de> for Constraints {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(deserializer.deserialize_u8()?)
            .map_err(|_| de::Error::custom("invalid bitflags for Constraints"))
    }
}

impl ser::Serialize for Constraints {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.bits())
    }
}

impl<'de, T: de::Deserialize<'de> + Clone> de::Deserialize<'de> for NonEmpty<T> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let arr: Vec<T> = deserializer.deserialize_array(BasicVecVisitor)?;
        NonEmpty::from_slice(&arr).ok_or_else(|| {
            de::Error::custom(format!(
                "invalid NonEmpty<{}>. Len is {}",
                std::any::type_name::<T>(),
                arr.len()
            ))
        })
    }
}

impl<T: ser::Serialize> ser::Serialize for NonEmpty<T> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut arr = serializer.serialize_array(self.len())?;
        for x in self {
            arr.serialize_element(x)?;
        }
        arr.end()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Display, de::Deserialize, ser::Serialize)]
#[sats(crate = crate)]
pub enum IndexType {
    BTree = 0,
    Hash = 1,
}

impl From<IndexType> for u8 {
    fn from(value: IndexType) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for IndexType {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(IndexType::BTree),
            1 => Ok(IndexType::Hash),
            _ => Err(()),
        }
    }
}

impl TryFrom<&str> for IndexType {
    type Error = ();
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        match v {
            "BTree" => Ok(IndexType::BTree),
            "Hash" => Ok(IndexType::Hash),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    pub sequence_id: SequenceId,
    pub sequence_name: String,
    pub table_id: TableId,
    pub col_id: ColId,
    pub increment: i128,
    pub start: i128,
    pub min_value: i128,
    pub max_value: i128,
    pub allocated: i128,
}

/// This type is just the [SequenceSchema] without the autoinc fields
/// It's also adjusted to be convenient for specifying a new sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceDef {
    pub sequence_name: String,
    pub table_id: TableId,
    pub col_id: ColId,
    pub increment: i128,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    pub index_id: IndexId,
    pub table_id: TableId,
    pub index_name: String,
    pub is_unique: bool,
    pub cols: NonEmpty<ColId>,
    pub index_type: IndexType,
}

/// This type is just the [IndexSchema] without the autoinc fields
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, de::Deserialize, ser::Serialize)]
#[sats(crate = crate)]
pub struct IndexDef {
    pub table_id: TableId,
    pub cols: NonEmpty<ColId>,
    pub name: String,
    pub is_unique: bool,
    pub index_type: IndexType,
}

impl IndexDef {
    pub fn new(name: String, table_id: TableId, col_id: ColId, is_unique: bool) -> Self {
        Self {
            cols: NonEmpty::new(col_id),
            name,
            is_unique,
            table_id,
            index_type: IndexType::BTree,
        }
    }

    pub fn new_cols<Col: Into<NonEmpty<ColId>>>(name: String, table_id: TableId, is_unique: bool, cols: Col) -> Self {
        Self {
            cols: cols.into(),
            name,
            is_unique,
            table_id,
            index_type: IndexType::BTree,
        }
    }
}

impl From<IndexSchema> for IndexDef {
    fn from(value: IndexSchema) -> Self {
        Self {
            table_id: value.table_id,
            cols: value.cols,
            name: value.index_name,
            is_unique: value.is_unique,
            index_type: value.index_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    pub table_id: TableId,
    pub col_id: ColId,
    pub col_name: String,
    pub col_type: AlgebraicType,
    pub is_autoinc: bool,
}

impl From<&ColumnSchema> for ProductTypeElement {
    fn from(value: &ColumnSchema) -> Self {
        Self {
            name: Some(value.col_name.clone()),
            algebraic_type: value.col_type.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ColumnDefMeta {
    pub column: ProductTypeElement,
    pub attr: ColumnIndexAttribute,
    pub pos: usize,
}

impl From<&ColumnSchema> for ColumnDefMeta {
    fn from(value: &ColumnSchema) -> Self {
        Self {
            column: ProductTypeElement::from(value),
            // TODO(cloutiertyler): !!! This is not correct !!! We do not have the information regarding constraints here.
            // We should remove this field from the ColumnDef struct.
            attr: if value.is_autoinc {
                ColumnIndexAttribute::AUTO_INC
            } else {
                ColumnIndexAttribute::UNSET
            },
            pos: value.col_id.idx(),
        }
    }
}

/// This type is just the [ColumnSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub col_name: String,
    pub col_type: AlgebraicType,
    pub is_autoinc: bool,
}

impl From<ColumnSchema> for ColumnDef {
    fn from(value: ColumnSchema) -> Self {
        Self {
            col_name: value.col_name,
            col_type: value.col_type,
            is_autoinc: value.is_autoinc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSchema {
    pub constraint_id: ConstraintId,
    pub constraint_name: String,
    pub kind: Constraints,
    pub table_id: TableId,
    pub columns: NonEmpty<ColId>,
}

/// This type is just the [ConstraintSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintDef {
    pub(crate) constraint_name: String,
    pub(crate) kind: Constraints,
    pub(crate) table_id: TableId,
    pub(crate) columns: Vec<ColId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table_id: TableId,
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub indexes: Vec<IndexSchema>,
    pub constraints: Vec<ConstraintSchema>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl TableSchema {
    /// Check if the `name` of the [FieldName] exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_field(&self, field: &FieldName) -> Option<&ColumnSchema> {
        match field.field() {
            FieldOnly::Name(x) => self.get_column_by_name(x),
            FieldOnly::Pos(x) => self.get_column(x),
        }
    }

    /// Check if there is an index for this [FieldName]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_index_by_field(&self, field: &FieldName) -> Option<&IndexSchema> {
        let ColumnSchema { col_id, .. } = self.get_column_by_field(field)?;
        self.indexes.iter().find(
            |IndexSchema {
                 cols: NonEmpty { head: index_col, tail },
                 ..
             }| tail.is_empty() && index_col == col_id,
        )
    }

    pub fn get_column(&self, pos: usize) -> Option<&ColumnSchema> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|x| x.col_name == col_name)
    }

    /// Turn a [TableField] that could be an unqualified field `id` into `table.id`
    pub fn normalize_field(&self, or_use: &TableField) -> FieldName {
        FieldName::named(or_use.table.unwrap_or(&self.table_name), or_use.field)
    }

    /// Project the fields from the supplied `columns`.
    pub fn project(&self, columns: impl Iterator<Item = ColId>) -> Result<Vec<&ColumnSchema>, InvalidFieldError> {
        columns
            .map(|pos| {
                self.get_column(usize::from(pos)).ok_or(InvalidFieldError {
                    col_pos: pos,
                    name: None,
                })
            })
            .collect()
    }

    /// Utility for project the fields from the supplied `columns` that is a [NonEmpty<u32>],
    /// used for when the list of field columns have at least one value.
    pub fn project_not_empty(&self, columns: &NonEmpty<ColId>) -> Result<Vec<&ColumnSchema>, InvalidFieldError> {
        self.project(columns.iter().copied())
    }
}

impl From<&TableSchema> for ProductType {
    fn from(value: &TableSchema) -> Self {
        ProductType::new(
            value
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some(c.col_name.clone()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        )
    }
}

impl From<&TableSchema> for DbTable {
    fn from(value: &TableSchema) -> Self {
        DbTable::new(value.into(), value.table_id, value.table_type, value.table_access)
    }
}

impl From<TableSchema> for DbTable {
    fn from(value: TableSchema) -> Self {
        (&value).into()
    }
}

impl From<&TableSchema> for Header {
    fn from(value: &TableSchema) -> Self {
        Header::new(
            value.table_name.clone(),
            value
                .columns
                .iter()
                .map(|x| {
                    let field = FieldName::named(&value.table_name, &x.col_name);
                    let is_indexed = value.get_index_by_field(&field).is_some();

                    Column::new(field, x.col_type.clone(), x.col_id, is_indexed)
                })
                .collect(),
        )
    }
}

/// The magic table id zero, for use in [`IndexDef`]s.
///
/// The actual table id is usually not yet known when constructing an
/// [`IndexDef`]. [`AUTO_TABLE_ID`] can be used instead, which the storage
/// engine will replace with the actual table id upon creation of the table
/// respectively index.
pub const AUTO_TABLE_ID: TableId = TableId(0);

/// This type is just the [TableSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl TableDef {
    pub fn get_row_type(&self) -> ProductType {
        ProductType::new(
            self.columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: None,
                    algebraic_type: c.col_type.clone(),
                })
                .collect(),
        )
    }
}

impl From<ProductType> for TableDef {
    fn from(value: ProductType) -> Self {
        Self {
            table_name: "".to_string(),
            columns: value
                .elements
                .iter()
                .enumerate()
                .map(|(i, e)| ColumnDef {
                    col_name: e.name.to_owned().unwrap_or_else(|| i.to_string()),
                    col_type: e.algebraic_type.clone(),
                    is_autoinc: false,
                })
                .collect(),
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }
    }
}

impl From<&TableSchema> for TableDef {
    fn from(value: &TableSchema) -> Self {
        Self {
            table_name: value.table_name.clone(),
            columns: value.columns.iter().cloned().map(Into::into).collect(),
            indexes: value.indexes.iter().cloned().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
        }
    }
}

impl From<TableSchema> for TableDef {
    fn from(value: TableSchema) -> Self {
        Self {
            table_name: value.table_name,
            columns: value.columns.into_iter().map(Into::into).collect(),
            indexes: value.indexes.into_iter().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
        }
    }
}

/// For get the original `table_name` for where a [ColumnSchema] belongs.
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub column: ColumnSchema,
    pub table_name: String,
}

impl From<FieldDef> for FieldName {
    fn from(value: FieldDef) -> Self {
        FieldName::named(&value.table_name, &value.column.col_name)
    }
}

impl From<&FieldDef> for FieldName {
    fn from(value: &FieldDef) -> Self {
        FieldName::named(&value.table_name, &value.column.col_name)
    }
}

impl From<FieldDef> for ProductTypeElement {
    fn from(value: FieldDef) -> Self {
        let f: FieldName = (&value).into();
        ProductTypeElement::new(value.column.col_type, Some(f.to_string()))
    }
}

/// Describe the columns + meta attributes
/// TODO(cloutiertyler): This type should be deprecated and replaced with
/// ColumnDef or ColumnSchema where appropriate
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProductTypeMeta {
    pub columns: ProductType,
    pub attr: Vec<ColumnIndexAttribute>,
}

impl ProductTypeMeta {
    pub fn new(columns: ProductType) -> Self {
        Self {
            attr: vec![ColumnIndexAttribute::UNSET; columns.elements.len()],
            columns,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            attr: Vec::with_capacity(capacity),
            columns: ProductType::new(Vec::with_capacity(capacity)),
        }
    }

    pub fn clear(&mut self) {
        self.columns.elements.clear();
        self.attr.clear();
    }

    pub fn push(&mut self, name: &str, ty: AlgebraicType, attr: ColumnIndexAttribute) {
        self.columns
            .elements
            .push(ProductTypeElement::new(ty, Some(name.to_string())));
        self.attr.push(attr);
    }

    /// Removes the data at position `index` and returns it.
    ///
    /// # Panics
    ///
    /// If `index` is out of bounds.
    pub fn remove(&mut self, index: usize) -> (ProductTypeElement, ColumnIndexAttribute) {
        (self.columns.elements.remove(index), self.attr.remove(index))
    }

    /// Return mutable references to the data at position `index`, or `None` if
    /// the index is out of bounds.
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut ProductTypeElement, &mut ColumnIndexAttribute)> {
        self.columns
            .elements
            .get_mut(index)
            .and_then(|pte| self.attr.get_mut(index).map(|attr| (pte, attr)))
    }

    pub fn with_attributes(iter: impl Iterator<Item = (ProductTypeElement, ColumnIndexAttribute)>) -> Self {
        let mut columns = Vec::new();
        let mut attrs = Vec::new();
        for (col, attr) in iter {
            columns.push(col);
            attrs.push(attr);
        }
        Self {
            attr: attrs,
            columns: ProductType::new(columns),
        }
    }

    pub fn len(&self) -> usize {
        self.columns.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.elements.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = ColumnDefMeta> + '_ {
        self.columns
            .elements
            .iter()
            .zip(self.attr.iter())
            .enumerate()
            .map(|(pos, (column, attr))| ColumnDefMeta {
                column: column.clone(),
                attr: *attr,
                pos,
            })
    }

    pub fn with_defaults<'a>(
        &'a self,
        row: &'a mut ProductValue,
    ) -> impl Iterator<Item = (ColumnDefMeta, &'a mut AlgebraicValue)> + 'a {
        self.iter()
            .zip(row.elements.iter_mut())
            .filter(|(col, _)| col.attr.has_autoinc())
    }
}

impl From<ProductType> for ProductTypeMeta {
    fn from(value: ProductType) -> Self {
        ProductTypeMeta::new(value)
    }
}

impl From<ProductTypeMeta> for ProductType {
    fn from(value: ProductTypeMeta) -> Self {
        value.columns
    }
}
