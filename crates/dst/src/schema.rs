use spacetimedb_lib::db::raw_def::v10::*;
use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, TableAccess, TableType};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, ProductType, ProductTypeElement};

pub fn default_schema(rng: Rng) -> SchemaPlan {
    let profile = SchemaProfile::default();
    let plan = SchemaGenerator::new(rng, profile).gen_schema();
    plan
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Bool,
    I64,
    U64,
    String,
    Bytes,
}

impl Type {
    pub const ALL: &'static [Type] = &[Type::Bool, Type::I64, Type::U64, Type::String, Type::Bytes];

    pub fn to_algebraic(self) -> AlgebraicType {
        match self {
            Type::Bool => AlgebraicType::Bool,
            Type::I64 => AlgebraicType::I64,
            Type::U64 => AlgebraicType::U64,
            Type::String => AlgebraicType::String,
            Type::Bytes => AlgebraicType::Array(ArrayType {
                elem_ty: Box::new(AlgebraicType::U8),
            }),
        }
    }

    pub fn is_integral(self) -> bool {
        matches!(self, Type::I64 | Type::U64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Bool(bool),
    I64(i64),
    U64(u64),
    String(String),
    Bytes(Vec<u8>),
}

impl Value {
    fn to_algebraic(&self) -> AlgebraicValue {
        match self {
            Value::Bool(b) => AlgebraicValue::Bool(*b),
            Value::I64(v) => AlgebraicValue::I64(*v),
            Value::U64(v) => AlgebraicValue::U64(*v),
            Value::String(s) => AlgebraicValue::String(s.clone().into()),
            Value::Bytes(b) => AlgebraicValue::Array(ArrayValue::U8(b.clone().into())),
        }
    }
}

// Schema plan — the canonical source of truth.
// This Schema should be able to translate to valid `RawModuleDefV10`.
#[derive(Debug, Clone)]
pub struct SchemaPlan {
    pub tables: Vec<TablePlan>,
}

impl SchemaPlan {
    fn new(rng: Rng) {
        let profile = SchemaProfile::default();
        let schema = SchemaGenerator::new(rng, profile).gen_schema();
    }
}

#[derive(Debug, Clone)]
pub struct TablePlan {
    pub name: String,
    pub columns: Vec<ColumnPlan>,
    pub primary_key: Option<usize>,
    pub indexes: Vec<IndexPlan>,
    pub unique_constraints: Vec<UniqueConstraintPlan>,
    pub sequences: Vec<SequencePlan>,
    pub default_values: Vec<DefaultPlan>,
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub struct ColumnPlan {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct IndexPlan {
    /// Indices into `TablePlan.columns`.
    pub columns: Vec<usize>,
    pub algorithm: IndexAlgorithm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexAlgorithm {
    BTree,
    Hash,
}

#[derive(Debug, Clone)]
pub struct UniqueConstraintPlan {
    /// Indices into `TablePlan.columns`. Non-empty.
    pub columns: Vec<usize>,
}

/// A sequence on a specific column. The column's type is carried inline
/// so callers cannot create a sequence on a non-integral column —
/// the constructor requires `ty.is_integral()`.
#[derive(Debug, Clone)]
pub struct SequencePlan {
    /// Index into `TablePlan.columns`.
    pub column: usize,
    /// The type of that column. Must be integral (I64 or U64).
    pub ty: Type,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
    pub increment: i128,
}

impl SequencePlan {
    /// Create a sequence plan. Returns `None` if the type is not integral.
    pub fn new(column: usize, ty: Type) -> Option<Self> {
        if !ty.is_integral() {
            return None;
        }
        Some(Self {
            column,
            ty,
            start: None,
            min_value: None,
            max_value: None,
            increment: 1,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DefaultPlan {
    /// Index into `TablePlan.columns`.
    pub column: usize,
    pub value: Value,
}

// Lowering into RawModuleDefV10.
pub fn lower_schema(schema: &SchemaPlan) -> RawModuleDefV10 {
    let mut builder = RawModuleDefV10Builder::new();
    builder.set_case_conversion_policy(CaseConversionPolicy::None);

    for table in &schema.tables {
        lower_table(&mut builder, table);
    }

    builder.finish()
}

fn lower_table(builder: &mut RawModuleDefV10Builder, table: &TablePlan) {
    let product_type = ProductType {
        elements: table
            .columns
            .iter()
            .map(|col| ProductTypeElement {
                name: Some(col.name.clone().into()),
                algebraic_type: col.ty.to_algebraic(),
            })
            .collect(),
    };

    let mut tbl = builder.build_table_with_new_type(table.name.clone(), product_type, true);

    tbl = tbl.with_type(TableType::User);
    tbl = tbl.with_access(if table.is_public {
        TableAccess::Public
    } else {
        TableAccess::Private
    });
    // Primary key.
    if let Some(pk) = table.primary_key {
        tbl = tbl.with_primary_key(ColId(pk as u16));
    }

    // Unique constraints — all of them, including PK-matching.
    for constraint in &table.unique_constraints {
        let col_list: ColList = constraint.columns.iter().map(|&c| ColId(c as u16)).collect();
        tbl = tbl.with_unique_constraint(col_list);
    }

    // Indexes.
    for index in &table.indexes {
        let col_list: ColList = index.columns.iter().map(|&c| ColId(c as u16)).collect();

        let algorithm = match index.algorithm {
            IndexAlgorithm::BTree => RawIndexAlgorithm::BTree { columns: col_list },
            IndexAlgorithm::Hash => RawIndexAlgorithm::Hash { columns: col_list },
        };

        let source_name = format!(
            "{}_{}_idx",
            table.name,
            index
                .columns
                .iter()
                .map(|&c| table.columns[c].name.as_str())
                .collect::<Vec<_>>()
                .join("_")
        );

        tbl = tbl.with_index_no_accessor_name(algorithm, source_name);
    }

    // Sequences — all of them.
    for seq in &table.sequences {
        tbl = tbl.with_column_sequence(ColId(seq.column as u16));
    }

    // Default values.
    for default in &table.default_values {
        let algebraic_val = default.value.to_algebraic();
        tbl = tbl.with_default_column_value(ColId(default.column as u16), algebraic_val);
    }

    tbl.finish();
}

/// Controls the shape of generated schemas.
#[derive(Debug, Clone)]
pub struct SchemaProfile {
    pub table_count: (usize, usize),
    pub columns: (usize, usize),
    pub pk_prob: f64,
    pub auto_inc_prob: f64,
    pub indexes: (usize, usize),
    pub unique_constraints: (usize, usize),
    pub btree_prob: f64,
    pub private_prob: f64,
}

impl Default for SchemaProfile {
    fn default() -> Self {
        Self {
            table_count: (1, 100),
            columns: (1, 10),
            pk_prob: 0.7,
            auto_inc_prob: 0.3,
            indexes: (0, 3),
            unique_constraints: (0, 2),
            btree_prob: 0.7,
            private_prob: 0.1,
        }
    }
}

pub struct SchemaGenerator {
    rng: Rng,
    profile: SchemaProfile,
}

impl SchemaGenerator {
    pub fn new(rng: Rng, profile: SchemaProfile) -> Self {
        Self { rng, profile }
    }

    fn range(&self, (lo, hi): (usize, usize)) -> usize {
        if lo >= hi {
            return lo;
        }
        lo + (self.rng.next_u64() as usize % (hi - lo + 1))
    }

    fn gen_type(&self) -> Type {
        Type::ALL[self.rng.index(Type::ALL.len())]
    }

    fn gen_columns(&self) -> Vec<ColumnPlan> {
        let n = self.range(self.profile.columns);
        let mut names = Vec::with_capacity(n);
        let mut seen = Vec::with_capacity(n);
        for _ in 0..n {
            let name = self.gen_column_name(&seen);
            seen.push(name.clone());
            names.push(ColumnPlan {
                name,
                ty: self.gen_type(),
            });
        }
        names
    }

    fn gen_ident(&self) -> String {
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_";
        const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyz_";
        let len = 4 + (self.rng.next_u64() as usize % 12);
        let mut s = String::with_capacity(len);
        s.push(FIRST[self.rng.index(FIRST.len())] as char);
        for _ in 1..len {
            s.push(CHARS[self.rng.index(CHARS.len())] as char);
        }
        s
    }

    fn gen_column_name(&self, seen: &[String]) -> String {
        loop {
            let name = self.gen_ident();
            if !seen.contains(&name) {
                return name;
            }
        }
    }

    fn gen_unique_constraints(&self, columns: &[ColumnPlan], pk: &Option<usize>) -> Vec<UniqueConstraintPlan> {
        let n = self.range(self.profile.unique_constraints);
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut result = Vec::new();
        for _ in 0..n {
            let num_cols = 1 + self.rng.index(columns.len().min(3));
            let mut cols: Vec<usize> = (0..num_cols).map(|_| self.rng.index(columns.len())).collect();
            cols.sort();
            cols.dedup();
            if !cols.is_empty() && !seen.contains(&cols) {
                seen.push(cols.clone());
                result.push(UniqueConstraintPlan { columns: cols });
            }
        }
        // Ensure PK has a matching unique constraint.
        if let Some(pk) = pk {
            if !seen.contains(&vec![*pk]) {
                result.push(UniqueConstraintPlan { columns: vec![*pk] });
            }
        }
        result
    }

    fn gen_indexes(
        &self,
        columns: &[ColumnPlan],
        unique_constraints: &[UniqueConstraintPlan],
        pk: &Option<usize>,
    ) -> Vec<IndexPlan> {
        // Every unique constraint and PK needs a matching index.
        let mut seen_cols: Vec<Vec<usize>> = Vec::new();
        let mut indexes: Vec<IndexPlan> = Vec::new();

        // Index for PK.
        if let Some(pk) = pk {
            seen_cols.push(vec![*pk]);
            indexes.push(IndexPlan {
                columns: vec![*pk],
                algorithm: IndexAlgorithm::BTree,
            });
        }

        // Indexes for unique constraints.
        for constraint in unique_constraints {
            if seen_cols.contains(&constraint.columns) {
                continue;
            }
            seen_cols.push(constraint.columns.clone());
            indexes.push(IndexPlan {
                columns: constraint.columns.clone(),
                algorithm: IndexAlgorithm::BTree,
            });
        }

        // Additional random indexes.
        let n = self.range(self.profile.indexes);
        for _ in 0..n {
            let num_cols = 1 + self.rng.index(columns.len().min(3));
            let mut cols: Vec<usize> = (0..num_cols).map(|_| self.rng.index(columns.len())).collect();
            cols.sort();
            cols.dedup();
            if cols.is_empty() || seen_cols.contains(&cols) {
                continue;
            }
            seen_cols.push(cols.clone());
            let algorithm = if self.rng.sample_probability(self.profile.btree_prob) {
                IndexAlgorithm::BTree
            } else {
                IndexAlgorithm::Hash
            };
            indexes.push(IndexPlan {
                columns: cols,
                algorithm,
            });
        }

        indexes
    }

    fn gen_table(&self, _table_index: usize) -> TablePlan {
        let columns = self.gen_columns();

        let primary_key = if self.rng.sample_probability(self.profile.pk_prob) && !columns.is_empty() {
            Some(self.rng.index(columns.len()))
        } else {
            None
        };

        let unique_constraints = self.gen_unique_constraints(&columns, &primary_key);

        let sequences = if let Some(pk) = primary_key {
            if columns[pk].ty.is_integral() && self.rng.sample_probability(self.profile.auto_inc_prob) {
                SequencePlan::new(pk, columns[pk].ty).into_iter().collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let indexes = self.gen_indexes(&columns, &unique_constraints, &primary_key);

        let name = format!("tbl_{}", self.gen_ident());

        TablePlan {
            name,
            columns,
            primary_key,
            indexes,
            unique_constraints,
            sequences,
            default_values: vec![],
            is_public: !self.rng.sample_probability(self.profile.private_prob),
        }
    }

    pub fn gen_schema(&self) -> SchemaPlan {
        let table_count = self.range(self.profile.table_count);
        let tables = (0..table_count).map(|i| self.gen_table(i)).collect();
        SchemaPlan { tables }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_single_table() {
        let schema = SchemaPlan {
            tables: vec![TablePlan {
                name: "users".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "name".into(),
                        ty: Type::String,
                    },
                    ColumnPlan {
                        name: "score".into(),
                        ty: Type::I64,
                    },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![2],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![SequencePlan::new(0, Type::U64).unwrap()],
                default_values: vec![],
                is_public: true,
            }],
        };

        let raw = lower_schema(&schema);

        // Should have Typespace, Types, and Tables sections.
        assert!(raw.typespace().is_some());
        assert!(raw.types().is_some());
        let tables = raw.tables().unwrap();
        assert_eq!(tables.len(), 1);

        let t = &tables[0];
        assert_eq!(t.source_name.as_ref(), "users");
        assert_eq!(t.table_type, TableType::User);
        assert_eq!(t.table_access, TableAccess::Public);
        assert_eq!(t.primary_key.len(), 1);
        assert_eq!(t.indexes.len(), 1);
        assert_eq!(t.sequences.len(), 1);
    }
}
