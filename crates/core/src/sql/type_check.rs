use crate::error::PlanError;
use crate::sql::ast::{From, Join};
use crate::sql::ast::{Selection, SqlAst};
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::db::def::ColumnSchema;
use spacetimedb_sats::relation::FieldName;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_vm::errors::ErrorType;
use spacetimedb_vm::expr::{FieldExpr, FieldOp};
use std::fmt;

fn find_field_name(from: &From, field: FieldName) -> Result<(Box<str>, &ColumnSchema), PlanError> {
    from.find_field_name(field).ok_or_else(|| PlanError::UnknownFieldName {
        field,
        tables: from.iter_tables().map(|t| t.table_name.clone()).collect(),
    })
}

#[derive(Debug)]
pub enum Typed {
    Field {
        table: Box<str>,
        field: Box<str>,
        ty: Option<AlgebraicType>,
    },
    Value {
        value: AlgebraicValue,
        ty: Option<AlgebraicType>,
    },
    Cmp {
        op: OpQuery,
        lhs: Box<Typed>,
        rhs: Box<Typed>,
        ty: AlgebraicType,
    },
}

impl Typed {
    pub fn ty(&self) -> Option<&AlgebraicType> {
        match self {
            Typed::Field { ty, .. } => ty.as_ref(),
            Typed::Value { ty, .. } => ty.as_ref(),
            Typed::Cmp { ty, .. } => Some(ty),
        }
    }

    pub fn set_ty(&mut self, ty: Option<AlgebraicType>) {
        match self {
            Typed::Field { ty: ty_lhs, .. } => {
                *ty_lhs = ty;
            }
            Typed::Value { ty: ty_lhs, .. } => {
                *ty_lhs = ty;
            }
            Typed::Cmp { .. } => {}
        }
    }
}

impl fmt::Display for Typed {
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

struct QueryFragment<'a, T> {
    from: &'a From,
    q: &'a T,
}

/// Type check trait for `sql` query fragments
pub(crate) trait TypeCheck {
    /// Type check the query fragment
    fn type_check(&self) -> Result<(), PlanError>;
}

fn resolve_type(field: &FieldExpr, ty: Option<AlgebraicType>) -> Result<Option<AlgebraicType>, PlanError> {
    // The `SumType` returns `None` on `type_of` so we need to check against the value
    if let Some(AlgebraicType::Sum(ty)) = &ty {
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

    if let Some(AlgebraicType::Product(_)) = &ty {
        if let FieldExpr::Value(val) = field {
            if val.as_bytes().is_some() {
                return Ok(Some(AlgebraicType::bytes()));
            }
        }
    }
    Ok(ty)
}

fn check_both(op: OpQuery, lhs: &Typed, rhs: &Typed) -> Result<(), PlanError> {
    if lhs.ty() != rhs.ty() {
        Err(match op {
            OpQuery::Cmp(_) => ErrorType::TypeMismatch {
                lhs: lhs.to_string(),
                rhs: rhs.to_string(),
            },
            OpQuery::Logic(op) => ErrorType::TypeMismatchLogic {
                lhs: lhs.to_string(),
                rhs: rhs.to_string(),
                op,
                expected: fmt_algebraic_type(&AlgebraicType::Bool).to_string(),
            },
        }
        .into())
    } else {
        Ok(())
    }
}

/// Patch the type of the field if the type is a `Identity`, `Address` or `Enum`
fn patch_type(lhs: &FieldOp, ty_lhs: &mut Typed, ty_rhs: &Typed) -> Result<(), PlanError> {
    if let FieldOp::Field(f) = lhs {
        if let Some(ty) = ty_rhs.ty() {
            if ty.is_sum() || ty.as_product().map_or(false, |x| x.is_special()) {
                ty_lhs.set_ty(resolve_type(f, Some(ty.clone()))?);
            }
        }
    }
    Ok(())
}

fn type_check(of: &QueryFragment<FieldOp>) -> Result<Typed, PlanError> {
    match of.q {
        FieldOp::Field(expr) => match expr {
            FieldExpr::Name(x) => {
                let (table, col) = find_field_name(of.from, *x)?;

                Ok(Typed::Field {
                    table,
                    field: col.col_name.clone(),
                    ty: Some(col.col_type.clone()),
                })
            }
            FieldExpr::Value(x) => Ok(Typed::Value {
                value: x.clone(),
                ty: x.type_of(),
            }),
        },
        FieldOp::Cmp { op, lhs, rhs } => {
            let mut ty_lhs = type_check(&QueryFragment { from: of.from, q: lhs })?;
            let mut ty_rhs = type_check(&QueryFragment { from: of.from, q: rhs })?;

            let op = match op {
                OpQuery::Cmp(op) => (*op).into(),
                OpQuery::Logic(op) => (*op).into(),
            };

            // TODO: For the cases of `Identity, Address, Enum` we need to resolve the type from the value we are comparing,
            // because the type is not lifted when we parse the query on `spacetimedb_vm::ops::parse`.
            //
            // This is a temporary solution until we have a better way to resolve the type of the field.
            patch_type(lhs, &mut ty_lhs, &ty_rhs)?;
            patch_type(rhs, &mut ty_rhs, &ty_lhs)?;

            // If both sides are the same type, then return `Bool` to indicate a logical comparison
            check_both(op, &ty_lhs, &ty_rhs)?;

            Ok(Typed::Cmp {
                op,
                lhs: Box::new(ty_lhs),
                rhs: Box::new(ty_rhs),
                ty: AlgebraicType::Bool,
            })
        }
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
                Join::Inner { rhs: _, on } => {
                    let (table_lhs, lhs) = find_field_name(self.from, on.lhs)?;
                    let (table_rhs, rhs) = find_field_name(self.from, on.rhs)?;
                    let ty_lhs = resolve_type(&FieldExpr::Name(on.lhs), Some(lhs.col_type.clone()))?.unwrap();
                    let ty_rhs = resolve_type(&FieldExpr::Name(on.rhs), Some(rhs.col_type.clone()))?.unwrap();

                    if ty_lhs != ty_rhs {
                        return Err(ErrorType::TypeMismatchJoin {
                            lhs: Typed::Field {
                                table: table_lhs,
                                field: lhs.col_name.clone(),
                                ty: Some(ty_lhs),
                            }
                            .to_string(),
                            rhs: Typed::Field {
                                table: table_rhs,
                                field: rhs.col_name.clone(),
                                ty: Some(ty_rhs),
                            }
                            .to_string(),
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
