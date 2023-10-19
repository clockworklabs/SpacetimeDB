use derive_more::From;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::auth::{StAccess, StTableType};
use crate::error::RelationError;
use crate::table::ColumnDef;
use crate::DataKey;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{algebraic_type, AlgebraicType, ProductType, ProductTypeElement, Typespace, WithTypespace};

impl ColumnDef {
    pub fn name(&self) -> FieldOnly {
        if let Some(name) = &self.column.name {
            FieldOnly::Name(name)
        } else {
            FieldOnly::Pos(self.pos)
        }
    }
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct TableField<'a> {
    pub table: Option<&'a str>,
    pub field: &'a str,
}

pub fn extract_table_field(ident: &str) -> Result<TableField, RelationError> {
    let parts: Vec<_> = ident.split('.').take(3).collect();

    match parts[..] {
        [table, field] => Ok(TableField {
            table: Some(table),
            field,
        }),
        [field] => Ok(TableField { table: None, field }),
        _ => Err(RelationError::FieldPathInvalid(ident.to_string())),
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum FieldOnly<'a> {
    Name(&'a str),
    Pos(usize),
}

impl fmt::Display for FieldOnly<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldOnly::Name(x) => {
                write!(f, "{x}")
            }
            FieldOnly::Pos(x) => {
                write!(f, "{x}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum FieldName {
    Name { table: String, field: String },
    Pos { table: String, field: usize },
}

impl FieldName {
    pub fn named(table: &str, field: &str) -> Self {
        Self::Name {
            table: table.to_string(),
            field: field.to_string(),
        }
    }

    pub fn positional(table: &str, field: usize) -> Self {
        Self::Pos {
            table: table.to_string(),
            field,
        }
    }

    pub fn table(&self) -> &str {
        let (FieldName::Name { table, .. } | FieldName::Pos { table, .. }) = self;
        table
    }

    pub fn field(&self) -> FieldOnly {
        match self {
            FieldName::Name { field, .. } => FieldOnly::Name(field),
            FieldName::Pos { field, .. } => FieldOnly::Pos(*field),
        }
    }

    pub fn field_name(&self) -> Option<&str> {
        match self {
            FieldName::Name { field, .. } => Some(field),
            FieldName::Pos { .. } => None,
        }
    }

    pub fn into_field_name(self) -> Option<String> {
        match self {
            FieldName::Name { field, .. } => Some(field),
            FieldName::Pos { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, From)]
pub enum FieldExpr {
    Name(FieldName),
    Value(AlgebraicValue),
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldName::Name { table, field } => {
                write!(f, "{table}.{field}")
            }
            FieldName::Pos { table, field } => {
                write!(f, "{table}.{field}")
            }
        }
    }
}

impl fmt::Display for FieldExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldExpr::Name(x) => {
                write!(f, "{x}")
            }
            FieldExpr::Value(x) => {
                let ty = x.type_of();
                let ts = Typespace::new(vec![]);
                write!(f, "{}", WithTypespace::new(&ts, &ty).with_value(x).to_satn())
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ColumnOnlyField<'a> {
    pub field: FieldOnly<'a>,
    pub algebraic_type: &'a AlgebraicType,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Column {
    pub field: FieldName,
    pub algebraic_type: AlgebraicType,
}

impl Column {
    pub fn new(field: FieldName, algebraic_type: AlgebraicType) -> Self {
        Self { field, algebraic_type }
    }

    pub fn as_without_table(&self) -> ColumnOnlyField {
        ColumnOnlyField {
            field: self.field.field(),
            algebraic_type: &self.algebraic_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeaderOnlyField<'a> {
    pub fields: Vec<ColumnOnlyField<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Header {
    pub table_name: String,
    pub fields: Vec<Column>,
}

impl From<Header> for ProductType {
    fn from(value: Header) -> Self {
        ProductType::from_iter(
            value
                .fields
                .into_iter()
                .map(|x| ProductTypeElement::new(x.algebraic_type, x.field.into_field_name())),
        )
    }
}

impl Header {
    pub fn new(table_name: String, fields: Vec<Column>) -> Self {
        Self { table_name, fields }
    }

    pub fn from_product_type(table_name: String, fields: ProductType) -> Self {
        let cols = fields
            .elements
            .into_iter()
            .enumerate()
            .map(|(pos, f)| {
                let table = table_name.clone();
                let name = match f.name {
                    None => FieldName::Pos { table, field: pos },
                    Some(field) => FieldName::Name { table, field },
                };
                Column::new(name, f.algebraic_type)
            })
            .collect();

        Self::new(table_name, cols)
    }

    pub fn for_mem_table(fields: ProductType) -> Self {
        let table_name = format!("mem#{:x}", calculate_hash(&fields));
        Self::from_product_type(table_name, fields)
    }

    pub fn as_without_table_name(&self) -> HeaderOnlyField {
        HeaderOnlyField {
            fields: self.fields.iter().map(|x| x.as_without_table()).collect(),
        }
    }

    pub fn ty(&self) -> ProductType {
        ProductType::from_iter(
            self.fields
                .iter()
                .map(|x| (x.field.field_name(), x.algebraic_type.clone())),
        )
    }

    pub fn find_by_name(&self, field_name: &str) -> Option<&Column> {
        self.fields.iter().find(|x| x.field.field_name() == Some(field_name))
    }

    pub fn column_pos<'a>(&'a self, col: &'a FieldName) -> Option<usize> {
        match col {
            FieldName::Name { .. } => self.fields.iter().position(|f| &f.field == col),
            FieldName::Pos { field, .. } => self
                .fields
                .iter()
                .enumerate()
                .position(|(pos, f)| &f.field == col || *field == pos),
        }
    }

    /// Finds the position of a field with `name`.
    pub fn find_pos_by_name(&self, name: &str) -> Option<usize> {
        let field = FieldName::named(&self.table_name, name);
        self.column_pos(&field)
    }

    pub fn column<'a>(&'a self, col: &'a FieldName) -> Option<&Column> {
        self.fields.iter().find(|f| &f.field == col)
    }

    pub fn project<T>(&self, cols: &[T]) -> Result<Self, RelationError>
    where
        T: Into<FieldExpr> + Clone,
    {
        let mut p = Vec::with_capacity(cols.len());

        for (pos, col) in cols.iter().enumerate() {
            match col.clone().into() {
                FieldExpr::Name(col) => {
                    if let Some(pos) = self.column_pos(&col) {
                        p.push(self.fields[pos].clone());
                    } else {
                        return Err(RelationError::FieldNotFound(self.clone(), col));
                    }
                }
                FieldExpr::Value(col) => {
                    p.push(Column::new(
                        FieldName::Pos {
                            table: self.table_name.clone(),
                            field: pos,
                        },
                        col.type_of(),
                    ));
                }
            }
        }

        Ok(Self::new(self.table_name.clone(), p))
    }

    /// Adds the fields from `right` to this [`Header`],
    /// renaming duplicated fields with a counter like `a, a => a, a0`.
    pub fn extend(&self, right: &Self) -> Self {
        let count = self.fields.len() + right.fields.len();
        let mut fields = self.fields.clone();
        fields.reserve(count - fields.len());

        let mut cont = 0;
        //Avoid duplicated field names...
        for mut f in right.fields.iter().cloned() {
            if f.field.table() == self.table_name && self.column_pos(&f.field).is_some() {
                let name = format!("{}_{}", f.field.field(), cont);
                f.field = FieldName::Name {
                    table: f.field.table().into(),
                    field: name,
                };

                cont += 1;
            }
            fields.push(f);
        }

        Self::new(self.table_name.clone(), fields)
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[",)?;
        for (pos, col) in self.fields.iter().enumerate() {
            write!(
                f,
                "{}: {}",
                col.field,
                algebraic_type::fmt::fmt_algebraic_type(&col.algebraic_type)
            )?;

            if pos + 1 < self.fields.len() {
                write!(f, ", ")?;
            }
        }
        write!(f, "]",)
    }
}

impl From<ProductType> for Header {
    fn from(value: ProductType) -> Self {
        Header::for_mem_table(value)
    }
}

impl From<AlgebraicType> for Header {
    fn from(value: AlgebraicType) -> Self {
        Header::for_mem_table(value.into())
    }
}

/// An estimate for the range of rows in the [Relation]
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct RowCount {
    pub min: usize,
    pub max: Option<usize>,
}

impl RowCount {
    pub fn exact(rows: usize) -> Self {
        Self {
            min: rows,
            max: Some(rows),
        }
    }

    pub fn unknown() -> Self {
        Self { min: 0, max: None }
    }

    pub fn add_exact(&mut self, count: usize) {
        self.min += count;
        self.max = Some(self.min);
    }
}

/// A [Relation] is anything that could be represented as a [Header] of `[ColumnName:ColumnType]` that
/// generates rows/tuples of [AlgebraicValue] that exactly match that [Header].
pub trait Relation {
    fn head(&self) -> &Header;
    /// Specify the size in rows of the [Relation].
    ///
    /// Warning: It should at least be precise in the lower-bound estimate.
    fn row_count(&self) -> RowCount;
}

/// Common wrapper for relational iterators that work like cursors.
#[derive(Debug)]
pub struct RelIter<T> {
    pub head: Header,
    pub row_count: RowCount,
    pub pos: usize,
    pub of: T,
}

impl<T> RelIter<T> {
    pub fn new(head: Header, row_count: RowCount, of: T) -> Self {
        Self {
            head,
            row_count,
            pos: 0,
            of,
        }
    }
}

/// A borrowed version of [RelValue].
#[derive(Debug, Clone, Copy)]
pub struct RelValueRef<'a> {
    pub data: &'a ProductValue,
}

impl<'a> RelValueRef<'a> {
    pub fn new(data: &'a ProductValue) -> Self {
        Self { data }
    }

    pub fn get(&self, col: &'a FieldExpr, header: &'a Header) -> Result<&'a AlgebraicValue, RelationError> {
        let val = match col {
            FieldExpr::Name(col) => {
                let pos = header
                    .column_pos(col)
                    .ok_or_else(|| RelationError::FieldNotFound(header.clone(), col.clone()))?;
                self.data
                    .elements
                    .get(pos)
                    .ok_or_else(|| RelationError::FieldNotFoundAtPos(pos, col.clone()))?
            }
            FieldExpr::Value(x) => x,
        };

        Ok(val)
    }

    pub fn project(&self, cols: &[FieldExpr], header: &'a Header) -> Result<ProductValue, RelationError> {
        let mut elements = Vec::with_capacity(cols.len());

        for col in cols {
            match col {
                FieldExpr::Name(col) => {
                    if let Some(pos) = header.column_pos(col) {
                        elements.push(self.data.elements[pos].clone());
                    } else {
                        return Err(RelationError::FieldNotFound(header.clone(), col.clone()));
                    }
                }
                FieldExpr::Value(col) => {
                    elements.push(col.clone());
                }
            }
        }

        Ok(ProductValue::new(&elements))
    }
}

/// RelValue represents a materialized row during query execution.
/// In particular it is the type generated/consumed by a [Relation] operator.
/// This is in contrast to a `DataRef` which represents a row belonging to a table.
/// The difference being that a RelValue's [DataKey] is optional since relational
/// operators can modify their input rows.
#[derive(Debug, Clone, Eq)]
pub struct RelValue {
    pub id: Option<DataKey>,
    pub data: ProductValue,
}

impl RelValue {
    pub fn new(data: ProductValue, id: Option<DataKey>) -> Self {
        Self { id, data }
    }

    pub fn as_val_ref(&self) -> RelValueRef {
        RelValueRef::new(&self.data)
    }

    pub fn extend(self, with: RelValue) -> RelValue {
        let mut x = self;
        x.id = None;
        x.data.elements.extend(with.data.elements);
        x
    }
}

impl PartialEq for RelValue {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl Ord for RelValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.data.cmp(&other.data)
    }
}

impl PartialOrd for RelValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An in-memory table
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemTableWithoutTableName<'a> {
    pub head: HeaderOnlyField<'a>,
    pub data: &'a [RelValue],
}

/// An in-memory table
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemTable {
    pub head: Header,
    pub data: Vec<RelValue>,
    pub table_access: StAccess,
}

impl MemTable {
    pub fn new(head: Header, table_access: StAccess, data: Vec<RelValue>) -> Self {
        assert_eq!(
            head.fields.len(),
            data.first()
                .map(|x| x.data.elements.len())
                .unwrap_or_else(|| head.fields.len()),
            "number of columns in `header.len() != data.len()`"
        );
        Self {
            head,
            data,
            table_access,
        }
    }

    pub fn from_value(of: AlgebraicValue) -> Self {
        let head = Header::for_mem_table(of.type_of().into());
        let row = RelValue::new(of.into(), None);
        Self::new(head, StAccess::Public, [row].into())
    }

    pub fn from_iter(head: Header, data: impl Iterator<Item = ProductValue>) -> Self {
        Self {
            head,
            data: data.map(|row| RelValue::new(row, None)).collect(),
            table_access: StAccess::Public,
        }
    }

    pub fn as_without_table_name(&self) -> MemTableWithoutTableName {
        MemTableWithoutTableName {
            head: self.head.as_without_table_name(),
            data: &self.data,
        }
    }

    pub fn get_field(&self, pos: usize) -> Option<&FieldName> {
        self.head.fields.get(pos).map(|x| &x.field)
    }

    pub fn get_field_named(&self, name: &str) -> Option<&FieldName> {
        self.head.find_by_name(name).map(|x| &x.field)
    }
}

impl Relation for MemTable {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::exact(self.data.len())
    }
}

/// A stored table from [RelationalDB]
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct DbTable {
    pub head: Header,
    pub table_id: TableId,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl DbTable {
    pub fn new(head: Header, table_id: TableId, table_type: StTableType, table_access: StAccess) -> Self {
        Self {
            head,
            table_id,
            table_type,
            table_access,
        }
    }
}

impl Relation for DbTable {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, From)]
pub enum Table {
    MemTable(MemTable),
    DbTable(DbTable),
}

impl Table {
    pub fn table_name(&self) -> &str {
        match self {
            Self::MemTable(x) => &x.head.table_name,
            Self::DbTable(x) => &x.head.table_name,
        }
    }

    pub fn table_type(&self) -> StTableType {
        match self {
            Self::MemTable(_) => StTableType::User,
            Self::DbTable(x) => x.table_type,
        }
    }

    pub fn table_access(&self) -> StAccess {
        match self {
            Self::MemTable(x) => x.table_access,
            Self::DbTable(x) => x.table_access,
        }
    }
}

impl Relation for Table {
    fn head(&self) -> &Header {
        match self {
            Table::MemTable(x) => x.head(),
            Table::DbTable(x) => x.head(),
        }
    }

    fn row_count(&self) -> RowCount {
        match self {
            Table::MemTable(x) => x.row_count(),
            Table::DbTable(x) => x.row_count(),
        }
    }
}
