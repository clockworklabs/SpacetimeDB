use crate::algebraic_value::AlgebraicValue;
use crate::db::auth::{StAccess, StTableType};
use crate::db::error::RelationError;
use crate::satn::Satn;
use crate::{algebraic_type, AlgebraicType, ProductType, ProductTypeElement, Typespace, WithTypespace};
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
