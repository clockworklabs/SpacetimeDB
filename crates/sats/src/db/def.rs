use crate::db::auth::{StAccess, StTableType};
use crate::de::BasicVecVisitor;
use crate::product_value::InvalidFieldError;
use crate::relation::{Column, DbTable, FieldName, FieldOnly, Header, TableField};
use crate::ser::SerializeArray;
use crate::{de, impl_deserialize, impl_serialize, ser, AlgebraicValue, ProductValue};
use crate::{AlgebraicType, ProductType, ProductTypeElement};
use derive_more::Display;
use nonempty::NonEmpty;
use spacetimedb_primitives::*;

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

impl_deserialize!([] Constraints, de => Self::try_from(de.deserialize_u8()?)
    .map_err(|_| de::Error::custom("invalid bitflags for `Constraints`"))
);
impl_serialize!([] Constraints, (self, ser) => ser.serialize_u8(self.bits()));

impl_deserialize!([T: de::Deserialize<'de> + Clone] NonEmpty<T>, de => {
    let arr: Vec<T> = de.deserialize_array(BasicVecVisitor)?;
    Self::from_slice(&arr).ok_or_else(|| {
        de::Error::custom(format!(
            "invalid NonEmpty<{}>. Len is {}",
            std::any::type_name::<T>(),
            arr.len()
        ))
    })
});
impl_serialize!([T: ser::Serialize] NonEmpty<T>, (self, ser) => {
    let mut arr = ser.serialize_array(self.len())?;
    self.into_iter().try_for_each(|x| arr.serialize_element(x))?;
    arr.end()
});

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

    pub fn new_cols(name: String, table_id: TableId, is_unique: bool, cols: impl Into<NonEmpty<ColId>>) -> Self {
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
    pub attr: Constraints,
    pub pos: usize,
}

/// Describe the columns + meta attributes
/// TODO(cloutiertyler): This type should be deprecated and replaced with
/// ColumnDef or ColumnSchema where appropriate
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProductTypeMeta {
    pub columns: ProductType,
    pub attr: Vec<Constraints>,
}

impl ProductTypeMeta {
    pub fn new(columns: ProductType) -> Self {
        Self {
            attr: vec![Constraints::unset(); columns.elements.len()],
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

    pub fn push(&mut self, name: &str, ty: AlgebraicType, attr: Constraints) {
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
    pub fn remove(&mut self, index: usize) -> (ProductTypeElement, Constraints) {
        (self.columns.elements.remove(index), self.attr.remove(index))
    }

    /// Return mutable references to the data at position `index`, or `None` if
    /// the index is out of bounds.
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut ProductTypeElement, &mut Constraints)> {
        self.columns
            .elements
            .get_mut(index)
            .and_then(|pte| self.attr.get_mut(index).map(|attr| (pte, attr)))
    }

    pub fn with_attributes(iter: impl Iterator<Item = (ProductTypeElement, Constraints)>) -> Self {
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
    pub constraints: Constraints,
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
                let index = pos.idx();
                self.get_column(index).ok_or(InvalidFieldError { index, name: None })
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
        (&value).into()
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
