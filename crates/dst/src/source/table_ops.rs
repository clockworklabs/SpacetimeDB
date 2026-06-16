use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_runtime::sim::Rng;

use super::schema::{SchemaPlan, TablePlan, Type};

/// A row is a product value aligned to a table's columns.
pub type Row = ProductValue;

/// A single interaction against the database.
#[derive(Debug, Clone)]
pub enum Interaction {
    Insert { table: usize, row: Row },
    Delete { table: usize, row_index: usize },
    Count { table: usize },
}

/// Observation returned by executing an interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Inserted { count_after: u64 },
    Deleted { count_after: u64 },
    Counted { count: u64 },
}

/// Model stores all rows per table — the ground truth.
#[derive(Debug)]
pub struct Model {
    rows: Vec<Vec<Row>>,
}

impl Model {
    pub fn new(schema: &SchemaPlan) -> Self {
        Self {
            rows: schema.tables.iter().map(|_| Vec::new()).collect(),
        }
    }

    pub fn apply(&mut self, interaction: &Interaction) -> Observation {
        match interaction {
            Interaction::Insert { table, row } => {
                self.rows[*table].push(row.clone());
                Observation::Inserted {
                    count_after: self.rows[*table].len() as u64,
                }
            }
            Interaction::Delete { table, row_index } => {
                if *row_index < self.rows[*table].len() {
                    self.rows[*table].remove(*row_index);
                }
                Observation::Deleted {
                    count_after: self.rows[*table].len() as u64,
                }
            }
            Interaction::Count { table } => Observation::Counted {
                count: self.rows[*table].len() as u64,
            },
        }
    }

    pub fn row_count(&self, table: usize) -> u64 {
        self.rows[table].len() as u64
    }

    pub fn rows(&self, table: usize) -> &[Row] {
        &self.rows[table]
    }
}

/// Generates random interactions from a schema plan.
pub struct InteractionGen<'a> {
    rng: &'a Rng,
    schema: &'a SchemaPlan,
}

impl<'a> InteractionGen<'a> {
    pub fn new(rng: &'a Rng, schema: &'a SchemaPlan) -> Self {
        Self { rng, schema }
    }

    fn gen_value(&self, ty: Type) -> AlgebraicValue {
        match ty {
            Type::Bool => AlgebraicValue::Bool(self.rng.next_u64() % 2 == 0),
            Type::I64 => AlgebraicValue::I64(self.rng.next_u64() as i64),
            Type::U64 => AlgebraicValue::U64(self.rng.next_u64()),
            Type::String => {
                AlgebraicValue::String(format!("v_{}", self.rng.next_u64()).into())
            }
            Type::Bytes => {
                let len = (self.rng.next_u64() % 16) as usize;
                let bytes: Vec<u8> = (0..len).map(|_| self.rng.next_u64() as u8).collect();
                AlgebraicValue::Array(ArrayValue::U8(bytes.into()))
            }
        }
    }

    fn gen_row(&self, table: &TablePlan) -> Row {
        table
            .columns
            .iter()
            .map(|c| self.gen_value(c.ty))
            .collect::<ProductValue>()
    }

    /// Generate an interaction given the current model state.
    pub fn next_interaction(&self, model: &Model) -> Interaction {
        let table_idx = self.rng.index(self.schema.tables.len());

        // ~60% insert, ~20% delete (if rows exist), ~20% count
        let coin = self.rng.next_u64() % 10;
        if coin < 6 {
            Interaction::Insert {
                table: table_idx,
                row: self.gen_row(&self.schema.tables[table_idx]),
            }
        } else if coin < 8 && !model.rows(table_idx).is_empty() {
            let row_index = self.rng.index(model.rows(table_idx).len());
            Interaction::Delete {
                table: table_idx,
                row_index,
            }
        } else {
            Interaction::Count { table: table_idx }
        }
    }
}

use spacetimedb_sats::ArrayValue;

/// Serialize a row to BSATN bytes for the engine insert API.
pub fn row_to_bytes(row: &Row) -> Vec<u8> {
    to_vec(row).expect("row serialization must not fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tracks_rows() {
        let schema = SchemaPlan {
            tables: vec![TablePlan {
                name: "t".into(),
                columns: vec![],
                primary_key: None,
                indexes: vec![],
                unique_constraints: vec![],
                sequences: vec![],
                default_values: vec![],
                is_event: false,
                is_public: true,
            }],
        };
        let mut model = Model::new(&schema);

        let obs = model.apply(&Interaction::Insert {
            table: 0,
            row: ProductValue::default(),
        });
        assert_eq!(obs, Observation::Inserted { count_after: 1 });
        assert_eq!(model.row_count(0), 1);

        let obs = model.apply(&Interaction::Delete {
            table: 0,
            row_index: 0,
        });
        assert_eq!(obs, Observation::Deleted { count_after: 0 });
        assert_eq!(model.row_count(0), 0);
    }

    #[test]
    fn gen_produces_valid_interactions() {
        use spacetimedb_runtime::sim::Rng;
        let rng = Rng::new(42);
        let schema = super::super::schema_gen::SchemaGenerator::new(
            &rng,
            super::super::schema_gen::SchemaProfile::default(),
        )
        .gen_schema();
        let model = Model::new(&schema);
        let source = InteractionGen::new(&rng, &schema);
        for _ in 0..100 {
            let ix = source.next_interaction(&model);
            match ix {
                Interaction::Insert { table, .. } => assert!(table < schema.tables.len()),
                Interaction::Delete { table, .. } => assert!(table < schema.tables.len()),
                Interaction::Count { table } => assert!(table < schema.tables.len()),
            }
        }
    }
}
