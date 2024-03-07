use crate::algebraic_value::AlgebraicValue;
use crate::db::auth::{StAccess, StTableType};
use crate::db::error::RelationError;
use crate::satn::Satn;
use crate::{algebraic_type, AlgebraicType, ProductType, ProductTypeElement, Typespace, WithTypespace};
use derive_more::From;
use spacetimedb_primitives::{ColId, ColList, ColListBuilder, Constraints, TableId};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Index;
use std::slice::Iter;
use std::sync::Arc;
use std::vec::IntoIter;

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

    pub fn to_field_name(&self) -> Option<String> {
        match self {
            FieldName::Name { field, .. } => Some(field.clone()),
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

impl FieldExpr {
    /// Returns a borrowed version of `FieldExpr`.
    pub fn borrowed(&self) -> FieldExprRef<'_> {
        match self {
            Self::Name(x) => FieldExprRef::Name(x),
            Self::Value(x) => FieldExprRef::Value(x),
        }
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
    pub field: FieldOnly<'a>,
    pub algebraic_type: &'a AlgebraicType,
}

// TODO(perf): Remove `Clone` derivation.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Column {
    pub field: Arc<FieldName>,
    pub algebraic_type: AlgebraicType,
    pub col_id: ColId,
}

impl Column {
    pub fn new(field: FieldName, algebraic_type: AlgebraicType, col_id: ColId) -> Self {
        Self {
            field: field.into(),
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

/// Represents a collection of `fields` metadata made of [Column].
///
/// The `fields` need to be preserved by the order specified by the user.
#[derive(Debug, Clone, Eq)]
pub struct Fields {
    columns: Vec<Column>,
    /// Keeps an index to quickly look up the position
    idx: HashMap<Arc<FieldName>, usize>,
}

impl Fields {
    /// Creates a new instance of `Fields` with the given columns, and build an internal map of `FieldName` to their positions.
    pub fn new(columns: Vec<Column>) -> Self {
        let idx = columns
            .iter()
            .enumerate()
            .map(|(pos, x)| (x.field.clone(), pos))
            .collect();
        Self { columns, idx }
    }

    /// Returns the number of columns in the `Fields`.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Returns true if the `Fields` contains no columns.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Adds a new column to the `Fields`.
    pub fn add_field(&mut self, column: Column) {
        let idx = self.columns.len();
        self.idx.insert(column.field.clone(), idx);
        self.columns.push(column);
    }

    /// Removes all columns from the `Fields`.
    pub fn clear(&mut self) {
        self.columns.clear();
        self.idx.clear();
    }

    /// Removes and returns the last column in the `Fields`, or None if it is empty.
    pub fn pop(&mut self) -> Option<Column> {
        if let Some(col) = self.columns.pop() {
            self.idx.remove(&col.field);
            return Some(col);
        }
        None
    }

    /// Removes and returns the column at the specified index.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn remove(&mut self, index: usize) -> Column {
        let col = self.columns.remove(index);
        self.idx.remove(&col.field);
        col
    }

    /// Returns the index of the column with the specified [FieldName].
    pub fn get_field_index(&self, field_name: &FieldName) -> Option<&usize> {
        self.idx.get(field_name)
    }

    /// Returns a reference to the [Column] with the specified [FieldName].
    pub fn get_field(&self, field_name: &FieldName) -> Option<&Column> {
        self.get_field_index(field_name).and_then(|pos| self.columns.get(*pos))
    }

    /// Returns a reference to the [Column] at the specified index.
    pub fn column_by_pos(&self, idx: usize) -> Option<&Column> {
        self.columns.get(idx)
    }

    /// Returns an iterator over the columns in the `Fields`.
    pub fn iter(&self) -> Iter<'_, Column> {
        self.columns.iter()
    }

    /// Returns an iterator that consumes the `Fields` and returns owned columns.
    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> IntoIter<Column> {
        self.columns.into_iter()
    }

    /// Reserves capacity for at least `additional` more elements to be inserted in the `Fields`.
    pub fn reserve(&mut self, additional: usize) {
        self.columns.reserve(additional);
    }
}

impl PartialEq for Fields {
    // We must not take in account `self.idx` because the positions change on different schemas but the `columns` stay the same.
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns
    }
}

impl Hash for Fields {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.columns.hash(state);
    }
}

impl Index<usize> for Fields {
    type Output = Column;

    fn index(&self, index: usize) -> &Self::Output {
        &self.columns[index]
    }
}

impl From<Vec<Column>> for Fields {
    fn from(value: Vec<Column>) -> Self {
        Self::new(value)
    }
}

/// Represents a table header with column information and constraints.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Header {
    pub table_name: String,
    // Do not expose `fields` because we need to maintain their `idx`.
    fields: Fields,
    /// The list of constraints associated with the table's columns.
    pub constraints: Vec<(ColList, Constraints)>,
}

impl Header {
    /// Creates a new instance of `Header` with the given table name, fields, and constraints.
    pub fn new<F: Into<Fields>>(table_name: String, fields: F, constraints: Vec<(ColList, Constraints)>) -> Self {
        Self {
            table_name,
            fields: fields.into(),
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

    /// Creates a new [Header] from a [ProductType].
    pub fn from_product_type(table_name: String, fields: ProductType) -> Self {
        let cols: Vec<Column> = fields
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

    /// Converts the [Header] into a [ProductType].
    pub fn to_product_type(&self) -> ProductType {
        ProductType::from_iter(
            self.fields.iter().map(|x| {
                ProductTypeElement::new(x.algebraic_type.clone(), x.field.field_name().map(ToString::to_string))
            }),
        )
    }

    /// Creates a [Header] for a [MemTable].
    pub fn for_mem_table(fields: ProductType) -> Self {
        let table_name = format!("mem#{:x}", calculate_hash(&fields));
        Self::from_product_type(table_name, fields)
    }

    /// Returns a new [Header] containing only the fields names.
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

    /// Returns a slice of columns in the `Header`.
    pub fn fields(&self) -> &[Column] {
        &self.fields.columns
    }

    /// Returns the [Column] at the specified position.
    pub fn column_by_pos(&self, pos: usize) -> &Column {
        &self.fields[pos]
    }

    /// Returns the [FieldName] at the specified position.
    pub fn field_by_pos(&self, pos: usize) -> Option<&FieldName> {
        self.fields.column_by_pos(pos).map(|x| x.field.as_ref())
    }

    /// Returns the [Column] with the specified [FieldName].
    pub fn column(&self, col: &FieldName) -> Option<&Column> {
        match col {
            FieldName::Name { .. } => self.fields.get_field(col),
            FieldName::Pos { field, .. } => self.fields.column_by_pos(*field),
        }
    }

    /// Returns the [Column] with the specified `field_name`.
    pub fn column_by_name(&self, field_name: &str) -> Option<&Column> {
        self.column(&FieldName::named(&self.table_name, field_name))
    }

    /// Returns the [FieldName] with the specified `field_name`.
    pub fn field_by_name(&self, field_name: &str) -> Option<&FieldName> {
        self.column_by_name(field_name).map(|x| x.field.as_ref())
    }

    /// Returns the [Column] with the specified [ColId].
    pub fn column_by_id(&self, col_id: ColId) -> Option<&Column> {
        self.fields.iter().find(|x| x.col_id == col_id)
    }

    /// Returns the [FieldName] with the specified [ColId].
    pub fn field_by_id(&self, col_id: ColId) -> Option<&FieldName> {
        self.column_by_id(col_id).map(|x| x.field.as_ref())
    }

    /// Returns the [ColId] of the specified [FieldName].
    pub fn col_id_by_field<'a>(&'a self, col: &'a FieldName) -> Option<ColId> {
        self.column(col).map(|x| x.col_id)
    }

    /// Returns the [ColId] of the specified [FieldName] or [RelationError::FieldNotFound].
    pub fn col_id_by_field_or_err<'a>(&'a self, col: &'a FieldName) -> Result<ColId, RelationError> {
        self.col_id_by_field(col)
            .ok_or_else(|| RelationError::FieldNotFound(self.clone_for_error(), col.clone()))
    }

    /// Finds the [ColId] of a field with `name`.
    pub fn find_col_id_by_name(&self, name: &str) -> Option<ColId> {
        self.col_id_by_field(&FieldName::named(&self.table_name, name))
    }

    /// Converts the fields, cloning into [FieldExpr].
    pub fn fields_to_expr(&self) -> Vec<FieldExpr> {
        self.fields
            .iter()
            .cloned()
            .map(|Column { field, .. }| field.as_ref().clone().into())
            .collect()
    }

    /// Adds a [Column] to the [Header].
    pub fn add(&mut self, column: Column) {
        self.fields.add_field(column)
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
        self.col_id_by_field(field)
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
                    let pos = self.col_id_by_field_or_err(&col)?;
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
        for (pos, mut f) in right.fields.iter().cloned().enumerate() {
            if f.field.table() == self.table_name && self.col_id_by_field(&f.field).is_some() {
                let name = format!("{}_{}", f.field.field(), cont);
                f.field = FieldName::Name {
                    table: f.field.table().into(),
                    field: name,
                }
                .into();

                cont += 1;
            }
            f.col_id = ColId(len_lhs + pos as u32);
            fields.add_field(f);
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
                .map(|x| ProductTypeElement::new(x.algebraic_type, x.field.to_field_name())),
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
                Column::new(FieldName::named(table, fields.0), AlgebraicType::I8, pos_lhs.into()),
                Column::new(FieldName::named(table, fields.1), AlgebraicType::I8, pos_rhs.into()),
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
