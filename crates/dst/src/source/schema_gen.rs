//! Seed-based random schema generation.

use spacetimedb_runtime::sim::Rng;

use super::schema::*;

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
    pub event_prob: f64,
    pub private_prob: f64,
}

impl Default for SchemaProfile {
    fn default() -> Self {
        Self {
            table_count: (2, 5),
            columns: (1, 8),
            pk_prob: 0.7,
            auto_inc_prob: 0.8,
            indexes: (0, 3),
            unique_constraints: (0, 2),
            btree_prob: 0.7,
            event_prob: 0.1,
            private_prob: 0.1,
        }
    }
}

pub struct SchemaGenerator<'a> {
    rng: &'a Rng,
    profile: SchemaProfile,
}

impl<'a> SchemaGenerator<'a> {
    pub fn new(rng: &'a Rng, profile: SchemaProfile) -> Self {
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
        (0..n)
            .map(|i| ColumnPlan {
                name: format!("col_{i}"),
                ty: self.gen_type(),
            })
            .collect()
    }

    fn gen_unique_constraints(
        &self,
        columns: &[ColumnPlan],
        pk: &Option<usize>,
    ) -> Vec<UniqueConstraintPlan> {
        let n = self.range(self.profile.unique_constraints);
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut result = Vec::new();
        for _ in 0..n {
            let num_cols = 1 + self.rng.index(columns.len().min(3));
            let mut cols: Vec<usize> = (0..num_cols)
                .map(|_| self.rng.index(columns.len()))
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
            if !seen.contains(&vec![*pk]) {
                result.push(UniqueConstraintPlan {
                    columns: vec![*pk],
                });
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
            let mut cols: Vec<usize> = (0..num_cols)
                .map(|_| self.rng.index(columns.len()))
                .collect();
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

        let primary_key = if self.rng.sample_probability(self.profile.pk_prob) && !columns.is_empty()
        {
            Some(self.rng.index(columns.len()))
        } else {
            None
        };

        let unique_constraints = self.gen_unique_constraints(&columns, &primary_key);

        let sequences = if let Some(pk) = primary_key {
            if columns[pk].ty.is_integral()
                && self.rng.sample_probability(self.profile.auto_inc_prob)
            {
                SequencePlan::new(pk, columns[pk].ty).into_iter().collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let indexes = self.gen_indexes(&columns, &unique_constraints, &primary_key);

        // Generate a name from the RNG so different seeds produce different names.
        let name = format!("tbl_{}", self.rng.next_u64());

        TablePlan {
            name,
            columns,
            primary_key,
            indexes,
            unique_constraints,
            sequences,
            default_values: vec![],
            is_event: self.rng.sample_probability(self.profile.event_prob),
            is_public: !self.rng.sample_probability(self.profile.private_prob),
        }
    }

    pub fn gen_schema(&self) -> SchemaPlan {
        let table_count = self.range(self.profile.table_count);
        let tables = (0..table_count)
            .map(|i| self.gen_table(i))
            .collect();
        SchemaPlan { tables }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_runtime::sim::Rng;

    #[test]
    fn deterministic_from_seed() {
        let rng1 = Rng::new(42);
        let rng2 = Rng::new(42);
        let s1 = SchemaGenerator::new(&rng1, SchemaProfile::default()).gen_schema();
        let s2 = SchemaGenerator::new(&rng2, SchemaProfile::default()).gen_schema();
        assert_eq!(s1.tables.len(), s2.tables.len());
        for (a, b) in s1.tables.iter().zip(s2.tables.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.columns.len(), b.columns.len());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let rng1 = Rng::new(1);
        let rng2 = Rng::new(2);
        let s1 = SchemaGenerator::new(&rng1, SchemaProfile::default()).gen_schema();
        let s2 = SchemaGenerator::new(&rng2, SchemaProfile::default()).gen_schema();
        // At least one table name should differ.
        assert_ne!(s1.tables[0].name, s2.tables[0].name);
    }
}
