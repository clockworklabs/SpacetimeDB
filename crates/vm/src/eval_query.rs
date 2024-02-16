use crate::errors::ErrorVm;
use crate::eval::{compile_query, compile_query_expr, optimize_query};
use crate::expr::{Code, CrudCode, CrudExpr, Expr, ExprAstPrinter, ExprOpt};
use crate::program::ProgramVm;

/// Optimize & compile the [CrudExpr] for late execution
#[tracing::instrument(skip_all)]
pub fn build_ast(ast: CrudExpr) -> Code {
    match optimize_query(ast) {
        ExprOpt::Query(q) => {
            let q = compile_query(*q);
            Code::Crud(CrudCode::Query(q))
        }
        ExprOpt::Crud(q) => compile_query_expr(*q),
        ExprOpt::Halt(err) => Code::Halt(err),
        x => unreachable!("{x}"),
    }
}

/// Execute the code
#[tracing::instrument(skip_all)]
pub fn eval<P: ProgramVm>(p: &mut P, code: Code) -> Code {
    match code {
        Code::Value(_) => code.clone(),
        Code::Block(lines) => {
            let mut result = Vec::with_capacity(lines.len());
            for x in lines {
                let r = eval(p, x);
                if r != Code::Pass {
                    result.push(r);
                }
            }

            match result.len() {
                0 => Code::Pass,
                1 => result.pop().unwrap(),
                _ => Code::Block(result),
            }
        }
        Code::Crud(q) => p.eval_query(q).unwrap_or_else(|err| Code::Halt(err.into())),
        Code::Pass => Code::Pass,
        Code::Halt(_) => code,
        Code::Fun(_) => Code::Pass,
        Code::Table(_) => code,
        x => unreachable!("{x}"),
    }
}

fn to_vec(of: Vec<Expr>) -> Code {
    let mut new = Vec::with_capacity(of.len());
    for ast in of {
        let code = match ast {
            Expr::Block(x) => to_vec(x),
            Expr::Crud(x) => build_ast(*x),
            x => Code::Halt(ErrorVm::Unsupported(format!("{x:?}")).into()),
        };
        new.push(code);
    }
    Code::Block(new)
}

/// Optimize, compile & run the [Expr]
#[tracing::instrument(skip_all)]
pub fn run_ast<P: ProgramVm>(p: &mut P, ast: Expr) -> Code {
    let code = match ast {
        Expr::Block(x) => to_vec(x),
        Expr::Crud(x) => build_ast(*x),
        x => Code::Halt(ErrorVm::Unsupported(format!("{}", ExprAstPrinter { ast: &x })).into()),
    };
    eval(p, code)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::*;
    use crate::dsl::{query, scalar};
    use crate::program::Program;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::operator::OpCmp;
    use spacetimedb_sats::db::auth::StAccess;
    use spacetimedb_sats::relation::{MemTable, RelValue};

    #[test]
    fn test_select() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let input = MemTable::from_value(scalar(1));
        let field = input.get_field_pos(0).unwrap().clone();

        let q = query(input).with_select_cmp(OpCmp::Eq, field, scalar(1));

        let head = q.source.head().clone();

        let result = run_ast(p, q.into());
        let row = RelValue::new(scalar(1).into(), None);
        assert_eq!(
            result,
            Code::Table(MemTable::new(head, StAccess::Public, [row].into())),
            "Query"
        );
    }
}
