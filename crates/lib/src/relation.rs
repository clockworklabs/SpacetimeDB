use crate::db::auth::{StAccess, StTableType};
use crate::db::error::{RelationError, TypeError};
use core::fmt;
use core::hash::Hash;
use derive_more::From;
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_primitives::{ColId, ColList, ColSet, Constraints, TableId};
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{algebraic_type, AlgebraicType};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct FieldName {
    pub table: TableId,
    pub col: ColId,
}

impl FieldName {
    pub fn new(table: TableId, col: ColId) -> Self {
        Self { table, col }
    }

    pub fn table(&self) -> TableId {
        self.table
    }

    pub fn field(&self) -> ColId {
        self.col
    }
}

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, From)]
pub enum ColExpr {
    Col(ColId),
    Value(AlgebraicValue),
}

impl ColExpr {
    /// Returns a borrowed version of `ColExpr`.
    pub fn borrowed(&self) -> ColExprRef<'_> {
        match self {
            Self::Col(x) => ColExprRef::Col(*x),
            Self::Value(x) => ColExprRef::Value(x),
        }
    }
}

impl fmt::Debug for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table#{}.col#{}", self.table, self.col)
    }
}

impl fmt::Display for ColExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColExpr::Col(x) => write!(f, "{x}"),
            ColExpr::Value(x) => write!(f, "{}", x.to_satn()),
        }
    }
}

/// A borrowed version of `FieldExpr`.
#[derive(Clone, Copy)]
pub enum ColExprRef<'a> {
    Col(ColId),
    Value(&'a AlgebraicValue),
}

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Column {
    pub field: FieldName,
    pub algebraic_type: AlgebraicType,
}

impl Column {
    pub fn new(field: FieldName, algebraic_type: AlgebraicType) -> Self {
        Self { field, algebraic_type }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Header {
    pub table_id: TableId,
    pub table_name: Box<str>,
    pub fields: Vec<Column>,
    pub constraints: BTreeMap<ColList, Constraints>,
}

impl Header {
    /// Create a new header.
    ///
    /// `uncombined_constraints` will be normalized using [`combine_constraints`].
    pub fn new(
        table_id: TableId,
        table_name: Box<str>,
        fields: Vec<Column>,
        uncombined_constraints: impl IntoIterator<Item = (ColList, Constraints)>,
    ) -> Self {
        Self {
            table_id,
            table_name,
            fields,
            constraints: combine_constraints(uncombined_constraints),
        }
    }

    /// Equivalent to what [`Clone::clone`] would do.
    ///
    /// `Header` intentionally does not implement `Clone`,
    /// as we can't afford to clone it in normal execution paths.
    /// However, we don't care about performance in error paths,
    /// and we need to embed owned `Header`s in error objects to report useful messages.
    pub fn clone_for_error(&self) -> Self {
        Header::new(
            self.table_id,
            self.table_name.clone(),
            self.fields.clone(),
            self.constraints.clone(),
        )
    }

    /// Finds the index of the column with a matching `FieldName`.
    pub fn column_pos(&self, col: FieldName) -> Option<ColId> {
        self.fields.iter().position(|f| f.field == col).map(Into::into)
    }

    pub fn column_pos_or_err(&self, col: FieldName) -> Result<ColId, RelationError> {
        self.column_pos(col)
            .ok_or_else(|| RelationError::FieldNotFound(self.clone_for_error(), col))
    }

    pub fn field_name(&self, col: FieldName) -> Option<(ColId, FieldName)> {
        self.column_pos(col).map(|id| (id, self.fields[id.idx()].field))
    }

    /// Copy the [Constraints] that are referenced in the list of `for_columns`
    fn retain_constraints(&self, for_columns: &ColList) -> BTreeMap<ColList, Constraints> {
        // Copy the constraints of the selected columns and retain the multi-column ones...
        self.constraints
            .iter()
            // Keep constraints with a col list where at least one col is in `for_columns`.
            .filter(|(cols, _)| cols.iter().any(|c| for_columns.contains(c)))
            .map(|(cols, constraints)| (cols.clone(), *constraints))
            .collect()
    }

    pub fn has_constraint(&self, field: ColId, constraint: Constraints) -> bool {
        self.constraints
            .iter()
            .any(|(col, ct)| col.contains(field) && ct.contains(&constraint))
    }

    /// Project the [ColExpr]s & the [Constraints] that referenced them
    pub fn project(&self, cols: &[ColExpr]) -> Result<Self, RelationError> {
        let mut fields = Vec::with_capacity(cols.len());
        let mut to_keep = ColList::with_capacity(cols.len() as _);

        for (pos, col) in cols.iter().enumerate() {
            match col {
                ColExpr::Col(col) => {
                    to_keep.push(*col);
                    fields.push(self.fields[col.idx()].clone());
                }
                ColExpr::Value(val) => {
                    // TODO: why should this field name be relevant?
                    // We should generate immediate names instead.
                    let field = FieldName::new(self.table_id, pos.into());
                    let ty = val.type_of().ok_or_else(|| {
                        RelationError::TypeInference(field, TypeError::CannotInferType { value: val.clone() })
                    })?;
                    fields.push(Column::new(field, ty));
                }
            }
        }

        let constraints = self.retain_constraints(&to_keep);

        Ok(Self::new(self.table_id, self.table_name.clone(), fields, constraints))
    }

    /// Project the ourself onto the `ColList`, keeping constraints that reference the columns in the ColList.
    /// Does not change `ColIDs`.
    pub fn project_col_list(&self, cols: &ColList) -> Self {
        let mut fields = Vec::with_capacity(cols.len() as usize);

        for col in cols.iter() {
            fields.push(self.fields[col.idx()].clone());
        }

        let constraints = self.retain_constraints(cols);
        Self::new(self.table_id, self.table_name.clone(), fields, constraints)
    }

    /// Adds the fields &  [Constraints] from `right` to this [`Header`],
    /// renaming duplicated fields with a counter like `a, a => a, a0`.
    pub fn extend(&self, right: &Self) -> Self {
        // Increase the positions of the columns in `right.constraints`, adding the count of fields on `left`
        let mut constraints = self.constraints.clone();
        let len_lhs = self.fields.len() as u16;
        constraints.extend(right.constraints.iter().map(|(cols, c)| {
            let cols = cols.iter().map(|col| ColId(col.0 + len_lhs)).collect::<ColList>();
            (cols, *c)
        }));

        let mut fields = self.fields.clone();
        fields.extend(right.fields.iter().cloned());

        Self::new(self.table_id, self.table_name.clone(), fields, constraints)
    }
}

/// Combine constraints.
/// The result is a map from `ColList` to `Constraints`.
/// In particular, it includes all indexes, and tells you which of them are unique.
/// The result MAY contain multiple `ColList`s with the same columns in different orders.
/// This corresponds to differently-ordered indices.
///
/// Unique constraints are considered logically unordered. Information from them will
/// be propagated to all indices that contain the same columns.
pub fn combine_constraints(
    uncombined: impl IntoIterator<Item = (ColList, Constraints)>,
) -> BTreeMap<ColList, Constraints> {
    let mut constraints = BTreeMap::new();
    for (col_list, constraint) in uncombined {
        let slot = constraints.entry(col_list).or_insert(Constraints::unset());
        *slot = slot.push(constraint);
    }

    let mut uniques: HashSet<ColSet> = HashSet::default();
    for (col_list, constraint) in &constraints {
        if constraint.has_unique() {
            uniques.insert(col_list.into());
        }
    }

    for (cols, constraint) in constraints.iter_mut() {
        if uniques.contains(&ColSet::from(cols)) {
            *constraint = constraint.push(Constraints::unique());
        }
    }

    constraints
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
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
        write!(f, "]")
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_primitives::col_list;

    /// Build a [Header] using the initial `start_pos` as the column position for the [Constraints]
    fn head(id: impl Into<TableId>, name: &str, fields: (ColId, ColId), start_pos: u16) -> Header {
        let pos_lhs = start_pos;
        let pos_rhs = start_pos + 1;

        let ct = vec![
            (ColId(pos_lhs).into(), Constraints::indexed()),
            (ColId(pos_rhs).into(), Constraints::identity()),
            (col_list![pos_lhs, pos_rhs], Constraints::primary_key()),
            (col_list![pos_rhs, pos_lhs], Constraints::unique()),
        ];

        let id = id.into();
        let fields = [fields.0, fields.1].map(|col| Column::new(FieldName::new(id, col), AlgebraicType::I8));
        Header::new(id, name.into(), fields.into(), ct)
    }

    #[test]
    fn test_project() {
        let a = 0.into();
        let b = 1.into();

        let head = head(0, "t1", (a, b), 0);
        let new = head.project(&[] as &[ColExpr]).unwrap();

        let mut empty = head.clone_for_error();
        empty.fields.clear();
        empty.constraints.clear();
        assert_eq!(empty, new);

        let all = head.clone_for_error();
        let new = head.project(&[a, b].map(ColExpr::Col)).unwrap();
        assert_eq!(all, new);

        let mut first = head.clone_for_error();
        first.fields.pop();
        first.constraints = first.retain_constraints(&a.into());
        let new = head.project(&[a].map(ColExpr::Col)).unwrap();
        assert_eq!(first, new);

        let mut second = head.clone_for_error();
        second.fields.remove(0);
        second.constraints = second.retain_constraints(&b.into());
        let new = head.project(&[b].map(ColExpr::Col)).unwrap();
        assert_eq!(second, new);
    }

    #[test]
    fn test_extend() {
        let t1 = 0.into();
        let t2: TableId = 1.into();
        let a = 0.into();
        let b = 1.into();
        let c = 0.into();
        let d = 1.into();

        let head_lhs = head(t1, "t1", (a, b), 0);
        let head_rhs = head(t2, "t2", (c, d), 0);

        let new = head_lhs.extend(&head_rhs);

        let lhs = new.project(&[a, b].map(ColExpr::Col)).unwrap();
        assert_eq!(head_lhs, lhs);

        let mut head_rhs = head(t2, "t2", (c, d), 2);
        head_rhs.table_id = t1;
        head_rhs.table_name = head_lhs.table_name.clone();
        let rhs = new.project(&[2, 3].map(ColId).map(ColExpr::Col)).unwrap();
        assert_eq!(head_rhs, rhs);
    }

    #[test]
    fn test_combine_constraints() {
        let raw = vec![
            (col_list![0], Constraints::indexed()),
            (col_list![0], Constraints::unique()),
            (col_list![1], Constraints::identity()),
            (col_list![1, 0], Constraints::primary_key()),
            (col_list![0, 1], Constraints::unique()),
            (col_list![2], Constraints::indexed()),
            (col_list![3], Constraints::unique()),
        ];
        let expected = vec![
            (col_list![0], Constraints::indexed().push(Constraints::unique())),
            (col_list![1], Constraints::identity()),
            (col_list![1, 0], Constraints::primary_key().push(Constraints::unique())),
            (col_list![0, 1], Constraints::unique()),
            (col_list![2], Constraints::indexed()),
            (col_list![3], Constraints::unique()),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        assert_eq!(combine_constraints(raw), expected);
    }
}
