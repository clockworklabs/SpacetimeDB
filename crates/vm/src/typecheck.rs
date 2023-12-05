use crate::env::EnvTy;
use crate::errors::ErrorType;
use crate::expr::{CrudExprOpt, ExprOpt, SourceExprOpt};
use crate::types::Ty;
use spacetimedb_sats::algebraic_type::AlgebraicType;

fn get_type<'a>(_env: &'a mut EnvTy, node: &'a ExprOpt) -> &'a Ty {
    match node {
        ExprOpt::Value(x) => &x.ty,
        ExprOpt::Ty(x) => x,
        ExprOpt::Op(x, _) => &x.ty,
        ExprOpt::Fun(_) => {
            todo!()
        }
        ExprOpt::CallFn(..) => {
            todo!()
        }
        ExprOpt::CallLambda(..) => {
            todo!()
        }
        ExprOpt::Param(inner) => {
            let (_, node) = &**inner;
            get_type(_env, node)
        }
        ExprOpt::Let(inner) => {
            let (_, node) = &**inner;
            get_type(_env, node)
        }
        ExprOpt::Ident(_) => {
            todo!()
        }
        ExprOpt::If(_) => {
            todo!()
        }
        ExprOpt::Block(_) => {
            todo!()
        }
        ExprOpt::Halt(_) => {
            todo!()
        }
        ExprOpt::Query(_) => {
            todo!()
        }
        ExprOpt::Crud(_) => todo!(),
    }
}

fn ty_source(source: &SourceExprOpt) -> Ty {
    match source {
        SourceExprOpt::Value(x) => x.ty.clone(),
        SourceExprOpt::MemTable(x) => x.ty.clone(),
        SourceExprOpt::DbTable(x) => x.ty.clone(),
    }
}

pub(crate) fn check_types(env: &mut EnvTy, ast: &ExprOpt) -> Result<Ty, ErrorType> {
    match ast {
        ExprOpt::Op(op, args) => {
            if op.of.is_logical() {
                return Ok(Ty::Val(AlgebraicType::Bool));
            }

            let expects = match &op.ty {
                Ty::Multi(x) => x.clone(),
                _ => {
                    todo!("Check type")
                }
            };

            let mut args = args.iter();

            let expect = if let Some(child) = args.next() {
                let found = get_type(env, child);
                if expects.contains(found) {
                    found.clone()
                } else {
                    return Err(ErrorType::Expect(op.ty.clone(), found.clone()));
                }
            } else {
                return Err(ErrorType::OpMiss(op.of, 2, 1));
            };

            for child in args {
                let found = get_type(env, child);
                if &expect != found {
                    return Err(ErrorType::Expect(expect, found.clone()));
                }
            }
            Ok(expect)
        }
        ExprOpt::If(inner) => {
            let (test, if_true, if_false) = &**inner;
            let expect = Ty::Val(AlgebraicType::Bool);
            let found = check_types(env, test)?;
            if check_types(env, test)? == expect {
                let lhs = check_types(env, if_true)?;
                let rhs = check_types(env, if_false)?;

                if lhs == rhs {
                    Ok(lhs)
                } else {
                    Err(ErrorType::Expect(lhs, rhs))
                }
            } else {
                Err(ErrorType::Expect(expect, found))
            }
        }
        ExprOpt::Value(x) => Ok(x.ty.clone()),
        ExprOpt::Ty(x) => Ok(x.clone()),
        ExprOpt::Fun(f) => Ok(f.head.result.clone().into()),
        ExprOpt::CallLambda(name, _) => {
            let ty = env.get(name).expect("Lambda not found for type check").clone();
            Ok(ty)
        }
        ExprOpt::CallFn(name, _) => {
            let ty = env.get(name).expect("Function not found for type check").clone();
            Ok(ty)
        }
        ExprOpt::Let(inner) => {
            let (name, expr) = &**inner;

            let ty = check_types(env, expr)?;
            env.add(name, ty.clone());
            Ok(ty)
        }
        ExprOpt::Ident(name) => Ok(env.get(name).unwrap().clone()),
        ExprOpt::Block(x) => {
            if let Some(x) = x.last() {
                check_types(env, x)
            } else {
                Ok(Ty::Unknown)
            }
        }
        ExprOpt::Query(q) => Ok(ty_source(&q.source)),
        ExprOpt::Crud(q) => {
            let q = &**q;
            match q {
                CrudExprOpt::Insert { source, .. } => Ok(ty_source(source)),
                CrudExprOpt::Update { delete, .. } => Ok(ty_source(&delete.source)),
                CrudExprOpt::Delete { query } => Ok(ty_source(&query.source)),
                CrudExprOpt::CreateTable { table } => {
                    Ok(AlgebraicType::Product(table.columns.iter().map(|x| x.col_type.clone()).collect()).into())
                }
                CrudExprOpt::Drop { .. } => {
                    //todo: Extract the type from the catalog...
                    Ok(Ty::Unknown)
                }
            }
        }
        x => {
            todo!("{x:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_sats::algebraic_type::AlgebraicType;

    use crate::dsl::{bin_op, scalar};
    use crate::eval::optimize;
    use crate::expr::Expr;
    use crate::operator::OpMath;
    use crate::program::Program;

    fn _expect_ast(p: &mut Program, given: Expr, _expect: AlgebraicType) {
        match optimize(p, given) {
            Ok(ast) => {
                println!("{}", ast);
            }
            Err(err) => {
                eprintln!("{}", err);
            }
        }
    }

    // #[test]
    // fn ty_value() {
    //     let p = &mut Program::new();
    //     let zero = scalar(0);
    //     _expect_ast(p, zero, AlgebraicType::I32)
    // }

    #[test]
    fn ty_op() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let ast = bin_op(OpMath::Add, scalar(0), scalar(1));
        _expect_ast(p, ast, AlgebraicType::I32)
    }
}
