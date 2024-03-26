use crate::algebraic_value::AlgebraicValue;
use crate::db::auth::{StAccess, StTableType};
use crate::db::error::RelationError;
use crate::satn::Satn;
use crate::{algebraic_type, AlgebraicType, ProductType, Typespace, WithTypespace};
use derive_more::From;
use spacetimedb_primitives::{ColId, ColList, ColListBuilder, Constraints, TableId};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
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

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct FieldName {
    table: String,
    field: ColId,
}

impl FieldName {
    pub fn positional(table: &str, field: ColId) -> Self {
        Self {
            table: table.to_string(),
            field,
        }
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn field(&self) -> ColId {
        self.field
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { table, field } = self;
        write!(f, "{table}.{field}")
    }
}

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, From)]
pub enum FieldExpr {
    Name(FieldName),
    Value(AlgebraicValue),
}

impl FieldExpr {
    /// Returns a borrowed version of `FieldExpr`.
    pub fn borrowed(&self) -> FieldExprRef<'_> {
        match self {
            Self::Name(x) => FieldExprRef::Name(x),
            Self::Value(x) => FieldExprRef::Value(x),
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

/// A borrowed version of `FieldExpr`.
#[derive(Clone, Copy)]
pub enum FieldExprRef<'a> {
    Name(&'a FieldName),
    Value(&'a AlgebraicValue),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ColumnOnlyField<'a> {
    pub field: ColId,
    pub algebraic_type: &'a AlgebraicType,
}

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Column {
    pub field: FieldName,
    pub algebraic_type: AlgebraicType,
    pub col_id: ColId,
}

impl Column {
    pub fn new(field: FieldName, algebraic_type: AlgebraicType, col_id: ColId) -> Self {
        Self {
            field,
            algebraic_type,
            col_id,
        }
    }

    pub fn as_without_table(&self) -> ColumnOnlyField {
        ColumnOnlyField {
            field: self.field.field(),
            algebraic_type: &self.algebraic_type,
        }
    }
}

// TODO(perf): Remove `Clone` impl.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeaderOnlyField<'a> {
    pub fields: Vec<ColumnOnlyField<'a>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Header {
    pub table_name: String,
    pub fields: Vec<Column>,
    pub constraints: Vec<(ColList, Constraints)>,
}

impl Header {
    pub fn new(table_name: String, fields: Vec<Column>, constraints: Vec<(ColList, Constraints)>) -> Self {
        Self {
            table_name,
            fields,
            constraints,
        }
    }

    /// Equivalent to what [`Clone::clone`] would do.
    ///
    /// `Header` intentionally does not implement `Clone`,
    /// as we can't afford to clone it in normal execution paths.
    /// However, we don't care about performance in error paths,
    /// and we need to embed owned `Header`s in error objects to report useful messages.
    pub fn clone_for_error(&self) -> Self {
        Header {
            table_name: self.table_name.clone(),
            fields: self.fields.clone(),
            constraints: self.constraints.clone(),
        }
    }

    pub fn from_product_type(table_name: String, fields: ProductType) -> Self {
        let cols = fields
            .elements
            .into_iter()
            .enumerate()
            .map(|(pos, f)| {
                let col = pos.into();
                let name = FieldName::positional(&table_name, col);
                Column::new(name, f.algebraic_type, col)
            })
            .collect();

        Self::new(table_name, cols, Default::default())
    }

    pub fn to_product_type(&self) -> ProductType {
        self.fields.iter().map(|x| x.algebraic_type.clone()).collect()
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

    pub fn column_pos<'a>(&'a self, col: &'a FieldName) -> Option<ColId> {
        self.fields
            .iter()
            .enumerate()
            .position(|(pos, f)| &f.field == col || col.field.idx() == pos)
            .map(Into::into)
    }

    pub fn column_pos_or_err<'a>(&'a self, col: &'a FieldName) -> Result<ColId, RelationError> {
        self.column_pos(col)
            .ok_or_else(|| RelationError::FieldNotFound(self.clone_for_error(), col.clone()))
    }

    pub fn column<'a>(&'a self, col: &'a FieldName) -> Option<&Column> {
        self.column_pos(col).map(|id| &self.fields[id.idx()])
    }

    /// Copy the [Constraints] that are referenced in the list of `for_columns`
    fn retain_constraints(&self, for_columns: &ColList) -> Vec<(ColList, Constraints)> {
        // Copy the constraints of the selected columns and retain the multi-column ones...
        self.constraints
            .iter()
            // Keep constraints with a col list where at least one col is in `for_columns`.
            .filter(|(cols, _)| cols.iter().any(|c| for_columns.contains(c)))
            .cloned()
            .collect()
    }

    pub fn has_constraint(&self, field: &FieldName, constraint: Constraints) -> bool {
        self.column_pos(field)
            .map(|find| {
                self.constraints
                    .iter()
                    .any(|(col, ct)| col.contains(find) && ct.contains(&constraint))
            })
            .unwrap_or(false)
    }

    /// Project the [FieldExpr] & the [Constraints] that referenced them
    pub fn project(&self, cols: &[impl Into<FieldExpr> + Clone]) -> Result<Self, RelationError> {
        let mut p = Vec::with_capacity(cols.len());
        let mut to_keep = ColListBuilder::new();

        for (pos, col) in cols.iter().enumerate() {
            match col.clone().into() {
                FieldExpr::Name(col) => {
                    let pos = self.column_pos_or_err(&col)?;
                    to_keep.push(pos);
                    p.push(self.fields[pos.idx()].clone());
                }
                FieldExpr::Value(col) => {
                    let pos = pos.into();
                    let field = FieldName::positional(&self.table_name, pos);
                    p.push(Column::new(field, col.type_of(), pos));
                }
            }
        }

        let constraints = self.retain_constraints(&to_keep.build().unwrap());

        Ok(Self::new(self.table_name.clone(), p, constraints))
    }

    /// Adds the fields &  [Constraints] from `right` to this [`Header`].
    pub fn extend(&self, right: &Self) -> Self {
        // Increase the positions of the columns in `right.constraints`, adding the count of fields on `left`
        let mut constraints = self.constraints.clone();
        let adjust_by_len_lhs = |col: ColId| ColId(col.0 + self.fields.len() as u32);
        constraints.extend(right.constraints.iter().map(|(cols, c)| {
            let cols = cols
                .iter()
                .map(adjust_by_len_lhs)
                .collect::<ColListBuilder>()
                .build()
                .unwrap();
            (cols, *c)
        }));

        let mut fields = self.fields.clone();
        fields.extend(right.fields.iter().cloned().map(|mut col| {
            col.col_id = adjust_by_len_lhs(col.col_id);
            col
        }));

        Self::new(self.table_name.clone(), fields, constraints)
    }
}

impl From<Header> for ProductType {
    fn from(value: Header) -> Self {
        value.fields.into_iter().map(|x| x.algebraic_type).collect()
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
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
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
}

/// A [Relation] is anything that could be represented as a [Header] of `[ColumnName:ColumnType]` that
/// generates rows/tuples of [AlgebraicValue] that exactly match that [Header].
pub trait Relation {
    fn head(&self) -> &Arc<Header>;
    /// Specify the size in rows of the [Relation].
    ///
    /// Warning: It should at least be precise in the lower-bound estimate.
    fn row_count(&self) -> RowCount;
}

/// A stored table from [RelationalDB]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct DbTable {
    pub head: Arc<Header>,
    pub table_id: TableId,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl DbTable {
    pub fn new(head: Arc<Header>, table_id: TableId, table_type: StTableType, table_access: StAccess) -> Self {
        Self {
            head,
            table_id,
            table_type,
            table_access,
        }
    }
}

impl Relation for DbTable {
    fn head(&self) -> &Arc<Header> {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        RowCount::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_primitives::col_list;

    /// Build a [Header] using the initial `start_pos` as the column position for the [Constraints]
    fn head(table: &str, fields: (&str, &str), start_pos: u32) -> Header {
        let pos_lhs = start_pos;
        let pos_rhs = start_pos + 1;

        let ct = vec![
            (ColId(pos_lhs).into(), Constraints::indexed()),
            (ColId(pos_rhs).into(), Constraints::identity()),
            (col_list![pos_lhs, pos_rhs], Constraints::primary_key()),
            (col_list![pos_rhs, pos_lhs], Constraints::unique()),
        ];

        Header::new(
            table.into(),
            vec![
                Column::new(FieldName::named(table, fields.0), AlgebraicType::I8, 0.into()),
                Column::new(FieldName::named(table, fields.1), AlgebraicType::I8, 0.into()),
            ],
            ct,
        )
    }

    #[test]
    fn test_project() {
        let head = head("t1", ("a", "b"), 0);
        let new = head.project(&[] as &[FieldName]).unwrap();

        let mut empty = head.clone_for_error();
        empty.fields.clear();
        empty.constraints.clear();

        assert_eq!(empty, new);

        let all = head.clone_for_error();
        let new = head
            .project(&[FieldName::named("t1", "a"), FieldName::named("t1", "b")])
            .unwrap();

        assert_eq!(all, new);

        let mut first = head.clone_for_error();
        first.fields.pop();
        first.constraints = first.retain_constraints(&0.into());

        let new = head.project(&[FieldName::named("t1", "a")]).unwrap();

        assert_eq!(first, new);

        let mut second = head.clone_for_error();
        second.fields.remove(0);
        second.constraints = second.retain_constraints(&1.into());

        let new = head.project(&[FieldName::named("t1", "b")]).unwrap();

        assert_eq!(second, new);
    }

    #[test]
    fn test_extend() {
        let head_lhs = head("t1", ("a", "b"), 0);
        let head_rhs = head("t2", ("c", "d"), 0);

        let new = head_lhs.extend(&head_rhs);

        let lhs = new
            .project(&[FieldName::named("t1", "a"), FieldName::named("t1", "b")])
            .unwrap();

        assert_eq!(head_lhs, lhs);

        let mut head_rhs = head("t2", ("c", "d"), 2);
        head_rhs.table_name = head_lhs.table_name.clone();

        let rhs = new
            .project(&[FieldName::named("t2", "c"), FieldName::named("t2", "d")])
            .unwrap();

        assert_eq!(head_rhs, rhs);
    }
}
