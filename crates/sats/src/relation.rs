use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::algebraic_type::TypeError;
use crate::algebraic_value::AlgebraicValue;
use crate::auth::*;
use crate::product_value::ProductValue;
use crate::satn::Satn;
use crate::{algebraic_type, AlgebraicType, ProductType, ProductTypeElement, TypeInSpace, Typespace};

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Table `{named}` is private")]
    TablePrivate { named: String },
    #[error("Index `{named}` is private")]
    IndexPrivate { named: String },
    #[error("Sequence `{named}` is private")]
    SequencePrivate { named: String },
}

#[derive(thiserror::Error, Debug)]
pub enum RelationError {
    #[error("Field `{1}` not found. Must be one of {0}")]
    FieldNotFound(Header, FieldName),
    #[error("Field `{0}` fail to infer the type: {1}")]
    TypeInference(FieldName, TypeError),
    #[error("Field declaration only support `table.field` or `field`. It gets instead `{0}`")]
    FieldPathInvalid(String),
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
        match self {
            FieldName::Name { table, .. } => table,
            FieldName::Pos { table, .. } => table,
        }
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
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum FieldExpr {
    Name(FieldName),
    Value(AlgebraicValue),
}

impl From<AlgebraicValue> for FieldExpr {
    fn from(x: AlgebraicValue) -> Self {
        Self::Value(x)
    }
}

impl From<FieldName> for FieldExpr {
    fn from(x: FieldName) -> Self {
        Self::Name(x)
    }
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
                write!(f, "{}", TypeInSpace::new(&ts, &ty).with_value(x).to_satn())
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
        ProductType::from_iter(value.fields.iter().map(|x| match &x.field {
            FieldName::Name { field, .. } => ProductTypeElement::new_named(x.algebraic_type.clone(), field),
            FieldName::Pos { .. } => ProductTypeElement::new(x.algebraic_type.clone(), None),
        }))
    }
}

impl Header {
    pub fn new(table_name: &str, fields: &[Column]) -> Self {
        Self {
            table_name: table_name.into(),
            fields: fields.into(),
        }
    }

    pub fn from_product_type(table_name: &str, fields: ProductType) -> Self {
        let mut cols = Vec::with_capacity(fields.elements.len());

        for (pos, f) in fields.elements.into_iter().enumerate() {
            let name = match f.name {
                None => FieldName::Pos {
                    table: table_name.into(),
                    field: pos,
                },
                Some(x) => FieldName::Name {
                    table: table_name.into(),
                    field: x,
                },
            };
            cols.push(Column::new(name, f.algebraic_type));
        }

        Self::new(table_name, &cols)
    }

    pub fn for_mem_table(fields: ProductType) -> Self {
        let table_name = format!("mem#{:x}", calculate_hash(&fields));
        Self::from_product_type(&table_name, fields)
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

        Ok(Self::new(&self.table_name, &p))
    }

    pub fn extend(&self, right: &Self) -> Self {
        let count = self.fields.len() + right.fields.len();
        let mut fields = Vec::with_capacity(count);
        let mut left = self.fields.clone();
        let mut _right = right.fields.clone();

        fields.append(&mut left);

        let mut cont = 0;
        //Avoid duplicated field names...
        for mut f in _right.into_iter() {
            if f.field.table() == self.table_name && self.column_pos(&f.field).is_some() {
                let name = format!("{}_{}", f.field.field(), cont);
                f.field = FieldName::Name {
                    table: f.field.table().into(),
                    field: name,
                };
                fields.push(f);
                cont += 1;
            } else {
                fields.push(f);
            }
        }

        Self::new(&format!("{} | {}", self.table_name, right.table_name), &fields)
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
                algebraic_type::satn::Formatter::new(&col.algebraic_type)
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
    fn head(&self) -> Header;
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

#[derive(Debug, Clone, Copy)]
pub struct RelValueRef<'a> {
    pub head: &'a Header,
    pub data: &'a ProductValue,
}

impl<'a> RelValueRef<'a> {
    pub fn new(head: &'a Header, data: &'a ProductValue) -> Self {
        Self { head, data }
    }

    pub fn get(&self, col: &'a FieldExpr) -> &'a AlgebraicValue {
        match col {
            FieldExpr::Name(col) => {
                if let Some(pos) = self.head.column_pos(col) {
                    if let Some(v) = self.data.elements.get(pos) {
                        v
                    } else {
                        unreachable!("Field {col} at pos {pos} not found on row {:?}", self.data.elements)
                    }
                } else {
                    unreachable!(
                        "Field {col} not found on {}. Fields:{}",
                        self.head.table_name, self.head
                    )
                }
            }
            FieldExpr::Value(x) => x,
        }
    }

    pub fn project(&self, cols: &[FieldExpr]) -> Result<ProductValue, RelationError> {
        let mut elements = Vec::with_capacity(cols.len());

        for col in cols {
            match col {
                FieldExpr::Name(col) => {
                    if let Some(pos) = self.head.column_pos(col) {
                        elements.push(self.data.elements[pos].clone());
                    } else {
                        return Err(RelationError::FieldNotFound(self.head.clone(), col.clone()));
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

impl Relation for RelValueRef<'_> {
    fn head(&self) -> Header {
        self.head.clone()
    }

    fn row_count(&self) -> RowCount {
        RowCount::exact(1)
    }
}

/// The row/tuple generated by a [Relation] operator
#[derive(Debug, Clone)]
pub struct RelValue {
    pub head: Header,
    pub data: ProductValue,
}

impl RelValue {
    pub fn new(head: &Header, data: &ProductValue) -> Self {
        Self {
            head: head.clone(),
            data: data.clone(),
        }
    }

    pub fn as_val_ref(&self) -> RelValueRef {
        RelValueRef::new(&self.head, &self.data)
    }

    pub fn extend(self, head: &Header, with: RelValue) -> RelValue {
        let mut x = self;
        x.head = head.clone();
        x.data.elements.extend(with.data.elements.into_iter());
        x
    }
}

/// An in-memory table
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemTableWithoutTableName<'a> {
    pub head: HeaderOnlyField<'a>,
    pub data: &'a [ProductValue],
}

/// An in-memory table
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemTable {
    pub head: Header,
    pub data: Vec<ProductValue>,
}

impl MemTable {
    pub fn new(head: &Header, data: &[ProductValue]) -> Self {
        assert_eq!(
            head.fields.len(),
            data.first()
                .map(|x| x.elements.len())
                .unwrap_or_else(|| head.fields.len()),
            "Not match the number of columns between the header.len() <> data.len()"
        );
        Self {
            head: head.clone(),
            data: data.into(),
        }
    }

    pub fn from_value(of: AlgebraicValue) -> Self {
        let head = Header::for_mem_table(of.type_of().into());
        Self::new(&head, &[of.into()])
    }

    pub fn from_iter(head: &Header, data: impl Iterator<Item = ProductValue>) -> Self {
        Self {
            head: head.clone(),
            data: data.collect(),
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
    fn head(&self) -> Header {
        self.head.clone()
    }

    fn row_count(&self) -> RowCount {
        RowCount::exact(self.data.len())
    }
}

/// A stored table from [RelationalDB]
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct DbTable {
    pub head: Header,
    pub table_id: u32,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl DbTable {
    pub fn new(head: &Header, table_id: u32, table_type: StTableType, table_access: StAccess) -> Self {
        Self {
            head: head.clone(),
            table_id,
            table_type,
            table_access,
        }
    }
}

impl Relation for DbTable {
    fn head(&self) -> Header {
        self.head.clone()
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
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
            Self::MemTable(_) => StAccess::Public,
            Self::DbTable(x) => x.table_access,
        }
    }
}

impl Relation for Table {
    fn head(&self) -> Header {
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
