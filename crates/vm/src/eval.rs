use std::collections::HashMap;

use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::relation::{FieldExpr, MemTable, RelIter, Relation, Table};
use spacetimedb_sats::{product, ProductType, ProductValue};

use crate::dsl::{bin_op, call_fn, if_, mem_table, scalar, var};
use crate::errors::{ErrorKind, ErrorLang, ErrorType, ErrorVm};
use crate::expr::{
    Code, CrudCode, CrudExpr, CrudExprOpt, Expr, ExprOpt, FunctionOpt, QueryCode, QueryExpr, QueryExprOpt, SourceExpr,
    SourceExprOpt, TyExpr,
};
use crate::expr::{Function, Query};
use crate::functions::{Args, Param};
use crate::operator::*;
use crate::program::ProgramVm;
use crate::rel_ops::RelOps;
use crate::typecheck::check_types;
use crate::types::{ty_op, Ty};

fn to_vec<P: ProgramVm>(p: &mut P, of: &[Expr]) -> Vec<ExprOpt> {
    let mut new = Vec::with_capacity(of.len());
    for x in of {
        new.push(build_typed(p, x.clone()));
    }
    new
}

fn build_source(source: SourceExpr) -> SourceExprOpt {
    match source {
        SourceExpr::MemTable(x) => {
            SourceExprOpt::MemTable(TyExpr::new(x.clone(), Ty::Val(AlgebraicType::Product(x.head.ty()))))
        }
        SourceExpr::DbTable(x) => {
            SourceExprOpt::DbTable(TyExpr::new(x.clone(), Ty::Val(AlgebraicType::Product(x.head.ty()))))
        }
    }
}

fn build_query_opt(q: QueryExpr) -> QueryExprOpt {
    let source = build_source(q.source);

    QueryExprOpt { source, query: q.query }
}

fn build_typed<P: ProgramVm>(p: &mut P, node: Expr) -> ExprOpt {
    match node {
        Expr::Value(x) => ExprOpt::Value(TyExpr::new(x.clone(), x.type_of().into())),
        Expr::Ty(x) => ExprOpt::Ty(Ty::Val(x)),
        Expr::Op(op, args) => {
            let new = to_vec(p, &args);
            ExprOpt::Op(TyExpr::new(op, Ty::Multi(ty_op(op))), new)
        }
        Expr::If(inner) => {
            let (test, if_true, if_false) = *inner;
            let test = build_typed(p, test);
            let if_true = build_typed(p, if_true);
            let if_false = build_typed(p, if_false);

            ExprOpt::If(Box::new((test, if_true, if_false)))
        }
        Expr::Fun(f) => {
            p.env_mut().ty.add(&f.head.name, f.head.result.clone().into());

            ExprOpt::Fun(FunctionOpt::new(f.head, &to_vec(p, &f.body)))
        }
        Expr::Block(lines) => ExprOpt::Block(to_vec(p, &lines)),
        Expr::Ident(name) => ExprOpt::Ident(name),
        Expr::CallFn(name, params) => {
            if p.env_mut().functions.get_by_name(&name).is_some() {
                let params = params.into_iter().map(|(k, v)| (k, build_typed(p, v)));
                ExprOpt::CallFn(name, params.map(|(_, v)| v).collect())
            } else {
                let params = params.into_iter().map(|(k, v)| (k, build_typed(p, v)));
                ExprOpt::CallLambda(name, params.collect())
            }
        }
        Expr::Crud(q) => {
            let q = q.optimize(&|_, _| i64::MAX);
            match q {
                CrudExpr::Query(q) => {
                    let source = build_query_opt(q);

                    ExprOpt::Query(Box::new(source))
                }
                CrudExpr::Insert { source, rows: data } => {
                    let source = build_source(source);
                    let mut rows = Vec::with_capacity(data.len());
                    for x in data {
                        let mut row = Vec::with_capacity(x.len());
                        for v in x {
                            match v {
                                FieldExpr::Name(x) => {
                                    todo!("Deal with idents in insert?: {}", x)
                                }
                                FieldExpr::Value(x) => {
                                    row.push(x);
                                }
                            }
                        }
                        rows.push(ProductValue::new(&row))
                    }
                    ExprOpt::Crud(Box::new(CrudExprOpt::Insert { source, rows }))
                }
                CrudExpr::Update { delete, assignments } => {
                    let delete = build_query_opt(delete);

                    ExprOpt::Crud(Box::new(CrudExprOpt::Update { delete, assignments }))
                }
                CrudExpr::Delete { query } => {
                    let query = build_query_opt(query);

                    ExprOpt::Crud(Box::new(CrudExprOpt::Delete { query }))
                }
                CrudExpr::CreateTable { table } => ExprOpt::Crud(Box::new(CrudExprOpt::CreateTable { table })),
                CrudExpr::Drop {
                    name,
                    kind,
                    table_access,
                } => ExprOpt::Crud(Box::new(CrudExprOpt::Drop {
                    name,
                    kind,
                    table_access,
                })),
            }
        }
        x => {
            todo!("{:?}", x)
        }
    }
}

/// First pass:
///
/// Compile the [Expr] into a type-annotated AST [Tree<ExprOpt>].
///
/// Then validate & type-check it.
#[tracing::instrument(skip_all)]
pub fn optimize<P: ProgramVm>(p: &mut P, code: Expr) -> Result<ExprOpt, ErrorType> {
    let result = build_typed(p, code);
    check_types(&mut p.env_mut().ty, &result)?;

    Ok(result)
}

fn collect_vec<P: ProgramVm>(p: &mut P, iter: impl ExactSizeIterator<Item = ExprOpt>) -> Result<Vec<Code>, ErrorVm> {
    let mut code = Vec::with_capacity(iter.len());

    for x in iter {
        code.push(compile(p, x)?);
    }
    Ok(code)
}

fn collect_map<P: ProgramVm>(
    p: &mut P,
    iter: impl ExactSizeIterator<Item = (String, ExprOpt)>,
) -> Result<HashMap<String, Code>, ErrorVm> {
    let mut code = HashMap::with_capacity(iter.len());

    for (name, x) in iter {
        code.insert(name, compile(p, x)?);
    }
    Ok(code)
}

fn compile_query(q: QueryExprOpt) -> QueryCode {
    match q.source {
        SourceExprOpt::Value(x) => {
            let data = mem_table(x.of.type_of(), vec![x.of]);
            QueryCode {
                table: Table::MemTable(data),
                query: q.query.clone(),
            }
        }
        SourceExprOpt::MemTable(x) => QueryCode {
            table: Table::MemTable(x.of),
            query: q.query.clone(),
        },
        SourceExprOpt::DbTable(x) => QueryCode {
            table: Table::DbTable(x.of),
            query: q.query.clone(),
        },
    }
}

/// Second pass:
///
/// Compiles [Tree<ExprOpt>] into [Code] moving the execution into closures.
#[tracing::instrument(skip_all)]
fn compile<P: ProgramVm>(p: &mut P, node: ExprOpt) -> Result<Code, ErrorVm> {
    Ok(match node {
        ExprOpt::Value(x) => Code::Value(x.of),
        ExprOpt::Block(lines) => Code::Block(collect_vec(p, lines.into_iter())?),
        ExprOpt::Op(op, args) => {
            let function_id = p.env_mut().functions.get_function_id_op(op.of);
            let args = collect_vec(p, args.into_iter())?;

            Code::CallFn(function_id, args)
        }
        ExprOpt::Fun(f) => {
            p.add_lambda(f.head.clone(), Code::Pass);
            let body = collect_vec(p, f.body.into_iter())?;
            p.update_lambda(f.head, Code::Block(body));

            Code::Pass
        }
        ExprOpt::CallFn(name, args) => {
            let args: Vec<_> = collect_vec(p, args.into_iter())?;
            let f = p.env_mut().functions.get_by_name(&name).unwrap();

            Code::CallFn(f.idx, args)
        }
        ExprOpt::CallLambda(name, args) => {
            let args: HashMap<_, _> = collect_map(p, args.into_iter())?;
            let fid = p.env_mut().lambdas.get_id(&name).unwrap();
            Code::CallLambda(fid, args)
        }
        ExprOpt::Let(inner) => {
            let (name, rhs) = *inner;

            let rhs = compile(p, rhs)?;
            p.add_ident(&name, rhs);
            Code::Pass
        }
        ExprOpt::Ident(name) => Code::Ident(name),
        ExprOpt::Halt(x) => Code::Halt(x),
        ExprOpt::If(inner) => {
            let (test, if_true, if_false) = &*inner;
            let test = compile(p, test.clone())?;
            let if_true = compile(p, if_true.clone())?;
            let if_false = compile(p, if_false.clone())?;

            Code::If(Box::new((test, if_true, if_false)))
        }
        ExprOpt::Query(q) => {
            let q = compile_query(*q);
            Code::Crud(CrudCode::Query(q))
        }
        ExprOpt::Crud(q) => {
            let q = *q;

            match q {
                CrudExprOpt::Insert { source, rows } => {
                    let q = match source {
                        SourceExprOpt::Value(x) => {
                            let data = mem_table(x.of.type_of(), vec![x.of]);
                            CrudCode::Insert {
                                table: Table::MemTable(data),
                                rows,
                            }
                        }
                        SourceExprOpt::MemTable(x) => CrudCode::Insert {
                            table: Table::MemTable(x.of),
                            rows,
                        },
                        SourceExprOpt::DbTable(x) => CrudCode::Insert {
                            table: Table::DbTable(x.of),
                            rows,
                        },
                    };
                    Code::Crud(q)
                }
                CrudExprOpt::Update { delete, assignments } => {
                    let delete = compile_query(delete);
                    Code::Crud(CrudCode::Update { delete, assignments })
                }
                CrudExprOpt::Delete { query } => {
                    let query = compile_query(query);
                    Code::Crud(CrudCode::Delete { query })
                }
                CrudExprOpt::CreateTable { table } => Code::Crud(CrudCode::CreateTable { table }),
                CrudExprOpt::Drop {
                    name,
                    kind,
                    table_access,
                } => Code::Crud(CrudCode::Drop {
                    name,
                    kind,
                    table_access,
                }),
            }
        }
        x => todo!("{}", x),
    })
}

/// Third pass:
///
/// Execute the code
#[tracing::instrument(skip_all)]
pub fn eval<P: ProgramVm>(p: &mut P, code: Code) -> Code {
    match code {
        Code::Value(_) => code.clone(),
        Code::CallFn(id, old) => {
            let mut params = Vec::with_capacity(old.len());
            for param in old {
                let param = match eval(p, param) {
                    Code::Value(x) => x,
                    Code::Halt(x) => return Code::Halt(x),
                    Code::Pass => continue,
                    x => {
                        let name = &p.env().functions.get(id).unwrap().name;

                        return Code::Halt(ErrorLang::new(
                            ErrorKind::Params,
                            Some(&format!("Invalid parameter `{x}` calling function {name}")),
                        ));
                    }
                };

                params.push(param);
            }
            let f = p.env().functions.get(id).unwrap();

            let args = match params.len() {
                1 => Args::Unary(&params[0]),
                2 => Args::Binary(&params[0], &params[1]),
                _ => Args::Splat(&params),
            };
            f.call(p, args)
        }
        Code::CallLambda(id, args) => {
            let f = p.env().lambdas.get(id).unwrap();
            let body = f.body.clone();
            p.env_mut().push_scope();
            for (k, v) in args {
                let v = eval(p, v);
                p.add_ident(&k, v);
            }
            let r = eval(p, body);
            p.env_mut().pop_scope();

            r
        }
        Code::Ident(name) => p.find_ident(&name).unwrap().clone(),
        Code::If(inner) => {
            let (test, if_true, if_false) = &*inner;
            let test = eval(p, test.clone());

            match test {
                Code::Value(x) => {
                    if x == AlgebraicValue::from(true) {
                        eval(p, if_true.clone())
                    } else {
                        eval(p, if_false.clone())
                    }
                }
                x => unimplemented!("{x}"),
            }
        }
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
                1 => result[0].clone(),
                _ => Code::Block(result),
            }
        }
        Code::Crud(q) => {
            let result = p.eval_query(q);

            match result {
                Ok(x) => x,
                Err(err) => Code::Halt(err.into()),
            }
        }
        Code::Pass => Code::Pass,
        Code::Halt(_) => code,
        Code::Fun(_) => Code::Pass,
        Code::Table(_) => code,
    }
}

pub type IterRows<'a> = dyn RelOps + 'a;

#[tracing::instrument(skip_all)]
pub fn build_query(mut result: Box<IterRows>, query: Vec<Query>) -> Result<Box<IterRows<'_>>, ErrorVm> {
    for q in query {
        result = match q {
            Query::IndexScan(_) => {
                panic!("index scans unsupported on memory tables")
            }
            Query::IndexJoin(_) => {
                panic!("index joins unsupported on memory tables")
            }
            Query::Select(cmp) => {
                let header = result.head().clone();
                let iter = result.select(move |row| cmp.compare(row, &header));
                Box::new(iter)
            }
            Query::Project(cols, _) => {
                if cols.is_empty() {
                    result
                } else {
                    let header = result.head().clone();
                    let iter = result.project(&cols.clone(), move |row| Ok(row.project(&cols, &header)?))?;
                    Box::new(iter)
                }
            }
            Query::JoinInner(q) => {
                //Pick the smaller set to be at the left
                let col_lhs = FieldExpr::Name(q.col_lhs);
                let col_rhs = FieldExpr::Name(q.col_rhs);
                let key_lhs = col_lhs.clone();
                let key_rhs = col_rhs.clone();
                let row_rhs = q.rhs.source.row_count();

                let head = q.rhs.source.head().clone();
                let rhs = match q.rhs.source {
                    SourceExpr::MemTable(x) => Box::new(RelIter::new(head, row_rhs, x)) as Box<IterRows<'_>>,
                    SourceExpr::DbTable(_) => {
                        // let iter = stdb.scan(tx, x.table_id)?;
                        //
                        // Box::new(TableCursor::new(x, iter)?) as Box<IterRows<'_>>
                        //
                        // Box::new(RelIter::new(q.rhs.head(), row_rhs, x)) as Box<IterRows<'_>>;
                        todo!("How pass the db iter?")
                    }
                };

                let rhs = build_query(rhs, q.rhs.query)?;

                let lhs = result;
                let key_lhs_header = lhs.head().clone();
                let key_rhs_header = rhs.head().clone();
                let col_lhs_header = lhs.head().clone();
                let col_rhs_header = rhs.head().clone();

                let iter = lhs.join_inner(
                    rhs,
                    col_lhs_header.extend(&col_rhs_header),
                    move |row| {
                        let f = row.get(&key_lhs, &key_lhs_header)?;
                        Ok(f.into())
                    },
                    move |row| {
                        let f = row.get(&key_rhs, &key_rhs_header)?;
                        Ok(f.into())
                    },
                    move |l, r| {
                        let l = l.get(&col_lhs, &col_lhs_header)?;
                        let r = r.get(&col_rhs, &col_rhs_header)?;
                        Ok(l == r)
                    },
                    move |l, r| l.extend(r),
                )?;
                Box::new(iter)
            }
        };
    }
    Ok(result)
}

/// Optimize & compile the [Expr] for late execution
#[tracing::instrument(skip_all)]
pub fn build_ast<P: ProgramVm>(p: &mut P, ast: Expr) -> Result<Code, ErrorVm> {
    let ast = optimize(p, ast)?;
    compile(p, ast)
}

/// Optimize, compile & run the [Expr]
#[tracing::instrument(skip_all)]
pub fn run_ast<P: ProgramVm>(p: &mut P, ast: Expr) -> Code {
    match build_ast(p, ast) {
        Ok(code) => eval(p, code),
        Err(err) => Code::Halt(err.into()),
    }
}

// Used internally for testing recursion
#[doc(hidden)]
pub fn fibo(input: u64) -> Expr {
    let ty = AlgebraicType::U64;

    let less = |val: u64| bin_op(OpMath::Minus, var("n"), scalar(val));

    let f = Function::new(
        "fib",
        &[Param::new("n", ty.clone())],
        ty,
        &[if_(
            bin_op(OpCmp::Lt, var("n"), scalar(2u64)),
            var("n"),
            bin_op(
                OpMath::Add,
                call_fn("fib", &[("n", less(1))]),
                call_fn("fib", &[("n", less(2))]),
            ),
        )],
    );
    let f = Expr::Fun(f);

    Expr::Block(vec![f, call_fn("fib", &[("n", scalar(input))])])
}

// Used internally for testing SQL JOINS
#[doc(hidden)]
pub struct GameData {
    pub location: MemTable,
    pub inv: MemTable,
    pub player: MemTable,
}
// Used internally for testing  SQL JOINS
#[doc(hidden)]
pub fn create_game_data() -> GameData {
    let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
    let row = product!(1u64, "health");
    let inv = mem_table(head, [row]);

    let head = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
    let row1 = product!(100u64, 1u64);
    let row2 = product!(200u64, 1u64);
    let row3 = product!(300u64, 1u64);
    let player = mem_table(head, [row1, row2, row3]);

    let head = ProductType::from([
        ("entity_id", AlgebraicType::U64),
        ("x", AlgebraicType::F32),
        ("z", AlgebraicType::F32),
    ]);
    let row1 = product!(100u64, 0.0f32, 32.0f32);
    let row2 = product!(100u64, 1.0f32, 31.0f32);
    let location = mem_table(head, [row1, row2]);

    GameData { location, inv, player }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::*;
    use crate::dsl::{prefix_op, query, value};
    use crate::program::Program;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_sats::db::auth::StAccess;
    use spacetimedb_sats::db::error::RelationError;
    use spacetimedb_sats::relation::{FieldName, MemTable, RelValue};

    fn fib(n: u64) -> u64 {
        if n < 2 {
            return n;
        }

        fib(n - 1) + fib(n - 2)
    }

    fn run_bin_op<O, A, B>(p: &mut Program, op: O, lhs: A, rhs: B) -> Code
    where
        O: Into<Op>,
        A: Into<Expr>,
        B: Into<Expr>,
    {
        let ast = bin_op(op, lhs, rhs);
        run_ast(p, ast)
    }

    pub fn run_query(p: &mut Program, ast: Expr) -> MemTable {
        match run_ast(p, ast) {
            Code::Table(x) => x,
            x => panic!("Unexpected result on query: {x}"),
        }
    }

    #[test]
    fn test_optimize_values() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let zero = scalar(0);

        let x = value(zero);
        let ast = optimize(p, x);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_eval_scalar() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let zero = scalar(0);
        assert_eq!(run_ast(p, zero.clone().into()), Code::Value(zero));
    }

    #[test]
    fn test_optimize_ops() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let zero = scalar(0);
        let one = scalar(1);

        let plus = bin_op(OpMath::Add, zero, one);

        let ast = optimize(p, plus);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_math() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let one = scalar(1);
        let two = scalar(2);

        let result = run_bin_op(p, OpMath::Add, one.clone(), two.clone());
        assert_eq!(result, Code::Value(scalar(3)), "+");

        let result = run_bin_op(p, OpMath::Minus, one.clone(), two.clone());
        assert_eq!(result, Code::Value(scalar(-1)), "-");

        let result = run_bin_op(p, OpMath::Mul, one.clone(), two.clone());
        assert_eq!(result, Code::Value(scalar(2)), "*");

        let result = run_bin_op(p, OpMath::Div, one, two);
        assert_eq!(result, Code::Value(scalar(0)), "/ Int");

        let result = run_bin_op(p, OpMath::Div, scalar(1.0), scalar(2.0));
        assert_eq!(result, Code::Value(scalar(0.5)), "/ Float");

        // Checking a vectorized ops: 0 + 1 + 2...
        let nums = prefix_op(OpMath::Add, (0..9i64).map(value));
        let result = run_ast(p, nums);

        let total = (0..9i64).reduce(|a, b| a + b).unwrap();
        assert_eq!(result, Code::Value(scalar(total)), "+ range");
    }

    #[test]
    fn test_logic() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let a = scalar(true);
        let b = scalar(false);

        let result = run_bin_op(p, OpCmp::Eq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "Eq");

        let result = run_bin_op(p, OpCmp::NotEq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "NotEq");

        let result = run_bin_op(p, OpCmp::Lt, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "Less");

        let result = run_bin_op(p, OpCmp::LtEq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "LessThan");

        let result = run_bin_op(p, OpCmp::Gt, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "Greater");

        let result = run_bin_op(p, OpCmp::GtEq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "GreaterThan");

        let result = run_bin_op(p, OpLogic::And, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "And");

        let result = run_bin_op(p, OpUnary::Not, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "Not");

        let result = run_bin_op(p, OpLogic::Or, a, b);
        assert_eq!(result, Code::Value(scalar(true)), "Or");
    }

    #[test]
    fn test_eval_if() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let a = scalar(1);
        let b = scalar(2);

        let check = if_(bin_op(OpCmp::Eq, scalar(false), scalar(false)), a.clone(), b);
        let result = run_ast(p, check);
        assert_eq!(result, Code::Value(a), "if false = false then 1 else 2");
    }

    #[test]
    fn test_fun() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let ty = AlgebraicType::U64;
        let f = Function::new(
            "sum",
            &[Param::new("a", ty.clone()), Param::new("b", ty.clone())],
            ty,
            &[bin_op(OpMath::Add, var("a"), var("b"))],
        );

        let f = Expr::Fun(f);

        let check = Expr::Block(vec![f, call_fn("sum", &[("a", scalar(1u64)), ("b", scalar(2u64))])]);
        let result = run_ast(p, check);
        let a = scalar(3u64);
        assert_eq!(result, Code::Value(a), "Sum");
    }

    #[test]
    fn test_fibonacci() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let input = 2;
        let check = fibo(input);
        let result = run_ast(p, check);
        let a = scalar(fib(input));

        assert_eq!(result, Code::Value(a), "Fib");
    }

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

    #[test]
    fn test_project() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let input = scalar(1);
        let table = MemTable::from_value(scalar(1));
        let field = table.get_field_pos(0).unwrap().clone();

        let source = query(table.clone());
        let q = source.clone().with_project(&[field.into()], None);
        let head = q.source.head().clone();

        let result = run_ast(p, q.into());
        let row = RelValue::new(input.into(), None);
        assert_eq!(
            result,
            Code::Table(MemTable::new(head.clone(), StAccess::Public, [row].into())),
            "Project"
        );

        let field = FieldName::positional(&table.head.table_name, 1);
        let q = source.with_project(&[field.clone().into()], None);

        let result = run_ast(p, q.into());
        assert_eq!(
            result,
            Code::Halt(RelationError::FieldNotFound(head, field).into()),
            "Bad Project"
        );
    }

    #[test]
    fn test_join_inner() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let table = MemTable::from_value(scalar(1));
        let field = table.get_field_pos(0).unwrap().clone();

        let q = query(table.clone()).with_join_inner(table, field.clone(), field);
        let result = match run_ast(p, q.into()) {
            Code::Table(x) => x,
            x => panic!("Invalid result {x}"),
        };

        //The expected result
        let inv = ProductType::from([(None, AlgebraicType::I32), (Some("0_0"), AlgebraicType::I32)]);
        let row = product!(scalar(1), scalar(1));
        let input = mem_table(inv, vec![row]);

        println!("{}", &result.head);
        println!("{}", &input.head);

        assert_eq!(result.as_without_table_name(), input.as_without_table_name(), "Project");
    }

    #[test]
    fn test_query_logic() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let inv = input.clone();

        let q = query(input.clone()).with_select_cmp(OpLogic::And, scalar(true), scalar(true));

        let result = run_ast(p, q.into());

        assert_eq!(result, Code::Table(inv.clone()), "Query And");

        let q = query(input).with_select_cmp(OpLogic::Or, scalar(true), scalar(false));

        let result = run_ast(p, q.into());

        assert_eq!(result, Code::Table(inv), "Query Or");
    }

    #[test]
    /// Inventory
    /// | id: u64 | name : String |
    fn test_query() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let field = input.get_field_pos(0).unwrap().clone();

        let q = query(input.clone()).with_join_inner(input, field.clone(), field);

        let result = match run_ast(p, q.into()) {
            Code::Table(x) => x,
            x => panic!("Invalid result {x}"),
        };

        //The expected result
        let inv = ProductType::from([
            (None, AlgebraicType::U64),
            (Some("id"), AlgebraicType::U64),
            (Some("name"), AlgebraicType::String),
        ]);
        let row = product!(scalar(1u64), scalar("health"), scalar(1u64), scalar("health"));
        let input = mem_table(inv, vec![row]);
        assert_eq!(result.data, input.data, "Project");
    }

    #[test]
    /// Inventory
    /// | inventory_id: u64 | name : String |
    /// Player
    /// | entity_id: u64 | inventory_id : u64 |
    /// Location
    /// | entity_id: u64 | x : f32 | z : f32 |
    fn test_query_game() {
        let p = &mut Program::new(AuthCtx::for_testing());

        let data = create_game_data();

        let location_entity_id = data.location.get_field_named("entity_id").unwrap().clone();
        let inv_inventory_id = data.inv.get_field_named("inventory_id").unwrap().clone();
        let player_inventory_id = data.player.get_field_named("inventory_id").unwrap().clone();
        let player_entity_id = data.player.get_field_named("entity_id").unwrap().clone();

        let inv_name = data.inv.get_field_named("name").unwrap().clone();
        let location_x = data.location.get_field_named("x").unwrap().clone();
        let location_z = data.location.get_field_named("z").unwrap().clone();

        // SELECT
        // Player.*
        //     FROM
        // Player
        // JOIN Location
        // ON Location.entity_id = Player.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        let q = query(data.player.clone())
            .with_join_inner(
                data.location.clone(),
                player_entity_id.clone(),
                location_entity_id.clone(),
            )
            .with_select_cmp(OpCmp::Gt, location_x.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_x.clone(), scalar(32.0f32))
            .with_select_cmp(OpCmp::Gt, location_z.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_z.clone(), scalar(32.0f32))
            .with_project(
                &[player_entity_id.clone().into(), player_inventory_id.clone().into()],
                None,
            );

        let result = run_query(p, q.into());

        let head = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
        let row1 = product!(100u64, 1u64);
        let input = mem_table(head, [row1]);

        assert_eq!(result.as_without_table_name(), input.as_without_table_name(), "Player");

        // SELECT
        // Inventory.*
        //     FROM
        // Inventory
        // JOIN Player
        // ON Inventory.inventory_id = Player.inventory_id
        // JOIN Location
        // ON Player.entity_id = Location.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        let q = query(data.inv)
            .with_join_inner(data.player, inv_inventory_id.clone(), player_inventory_id)
            .with_join_inner(data.location, player_entity_id, location_entity_id)
            .with_select_cmp(OpCmp::Gt, location_x.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_x, scalar(32.0f32))
            .with_select_cmp(OpCmp::Gt, location_z.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_z, scalar(32.0f32))
            .with_project(&[inv_inventory_id.into(), inv_name.into()], None);

        let result = run_query(p, q.into());

        let head = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row1 = product!(1u64, "health");
        let input = mem_table(head, [row1]);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Inventory"
        );
    }
}
