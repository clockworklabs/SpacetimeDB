use std::fmt;

use crate::{product_type, TypeInSpace, Typespace};

use crate::algebraic_value::AlgebraicValue;
use crate::product_type::ProductType;
use crate::product_type_element::ProductTypeElement;
use crate::product_value::ProductValue;
use crate::satn::Satn;

#[derive(thiserror::Error, Debug)]
pub enum RelationError {
    #[error("Field `{1}` not found. Must be one of {0}.")]
    FieldNotFound(Header, FieldName),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum Field<'a> {
    Name(&'a ProductValue),
    Value(&'a AlgebraicValue),
}

impl<'a> Field<'a> {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Field::Name(x) => x.field_as_bool(0, None).ok(),
            Field::Value(x) => x.as_bool().copied(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum FieldName {
    Name(String),
    Pos(usize),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum FieldExpr {
    Name(FieldName),
    Value(AlgebraicValue),
}

impl From<usize> for FieldExpr {
    fn from(x: usize) -> Self {
        Self::Name(FieldName::Pos(x))
    }
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

impl From<&str> for FieldExpr {
    fn from(x: &str) -> Self {
        FieldName::Name(x.into()).into()
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldName::Name(name) => {
                write!(f, ".{name}")
            }
            FieldName::Pos(pos) => {
                write!(f, ".{pos}")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub head: ProductType,
}

impl Header {
    pub fn new(head: ProductType) -> Self {
        Self { head }
    }

    pub fn fields(&self) -> impl Iterator<Item = (FieldName, &ProductTypeElement)> {
        self.head.elements.iter().enumerate().map(|(pos, f)| {
            if let Some(name) = &f.name {
                (FieldName::Name(name.clone()), f)
            } else {
                (FieldName::Pos(pos), f)
            }
        })
    }

    pub fn field_by_pos(&self, pos: usize) -> Option<&ProductTypeElement> {
        self.head.elements.get(pos)
    }

    pub fn find_pos(&self, name: &str) -> Option<usize> {
        self.head.elements.iter().position(|f| Some(name) == f.name.as_deref())
    }

    pub fn field_by_name(&self, name: &str) -> Option<&ProductTypeElement> {
        if let Some(pos) = self.find_pos(name) {
            self.field_by_pos(pos)
        } else {
            None
        }
    }

    pub fn column_pos<'a>(&'a self, col: &'a FieldName) -> Option<usize> {
        match col {
            FieldName::Name(name) => self.find_pos(name),
            FieldName::Pos(x) => {
                if *x < self.head.elements.len() {
                    Some(*x)
                } else {
                    None
                }
            }
        }
    }

    pub fn column<'a>(&'a self, col: &'a FieldName) -> Option<&'a ProductTypeElement> {
        match col {
            FieldName::Name(name) => self.field_by_name(name),
            FieldName::Pos(pos) => self.field_by_pos(*pos),
        }
    }

    pub fn project(&self, cols: &[FieldName]) -> Result<Self, FieldName> {
        let mut p = Vec::with_capacity(cols.len());

        for col in cols {
            if let Some(pos) = self.column_pos(col) {
                p.push(self.head.elements[pos].clone());
            } else {
                return Err(col.clone());
            }
        }

        Ok(Self::new(ProductType::new(p)))
    }

    pub fn extend(&self, right: &Self) -> Self {
        let count = self.head.elements.len() + right.head.elements.len();
        let mut fields = Vec::with_capacity(count);
        let mut left = self.head.elements.clone();
        let mut _right = right.head.elements.clone();

        fields.append(&mut left);

        let mut cont = 0;
        //Avoid duplicated field names...
        for mut f in _right.into_iter() {
            if let Some(name) = &f.name {
                if self.find_pos(name).is_some() {
                    let name = format!("{}_{}", name, cont);
                    f.name = Some(name);
                    fields.push(f);
                    cont += 1;
                } else {
                    fields.push(f);
                }
            } else {
                fields.push(f);
            };
        }

        Self::new(ProductType::new(fields))
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", product_type::satn::Formatter::new(&self.head))
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

#[derive(Debug, Clone)]
pub struct RelValueRef<'a> {
    pub head: &'a Header,
    pub data: &'a ProductValue,
}

impl<'a> RelValueRef<'a> {
    pub fn new(head: &'a Header, data: &'a ProductValue) -> Self {
        Self { head, data }
    }

    pub fn get(&self, col: &'a FieldExpr) -> Field<'a> {
        match col {
            FieldExpr::Name(col) => {
                if let Some(pos) = self.head.column_pos(col) {
                    Field::Value(&self.data.elements[pos])
                } else {
                    panic!("Field {col} not found")
                }
            }
            FieldExpr::Value(x) => Field::Value(x),
        }
    }

    pub fn project(&self, cols: &[FieldName]) -> Result<ProductValue, RelationError> {
        let mut elements = Vec::with_capacity(cols.len());

        for col in cols {
            if let Some(pos) = self.head.column_pos(col) {
                elements.push(self.data.elements[pos].clone());
            } else {
                return Err(RelationError::FieldNotFound(self.head.clone(), col.clone()));
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
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemTable {
    pub head: Header,
    pub data: Vec<ProductValue>,
}

impl MemTable {
    pub fn new(head: &Header, data: &[ProductValue]) -> Self {
        Self {
            head: head.clone(),
            data: data.into(),
        }
    }

    pub fn from_iter(head: &Header, data: impl Iterator<Item = ProductValue>) -> Self {
        Self {
            head: head.clone(),
            data: data.collect(),
        }
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
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DbTable {
    pub head: Header,
    pub table_id: u32,
}

impl DbTable {
    pub fn new(head: &Header, table_id: u32) -> Self {
        Self {
            head: head.clone(),
            table_id,
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
