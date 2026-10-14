use anyhow::{bail, Result};
use spacetimedb_lib::{metrics::ExecutionMetrics, AlgebraicValue, ProductValue};
use spacetimedb_physical_plan::dml::{DeletePlan, InsertPlan, MutationPlan, UpdatePlan};
use spacetimedb_physical_plan::plan::ParamResolver;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::size_of::SizeOf;

use crate::{pipelined::PipelinedProject, Datastore, DeltaStore};

/// A mutable datastore can read as well as insert and delete rows
pub trait MutDatastore: Datastore + DeltaStore {
    fn insert_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<bool>;
    fn delete_product_value(&mut self, table_id: TableId, row: &ProductValue) -> Result<bool>;
}

#[derive(Clone, Copy)]
struct SpecialTableDml {
    is_scheduled_table: bool,
    is_outbox_table: bool,
}

impl SpecialTableDml {
    fn new(is_scheduled_table: bool, is_outbox_table: bool) -> Self {
        Self {
            is_scheduled_table,
            is_outbox_table,
        }
    }

    fn reject_sql_dml(self) -> Result<()> {
        if self.is_scheduled_table {
            bail!("SQL writes to scheduled tables are not supported");
        }
        if self.is_outbox_table {
            bail!("SQL writes to outbox tables are not supported");
        }
        Ok(())
    }
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
    pub fn execute<Tx: MutDatastore>(
        &self,
        tx: &mut Tx,
        params: &impl ParamResolver,
        metrics: &mut ExecutionMetrics,
    ) -> Result<()> {
        match self {
            Self::Insert(exec) => exec.execute(tx, metrics),
            Self::Delete(exec) => exec.execute(tx, params, metrics),
            Self::Update(exec) => exec.execute(tx, params, metrics),
        }
    }
}

/// Executes row insertions
pub struct InsertExecutor {
    table_id: TableId,
    special_table: SpecialTableDml,
    rows: Vec<ProductValue>,
}

impl From<InsertPlan> for InsertExecutor {
    fn from(plan: InsertPlan) -> Self {
        let table = plan.table.inner();
        Self {
            rows: plan.rows,
            table_id: plan.table.table_id,
            special_table: SpecialTableDml::new(table.schedule.is_some(), table.outbox.is_some()),
        }
    }
}

impl InsertExecutor {
    fn execute<Tx: MutDatastore>(&self, tx: &mut Tx, metrics: &mut ExecutionMetrics) -> Result<()> {
        self.special_table.reject_sql_dml()?;
        for row in &self.rows {
            if tx.insert_product_value(self.table_id, row)? {
                metrics.rows_inserted += 1;
            }
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
    special_table: SpecialTableDml,
    filter: PipelinedProject,
}

impl From<DeletePlan> for DeleteExecutor {
    fn from(plan: DeletePlan) -> Self {
        let table = plan.table.inner();
        Self {
            table_id: plan.table.table_id,
            special_table: SpecialTableDml::new(table.schedule.is_some(), table.outbox.is_some()),
            filter: plan.filter.into(),
        }
    }
}

impl DeleteExecutor {
    fn execute<Tx: MutDatastore>(
        &self,
        tx: &mut Tx,
        params: &impl ParamResolver,
        metrics: &mut ExecutionMetrics,
    ) -> Result<()> {
        self.special_table.reject_sql_dml()?;
        // TODO: Delete by row id instead of product value
        let mut deletes = vec![];
        self.filter.execute(tx, params, metrics, &mut |row| {
            deletes.push(row.to_product_value());
            Ok(())
        })?;
        // TODO: This metric should be updated inline when we serialize.
        // Note, that we don't update bytes written,
        // because deletes don't actually write out any bytes.
        metrics.bytes_scanned += deletes.iter().map(|row| row.size_of()).sum::<usize>();
        for row in &deletes {
            if tx.delete_product_value(self.table_id, row)? {
                metrics.rows_deleted += 1;
            }
        }
        Ok(())
    }
}

/// Executes row updates
pub struct UpdateExecutor {
    table_id: TableId,
    special_table: SpecialTableDml,
    columns: Vec<(ColId, AlgebraicValue)>,
    filter: PipelinedProject,
}

impl From<UpdatePlan> for UpdateExecutor {
    fn from(plan: UpdatePlan) -> Self {
        let table = plan.table.inner();
        Self {
            columns: plan.columns,
            table_id: plan.table.table_id,
            special_table: SpecialTableDml::new(table.schedule.is_some(), table.outbox.is_some()),
            filter: plan.filter.into(),
        }
    }
}

impl UpdateExecutor {
    fn execute<Tx: MutDatastore>(
        &self,
        tx: &mut Tx,
        params: &impl ParamResolver,
        metrics: &mut ExecutionMetrics,
    ) -> Result<()> {
        self.special_table.reject_sql_dml()?;
        let mut deletes = vec![];
        self.filter.execute(tx, params, metrics, &mut |row| {
            deletes.push(row.to_product_value());
            Ok(())
        })?;
        for row in &deletes {
            tx.delete_product_value(self.table_id, row)?;
        }
        // TODO: This metric should be updated inline when we serialize.
        metrics.bytes_scanned = deletes.iter().map(|row| row.size_of()).sum::<usize>();
        metrics.rows_updated += deletes.len() as u64;
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
