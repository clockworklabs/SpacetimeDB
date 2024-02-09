use derive_more::From;
use spacetimedb_sats::data_key::DataKey;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::error::RelationError;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{DbTable, FieldExpr, FieldName, Header, HeaderOnlyField, Relation, RowCount};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_table::table::RowRef;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

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
    pub data: &'a RelValue<'a>,
}

impl<'a> RelValueRef<'a> {
    pub fn new(data: &'a RelValue<'a>) -> Self {
        Self { data }
    }

    pub fn get(&self, col: &'a FieldExpr, header: &'a Header) -> Result<&'a AlgebraicValue, RelationError> {
        // let val = match col {
        //     FieldExpr::Name(col) => {
        //         let pos = header.column_pos_or_err(col)?.idx();
        //         self.data
        //             .elements
        //             .get(pos)
        //             .ok_or_else(|| RelationError::FieldNotFoundAtPos(pos, col.clone()))?
        //     }
        //     FieldExpr::Value(x) => x,
        // };

        todo!("read_column")

        // Ok(val)
    }

    pub fn project(&self, cols: &[FieldExpr], header: &'a Header) -> Result<ProductValue, RelationError> {
        // let mut elements = Vec::with_capacity(cols.len());

        // for col in cols {
        //     match col {
        //         FieldExpr::Name(col) => {
        //             let pos = header.column_pos_or_err(col)?.idx();
        //             elements.push(self.data.elements[pos].clone());
        //         }
        //         FieldExpr::Value(col) => {
        //             elements.push(col.clone());
        //         }
        //     }
        // }

        // Ok(ProductValue::new(&elements))
        todo!("project_not_empty)")
    }
}

/// RelValue represents a materialized row during query execution.
/// In particular it is the type generated/consumed by a [Relation] operator.
/// This is in contrast to a `DataRef` which represents a row belonging to a table.
/// The difference being that a RelValue's [DataKey] is optional since relational
/// operators can modify their input rows.
#[derive(Debug, Clone)]
pub enum RelValue<'a> {
    Row(RowRef<'a>),
    Projection(ProductValue),
}

impl<'a> RelValue<'a> {
    pub fn new(data: ProductValue, _id: Option<DataKey>) -> Self {
        RelValue::Projection(data)
    }

    pub fn as_val_ref(&'a self) -> RelValueRef<'a> {
        RelValueRef::new(self)
    }

    pub fn into_product_value(self) -> ProductValue {
        match self {
            Self::Row(row_ref) => row_ref.to_product_value(),
            Self::Projection(row) => row,
        }
    }

    pub fn clone_product_value(&self) -> ProductValue {
        match self {
            Self::Row(row_ref) => row_ref.to_product_value(),

            Self::Projection(row) => row.clone(),
        }
    }

    pub fn num_columns(&self) -> usize {
        match self {
            Self::Row(row_ref) => row_ref.row_layout().product().elements.len(),
            Self::Projection(row) => row.elements.len(),
        }
    }

    pub fn extend(self, with: RelValue) -> RelValue {
        let mut x = self.into_product_value();
        x.elements.extend(with.into_product_value().elements);
        RelValue::Projection(x)
    }
}

impl<'a> Eq for RelValue<'a> {}

impl<'a> PartialEq for RelValue<'a> {
    fn eq(&self, other: &Self) -> bool {
        todo!("eq_row_in_table")
        // self.data == other.data
    }
}

impl<'a> Ord for RelValue<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        todo!("no clue lmao")
    }
}

impl<'a> PartialOrd for RelValue<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
    pub table_access: StAccess,
}

impl MemTable {
    pub fn new(head: Header, table_access: StAccess, data: Vec<RelValue<'_>>) -> Self {
        assert_eq!(
            head.fields.len(),
            data.first()
                .map(RelValue::num_columns)
                .unwrap_or_else(|| head.fields.len()),
            "number of columns in `header.len() != data.len()`"
        );
        let data = data.into_iter().map(RelValue::into_product_value).collect();
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
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::exact(self.data.len())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, From, PartialOrd, Ord)]
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
