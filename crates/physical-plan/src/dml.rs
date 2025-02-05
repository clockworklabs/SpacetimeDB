use std::sync::Arc;

use anyhow::Result;
use spacetimedb_expr::{
    expr::{ProjectName, RelExpr, Relvar},
    statement::{TableDelete, TableInsert, TableUpdate},
};
use spacetimedb_lib::{AlgebraicValue, ProductValue};
use spacetimedb_primitives::ColId;
use spacetimedb_schema::schema::TableSchema;

use crate::{compile::compile_select, plan::ProjectPlan};

/// A plan for mutating a table in the database
#[derive(Debug)]
pub enum MutationPlan {
    Insert(InsertPlan),
    Delete(DeletePlan),
    Update(UpdatePlan),
}

impl MutationPlan {
    /// Optimizes the filters in updates and deletes
    pub fn optimize(self) -> Result<Self> {
        match self {
            Self::Insert(..) => Ok(self),
            Self::Delete(plan) => Ok(Self::Delete(plan.optimize()?)),
            Self::Update(plan) => Ok(Self::Update(plan.optimize()?)),
        }
    }
}

/// A plan for inserting rows into a table
#[derive(Debug)]
pub struct InsertPlan {
    pub table: Arc<TableSchema>,
    pub rows: Vec<ProductValue>,
}

impl From<TableInsert> for InsertPlan {
    fn from(insert: TableInsert) -> Self {
        let TableInsert { table, rows } = insert;
        let rows = rows.into_vec();
        Self { table, rows }
    }
}

/// A plan for deleting rows from a table
#[derive(Debug)]
pub struct DeletePlan {
    pub table: Arc<TableSchema>,
    pub filter: ProjectPlan,
}

impl DeletePlan {
    /// Optimize the filter part of the delete
    fn optimize(self) -> Result<Self> {
        let Self { table, filter } = self;
        let filter = filter.optimize()?;
        Ok(Self { table, filter })
    }

    /// Logical to physical conversion
    pub(crate) fn compile(delete: TableDelete) -> Self {
        let TableDelete { table, filter } = delete;
        let schema = table.clone();
        let alias = table.table_name.clone();
        let relvar = RelExpr::RelVar(Relvar {
            schema,
            alias,
            delta: None,
        });
        let project = match filter {
            None => ProjectName::None(relvar),
            Some(expr) => ProjectName::None(RelExpr::Select(Box::new(relvar), expr)),
        };
        let filter = compile_select(project);
        Self { table, filter }
    }
}

/// A plan for updating rows in a table
#[derive(Debug)]
pub struct UpdatePlan {
    pub table: Arc<TableSchema>,
    pub columns: Vec<(ColId, AlgebraicValue)>,
    pub filter: ProjectPlan,
}

impl UpdatePlan {
    /// Optimize the filter part of the update
    fn optimize(self) -> Result<Self> {
        let Self { table, columns, filter } = self;
        let filter = filter.optimize()?;
        Ok(Self { columns, table, filter })
    }

    /// Logical to physical conversion
    pub(crate) fn compile(update: TableUpdate) -> Self {
        let TableUpdate { table, columns, filter } = update;
        let schema = table.clone();
        let alias = table.table_name.clone();
        let relvar = RelExpr::RelVar(Relvar {
            schema,
            alias,
            delta: None,
        });
        let project = match filter {
            None => ProjectName::None(relvar),
            Some(expr) => ProjectName::None(RelExpr::Select(Box::new(relvar), expr)),
        };
        let filter = compile_select(project);
        let columns = columns.into_vec();
        Self { columns, table, filter }
    }
}
