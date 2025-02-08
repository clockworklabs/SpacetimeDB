use anyhow::Result;
use spacetimedb_lib::{metrics::ExecutionMetrics, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::dml::{DeletePlan, InsertPlan, MutationPlan, UpdatePlan};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::size_of::SizeOf;

use crate::{pipelined::PipelinedProject, Datastore, DeltaStore};

/// A mutable datastore can read as well as insert and delete rows
pub trait MutDatastore: Datastore + DeltaStore {
    fn insert_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<()>;
    fn delete_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<()>;
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
    pub fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
        match self {
            Self::Insert(exec) => exec.execute(tx, metrics),
            Self::Delete(exec) => exec.execute(tx, metrics),
            Self::Update(exec) => exec.execute(tx, metrics),
        }
    }
}

/// Executes row insertions
pub struct InsertExecutor {
    table_id: TableId,
    rows: Vec<ProductValue>,
}

impl From<InsertPlan> for InsertExecutor {
    fn from(plan: InsertPlan) -> Self {
        Self {
            rows: plan.rows,
            table_id: plan.table.table_id,
        }
    }
}

impl InsertExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
        for row in &self.rows {
            tx.insert_product_value(self.table_id, row)?;
        }
        // TODO: It would be better to get this metric from the bsatn buffer.
        // But we haven't been concerned with optimizing DML up to this point.
        metrics.bytes_written += self.rows.iter().map(|row| row.size_of()).sum::<usize>();
        Ok(())
    }
}

/// Executes row deletions
pub struct DeleteExecutor {
    table_id: TableId,
    filter: PipelinedProject,
}

impl From<DeletePlan> for DeleteExecutor {
    fn from(plan: DeletePlan) -> Self {
        Self {
            table_id: plan.table.table_id,
            filter: plan.filter.into(),
        }
    }
}

impl DeleteExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
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
        for row in &deletes {
            tx.delete_product_value(self.table_id, row)?;
        }
        Ok(())
    }
}

/// Executes row updates
pub struct UpdateExecutor {
    table_id: TableId,
    columns: Vec<(ColId, AlgebraicValue)>,
    filter: PipelinedProject,
}

impl From<UpdatePlan> for UpdateExecutor {
    fn from(plan: UpdatePlan) -> Self {
        Self {
            columns: plan.columns,
            table_id: plan.table.table_id,
            filter: plan.filter.into(),
        }
    }
}

impl UpdateExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
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
        }
        Ok(())
    }
}
