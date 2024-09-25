use std::collections::HashSet;

use bind::TypingResult;
use errors::{DuplicateName, InvalidLiteral, InvalidWildcard, UnexpectedType, Unresolved};
use expr::{Expr, Let, RelExpr};
use spacetimedb_lib::{from_hex_pad, Address, AlgebraicType, AlgebraicValue, Identity};
use spacetimedb_sql_parser::ast::{self, ProjectElem, ProjectExpr, SqlExpr, SqlLiteral};
use ty::{Symbol, TyCtx, TyEnv, TyId, Type, TypeWithCtx};

pub mod bind;
pub mod errors;
pub mod expr;
pub mod stmt;
pub mod ty;

/// Asserts that `$ty` is `$size` bytes in `static_assert_size($ty, $size)`.
///
/// Example:
///
/// ```ignore
/// static_assert_size!(u32, 4);
/// ```
#[macro_export]
macro_rules! static_assert_size {
    ($ty:ty, $size:expr) => {
        const _: [(); $size] = [(); ::core::mem::size_of::<$ty>()];
    };
}

/// Type check and lower a [SqlExpr]
pub(crate) fn type_select(
    ctx: &mut TyCtx,
    input: RelExpr,
    alias: Option<Symbol>,
    expr: SqlExpr,
) -> TypingResult<RelExpr> {
    let mut vars = Vec::new();
    let mut tenv = TyEnv::default();
    if let Some(name) = alias {
        tenv.add(name, input.ty_id());
        vars.push((name, Expr::Input(input.ty_id())));
    }
    for (i, name, ty) in input.ty(ctx)?.expect_relation()?.iter() {
        tenv.add(name, ty);
        vars.push((name, Expr::Field(Box::new(Expr::Input(input.ty_id())), i, ty)));
    }
    let expr = type_expr(ctx, &tenv, expr, Some(TyId::BOOL))?;
    Ok(RelExpr::select(
        input,
        Let {
            vars,
            exprs: vec![expr],
        },
    ))
}

/// Type check and lower a [ast::Project]
pub(crate) fn type_proj(
    ctx: &mut TyCtx,
    input: RelExpr,
    alias: Option<Symbol>,
    proj: ast::Project,
) -> TypingResult<RelExpr> {
    match proj {
        ast::Project::Star(None) => {
            // Check that * is not applied to a join
            for (_, _, ty) in input.ty(ctx)?.expect_relation()?.iter() {
                ctx.try_resolve(ty)?
                    .expect_scalar()
                    .map_err(|_| InvalidWildcard::Join)?;
            }
            Ok(input)
        }
        ast::Project::Star(Some(var)) => {
            // Get the symbol for this variable
            let name = ctx.get_symbol(&var.name).ok_or_else(|| Unresolved::var(&var.name))?;

            match alias {
                Some(alias) if alias == name => {
                    // Check that * is applied to a row type
                    let input_type = input.ty_id();

                    ctx.try_resolve(input_type)?
                        .expect_relation()
                        .map_err(|_| InvalidWildcard::Scalar)?;

                    // Create a single field expression for the projection.
                    // Note the variable reference has been inlined.
                    // Hence no let variables are needed for this expression.
                    Ok(RelExpr::project(
                        input,
                        Let {
                            vars: vec![],
                            exprs: vec![Expr::Input(input_type)],
                        },
                    ))
                }
                Some(_) | None => {
                    // Is it in scope?
                    let (i, ty) = input
                        .ty(ctx)?
                        .expect_relation()?
                        .find(name)
                        .ok_or_else(|| Unresolved::var(&var.name))?;

                    // Check that * is applied to a row type
                    ctx.try_resolve(ty)?
                        .expect_relation()
                        .map_err(|_| InvalidWildcard::Scalar)?;

                    let input_type = input.ty_id();

                    // Create a single field expression for the projection.
                    // Note the variable reference has been inlined.
                    // Hence no let variables are needed for this expression.
                    Ok(RelExpr::project(
                        input,
                        Let {
                            vars: vec![],
                            exprs: vec![Expr::Field(Box::new(Expr::Input(input_type)), i, ty)],
                        },
                    ))
                }
            }
        }
        ast::Project::Exprs(elems) => {
            // Create let variables and a type environment for the projection
            let mut vars = Vec::new();
            let mut tenv = TyEnv::default();
            if let Some(name) = alias {
                tenv.add(name, input.ty_id());
                vars.push((name, Expr::Input(input.ty_id())));
            }
            for (i, name, ty) in input.ty(ctx)?.expect_relation()?.iter() {
                tenv.add(name, ty);
                vars.push((name, Expr::Field(Box::new(Expr::Input(input.ty_id())), i, ty)));
            }

            // Type and lower the projection expressions
            let mut field_exprs = Vec::new();
            let mut field_types = Vec::new();
            let mut names = HashSet::new();

            for elem in elems {
                match elem {
                    ProjectElem(ProjectExpr::Var(field), None) => {
                        let name = ctx.gen_symbol(&field.name);
                        if !names.insert(name) {
                            return Err(DuplicateName(field.name).into());
                        }
                        let expr = type_expr(ctx, &tenv, SqlExpr::Var(field), None)?;
                        field_types.push((name, expr.ty_id()));
                        field_exprs.push((name, expr));
                    }
                    ProjectElem(ProjectExpr::Var(field), Some(alias)) => {
                        let name = ctx.gen_symbol(&alias.name);
                        if !names.insert(name) {
                            return Err(DuplicateName(alias.name).into());
                        }
                        let expr = type_expr(ctx, &tenv, SqlExpr::Var(field), None)?;
                        field_types.push((name, expr.ty_id()));
                        field_exprs.push((name, expr));
                    }
                    ProjectElem(ProjectExpr::Field(table, field), None) => {
                        let name = ctx.gen_symbol(&field.name);
                        if !names.insert(name) {
                            return Err(DuplicateName(field.name).into());
                        }
                        let expr = type_expr(ctx, &tenv, SqlExpr::Field(table, field), None)?;
                        field_types.push((name, expr.ty_id()));
                        field_exprs.push((name, expr));
                    }
                    ProjectElem(ProjectExpr::Field(table, field), Some(alias)) => {
                        let name = ctx.gen_symbol(&alias.name);
                        if !names.insert(name) {
                            return Err(DuplicateName(alias.name).into());
                        }
                        let expr = type_expr(ctx, &tenv, SqlExpr::Field(table, field), None)?;
                        field_types.push((name, expr.ty_id()));
                        field_exprs.push((name, expr));
                    }
                }
            }

            // Column projections produce a new type.
            // So we must make sure to add it to the typing context.
            let ty = Type::Row(field_types.into_boxed_slice());
            let id = ctx.add(ty);
            Ok(RelExpr::project(
                input,
                Let {
                    vars,
                    exprs: vec![Expr::Row(field_exprs.into_boxed_slice(), id)],
                },
            ))
        }
    }
}

/// Type check and lower a [SqlExpr] into a logical [Expr].
pub(crate) fn type_expr(ctx: &TyCtx, vars: &TyEnv, expr: SqlExpr, expected: Option<TyId>) -> TypingResult<Expr> {
    match (expr, expected) {
        (SqlExpr::Lit(SqlLiteral::Bool(v)), None | Some(TyId::BOOL)) => Ok(Expr::bool(v)),
        (SqlExpr::Lit(SqlLiteral::Bool(_)), Some(id)) => {
            let expected = ctx.bool();
            let inferred = id.try_with_ctx(ctx)?;
            Err(UnexpectedType::new(&expected, &inferred).into())
        }
        (SqlExpr::Lit(SqlLiteral::Str(v)), None | Some(TyId::STR)) => Ok(Expr::str(v)),
        (SqlExpr::Lit(SqlLiteral::Str(_)), Some(id)) => {
            let expected = ctx.str();
            let inferred = id.try_with_ctx(ctx)?;
            Err(UnexpectedType::new(&expected, &inferred).into())
        }
        (SqlExpr::Lit(SqlLiteral::Num(_) | SqlLiteral::Hex(_)), None) => Err(Unresolved::Literal.into()),
        (SqlExpr::Lit(SqlLiteral::Num(v) | SqlLiteral::Hex(v)), Some(id)) => {
            let t = id.try_with_ctx(ctx)?;
            let v = parse(v, t)?;
            Ok(Expr::Lit(v, id))
        }
        (SqlExpr::Var(var), None) => {
            // Is this variable in scope?
            let var_name = ctx.get_symbol(&var.name).ok_or_else(|| Unresolved::var(&var.name))?;
            let var_type = vars.find(var_name).ok_or_else(|| Unresolved::var(&var.name))?;
            Ok(Expr::Var(var_name, var_type))
        }
        (SqlExpr::Var(var), Some(id)) => {
            // Is this variable in scope?
            let var_name = ctx.get_symbol(&var.name).ok_or_else(|| Unresolved::var(&var.name))?;
            let var_type = vars.find(var_name).ok_or_else(|| Unresolved::var(&var.name))?;
            // Is it the correct type?
            assert_eq_types(ctx, var_type, id)?;
            Ok(Expr::Var(var_name, var_type))
        }
        (SqlExpr::Field(table, field), None) => {
            // Is the table variable in scope?
            let table_name = ctx
                .get_symbol(&table.name)
                .ok_or_else(|| Unresolved::var(&table.name))?;
            let field_name = ctx
                .get_symbol(&field.name)
                .ok_or_else(|| Unresolved::var(&field.name))?;
            let table_type = vars.find(table_name).ok_or_else(|| Unresolved::var(&table.name))?;
            // Is it a row type, and if so, does it have this field?
            let (i, field_type) = ctx
                .try_resolve(table_type)?
                .expect_relation()?
                .find(field_name)
                .ok_or_else(|| Unresolved::field(&table.name, &field.name))?;
            Ok(Expr::Field(Box::new(Expr::Var(table_name, table_type)), i, field_type))
        }
        (SqlExpr::Field(table, field), Some(id)) => {
            // Is the table variable in scope?
            let table_name = ctx
                .get_symbol(&table.name)
                .ok_or_else(|| Unresolved::var(&table.name))?;
            let field_name = ctx
                .get_symbol(&field.name)
                .ok_or_else(|| Unresolved::var(&field.name))?;
            let table_type = vars.find(table_name).ok_or_else(|| Unresolved::var(&table.name))?;
            // Is it a row type, and if so, does it have this field?
            let (i, field_type) = ctx
                .try_resolve(table_type)?
                .expect_relation()?
                .find(field_name)
                .ok_or_else(|| Unresolved::field(&table.name, &field.name))?;
            // Is the field type correct?
            assert_eq_types(ctx, field_type, id)?;
            Ok(Expr::Field(Box::new(Expr::Var(table_name, table_type)), i, field_type))
        }
        (SqlExpr::Bin(a, b, op), None | Some(TyId::BOOL)) => match (*a, *b) {
            (a, b @ SqlExpr::Lit(_)) | (b @ SqlExpr::Lit(_), a) | (a, b) => {
                let a = type_expr(ctx, vars, a, None)?;
                let b = type_expr(ctx, vars, b, Some(a.ty_id()))?;
                // At this point we know both expressions have the same type.
                // Therefore we only need to perform one compatibility check.
                a.ty(ctx)?.expect_op(op)?;
                Ok(Expr::Bin(op, Box::new(a), Box::new(b)))
            }
        },
        (SqlExpr::Bin(..), Some(id)) => {
            let expected = ctx.bool();
            let inferred = id.try_with_ctx(ctx)?;
            Err(UnexpectedType::new(&expected, &inferred).into())
        }
    }
}

/// Assert types are structurally equivalent
pub(crate) fn assert_eq_types(ctx: &TyCtx, a: TyId, b: TyId) -> TypingResult<()> {
    if !ctx.eq(a, b)? {
        return Err(UnexpectedType::new(&a.try_with_ctx(ctx)?, &b.try_with_ctx(ctx)?).into());
    }
    Ok(())
}

/// Parses a source text literal as a particular type
pub(crate) fn parse(value: String, ty: TypeWithCtx) -> Result<AlgebraicValue, InvalidLiteral> {
    match &*ty {
        Type::Alg(AlgebraicType::I8) => value
            .parse::<i8>()
            .map(AlgebraicValue::I8)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::U8) => value
            .parse::<u8>()
            .map(AlgebraicValue::U8)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::I16) => value
            .parse::<i16>()
            .map(AlgebraicValue::I16)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::U16) => value
            .parse::<u16>()
            .map(AlgebraicValue::U16)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::I32) => value
            .parse::<i32>()
            .map(AlgebraicValue::I32)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::U32) => value
            .parse::<u32>()
            .map(AlgebraicValue::U32)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::I64) => value
            .parse::<i64>()
            .map(AlgebraicValue::I64)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::U64) => value
            .parse::<u64>()
            .map(AlgebraicValue::U64)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::F32) => value
            .parse::<f32>()
            .map(|value| AlgebraicValue::F32(value.into()))
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::F64) => value
            .parse::<f64>()
            .map(|value| AlgebraicValue::F64(value.into()))
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::I128) => value
            .parse::<i128>()
            .map(|value| AlgebraicValue::I128(value.into()))
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(AlgebraicType::U128) => value
            .parse::<u128>()
            .map(|value| AlgebraicValue::U128(value.into()))
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(t) if t.is_bytes() => from_hex_pad::<Vec<u8>, _>(&value)
            .map(|value| AlgebraicValue::Bytes(value.into_boxed_slice()))
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(t) if t.is_identity() => Identity::from_hex(&value)
            .map(AlgebraicValue::from)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        Type::Alg(t) if t.is_address() => Address::from_hex(&value)
            .map(AlgebraicValue::from)
            .map_err(|_| InvalidLiteral::new(value, &ty)),
        _ => Err(InvalidLiteral::new(value, &ty)),
    }
}
