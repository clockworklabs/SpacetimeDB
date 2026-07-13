use crate::rng;
use spacetimedb_lib::db::raw_def::v10::*;
use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, TableAccess, TableType};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_runtime::sim::Rng;
use spacetimedb_sats::{
    AlgebraicType, AlgebraicValue, ArrayType, ArrayValue, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};

pub fn default_schema(rng: Rng) -> SchemaPlan {
    let profile = SchemaProfile::default();
    let mut plan = SchemaGenerator::new(rng, profile).gen_schema();
    plan.ensure_auto_inc_table();
    plan.ensure_engine_migration_coverage();
    plan
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Bool,
    I64,
    U64,
    String,
    Bytes,
    Sum { variants: u8 },
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
            Type::Sum { variants } => {
                debug_assert!(variants > 0);
                AlgebraicType::Sum(SumType::new(
                    (0..variants)
                        .map(|variant| SumTypeVariant::new_named(AlgebraicType::U8, format!("variant_{variant}")))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ))
            }
        }
    }

    pub fn default_value(self) -> AlgebraicValue {
        match self {
            Type::Bool => AlgebraicValue::Bool(false),
            Type::I64 => AlgebraicValue::I64(0),
            Type::U64 => AlgebraicValue::U64(0),
            Type::String => AlgebraicValue::String("".into()),
            Type::Bytes => AlgebraicValue::Array(ArrayValue::U8(Vec::new().into())),
            Type::Sum { .. } => AlgebraicValue::sum(0, AlgebraicValue::U8(0)),
        }
    }

    pub fn is_integral(self) -> bool {
        matches!(self, Type::I64 | Type::U64)
    }
}

pub struct SchemaDecisions;

impl SchemaDecisions {
    pub fn range(rng: &Rng, (lo, hi): (usize, usize)) -> usize {
        rng::range_inclusive(rng, lo, hi)
    }

    pub fn index(rng: &Rng, len: usize) -> usize {
        rng::choose_index(rng, len).expect("len must be non-zero")
    }

    pub fn choose_index(rng: &Rng, len: usize) -> Option<usize> {
        rng::choose_index(rng, len)
    }

    pub fn sample_probability(rng: &Rng, probability: f64) -> bool {
        rng.sample_probability(probability)
    }

    pub fn gen_type(rng: &Rng) -> Type {
        Type::ALL[Self::index(rng, Type::ALL.len())]
    }

    pub fn gen_table_name(rng: &Rng, tables: &[TablePlan]) -> String {
        loop {
            let name = format!("tbl_{}", Self::gen_ident(rng));
            if tables.iter().all(|table| table.name != name) {
                return name;
            }
        }
    }

    pub fn gen_column_name(rng: &Rng, seen: &[String]) -> String {
        loop {
            let name = Self::gen_ident(rng);
            if !seen.contains(&name) {
                return name;
            }
        }
    }

    fn gen_ident(rng: &Rng) -> String {
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_";
        const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyz_";
        let len = 4 + (rng.next_u64() as usize % 12);
        let mut s = String::with_capacity(len);
        s.push(FIRST[Self::index(rng, FIRST.len())] as char);
        for _ in 1..len {
            s.push(CHARS[Self::index(rng, CHARS.len())] as char);
        }
        s
    }
}

pub struct SchemaNames;

impl SchemaNames {
    pub fn fresh_column_name(table: &TablePlan, base: &str) -> String {
        if table.columns.iter().all(|column| column.name != base) {
            return base.into();
        }

        for suffix in 0.. {
            let candidate = format!("{base}_{suffix}");
            if table.columns.iter().all(|column| column.name != candidate) {
                return candidate;
            }
        }

        unreachable!("unbounded suffix search must find a unique column name")
    }

    pub fn fresh_table_name(tables: &[TablePlan], base: &str) -> String {
        if tables.iter().all(|table| table.name != base) {
            return base.into();
        }

        for suffix in 0.. {
            let candidate = format!("{base}_{suffix}");
            if tables.iter().all(|table| table.name != candidate) {
                return candidate;
            }
        }

        unreachable!("unbounded suffix search must find a unique table name")
    }

    pub fn index_name(table: &TablePlan, index: &IndexPlan) -> String {
        format!(
            "{}_{}_idx",
            table.name,
            index
                .columns
                .iter()
                .map(|&c| table.columns[c].name.as_str())
                .collect::<Vec<_>>()
                .join("_")
        )
    }

    pub fn constraint_name(table: &TablePlan, constraint: &UniqueConstraintPlan) -> String {
        format!(
            "{}_{}_key",
            table.name,
            constraint
                .columns
                .iter()
                .map(|&c| table.columns[c].name.as_str())
                .collect::<Vec<_>>()
                .join("_")
        )
    }
}

// Schema plan — the canonical source of truth.
// This Schema should be able to translate to valid `RawModuleDefV10`.
#[derive(Debug, Clone)]
pub struct SchemaPlan {
    pub tables: Vec<TablePlan>,
}

impl SchemaPlan {
    pub fn ensure_engine_migration_coverage(&mut self) {
        self.ensure_widenable_sum_column();
        self.ensure_event_table();
        self.ensure_sequence_mutation_column();
        self.ensure_unique_constraint_mutation_column();
        self.ensure_standalone_index();
    }

    pub fn auto_inc_table_and_column(&self) -> Option<(usize, usize)> {
        self.tables
            .iter()
            .enumerate()
            .find_map(|(table_idx, table)| table.sequences.first().map(|sequence| (table_idx, sequence.column)))
    }

    pub fn ensure_auto_inc_table(&mut self) {
        if self.auto_inc_table_and_column().is_some() {
            return;
        }

        let table = self.tables.first_mut().expect("schema must contain at least one table");
        if table.columns.is_empty() {
            table.columns.push(ColumnPlan {
                name: "id".into(),
                ty: Type::U64,
            });
        } else {
            table.columns[0].ty = Type::U64;
        }

        table.primary_key = Some(0);
        if !table
            .unique_constraints
            .iter()
            .any(|constraint| constraint.columns == [0])
        {
            table.unique_constraints.push(UniqueConstraintPlan { columns: vec![0] });
        }
        if !table.indexes.iter().any(|index| index.columns == [0]) {
            table.indexes.push(IndexPlan {
                columns: vec![0],
                algorithm: IndexAlgorithm::BTree,
            });
        }
        table.sequences = vec![SequencePlan::new(0, Type::U64).expect("u64 is integral")];
    }

    fn ensure_widenable_sum_column(&mut self) {
        if self
            .tables
            .iter()
            .any(|table| !table.is_event && table.columns.iter().any(|column| matches!(column.ty, Type::Sum { .. })))
        {
            return;
        }

        let table = self
            .tables
            .iter_mut()
            .find(|table| !table.is_event)
            .expect("schema must contain at least one non-event table");
        table.columns.push(ColumnPlan {
            name: SchemaNames::fresh_column_name(table, "dst_sum"),
            ty: Type::Sum { variants: 1 },
        });
    }

    fn ensure_event_table(&mut self) {
        if self.tables.iter().any(|table| table.is_event) {
            return;
        }

        let name = SchemaNames::fresh_table_name(&self.tables, "dst_events");
        self.tables.push(TablePlan {
            name,
            columns: vec![ColumnPlan {
                name: "payload".into(),
                ty: Type::U64,
            }],
            primary_key: None,
            indexes: vec![],
            unique_constraints: vec![],
            sequences: vec![],
            is_public: true,
            is_event: true,
        });
    }

    fn ensure_sequence_mutation_column(&mut self) {
        if self.tables.iter().any(Self::has_sequence_mutation_column) {
            return;
        }

        let table = self
            .tables
            .iter_mut()
            .find(|table| !table.is_event)
            .expect("schema must contain at least one non-event table");
        let column = table.columns.len();
        table.columns.push(ColumnPlan {
            name: SchemaNames::fresh_column_name(table, "dst_seq"),
            ty: Type::U64,
        });
        table.indexes.push(IndexPlan {
            columns: vec![column],
            algorithm: IndexAlgorithm::BTree,
        });
        table
            .unique_constraints
            .push(UniqueConstraintPlan { columns: vec![column] });
    }

    fn ensure_unique_constraint_mutation_column(&mut self) {
        if self.tables.iter().any(|table| {
            !table.is_event
                && table.columns.iter().enumerate().any(|(column, _)| {
                    !table
                        .unique_constraints
                        .iter()
                        .any(|constraint| constraint.columns == [column])
                })
        }) {
            return;
        }

        let table = self
            .tables
            .iter_mut()
            .find(|table| !table.is_event)
            .expect("schema must contain at least one non-event table");
        table.columns.push(ColumnPlan {
            name: SchemaNames::fresh_column_name(table, "dst_unique"),
            ty: Type::U64,
        });
    }

    fn has_sequence_mutation_column(table: &TablePlan) -> bool {
        !table.is_event
            && table.columns.iter().enumerate().any(|(column, plan)| {
                plan.ty == Type::U64
                    && table.sequences.iter().all(|seq| seq.column != column)
                    && table.indexes.iter().any(|index| index.columns == [column])
                    && table
                        .unique_constraints
                        .iter()
                        .any(|constraint| constraint.columns == [column])
            })
    }

    fn ensure_standalone_index(&mut self) {
        if self.tables.iter().any(|table| {
            !table.is_event
                && table.indexes.iter().any(|index| {
                    !table.primary_key.is_some_and(|pk| index.columns == [pk])
                        && !table
                            .unique_constraints
                            .iter()
                            .any(|constraint| constraint.columns == index.columns)
                })
        }) {
            return;
        }

        let table = self
            .tables
            .iter_mut()
            .find(|table| !table.is_event)
            .expect("schema must contain at least one non-event table");
        let column = if table.columns.len() < 2 {
            table.columns.push(ColumnPlan {
                name: SchemaNames::fresh_column_name(table, "dst_indexed"),
                ty: Type::U64,
            });
            table.columns.len() - 1
        } else {
            (0..table.columns.len())
                .find(|&column| {
                    !table.primary_key.is_some_and(|pk| pk == column)
                        && !table
                            .unique_constraints
                            .iter()
                            .any(|constraint| constraint.columns == [column])
                })
                .unwrap_or(0)
        };

        if !table.indexes.iter().any(|index| index.columns == [column]) {
            table.indexes.push(IndexPlan {
                columns: vec![column],
                algorithm: IndexAlgorithm::BTree,
            });
        }
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
    pub is_public: bool,
    pub is_event: bool,
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

/// A sequence on a specific integral column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequencePlan {
    /// Index into `TablePlan.columns`.
    pub column: usize,
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
            start: None,
            min_value: None,
            max_value: None,
            increment: 1,
        })
    }

    pub fn with_bounds(
        column: usize,
        ty: Type,
        start: i128,
        min_value: i128,
        max_value: i128,
        increment: i128,
    ) -> Option<Self> {
        if !ty.is_integral() || increment == 0 || min_value >= max_value || start < min_value || start > max_value {
            return None;
        }
        Some(Self {
            column,
            start: Some(start),
            min_value: Some(min_value),
            max_value: Some(max_value),
            increment,
        })
    }

    pub fn with_existing_value_as_max(column: usize, ty: Type, existing_value: i128) -> Option<Self> {
        const DOMAIN_SIZE: i128 = 3;

        if existing_value < DOMAIN_SIZE {
            return None;
        }

        let min_value = existing_value - (DOMAIN_SIZE - 1);
        Self::with_bounds(column, ty, min_value, min_value, existing_value, 1)
    }
}

// Lowering into RawModuleDefV10.
pub fn to_raw_def(schema: &SchemaPlan) -> RawModuleDefV10 {
    let mut builder = RawModuleDefV10Builder::new();
    builder.set_case_conversion_policy(CaseConversionPolicy::None);

    for table in &schema.tables {
        to_raw_def_table(&mut builder, table);
    }

    let mut raw = builder.finish();
    apply_sequence_bounds(schema, &mut raw);
    raw
}

fn apply_sequence_bounds(schema: &SchemaPlan, raw: &mut RawModuleDefV10) {
    for (table_plan, raw_table) in schema.tables.iter().zip(raw.tables_mut_for_tests().iter_mut()) {
        for (sequence_plan, raw_sequence) in table_plan.sequences.iter().zip(raw_table.sequences.iter_mut()) {
            raw_sequence.start = sequence_plan.start;
            raw_sequence.min_value = sequence_plan.min_value;
            raw_sequence.max_value = sequence_plan.max_value;
            raw_sequence.increment = sequence_plan.increment;
        }
    }
}

fn to_raw_def_table(builder: &mut RawModuleDefV10Builder, table: &TablePlan) {
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

    let mut tbl = builder.build_table_with_new_type_for_tests(table.name.clone(), product_type, true);

    tbl = tbl.with_type(TableType::User);
    tbl = tbl.with_event(table.is_event);
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

        tbl = tbl.with_index_no_accessor_name(algorithm, SchemaNames::index_name(table, index));
    }

    // Sequences — all of them.
    for seq in &table.sequences {
        tbl = tbl.with_column_sequence(ColId(seq.column as u16));
    }

    // AddColumns needs defaults when existing rows are present. Supplying stable
    // defaults for all columns lets the engine keep only the newly-added tail.
    for (col_id, column) in table.columns.iter().enumerate() {
        tbl = tbl.with_default_column_value(ColId(col_id as u16), column.ty.default_value());
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

    fn gen_columns(&self) -> Vec<ColumnPlan> {
        let n = SchemaDecisions::range(&self.rng, self.profile.columns);
        let mut names = Vec::with_capacity(n);
        let mut seen = Vec::with_capacity(n);
        for _ in 0..n {
            let name = SchemaDecisions::gen_column_name(&self.rng, &seen);
            seen.push(name.clone());
            names.push(ColumnPlan {
                name,
                ty: SchemaDecisions::gen_type(&self.rng),
            });
        }
        names
    }

    fn gen_unique_constraints(&self, columns: &[ColumnPlan], pk: &Option<usize>) -> Vec<UniqueConstraintPlan> {
        let n = SchemaDecisions::range(&self.rng, self.profile.unique_constraints);
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut result = Vec::new();
        for _ in 0..n {
            let num_cols = 1 + SchemaDecisions::index(&self.rng, columns.len().min(3));
            let mut cols: Vec<usize> = (0..num_cols)
                .map(|_| SchemaDecisions::index(&self.rng, columns.len()))
                .collect();
            cols.sort();
            cols.dedup();
            if !cols.is_empty() && !seen.contains(&cols) {
                seen.push(cols.clone());
                result.push(UniqueConstraintPlan { columns: cols });
            }
        }
        // Ensure PK has a matching unique constraint.
        if let Some(pk) = pk {
            if !seen.iter().any(|cols| cols.len() == 1 && cols[0] == *pk) {
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
        let n = SchemaDecisions::range(&self.rng, self.profile.indexes);
        for _ in 0..n {
            let num_cols = 1 + SchemaDecisions::index(&self.rng, columns.len().min(3));
            let mut cols: Vec<usize> = (0..num_cols)
                .map(|_| SchemaDecisions::index(&self.rng, columns.len()))
                .collect();
            cols.sort();
            cols.dedup();
            if cols.is_empty() || seen_cols.contains(&cols) {
                continue;
            }
            seen_cols.push(cols.clone());
            let algorithm = if SchemaDecisions::sample_probability(&self.rng, self.profile.btree_prob) {
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

    fn gen_table(&self, existing_tables: &[TablePlan]) -> TablePlan {
        let columns = self.gen_columns();

        let primary_key = if SchemaDecisions::sample_probability(&self.rng, self.profile.pk_prob) && !columns.is_empty()
        {
            Some(SchemaDecisions::index(&self.rng, columns.len()))
        } else {
            None
        };

        let unique_constraints = self.gen_unique_constraints(&columns, &primary_key);

        let sequences = if let Some(pk) = primary_key {
            if columns[pk].ty.is_integral()
                && SchemaDecisions::sample_probability(&self.rng, self.profile.auto_inc_prob)
            {
                SequencePlan::new(pk, columns[pk].ty).into_iter().collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let indexes = self.gen_indexes(&columns, &unique_constraints, &primary_key);

        let name = SchemaDecisions::gen_table_name(&self.rng, existing_tables);

        TablePlan {
            name,
            columns,
            primary_key,
            indexes,
            unique_constraints,
            sequences,
            is_public: !SchemaDecisions::sample_probability(&self.rng, self.profile.private_prob),
            is_event: false,
        }
    }

    pub fn gen_schema(&self) -> SchemaPlan {
        let table_count = SchemaDecisions::range(&self.rng, self.profile.table_count);
        let mut tables = Vec::with_capacity(table_count);
        for _ in 0..table_count {
            tables.push(self.gen_table(&tables));
        }
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
                is_public: true,
                is_event: false,
            }],
        };

        let raw = to_raw_def(&schema);

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
