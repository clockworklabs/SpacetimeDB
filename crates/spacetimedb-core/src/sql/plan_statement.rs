use crate::{database_instance_context_controller::DatabaseInstanceContextController, sql::plan};
use sqlparser::ast::{Query, Select, SelectItem, SetExpr, Statement};

use super::plan::{Plan, PlanError, RelationExpr};

pub fn plan_statement(
    db_inst_ctx_controller: &DatabaseInstanceContextController,
    database_instance_id: u64,
    statement: Statement,
) -> Result<Plan, PlanError> {
    match statement {
        Statement::Analyze {
            table_name: _,
            partitions: _,
            for_columns: _,
            columns: _,
            cache_metadata: _,
            noscan: _,
            compute_statistics: _,
        } => Err(PlanError::Unsupported {
            feature: "Analyze".into(),
            issue_no: None,
        }),
        Statement::Truncate {
            table_name: _,
            partitions: _,
        } => Err(PlanError::Unsupported {
            feature: "Msck".into(),
            issue_no: None,
        }),
        Statement::Msck {
            table_name: _,
            repair: _,
            partition_action: _,
        } => Err(PlanError::Unsupported {
            feature: "Msck".into(),
            issue_no: None,
        }),
        Statement::Query(query) => plan_query(db_inst_ctx_controller, database_instance_id, *query),
        Statement::Insert {
            or: _,
            into: _,
            table_name: _,
            columns: _,
            overwrite: _,
            source: _,
            partitioned: _,
            after_columns: _,
            table: _,
            on: _,
        } => Err(PlanError::Unsupported {
            feature: "Insert".into(),
            issue_no: None,
        }),
        Statement::Directory {
            overwrite: _,
            local: _,
            path: _,
            file_format: _,
            source: _,
        } => Err(PlanError::Unsupported {
            feature: "Directory".into(),
            issue_no: None,
        }),
        Statement::Copy {
            table_name: _,
            columns: _,
            to: _,
            target: _,
            options: _,
            legacy_options: _,
            values: _,
        } => Err(PlanError::Unsupported {
            feature: "Copy".into(),
            issue_no: None,
        }),
        Statement::Close { cursor: _ } => Err(PlanError::Unsupported {
            feature: "Close".into(),
            issue_no: None,
        }),
        Statement::Update {
            table: _,
            assignments: _,
            from: _,
            selection: _,
        } => Err(PlanError::Unsupported {
            feature: "Update".into(),
            issue_no: None,
        }),
        Statement::Delete {
            table_name: _,
            using: _,
            selection: _,
        } => Err(PlanError::Unsupported {
            feature: "Delete".into(),
            issue_no: None,
        }),
        Statement::CreateView {
            or_replace: _,
            materialized: _,
            name: _,
            columns: _,
            query: _,
            with_options: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateView".into(),
            issue_no: None,
        }),
        Statement::CreateTable {
            or_replace: _,
            temporary: _,
            external: _,
            global: _,
            if_not_exists: _,
            name: _,
            columns: _,
            constraints: _,
            hive_distribution: _,
            hive_formats: _,
            table_properties: _,
            with_options: _,
            file_format: _,
            location: _,
            query: _,
            without_rowid: _,
            like: _,
            clone: _,
            engine: _,
            default_charset: _,
            collation: _,
            on_commit: _,
            on_cluster: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateTable".into(),
            issue_no: None,
        }),
        Statement::CreateVirtualTable {
            name: _,
            if_not_exists: _,
            module_name: _,
            module_args: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateVirtualTable".into(),
            issue_no: None,
        }),
        Statement::CreateIndex {
            name: _,
            table_name: _,
            columns: _,
            unique: _,
            if_not_exists: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateIndex".into(),
            issue_no: None,
        }),
        Statement::AlterTable { name: _, operation: _ } => Err(PlanError::Unsupported {
            feature: "AlterTable".into(),
            issue_no: None,
        }),
        Statement::Drop {
            object_type: _,
            if_exists: _,
            names: _,
            cascade: _,
            purge: _,
        } => Err(PlanError::Unsupported {
            feature: "Drop".into(),
            issue_no: None,
        }),
        Statement::Declare {
            name: _,
            binary: _,
            sensitive: _,
            scroll: _,
            hold: _,
            query: _,
        } => Err(PlanError::Unsupported {
            feature: "Declare".into(),
            issue_no: None,
        }),
        Statement::Fetch {
            name: _,
            direction: _,
            into: _,
        } => Err(PlanError::Unsupported {
            feature: "Fetch".into(),
            issue_no: None,
        }),
        Statement::Discard { object_type: _ } => Err(PlanError::Unsupported {
            feature: "Discard".into(),
            issue_no: None,
        }),
        Statement::SetRole {
            local: _,
            session: _,
            role_name: _,
        } => Err(PlanError::Unsupported {
            feature: "SetRole".into(),
            issue_no: None,
        }),
        Statement::SetVariable {
            local: _,
            hivevar: _,
            variable: _,
            value: _,
        } => Err(PlanError::Unsupported {
            feature: "SetVariable".into(),
            issue_no: None,
        }),
        Statement::SetNames {
            charset_name: _,
            collation_name: _,
        } => Err(PlanError::Unsupported {
            feature: "SetNames".into(),
            issue_no: None,
        }),
        Statement::SetNamesDefault {} => Err(PlanError::Unsupported {
            feature: "SetNamesDefault".into(),
            issue_no: None,
        }),
        Statement::ShowVariable { variable: _ } => Err(PlanError::Unsupported {
            feature: "ShowVariable".into(),
            issue_no: None,
        }),
        Statement::ShowVariables { filter: _ } => Err(PlanError::Unsupported {
            feature: "ShowVariables".into(),
            issue_no: None,
        }),
        Statement::ShowCreate {
            obj_type: _,
            obj_name: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowCreate".into(),
            issue_no: None,
        }),
        Statement::ShowColumns {
            extended: _,
            full: _,
            table_name: _,
            filter: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowColumns".into(),
            issue_no: None,
        }),
        Statement::ShowTables {
            extended: _,
            full: _,
            db_name: _,
            filter: _,
        } => Err(PlanError::Unsupported {
            feature: "ShowTables".into(),
            issue_no: None,
        }),
        Statement::ShowCollation { filter: _ } => Err(PlanError::Unsupported {
            feature: "ShowCollation".into(),
            issue_no: None,
        }),
        Statement::Use { db_name: _ } => Err(PlanError::Unsupported {
            feature: "Use".into(),
            issue_no: None,
        }),
        Statement::StartTransaction { modes: _ } => Err(PlanError::Unsupported {
            feature: "StartTransaction".into(),
            issue_no: None,
        }),
        Statement::SetTransaction {
            modes: _,
            snapshot: _,
            session: _,
        } => Err(PlanError::Unsupported {
            feature: "SetTransaction".into(),
            issue_no: None,
        }),
        Statement::Comment {
            object_type: _,
            object_name: _,
            comment: _,
        } => Err(PlanError::Unsupported {
            feature: "Comment".into(),
            issue_no: None,
        }),
        Statement::Commit { chain: _ } => Err(PlanError::Unsupported {
            feature: "Commit".into(),
            issue_no: None,
        }),
        Statement::Rollback { chain: _ } => Err(PlanError::Unsupported {
            feature: "Rollback".into(),
            issue_no: None,
        }),
        Statement::CreateSchema {
            schema_name: _,
            if_not_exists: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateSchema".into(),
            issue_no: None,
        }),
        Statement::CreateDatabase {
            db_name: _,
            if_not_exists: _,
            location: _,
            managed_location: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateDatabase".into(),
            issue_no: None,
        }),
        Statement::CreateFunction {
            temporary: _,
            name: _,
            class_name: _,
            using: _,
        } => Err(PlanError::Unsupported {
            feature: "CreateFunction".into(),
            issue_no: None,
        }),
        Statement::Assert {
            condition: _,
            message: _,
        } => Err(PlanError::Unsupported {
            feature: "Assert".into(),
            issue_no: None,
        }),
        Statement::Grant {
            privileges: _,
            objects: _,
            grantees: _,
            with_grant_option: _,
            granted_by: _,
        } => Err(PlanError::Unsupported {
            feature: "Grant".into(),
            issue_no: None,
        }),
        Statement::Revoke {
            privileges: _,
            objects: _,
            grantees: _,
            granted_by: _,
            cascade: _,
        } => Err(PlanError::Unsupported {
            feature: "Revoke".into(),
            issue_no: None,
        }),
        Statement::Deallocate { name: _, prepare: _ } => Err(PlanError::Unsupported {
            feature: "Deallocate".into(),
            issue_no: None,
        }),
        Statement::Execute { name: _, parameters: _ } => Err(PlanError::Unsupported {
            feature: "Execute".into(),
            issue_no: None,
        }),
        Statement::Prepare {
            name: _,
            data_types: _,
            statement: _,
        } => Err(PlanError::Unsupported {
            feature: "Prepare".into(),
            issue_no: None,
        }),
        Statement::Kill { modifier: _, id: _ } => Err(PlanError::Unsupported {
            feature: "Kill".into(),
            issue_no: None,
        }),
        Statement::ExplainTable {
            describe_alias: _,
            table_name: _,
        } => Err(PlanError::Unsupported {
            feature: "ExplainTable".into(),
            issue_no: None,
        }),
        Statement::Explain {
            describe_alias: _,
            analyze: _,
            verbose: _,
            statement: _,
        } => Err(PlanError::Unsupported {
            feature: "Explain".into(),
            issue_no: None,
        }),
        Statement::Savepoint { name: _ } => Err(PlanError::Unsupported {
            feature: "Savepoint".into(),
            issue_no: None,
        }),
        Statement::Merge {
            into: _,
            table: _,
            source: _,
            on: _,
            clauses: _,
        } => Err(PlanError::Unsupported {
            feature: "Merge".into(),
            issue_no: None,
        }),
    }
}

fn plan_query(
    db_inst_ctx_controller: &DatabaseInstanceContextController,
    database_instance_id: u64,
    query: Query,
) -> Result<Plan, PlanError> {
    match *query.body {
        SetExpr::Select(select) => Ok(Plan::Query(plan::QueryPlan {
            source: plan_select(db_inst_ctx_controller, database_instance_id, *select)?,
        })),
        SetExpr::Query(_) => Err(PlanError::Unsupported {
            feature: "Query".into(),
            issue_no: None,
        }),
        SetExpr::SetOperation {
            op: _,
            all: _,
            left: _,
            right: _,
        } => Err(PlanError::Unsupported {
            feature: "SetOperation".into(),
            issue_no: None,
        }),
        SetExpr::Values(_) => Err(PlanError::Unsupported {
            feature: "Values".into(),
            issue_no: None,
        }),
        SetExpr::Insert(_) => Err(PlanError::Unsupported {
            feature: "SetExpr::Insert".into(),
            issue_no: None,
        }),
    }
}

fn plan_select(
    db_inst_ctx_controller: &DatabaseInstanceContextController,
    database_instance_id: u64,
    select: Select,
) -> Result<RelationExpr, PlanError> {
    if select.from.len() > 1 {
        return Err(PlanError::Unsupported {
            feature: "Multiple table from expressions.".into(),
            issue_no: None,
        });
    }

    let table_with_joins = match select.from.first() {
        Some(table_with_joins) => table_with_joins,
        None => {
            return Err(PlanError::Unstructured("Missing from expression.".into()));
        }
    };

    let table_name = match &table_with_joins.relation {
        sqlparser::ast::TableFactor::Table {
            name,
            alias: _,
            args: _,
            with_hints: _,
        } => match name.0.first() {
            Some(ident) => ident.value.clone(),
            None => {
                return Err(PlanError::Unstructured("Missing table name.".into()));
            }
        },
        sqlparser::ast::TableFactor::Derived {
            lateral: _,
            subquery: _,
            alias: _,
        } => {
            return Err(PlanError::Unsupported {
                feature: "Derived".into(),
                issue_no: None,
            });
        }
        sqlparser::ast::TableFactor::TableFunction { expr: _, alias: _ } => {
            return Err(PlanError::Unsupported {
                feature: "TableFunction".into(),
                issue_no: None,
            });
        }
        sqlparser::ast::TableFactor::UNNEST {
            alias: _,
            array_expr: _,
            with_offset: _,
            with_offset_alias: _,
        } => {
            return Err(PlanError::Unsupported {
                feature: "UNNEST".into(),
                issue_no: None,
            });
        }
        sqlparser::ast::TableFactor::NestedJoin {
            table_with_joins: _,
            alias: _,
        } => {
            return Err(PlanError::Unsupported {
                feature: "NestedJoin".into(),
                issue_no: None,
            });
        }
    };

    let database_instance_context = db_inst_ctx_controller.get(database_instance_id).unwrap();
    let mut db = database_instance_context.relational_db.lock().unwrap();
    let mut tx = db.begin_tx();
    let table_id = match tx.with(|tx, db| db.table_id_from_name(tx, &table_name)) {
        Ok(table_id) => table_id,
        Err(err) => return Err(PlanError::DatabaseInternal(err)),
    };
    let table_id = match table_id {
        Some(t) => t,
        None => return Err(PlanError::UnknownTable { table: table_name }),
    };
    tx.rollback();

    let mut col_ids = Vec::new();
    for select_item in select.projection {
        match select_item {
            SelectItem::UnnamedExpr(expr) => match expr {
                sqlparser::ast::Expr::Identifier(ident) => {
                    let col_name = ident.to_string();
                    let mut tx = db.begin_tx();
                    let col_id = tx.with(|tx, db| db.column_id_from_name(tx, table_id, &col_name).unwrap());
                    tx.rollback();
                    let col_id = if let Some(col_id) = col_id {
                        col_id
                    } else {
                        return Err(PlanError::UnknownColumn {
                            table: Some(table_name),
                            column: col_name,
                        });
                    };
                    col_ids.push(col_id);
                }
                _ => {
                    return Err(PlanError::Unsupported {
                        feature: "Only identifier columns are supported.".into(),
                        issue_no: None,
                    })
                }
            },
            SelectItem::ExprWithAlias { expr: _, alias: _ } => {
                return Err(PlanError::Unsupported {
                    feature: "ExprWithAlias".into(),
                    issue_no: None,
                })
            }
            SelectItem::QualifiedWildcard(_) => {
                return Err(PlanError::Unsupported {
                    feature: "QualifiedWildcard".into(),
                    issue_no: None,
                })
            }
            SelectItem::Wildcard => {
                return Ok(RelationExpr::GetTable { table_id });
            }
        }
    }

    Ok(RelationExpr::Project {
        input: Box::new(RelationExpr::GetTable { table_id }),
        col_ids,
    })
}
