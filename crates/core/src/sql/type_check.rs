use crate::error::PlanError;
use crate::sql::ast::From;
use crate::sql::ast::{Selection, SqlAst};
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_lib::relation::FieldName;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_schema::schema::ColumnSchema;
use spacetimedb_vm::errors::ErrorType;
use spacetimedb_vm::expr::{FieldExpr, FieldOp};
use std::fmt;

fn find_field_name(from: &From, field: FieldName) -> Result<(&str, &ColumnSchema), PlanError> {
    from.find_field_name(field).ok_or_else(|| PlanError::UnknownFieldName {
        field,
        tables: from.iter_tables().map(|t| t.table_name.clone()).collect(),
    })
}

#[derive(Debug)]
enum Typed<'a> {
    Field {
        table: &'a str,
        field: &'a str,
        ty: Option<AlgebraicType>,
    },
    Value {
        value: &'a AlgebraicValue,
        ty: Option<AlgebraicType>,
    },
    Cmp {
        op: OpQuery,
        lhs: Box<Typed<'a>>,
        rhs: Box<Typed<'a>>,
    },
}

impl Typed<'_> {
    pub fn ty(&self) -> Option<&AlgebraicType> {
        match self {
            Typed::Field { ty, .. } | Typed::Value { ty, .. } => ty.as_ref(),
            Typed::Cmp { .. } => Some(&AlgebraicType::Bool),
        }
    }

    pub fn set_ty(&mut self, ty: Option<AlgebraicType>) {
        match self {
            Typed::Field { ty: ty_lhs, .. } | Typed::Value { ty: ty_lhs, .. } => {
                *ty_lhs = ty;
            }
            Typed::Cmp { .. } => {}
        }
    }
}

impl fmt::Display for Typed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Typed::Field { table, field, ty } => {
                if let Some(ty) = ty {
                    write!(f, "{table}.{field}: {}", fmt_algebraic_type(ty))
                } else {
                    write!(f, "{table}.{field}: ?",)
                }
            }
            Typed::Value { value, ty } => {
                if let Some(ty) = ty {
                    write!(f, "{value:?}: {}", fmt_algebraic_type(ty))
                } else {
                    write!(f, "{value:?}: ?")
                }
            }
            Typed::Cmp { op, lhs, rhs, .. } => {
                write!(f, "{lhs} {op} {rhs}")
            }
        }
    }
}

#[derive(Debug)]
struct QueryFragment<'a, T> {
    from: &'a From,
    q: &'a T,
}

/// Type check trait for `sql` query fragments
pub(crate) trait TypeCheck {
    /// Type check the query fragment
    fn type_check(&self) -> Result<(), PlanError>;
}

/// Resolve the type of the field, that in the case of `SumType` we need to resolve using the `field`
fn resolve_type(field: &FieldExpr, ty: AlgebraicType) -> Result<Option<AlgebraicType>, PlanError> {
    // The `SumType` returns `None` on `type_of` so we need to check against the value
    if let AlgebraicType::Sum(ty) = &ty {
        // We can use in `sql` coercion from string to sum type: `tag = 'name'`
        if let FieldExpr::Value(val_rhs) = field {
            if let Some(val_rhs) = val_rhs.as_string() {
                if ty.get_variant_simple(val_rhs).is_some() {
                    return Ok(Some(AlgebraicType::Sum(ty.clone())));
                }
            }
            // or check it against a `SumValue` type: `tag = { tag: 0, value: 1 }`
            if let Some(val_rhs) = val_rhs.as_sum() {
                if ty.is_simple_enum() && ty.get_variant_by_tag(val_rhs.tag).is_some() {
                    return Ok(Some(AlgebraicType::Sum(ty.clone())));
                }
            }
        }
    }

    if let (AlgebraicType::Product(_), FieldExpr::Value(val)) = (&ty, field) {
        match val {
            AlgebraicValue::U128(_) => return Ok(Some(AlgebraicType::U128)),
            AlgebraicValue::U256(_) => return Ok(Some(AlgebraicType::U256)),
            _ => {}
        }
    }
    Ok(Some(ty))
}

fn check_both(op: OpQuery, lhs: &Typed, rhs: &Typed) -> Result<(), PlanError> {
    match op {
        OpQuery::Cmp(_) => {
            if lhs.ty() != rhs.ty() {
                return Err(ErrorType::TypeMismatch {
                    lhs: lhs.to_string(),
                    rhs: rhs.to_string(),
                }
                .into());
            }
        }
        OpQuery::Logic(op) => {
            if (lhs.ty(), rhs.ty()) != (Some(&AlgebraicType::Bool), Some(&AlgebraicType::Bool)) {
                return Err(ErrorType::TypeMismatchLogic {
                    lhs: lhs.to_string(),
                    rhs: rhs.to_string(),
                    op,
                    expected: fmt_algebraic_type(&AlgebraicType::Bool).to_string(),
                }
                .into());
            }
        }
    }
    Ok(())
}

/// Patch the type of the field if the type is an `Identity`, `ConnectionId` or `Enum`
fn patch_type(lhs: &FieldOp, ty_lhs: &mut Typed, ty_rhs: &Typed) -> Result<(), PlanError> {
    if let FieldOp::Field(lhs_field) = lhs {
        if let Some(ty) = ty_rhs.ty() {
            if ty.is_sum() || ty.as_product().is_some_and(|x| x.is_special()) {
                ty_lhs.set_ty(resolve_type(lhs_field, ty.clone())?);
            }
        }
    }
    Ok(())
}

fn type_check(of: QueryFragment<FieldOp>) -> Result<Typed, PlanError> {
    match of.q {
        FieldOp::Field(expr) => match expr {
            FieldExpr::Name(x) => {
                let (table, col) = find_field_name(of.from, *x)?;

                Ok(Typed::Field {
                    table,
                    field: &col.col_name,
                    ty: Some(col.col_type.clone()),
                })
            }
            FieldExpr::Value(value) => Ok(Typed::Value {
                value,
                ty: value.type_of(),
            }),
        },
        FieldOp::Cmp { op, lhs, rhs } => {
            let mut ty_lhs = type_check(QueryFragment { from: of.from, q: lhs })?;
            let mut ty_rhs = type_check(QueryFragment { from: of.from, q: rhs })?;

            // TODO: For the cases of `Identity, ConnectionId, Enum` we need to resolve the type from the value we are comparing,
            // because the type is not lifted when we parse the query on `spacetimedb_vm::ops::parse`.
            //
            // This is a temporary solution until we have a better way to resolve the type of the field.
            patch_type(lhs, &mut ty_lhs, &ty_rhs)?;
            patch_type(rhs, &mut ty_rhs, &ty_lhs)?;

            check_both(*op, &ty_lhs, &ty_rhs)?;

            Ok(Typed::Cmp {
                op: *op,
                lhs: Box::new(ty_lhs),
                rhs: Box::new(ty_rhs),
            })
        }
    }
}

impl TypeCheck for QueryFragment<'_, Selection> {
    fn type_check(&self) -> Result<(), PlanError> {
        type_check(QueryFragment {
            from: self.from,
            q: &self.q.clause,
        })?;
        Ok(())
    }
}

impl TypeCheck for SqlAst {
    // TODO: Other options deferred for the new query engine
    fn type_check(&self) -> Result<(), PlanError> {
        if let SqlAst::Select {
            from,
            project: _,
            selection: Some(selection),
        } = self
        {
            QueryFragment { from, q: selection }.type_check()?;
        }

        Ok(())
    }
}
