use crate::algebraic_value::AlgebraicValue;
use crate::db::auth::{StAccess, StTableType};
use crate::db::error::RelationError;
use crate::satn::Satn;
use crate::{algebraic_type, AlgebraicType, ProductType, ProductTypeElement, Typespace, WithTypespace};
use derive_more::From;
use itertools::Itertools;
use nonempty::NonEmpty;
use spacetimedb_primitives::{ColId, ColList, ColListBuilder, Constraints, TableId};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
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

// TODO(perf): Remove `Clone` derivation.
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

// TODO(perf): Remove `Clone` derivation.
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ColumnOnlyField<'a> {
    pub field: FieldOnly<'a>,
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

/// TODO: This is duplicate from `crates/lib/src/operator.rs`
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpCmpIdx {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl OpCmpIdx {
    /// Reverse the order of the `cmp`, to helps in reducing the cases on evaluation, ie:
    pub fn reverse(self) -> Self {
        match self {
            Self::Eq => self,
            Self::NotEq => self,
            Self::Lt => Self::Gt,
            Self::LtEq => Self::GtEq,
            Self::Gt => Self::Lt,
            Self::GtEq => Self::LtEq,
        }
    }
}

#[derive(Debug)]
pub struct FieldValue<'a> {
    cmp: OpCmpIdx,
    field: &'a Column,
    value: &'a AlgebraicValue,
}

impl<'a> FieldValue<'a> {
    pub fn new(cmp: OpCmpIdx, field: &'a Column, value: &'a AlgebraicValue) -> Self {
        Self { cmp, field, value }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScanIndex<'a> {
    Index {
        cmp: OpCmpIdx,
        columns: NonEmpty<&'a Column>,
        value: AlgebraicValue,
    },
    Scan {
        cmp: OpCmpIdx,
        column: &'a Column,
        value: AlgebraicValue,
    },
}

// TODO(perf): Remove `Clone` impl.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeaderOnlyField<'a> {
    pub fields: Vec<ColumnOnlyField<'a>>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
                let name = match f.name {
                    None => FieldName::Pos {
                        table: table_name.clone(),
                        field: pos,
                    },
                    Some(field) => FieldName::Name {
                        table: table_name.clone(),
                        field,
                    },
                };
                Column::new(name, f.algebraic_type, ColId(pos as u32))
            })
            .collect();

        Self::new(table_name, cols, Default::default())
    }

    pub fn to_product_type(&self) -> ProductType {
        ProductType::from_iter(
            self.fields.iter().map(|x| {
                ProductTypeElement::new(x.algebraic_type.clone(), x.field.field_name().map(ToString::to_string))
            }),
        )
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

    pub fn column_pos<'a>(&'a self, col: &'a FieldName) -> Option<ColId> {
        match col {
            FieldName::Name { .. } => self.fields.iter().position(|f| &f.field == col),
            FieldName::Pos { field, .. } => self
                .fields
                .iter()
                .enumerate()
                .position(|(pos, f)| &f.field == col || *field == pos),
        }
        .map(Into::into)
    }

    pub fn column_pos_or_err<'a>(&'a self, col: &'a FieldName) -> Result<ColId, RelationError> {
        self.column_pos(col)
            .ok_or_else(|| RelationError::FieldNotFound(self.clone_for_error(), col.clone()))
    }

    /// Finds the position of a field with `name`.
    pub fn find_pos_by_name(&self, name: &str) -> Option<ColId> {
        self.column_pos(&FieldName::named(&self.table_name, name))
    }

    pub fn column<'a>(&'a self, col: &'a FieldName) -> Option<&Column> {
        self.fields.iter().find(|f| &f.field == col)
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

    /// Pick the best index that matches the *permutation* of the supplied `fields`.
    ///
    /// This function is designed to handle complex scenarios when selecting the optimal index for a query. The scenarios include:
    ///
    /// - Combination of multi and single column indexes that could refer to the same field,
    ///   so, if we could have `indexes`: `[a], [a, b]` and being asked: `a = 1 AND b = 2 AND a = 3`.
    /// - Query expression can be supplied in any order, ie: `a = 1 AND b = 2` or `b = 2 AND a = 1`.
    /// - Different `operators` need to match different `index` operations, ie: `a > 1 AND a = 2 AND a < 3`,
    ///   must return
    ///
    ///   -`ScanIndex::Index(Range(a > 1, a < 3))`
    ///   -`ScanIndex::Index(a = 2)`
    /// - The use of multiple tables could generated redundant/duplicate operations like `[ScanIndex::Index(a = 1), ScanIndex::Index(a = 1), ScanIndex::Scan(a = 1)]`.
    ///   This *can't* be handled here.
    ///
    /// # Parameters
    ///
    /// - `fields`: A slice of `FieldValue` representing the query conditions.
    ///
    /// # Returns
    ///
    /// - A vector of `ScanIndex` representing the selected `index` OR `scan` operations.
    /// - A HashSet of `(FieldName, OpCmpIdx)` representing the fields and operators that where matched by a `index` operation.
    ///   This is required to remove the redundant operation on `[ScanIndex::Index(a = 1), ScanIndex::Index(a = 1), ScanIndex::Scan(a = 1)]`,
    ///   that could be generated by calling this function several times by using multiples `JOINS`
    ///
    /// # Example
    ///
    /// If we have a table with `indexes`: `[a], [b], [b, c]` and then try to
    /// optimize `WHERE a = 1 AND d > 2 AND c = 2 AND b = 1` we should return
    ///
    /// -`ScanIndex::Index(a = 1)`
    /// -`ScanIndex::Scan(c = 2)`
    /// -`ScanIndex::Index([c,b] = [1, 2])`
    ///
    /// NOTE:
    ///
    /// -We don't do extra optimization here (like last the `Scan`), this should be handled in upper layers.
    /// -We only check up to 4 fields, to keep the permutation small. After that we will miss indexes.
    pub fn select_best_index<'a>(
        &'a self,
        fields: &[FieldValue<'a>],
    ) -> (Vec<ScanIndex<'a>>, HashSet<(FieldName, OpCmpIdx)>) {
        let total = std::cmp::min(4, fields.len());
        let mut found = Vec::with_capacity(total);
        let mut done = HashSet::with_capacity(total);
        let mut fields_indexed = HashSet::with_capacity(total);

        let index = Constraints::indexed();
        for fields in (1..=total).rev().flat_map(|len| fields.iter().permutations(len)) {
            if fields.iter().any(|x| done.contains(&(x.field, &x.cmp))) {
                continue;
            }

            let find = NonEmpty::collect(fields.iter().map(|x| x.field)).unwrap();
            let find_cols = ColListBuilder::from_iter(fields.iter().map(|x| x.field.col_id))
                .build()
                .unwrap();
            if self
                .constraints
                .iter()
                .any(|(col, ct)| col == &find_cols && ct.contains(&index))
            {
                if fields.len() == 1 {
                    done.extend(fields.iter().map(|x| (x.field, &x.cmp)));
                    fields_indexed.extend(fields.iter().map(|x| (x.field, &x.cmp)));

                    found.push(ScanIndex::Index {
                        cmp: fields[0].cmp,
                        columns: find,
                        value: fields[0].value.clone(),
                    });
                } else if fields.iter().all(|x| x.cmp == x.cmp.reverse()) {
                    done.extend(fields.iter().map(|x| (x.field, &x.cmp)));
                    fields_indexed.extend(fields.iter().map(|x| (x.field, &x.cmp)));

                    found.push(ScanIndex::Index {
                        cmp: fields[0].cmp,
                        columns: find,
                        value: ProductValue::from_iter(fields.into_iter().map(|x| x.value.clone())).into(),
                    });
                }
            } else if fields.len() == 1 {
                let field = fields[0];
                if !done.contains(&(field.field, &field.cmp)) {
                    done.insert((field.field, &field.cmp));

                    found.push(ScanIndex::Scan {
                        cmp: field.cmp,
                        column: find.head,
                        value: field.value.clone(),
                    });
                }
            }
        }

        let done: HashSet<_> = fields_indexed
            .iter()
            .map(|(col, cmp)| (col.field.clone(), **cmp))
            .collect();

        (found, done)
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
                    p.push(Column::new(
                        FieldName::Pos {
                            table: self.table_name.clone(),
                            field: pos,
                        },
                        col.type_of(),
                        pos.into(),
                    ));
                }
            }
        }

        let constraints = self.retain_constraints(&to_keep.build().unwrap());

        Ok(Self::new(self.table_name.clone(), p, constraints))
    }

    /// Adds the fields &  [Constraints] from `right` to this [`Header`],
    /// renaming duplicated fields with a counter like `a, a => a, a0`.
    pub fn extend(&self, right: &Self) -> Self {
        let count = self.fields.len() + right.fields.len();

        // Increase the positions of the columns in `right.constraints`, adding the count of fields on `left`
        let mut constraints = self.constraints.clone();
        let len_lhs = self.fields.len() as u32;
        constraints.extend(right.constraints.iter().map(|(cols, c)| {
            let cols = cols
                .iter()
                .map(|col| ColId(col.0 + len_lhs))
                .collect::<ColListBuilder>()
                .build()
                .unwrap();
            (cols, *c)
        }));

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

        Self::new(self.table_name.clone(), fields, constraints)
    }
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
    fn head(&self) -> &Arc<Header>;
    /// Specify the size in rows of the [Relation].
    ///
    /// Warning: It should at least be precise in the lower-bound estimate.
    fn row_count(&self) -> RowCount;
}

/// A stored table from [RelationalDB]
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
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
    use spacetimedb_primitives::col_list;

    use super::*;
    use crate::product;

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

    #[test]
    fn best_index() {
        let fields: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .enumerate()
            .map(|(pos, x)| Column::new(FieldName::named("t1", x), AlgebraicType::I8, pos.into()))
            .collect();

        let a = ColId(0);
        let b = ColId(1);
        let c = ColId(2);
        let d = ColId(3);

        let col_a = fields[0].clone();
        let col_b = fields[1].clone();
        let col_c = fields[2].clone();
        let col_d = fields[3].clone();
        let col_e = fields[4].clone();

        let head1 = Header::new(
            "t1".into(),
            fields,
            vec![
                //Index a
                (a.into(), Constraints::primary_key()),
                //Index b
                (b.into(), Constraints::indexed()),
                //Index b + c
                (col_list![b, c], Constraints::unique()),
                //Index a+ b + c + d
                (col_list![a, b, c, d], Constraints::indexed()),
            ],
        );

        let val_a = AlgebraicValue::U64(1);
        let val_b = AlgebraicValue::U64(2);
        let val_c = AlgebraicValue::U64(3);
        let val_d = AlgebraicValue::U64(4);
        let val_e = AlgebraicValue::U64(5);

        // Check for simple scan
        assert_eq!(
            head1
                .select_best_index(&[FieldValue::new(OpCmpIdx::Eq, &col_d, &val_e)])
                .0,
            vec![ScanIndex::Scan {
                cmp: OpCmpIdx::Eq,
                column: &col_d,
                value: val_e.clone(),
            }]
        );

        assert_eq!(
            head1
                .select_best_index(&[FieldValue::new(OpCmpIdx::Eq, &col_a, &val_a)])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: NonEmpty::new(&col_a),
                value: val_a.clone()
            }]
        );

        assert_eq!(
            head1
                .select_best_index(&[FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b)])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: NonEmpty::new(&col_b),
                value: val_b.clone()
            }]
        );

        // Check for permutation
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c)
                ])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: (&col_b, vec![&col_c]).into(),
                value: product![val_b.clone(), val_c.clone()].into()
            }]
        );

        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c),
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b)
                ])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: (&col_c, vec![&col_b]).into(),
                value: product![val_c.clone(), val_b.clone()].into()
            }]
        );

        // Check for permutation
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c),
                    FieldValue::new(OpCmpIdx::Eq, &col_d, &val_d)
                ])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: (&col_a, vec![&col_b, &col_c, &col_d]).into(),
                value: product![val_a.clone(), val_b.clone(), val_c.clone(), val_d.clone()].into(),
            }]
        );

        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Eq, &col_d, &val_d),
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c)
                ])
                .0,
            vec![ScanIndex::Index {
                cmp: OpCmpIdx::Eq,
                columns: (&col_b, vec![&col_a, &col_d, &col_c]).into(),
                value: product![val_b.clone(), val_a.clone(), val_d.clone(), val_c.clone()].into(),
            }]
        );

        // Check mix scan + index
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Eq, &col_e, &val_e),
                    FieldValue::new(OpCmpIdx::Eq, &col_d, &val_d)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Eq,
                    columns: NonEmpty::new(&col_b),
                    value: val_b.clone(),
                },
                ScanIndex::Index {
                    cmp: OpCmpIdx::Eq,
                    columns: NonEmpty::new(&col_a),
                    value: val_a.clone(),
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Eq,
                    column: &col_e,
                    value: val_e.clone(),
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Eq,
                    column: &col_d,
                    value: val_d.clone(),
                }
            ]
        );

        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c),
                    FieldValue::new(OpCmpIdx::Eq, &col_d, &val_d)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Eq,
                    columns: (&col_b, vec![&col_c]).into(),
                    value: product![val_b.clone(), val_c.clone()].into()
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Eq,
                    column: &col_d,
                    value: val_d.clone(),
                }
            ]
        );
    }

    #[test]
    fn best_index_range() {
        let fields: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .enumerate()
            .map(|(pos, x)| Column::new(FieldName::named("t1", x), AlgebraicType::I8, pos.into()))
            .collect();

        let a = ColId(0);
        let b = ColId(1);
        let c = ColId(2);
        let d = ColId(3);

        let col_a = fields[0].clone();
        let col_b = fields[1].clone();
        let col_c = fields[2].clone();
        let col_d = fields[3].clone();

        let head1 = Header::new(
            "t1".into(),
            fields,
            vec![
                //Index a
                (a.into(), Constraints::primary_key()),
                //Index b
                (b.into(), Constraints::indexed()),
                //Index b + c
                (col_list![b, c], Constraints::unique()),
                //Index a+ b + c + d
                (col_list![a, b, c, d], Constraints::indexed()),
            ],
        );

        let val_a = AlgebraicValue::U64(1);
        let val_b = AlgebraicValue::U64(2);
        let val_c = AlgebraicValue::U64(3);
        let val_d = AlgebraicValue::U64(4);

        // Same field indexed
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Gt, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Lt, &col_a, &val_b)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Gt,
                    columns: NonEmpty::new(&col_a),
                    value: val_a.clone()
                },
                ScanIndex::Index {
                    cmp: OpCmpIdx::Lt,
                    columns: NonEmpty::new(&col_a),
                    value: val_b.clone()
                },
            ]
        );

        // Same field scan
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Gt, &col_d, &val_d),
                    FieldValue::new(OpCmpIdx::Lt, &col_d, &val_b)
                ])
                .0,
            vec![
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Gt,
                    column: &col_d,
                    value: val_d.clone()
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Lt,
                    column: &col_d,
                    value: val_b.clone()
                }
            ]
        );
        // One indexed other scan
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Gt, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Lt, &col_c, &val_c)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Gt,
                    columns: NonEmpty::new(&col_b),
                    value: val_b.clone()
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Lt,
                    column: &col_c,
                    value: val_c.clone()
                }
            ]
        );

        // 1 multi-indexed 1 index
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Eq, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::GtEq, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Eq, &col_c, &val_c)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Eq,
                    columns: (&col_b, vec![&col_c]).into(),
                    value: product![val_b.clone(), val_c.clone()].into()
                },
                ScanIndex::Index {
                    cmp: OpCmpIdx::GtEq,
                    columns: NonEmpty::new(&col_a),
                    value: val_a.clone()
                },
            ]
        );

        // 1 indexed 2 scan
        assert_eq!(
            head1
                .select_best_index(&[
                    FieldValue::new(OpCmpIdx::Gt, &col_b, &val_b),
                    FieldValue::new(OpCmpIdx::Eq, &col_a, &val_a),
                    FieldValue::new(OpCmpIdx::Lt, &col_c, &val_c)
                ])
                .0,
            vec![
                ScanIndex::Index {
                    cmp: OpCmpIdx::Gt,
                    columns: NonEmpty::new(&col_b),
                    value: val_b.clone()
                },
                ScanIndex::Index {
                    cmp: OpCmpIdx::Eq,
                    columns: NonEmpty::new(&col_a),
                    value: val_a.clone()
                },
                ScanIndex::Scan {
                    cmp: OpCmpIdx::Lt,
                    column: &col_c,
                    value: val_c.clone()
                }
            ]
        );
    }
}
