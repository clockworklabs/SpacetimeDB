use crate::error::PlanError;
use crate::sql::ast::{From, Join};
use crate::sql::ast::{Selection, SqlAst};
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_sats::db::def::ColumnSchema;
use spacetimedb_sats::relation::FieldName;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_vm::errors::ErrorType;
use spacetimedb_vm::expr::{FieldExpr, FieldOp};

fn find_field_name(from: &From, field: FieldName) -> Result<&ColumnSchema, PlanError> {
    from.find_field_name(field).ok_or(PlanError::UnknownFieldName {
        field,
        tables: from.iter_tables().map(|t| t.table_name.clone()).collect(),
    })
}

struct QueryFragment<'a, T> {
    from: &'a From,
    q: &'a T,
}

/// Type check trait for `sql` query fragments
pub(crate) trait TypeCheck {
    /// Type check the query fragment
    fn type_check(&self) -> Result<(), PlanError>;
}

fn type_check(of: &QueryFragment<FieldOp>) -> Result<Option<AlgebraicType>, PlanError> {
    match of.q {
        FieldOp::Field(x) => match x {
            FieldExpr::Name(x) => {
                let col = find_field_name(of.from, *x)?;

                Ok(Some(col.col_type.clone()))
            }
            FieldExpr::Value(x) => Ok(x.type_of()),
        },
        FieldOp::Cmp { op, lhs, rhs } => match op {
            OpQuery::Cmp(_) => {
                let ty_lhs = type_check(&QueryFragment { from: of.from, q: lhs })?;
                let ty_rhs = type_check(&QueryFragment { from: of.from, q: rhs })?;

                // The `SumType` returns `None` on `type_of` so we need to check against the value
                if let (Some(AlgebraicType::Sum(ty_lhs)), _) = (&ty_lhs, &ty_rhs) {
                    // We can use in `sql` coercion from string to sum type: `tag = 'name'`
                    if let FieldOp::Field(FieldExpr::Value(x)) = rhs.as_ref() {
                        if let Some(x) = x.as_string() {
                            if ty_lhs.get_variant_simple(x).is_some() {
                                return Ok(Some(AlgebraicType::Sum(ty_lhs.clone())));
                            }
                        }
                        // or check it against a `SumValue` type: `tag = { tag: 0, value: 1 }`
                        if let Some(x) = x.as_sum() {
                            if ty_lhs.is_simple_enum() && ty_lhs.get_variant_by_tag(x.tag).is_some() {
                                return Ok(Some(AlgebraicType::Sum(ty_lhs.clone())));
                            }
                        }
                    }
                }
                if ty_lhs != ty_rhs {
                    return Err(ErrorType::TypeMismatchCmp {
                        lhs: lhs.as_ref().clone(),
                        rhs: rhs.as_ref().clone(),
                        expect: ty_lhs,
                        given: ty_rhs,
                    }
                    .into());
                } else {
                    Ok(ty_lhs)
                }
            }
            OpQuery::Logic(_) => Ok(Some(AlgebraicType::Bool)),
        },
    }
}

impl TypeCheck for QueryFragment<'_, Selection> {
    fn type_check(&self) -> Result<(), PlanError> {
        type_check(&QueryFragment {
            from: self.from,
            q: &self.q.clause,
        })?;
        Ok(())
    }
}

impl TypeCheck for QueryFragment<'_, ()> {
    fn type_check(&self) -> Result<(), PlanError> {
        for join in &self.from.joins {
            match join {
                Join::Inner { rhs: table, on } => {
                    let lhs = find_field_name(self.from, on.lhs)?;
                    let rhs = find_field_name(self.from, on.rhs)?;

                    if lhs.col_type != rhs.col_type {
                        return Err(ErrorType::TypeMismatchJoin {
                            table: table.table_name.clone(),
                            lhs: lhs.col_name.clone(),
                            rhs: rhs.col_name.clone(),
                            expect: lhs.col_type.clone(),
                            given: rhs.col_type.clone(),
                        }
                        .into());
                    }
                }
            }
        }
        Ok(())
    }
}

impl TypeCheck for SqlAst {
    fn type_check(&self) -> Result<(), PlanError> {
        match self {
            SqlAst::Select {
                from,
                project: _,
                selection,
            } => {
                QueryFragment { from, q: &() }.type_check()?;
                if let Some(selection) = selection {
                    QueryFragment { from, q: selection }.type_check()?;
                }
            }

            _ => {
                // TODO: Other options deferred for the new query engine
            }
        }
        Ok(())
    }
}
