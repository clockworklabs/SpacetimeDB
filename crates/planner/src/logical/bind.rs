use std::sync::Arc;

use spacetimedb_lib::{from_hex_pad, Address, AlgebraicValue, Identity};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::{
    ast::{
        self,
        sub::{SqlAst, SqlSelect},
        BinOp, ProjectElem, SqlExpr, SqlFrom, SqlLiteral,
    },
    parser::sub::parse_subscription,
};

use super::{
    errors::{ConstraintViolation, TypingError, Unresolved, Unsupported},
    expr::{Expr, Ref, RelExpr, Vars},
    ty::{TyCtx, TyId, Type, TypeWithCtx},
};

/// The result of type checking and name resolution
pub type TypingResult<T> = core::result::Result<T, TypingError>;

pub trait SchemaView {
    fn schema(&self, name: &str, case_sensitive: bool) -> Option<Arc<TableSchema>>;
}

/// Parse and type check a subscription query
pub fn parse_and_type_sub(sql: &str, tx: &impl SchemaView) -> TypingResult<RelExpr> {
    let mut ctx = TyCtx::default();
    let expr = type_ast(&mut ctx, parse_subscription(sql)?, tx)?;
    expect_table_type(&ctx, expr)
}

/// Type check and lower a [SqlAst] into a [RelExpr].
/// This includes name resolution and variable binding.
pub fn type_ast(ctx: &mut TyCtx, expr: SqlAst, tx: &impl SchemaView) -> TypingResult<RelExpr> {
    match expr {
        SqlAst::Union(a, b) => {
            let a = type_ast(ctx, *a, tx)?;
            let b = type_ast(ctx, *b, tx)?;
            assert_eq_types(a.ty_id().try_with_ctx(ctx)?, b.ty_id().try_with_ctx(ctx)?)?;
            Ok(RelExpr::Union(Box::new(a), Box::new(b)))
        }
        SqlAst::Minus(a, b) => {
            let a = type_ast(ctx, *a, tx)?;
            let b = type_ast(ctx, *b, tx)?;
            assert_eq_types(a.ty_id().try_with_ctx(ctx)?, b.ty_id().try_with_ctx(ctx)?)?;
            Ok(RelExpr::Minus(Box::new(a), Box::new(b)))
        }
        SqlAst::Select(SqlSelect {
            project,
            from,
            filter: None,
        }) => {
            let (arg, vars) = type_from(ctx, from, tx)?;
            type_proj(ctx, project, arg, vars)
        }
        SqlAst::Select(SqlSelect {
            project,
            from,
            filter: Some(expr),
        }) => {
            let (from, vars) = type_from(ctx, from, tx)?;
            let arg = type_select(ctx, expr, from, vars.clone())?;
            type_proj(ctx, project, arg, vars.clone())
        }
    }
}

/// Type check and lower a [SqlFrom<SqlAst>]
pub fn type_from(ctx: &mut TyCtx, from: SqlFrom<SqlAst>, tx: &impl SchemaView) -> TypingResult<(RelExpr, Vars)> {
    match from {
        SqlFrom::Expr(expr, None) => type_rel(ctx, expr, tx),
        SqlFrom::Expr(expr, Some(alias)) => {
            let (expr, _) = type_rel(ctx, expr, tx)?;
            let ty = expr.ty_id();
            Ok((expr, vec![(alias.name, ty)].into()))
        }
        SqlFrom::Join(r, alias, joins) => {
            let (mut vars, mut args, mut exprs) = (Vars::default(), Vec::new(), Vec::new());

            let (r, _) = type_rel(ctx, r, tx)?;
            let ty = r.ty_id();

            args.push(r);
            vars.push((alias.name, ty));

            for join in joins {
                let (r, _) = type_rel(ctx, join.expr, tx)?;
                let ty = r.ty_id();

                args.push(r);
                vars.push((join.alias.name, ty));

                if let Some(on) = join.on {
                    exprs.push(type_expr(ctx, &vars, on, Some(TyId::BOOL))?);
                }
            }
            let types = vars.iter().map(|(_, ty)| *ty).collect();
            let ty = Type::Tup(types);
            let input = RelExpr::Join(args.into(), ctx.add(ty));
            Ok((RelExpr::select(input, vars.clone(), exprs), vars))
        }
    }
}

/// Type check and lower a [ast::RelExpr<SqlAst>]
fn type_rel(ctx: &mut TyCtx, expr: ast::RelExpr<SqlAst>, tx: &impl SchemaView) -> TypingResult<(RelExpr, Vars)> {
    match expr {
        ast::RelExpr::Var(var) => {
            let schema = tx
                .schema(&var.name, var.case_sensitive)
                .ok_or_else(|| Unresolved::table(&var.name))
                .map_err(TypingError::from)?;
            let mut types = Vec::new();
            for ColumnSchema { col_name, col_type, .. } in schema.columns() {
                let ty = Type::Alg(col_type.clone());
                let id = ctx.add(ty);
                types.push((col_name.to_string(), id));
            }
            let ty = Type::Var(types.into_boxed_slice());
            let id = ctx.add(ty);
            Ok((RelExpr::RelVar(schema, id), vec![(var.name, id)].into()))
        }
        ast::RelExpr::Ast(ast) => Ok((type_ast(ctx, *ast, tx)?, Vars::default())),
    }
}

/// Type check and lower a [SqlExpr]
fn type_select(ctx: &mut TyCtx, expr: SqlExpr, input: RelExpr, vars: Vars) -> TypingResult<RelExpr> {
    let exprs = vec![type_expr(ctx, &vars, expr, Some(TyId::BOOL))?];
    Ok(RelExpr::select(input, vars, exprs))
}

/// Type check and lower a [ast::Project]
fn type_proj(ctx: &mut TyCtx, proj: ast::Project, input: RelExpr, vars: Vars) -> TypingResult<RelExpr> {
    match proj {
        ast::Project::Star(None) => Ok(input),
        ast::Project::Star(Some(var)) => {
            let (i, ty) = vars.expect_var(ctx, &var.name, None)?;
            let refs = vec![Ref::Var(i, ty)];
            Ok(RelExpr::project(input, vars, refs, ty))
        }
        ast::Project::Exprs(elems) => {
            let (mut refs, mut fields) = (Vec::new(), Vec::new());
            for ProjectElem(expr, alias) in elems {
                if let SqlExpr::Var(_) = expr {
                    return Err(Unsupported::UnqualifiedProjectExpr.into());
                }
                let SqlExpr::Field(table, field) = expr else {
                    return Err(Unsupported::ProjectExpr.into());
                };
                let (i, j, ty) = vars.expect_field(ctx, &table.name, &field.name, None)?;
                refs.push(Ref::Field(i, j, ty));
                if let Some(alias) = alias {
                    fields.push((alias.name, ty));
                } else {
                    fields.push((field.name, ty));
                }
            }
            let ty = Type::Row(fields.into_boxed_slice());
            let ty = ctx.add(ty);
            Ok(RelExpr::project(input, vars, refs, ty))
        }
    }
}

/// Type check and lower a [SqlExpr] into a logical [Expr].
fn type_expr(ctx: &TyCtx, vars: &Vars, expr: SqlExpr, expected: Option<TyId>) -> TypingResult<Expr> {
    match (expr, expected) {
        (SqlExpr::Lit(SqlLiteral::Bool(v)), None | Some(TyId::BOOL)) => Ok(Expr::bool(v)),
        (SqlExpr::Lit(SqlLiteral::Bool(_)), Some(id)) => {
            Err(unexpected_type(Type::BOOL.with_ctx(ctx), id.try_with_ctx(ctx)?))
        }
        (SqlExpr::Lit(SqlLiteral::Str(v)), None | Some(TyId::STR)) => Ok(Expr::str(v)),
        (SqlExpr::Lit(SqlLiteral::Str(_)), Some(id)) => {
            Err(unexpected_type(Type::STR.with_ctx(ctx), id.try_with_ctx(ctx)?))
        }
        (SqlExpr::Lit(SqlLiteral::Num(_) | SqlLiteral::Hex(_)), None) => Err(Unresolved::Literal.into()),
        (SqlExpr::Lit(SqlLiteral::Num(v) | SqlLiteral::Hex(v)), Some(id)) => {
            parse(ctx, v, id).map(|v| Expr::Lit(v, id))
        }
        (SqlExpr::Var(var), expected) => vars.expect_var_ref(ctx, &var.name, expected),
        (SqlExpr::Field(table, field), expected) => vars.expect_field_ref(ctx, &table.name, &field.name, expected),
        (SqlExpr::Bin(a, b, op), None | Some(TyId::BOOL)) => match (*a, *b) {
            (a, b @ SqlExpr::Lit(_)) | (b @ SqlExpr::Lit(_), a) | (a, b) => {
                let a = expect_op_type(ctx, op, type_expr(ctx, vars, a, None)?)?;
                let b = expect_op_type(ctx, op, type_expr(ctx, vars, b, Some(a.ty_id()))?)?;
                Ok(Expr::Bin(op, Box::new(a), Box::new(b)))
            }
        },
        (SqlExpr::Bin(..), Some(id)) => Err(unexpected_type(Type::BOOL.with_ctx(ctx), id.try_with_ctx(ctx)?)),
    }
}

/// Parses a source text literal as a particular type
fn parse(ctx: &TyCtx, v: String, id: TyId) -> TypingResult<AlgebraicValue> {
    let err = |v, ty| TypingError::from(ConstraintViolation::lit(v, ty));
    match ctx.try_resolve(id)? {
        ty @ Type::Alg(AlgebraicType::I8) => v
            .parse::<i8>()
            .map(AlgebraicValue::I8)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::U8) => v
            .parse::<u8>()
            .map(AlgebraicValue::U8)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::I16) => v
            .parse::<i16>()
            .map(AlgebraicValue::I16)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::U16) => v
            .parse::<u16>()
            .map(AlgebraicValue::U16)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::I32) => v
            .parse::<i32>()
            .map(AlgebraicValue::I32)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::U32) => v
            .parse::<u32>()
            .map(AlgebraicValue::U32)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::I64) => v
            .parse::<i64>()
            .map(AlgebraicValue::I64)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::U64) => v
            .parse::<u64>()
            .map(AlgebraicValue::U64)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::F32) => v
            .parse::<f32>()
            .map(|v| AlgebraicValue::F32(v.into()))
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::F64) => v
            .parse::<f64>()
            .map(|v| AlgebraicValue::F64(v.into()))
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::I128) => v
            .parse::<i128>()
            .map(|v| AlgebraicValue::I128(v.into()))
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(AlgebraicType::U128) => v
            .parse::<u128>()
            .map(|v| AlgebraicValue::U128(v.into()))
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(t) if t.is_bytes() => from_hex_pad::<Vec<u8>, _>(&v)
            .map(|v| AlgebraicValue::Bytes(v.into_boxed_slice()))
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(t) if t.is_identity() => Identity::from_hex(&v)
            .map(AlgebraicValue::from)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty @ Type::Alg(t) if t.is_address() => Address::from_hex(&v)
            .map(AlgebraicValue::from)
            .map_err(|_| err(&v, ty.with_ctx(ctx))),
        ty => Err(err(&v, ty.with_ctx(ctx))),
    }
}

/// Returns a type constraint violation for an unexpected type
fn unexpected_type(expected: TypeWithCtx<'_>, inferred: TypeWithCtx<'_>) -> TypingError {
    ConstraintViolation::eq(expected, inferred).into()
}

/// Returns an error if the input type is not a table type [Type::Var]
fn expect_table_type(ctx: &TyCtx, expr: RelExpr) -> TypingResult<RelExpr> {
    match expr.ty(ctx)? {
        Type::Var(_) => Ok(expr),
        _ => Err(Unsupported::SubReturnType.into()),
    }
}

/// Assert that this type is compatible with this operator
fn expect_op_type(ctx: &TyCtx, op: BinOp, expr: Expr) -> TypingResult<Expr> {
    let ty = expr.ty(ctx)?;
    if ty.is_compatible_with(op) {
        Ok(expr)
    } else {
        Err(ConstraintViolation::bin(op, ty.with_ctx(ctx)).into())
    }
}

fn assert_eq_types(a: TypeWithCtx<'_>, b: TypeWithCtx<'_>) -> TypingResult<()> {
    if a == b {
        Ok(())
    } else {
        Err(unexpected_type(a, b))
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9Builder, AlgebraicType, ProductType};
    use spacetimedb_primitives::TableId;
    use spacetimedb_schema::{def::ModuleDef, schema::TableSchema};
    use std::sync::Arc;

    use super::{parse_and_type_sub, SchemaView};

    fn module_def() -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        builder.build_table_with_new_type(
            "t",
            ProductType::from([
                ("u32", AlgebraicType::U32),
                ("f32", AlgebraicType::F32),
                ("str", AlgebraicType::String),
                ("arr", AlgebraicType::array(AlgebraicType::String)),
            ]),
            true,
        );
        builder.build_table_with_new_type(
            "s",
            ProductType::from([
                ("id", AlgebraicType::identity()),
                ("u32", AlgebraicType::U32),
                ("arr", AlgebraicType::array(AlgebraicType::String)),
                ("bytes", AlgebraicType::bytes()),
            ]),
            true,
        );
        builder.finish().try_into().expect("failed to generate module def")
    }

    struct SchemaViewer(ModuleDef);

    impl SchemaView for SchemaViewer {
        fn schema(&self, name: &str, _: bool) -> Option<Arc<TableSchema>> {
            self.0.table(name).map(|def| {
                Arc::new(TableSchema::from_module_def(
                    def,
                    TableId(if *def.name == *"t" { 0 } else { 1 }),
                ))
            })
        }
    }

    #[test]
    fn valid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            "select * from t",
            "select * from t where true",
            "select * from t where t.u32 = 1",
            "select * from t where t.u32 = 1 or t.str = ''",
            "select * from s where s.bytes = 0xABCD",
            "select * from s where s.bytes = X'ABCD'",
            "select * from s as r where r.bytes = 0xABCD",
            "select * from (select t.* from t join s)",
            "select * from (select t.* from t join s on t.u32 = s.u32 where t.f32 = 0.1)",
            "select * from (select t.* from t join (select s.u32 from s) s on t.u32 = s.u32)",
            "select * from (select * from t union all select * from t)",
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn invalid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            // Table r does not exist
            "select * from r",
            // Field u32 is not in scope
            "select * from t where u32 = 1",
            // Field a does not exist on table t
            "select * from t where t.a = 1",
            // Field a does not exist on table t
            "select * from t as r where r.a = 1",
            // Field u32 is not a string
            "select * from t where t.u32 = 'str'",
            // Field u32 is not a float
            "select * from t where t.u32 = 1.3",
            // t is not in scope after alias
            "select * from t as r where t.u32 = 5",
            // Field u32 is not in scope
            "select u32 from t",
            // Subscriptions must be typed to a single table
            "select t.u32 from t",
            // Subscriptions must be typed to a single table
            "select * from t join s",
            // Product values are not comparable
            "select * from (select t.* from t join s on t.arr = s.arr)",
            // Subscriptions must be typed to a single table
            "select * from (select s.* from t join (select s.u32 from s) s on t.u32 = s.u32)",
            // Field bytes is no longer in scope
            "select * from (select t.* from t join (select s.u32 from s) s on s.bytes = 0xABCD)",
            // Union arguments are of different types
            "select * from (select * from t union all select * from s)",
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_err());
        }
    }
}
