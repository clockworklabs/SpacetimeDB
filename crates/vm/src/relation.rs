use derive_more::From;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::error::RelationError;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{DbTable, FieldExpr, FieldName, Header, HeaderOnlyField, Relation, RowCount};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_table::read_column::ReadColumn;
use spacetimedb_table::table::RowRef;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// RelValue represents either a reference to a row in a table,
/// or an ephemeral row constructed during query execution.
///
/// A `RelValue` is the type generated/consumed by a [Relation] operator.
#[derive(Debug, Clone)]
pub enum RelValue<'a> {
    Row(RowRef<'a>),
    Projection(ProductValue),
}

impl<'a> RelValue<'a> {
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

    pub fn read_column(&self, col: usize) -> Option<Cow<'_, AlgebraicValue>> {
        match self {
            Self::Row(row_ref) => AlgebraicValue::read_column(*row_ref, col).ok().map(Cow::Owned),
            Self::Projection(pv) => pv.elements.get(col).map(Cow::Borrowed),
        }
    }

    pub fn project_not_empty(&self, cols: &ColList) -> Option<ProductValue> {
        let av = match self {
            Self::Row(row_ref) => row_ref.project_not_empty(cols).ok()?,
            Self::Projection(pv) => pv.project_not_empty(cols).ok()?,
        };
        if av.is_product() {
            Some(av.into_product().unwrap())
        } else {
            Some(ProductValue::from_iter([av]))
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
}

impl<'a> Eq for RelValue<'a> {}

impl<'a> PartialEq for RelValue<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RelValue::Row(left), RelValue::Row(right)) => {
                let layout = left.row_layout();

                // Check that the rows have the same type,
                // so that we can use the unsafe `eq_row_in_page`.
                // TODO(perf): Determine if this check is expensive, and if so,
                // whether it can be optimized or removed.
                // If not, consider removing this branch and always following the other branch,
                // i.e. sequential `read_column` of each column in both rows.
                if right.row_layout() != layout {
                    return false;
                }

                let (left_page, left_offset) = left.page_and_offset();
                let (right_page, right_offset) = right.page_and_offset();
                unsafe {
                    // SAFETY:
                    // - Existence of a `RowRef` is sufficient proof that the row is valid,
                    //   so we can trust that the pages and offsets refer to valid rows.
                    // - We checked above that the row layouts are the same,
                    //   so `layout` applies to both of them.
                    spacetimedb_table::eq::eq_row_in_page(left_page, right_page, left_offset, right_offset, layout);
                }
                todo!("eq_row_in_table")
            }
            (left, right) => {
                let num_columns = left.num_columns();

                // Check that the rows have the same number of columns.
                // If not, there's no need to ever get an `AlgebraicValue` for comparison.
                if right.num_columns() != num_columns {
                    return false;
                }

                for col_idx in 0..num_columns {
                    // These unwraps will never fail because we've asserted above
                    // that both rows have exactly `num_columns` rows.
                    let left_col = left.read_column(col_idx).unwrap();
                    let right_col = right.read_column(col_idx).unwrap();
                    if left_col != right_col {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl<'a> Ord for RelValue<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        let left_num_cols = self.num_columns();
        let right_num_cols = self.num_columns();
        let shared_num_cols = usize::min(left_num_cols, right_num_cols);

        // First, compare all the columns for which both rows have a column at that index.
        // Upon finding a non-equal column, return that ordering.
        for col_idx in 0..shared_num_cols {
            // These unwraps will never fail because we've determined above
            // that both rows have at least `shared_num_columns` rows.
            let left_col = self.read_column(col_idx).unwrap();
            let right_col = other.read_column(col_idx).unwrap();
            match left_col.cmp(&right_col) {
                Ordering::Less => return Ordering::Less,
                Ordering::Greater => return Ordering::Greater,
                Ordering::Equal => (),
            }
        }

        // Finally, if all the shared columns match, i.e. one row is a prefix of the other,
        // the shorter row is ordered less.
        left_num_cols.cmp(&right_num_cols)
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
    pub fn new(head: Header, table_access: StAccess, data: Vec<ProductValue>) -> Self {
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
        Self::new(head, StAccess::Public, [of.into()].into())
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
