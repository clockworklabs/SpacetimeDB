use anyhow::Result;
use spacetimedb_lib::{metrics::ExecutionMetrics, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::{dml::{DeletePlan, InsertPlan, MutationPlan, UpdatePlan}, plan::{ProjectField, ProjectListPlan}};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::size_of::SizeOf;

use crate::{pipelined::PipelinedProject, Datastore, DeltaStore, Row};

/// A mutable datastore can read as well as insert and delete rows
pub trait MutDatastore: Datastore + DeltaStore {
    fn insert_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<bool>;
    fn delete_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<bool>;
}

/// Executes a physical mutation plan
pub enum MutExecutor {
    Insert(InsertExecutor),
    Delete(DeleteExecutor),
    Update(UpdateExecutor),
}

impl From<MutationPlan> for MutExecutor {
    fn from(plan: MutationPlan) -> Self {
        match plan {
            MutationPlan::Insert(plan) => Self::Insert(plan.into()),
            MutationPlan::Delete(plan) => Self::Delete(plan.into()),
            MutationPlan::Update(plan) => Self::Update(plan.into()),
        }
    }
}

impl MutExecutor {
    pub fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<Vec<ProductValue>> {
        match self {
            Self::Insert(exec) => exec.execute(tx, metrics),
            Self::Delete(exec) => exec.execute(tx, metrics),
            Self::Update(exec) => exec.execute(tx, metrics),
        }
    }
}

fn project_returning_row(returning: &ProjectListPlan, row: ProductValue) -> Option<ProductValue> {
    match returning {
        ProjectListPlan::Name(_) => {
            Some(row)
        }
        ProjectListPlan::List(_, fields) => {
            let row = Row::Ref(&row);
            Some(ProductValue::from_iter(fields.iter().map(|field| row.project(field))))
        }
        _ => None
    }
}

/// Executes row insertions
pub struct InsertExecutor {
    table_id: TableId,
    rows: Vec<ProductValue>,
    returning: Option<ProjectListPlan>,
}

impl From<InsertPlan> for InsertExecutor {
    fn from(plan: InsertPlan) -> Self {
        Self {
            table_id: plan.table.table_id,
            rows: plan.rows,
            returning: plan.returning,
        }
    }
}

impl InsertExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<Vec<ProductValue>> {
        let mut results = vec![];
        for row in &self.rows {
            if tx.insert_product_value(self.table_id, row)? {
                metrics.rows_inserted += 1;
                if let Some(returning) = &self.returning {
                    project_returning_row(returning, row.clone()).map(|res| results.push(res));
                }
            }
        }
        // TODO: It would be better to get this metric from the bsatn buffer.
        // But we haven't been concerned with optimizing DML up to this point.
        metrics.bytes_written += self.rows.iter().map(|row| row.size_of()).sum::<usize>();
        Ok(results)
    }
}

/// Executes row deletions
pub struct DeleteExecutor {
    table_id: TableId,
    filter: PipelinedProject,
    returning: Option<ProjectListPlan>,
}

impl From<DeletePlan> for DeleteExecutor {
    fn from(plan: DeletePlan) -> Self {
        Self {
            table_id: plan.table.table_id,
            filter: plan.filter.into(),
            returning: plan.returning,
        }
    }
}

impl DeleteExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<Vec<ProductValue>> {
        // TODO: Delete by row id instead of product value
        let mut deletes = vec![];
        self.filter.execute(tx, metrics, &mut |row| {
            deletes.push(row.to_product_value());
            Ok(())
        })?;
        // TODO: This metric should be updated inline when we serialize.
        // Note, that we don't update bytes written,
        // because deletes don't actually write out any bytes.
        metrics.bytes_scanned += deletes.iter().map(|row| row.size_of()).sum::<usize>();
        let mut results = vec![];
        for row in &deletes {
            if tx.delete_product_value(self.table_id, row)? {
                metrics.rows_deleted += 1;
                if let Some(returning) = &self.returning {
                    project_returning_row(returning, row.clone()).map(|res| results.push(res));
                }
            }
        }
        Ok(results)
    }
}

/// Executes row updates
pub struct UpdateExecutor {
    table_id: TableId,
    columns: Vec<(ColId, AlgebraicValue)>,
    filter: PipelinedProject,
    returning: Option<ProjectListPlan>,
}

impl From<UpdatePlan> for UpdateExecutor {
    fn from(plan: UpdatePlan) -> Self {
        Self {
            columns: plan.columns,
            table_id: plan.table.table_id,
            filter: plan.filter.into(),
            returning: plan.returning,
        }
    }
}

impl UpdateExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<Vec<ProductValue>> {
        let mut deletes = vec![];
        self.filter.execute(tx, metrics, &mut |row| {
            deletes.push(row.to_product_value());
            Ok(())
        })?;
        for row in &deletes {
            tx.delete_product_value(self.table_id, row)?;
        }
        // TODO: This metric should be updated inline when we serialize.
        metrics.bytes_scanned = deletes.iter().map(|row| row.size_of()).sum::<usize>();
        metrics.rows_updated += deletes.len() as u64;
        let mut results = vec![];
        for row in &deletes {
            let row = ProductValue::from_iter(
                row
                    // Update the deleted rows with the new field values
                    .into_iter()
                    .cloned()
                    .enumerate()
                    .map(|(i, elem)| {
                        self.columns
                            .iter()
                            .find(|(col_id, _)| i == col_id.idx())
                            .map(|(_, value)| value.clone())
                            .unwrap_or_else(|| elem)
                    }),
            );
            tx.insert_product_value(self.table_id, &row)?;
            metrics.bytes_written += row.size_of();
            if let Some(returning) = &self.returning {
                project_returning_row(returning, row).map(|res| results.push(res));
            }
        }
        Ok(results)
    }
}
