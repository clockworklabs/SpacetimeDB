use derive_more::From;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::error::RelationError;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{DbTable, FieldExpr, FieldName, Header, HeaderOnlyField, Relation, RowCount};
use spacetimedb_sats::{impl_serialize, AlgebraicValue};
use spacetimedb_table::read_column::ReadColumn;
use spacetimedb_table::table::RowRef;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::mem;
use std::sync::Arc;

/// RelValue represents either a reference to a row in a table,
/// or an ephemeral row constructed during query execution.
///
/// A `RelValue` is the type generated/consumed by a [Relation] operator.
#[derive(Debug, Clone)]
pub enum RelValue<'a> {
    Row(RowRef<'a>),
    Projection(ProductValue),
}

impl_serialize!(['a] RelValue<'a>, (self, ser) => match self {
    Self::Row(row) => row.serialize(ser),
    Self::Projection(row) => row.serialize(ser),
});

impl<'a> RelValue<'a> {
    /// Converts `self` into a `ProductValue`
    /// either by reading a value from a table or consuming the owned product.
    pub fn into_product_value(self) -> ProductValue {
        match self {
            Self::Row(row_ref) => row_ref.to_product_value(),
            Self::Projection(row) => row,
        }
    }

    /// Computes the number of columns in this value.
    pub fn num_columns(&self) -> usize {
        match self {
            Self::Row(row_ref) => row_ref.row_layout().product().elements.len(),
            Self::Projection(row) => row.elements.len(),
        }
    }

    /// Extends `self` with the columns in `other`.
    ///
    /// This will always cause `RowRef<'_>`s to be read out into
    pub fn extend(self, other: RelValue<'a>) -> RelValue<'a> {
        let mut x = self.into_product_value();
        x.elements.extend(other.into_product_value().elements);
        RelValue::Projection(x)
    }

    /// Read the column at index `col`.
    ///
    /// Use `read_or_take_column` instead if you have ownership of `self`.
    pub fn read_column(&self, col: usize) -> Option<Cow<'_, AlgebraicValue>> {
        match self {
            Self::Row(row_ref) => AlgebraicValue::read_column(*row_ref, col).ok().map(Cow::Owned),
            Self::Projection(pv) => pv.elements.get(col).map(Cow::Borrowed),
        }
    }

    pub fn get<'b>(&'a self, col: &'a FieldExpr, header: &'b Header) -> Result<Cow<'a, AlgebraicValue>, RelationError> {
        let val = match col {
            FieldExpr::Name(col) => {
                let pos = header.column_pos_or_err(col)?.idx();
                self.read_column(pos)
                    .ok_or_else(|| RelationError::FieldNotFoundAtPos(pos, col.clone()))?
            }
            FieldExpr::Value(x) => Cow::Borrowed(x),
        };

        Ok(val)
    }

    pub fn project(&self, cols: &[FieldExpr], header: &'a Header) -> Result<ProductValue, RelationError> {
        let mut elements = Vec::with_capacity(cols.len());
        for col in cols {
            elements.push(self.get(col, header)?.into_owned());
        }
        Ok(elements.into())
    }

    /// Reads or takes the column at `col`.
    /// Calling this method consumes the column at `col`
    /// so it should not be called again for the same input.
    fn read_or_take_column(&mut self, col: usize) -> Option<AlgebraicValue> {
        match self {
            Self::Row(row_ref) => AlgebraicValue::read_column(*row_ref, col).ok(),
            Self::Projection(pv) => {
                let elem = pv.elements.get_mut(col)?;
                Some(mem::replace(elem, AlgebraicValue::U8(0)))
            }
        }
    }

    pub fn project_owned(mut self, cols: &[FieldExpr], header: &Header) -> Result<ProductValue, RelationError> {
        let mut elements = Vec::with_capacity(cols.len());
        for col in cols {
            let val = match col {
                FieldExpr::Name(col) => {
                    let pos = header.column_pos_or_err(col)?.idx();
                    self.read_or_take_column(pos)
                        .ok_or_else(|| RelationError::FieldNotFoundAtPos(pos, col.clone()))?
                }
                FieldExpr::Value(x) => x.clone(),
            };
            elements.push(val);
        }
        Ok(elements.into())
    }
}

/// An in-memory table
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct MemTableWithoutTableName<'a> {
    pub head: HeaderOnlyField<'a>,
    pub data: &'a [ProductValue],
}

/// An in-memory table
// TODO(perf): Remove `Clone` impl.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemTable {
    pub head: Arc<Header>,
    pub data: Vec<ProductValue>,
    pub table_access: StAccess,
}

impl MemTable {
    pub fn new(head: Arc<Header>, table_access: StAccess, data: Vec<ProductValue>) -> Self {
        assert_eq!(
            head.fields.len(),
            data.first()
                .map(|pv| pv.elements.len())
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
        Self::new(Arc::new(head), StAccess::Public, [of.into()].into())
    }

    pub fn from_iter(head: Arc<Header>, data: impl Iterator<Item = ProductValue>) -> Self {
        Self {
            head,
            data: data.collect(),
            table_access: StAccess::Public,
        }
    }

    pub fn as_without_table_name(&self) -> MemTableWithoutTableName {
        MemTableWithoutTableName {
            head: self.head.as_without_table_name(),
            data: &self.data,
        }
    }

    pub fn get_field_pos(&self, pos: usize) -> Option<&FieldName> {
        self.head.fields.get(pos).map(|x| &x.field)
    }

    pub fn get_field_named(&self, name: &str) -> Option<&FieldName> {
        self.head.find_by_name(name).map(|x| &x.field)
    }
}

impl Relation for MemTable {
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::exact(self.data.len())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, From)]
pub enum Table {
    MemTable(MemTable),
    DbTable(DbTable),
}

impl Hash for Table {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // IMPORTANT: Required for hashing query plans.
        // In general a query plan will only contain static data.
        // However, currently it is possible to inline a virtual table.
        // Such plans though are hybrids and should not be hashed,
        // Since they contain raw data values.
        // Therefore we explicitly disallow it here.
        match self {
            Table::DbTable(t) => {
                t.hash(state);
            }
            Table::MemTable(_) => {
                panic!("Cannot hash a virtual table");
            }
        }
    }
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

    pub fn get_db_table(&self) -> Option<&DbTable> {
        match self {
            Self::DbTable(t) => Some(t),
            _ => None,
        }
    }
}

impl Relation for Table {
    fn head(&self) -> &Arc<Header> {
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
