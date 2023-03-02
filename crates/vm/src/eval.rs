use std::collections::HashMap;

use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::builtin_type::BuiltinType;
use spacetimedb_sats::relation::{Field, FieldExpr, RelIter, Relation, Table};

use crate::dsl::{bin_op, call_fn, if_, mem_table, scalar, var};
use crate::errors::{ErrorType, ErrorVm};
use crate::expr::{Code, Expr, ExprOpt, FunctionOpt, QueryCode, QueryExprOpt, SourceExpr, SourceExprOpt, TyExpr};
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
        Expr::Query(q) => {
            let source = match q.source {
                SourceExpr::Value(x) => SourceExprOpt::Value(TyExpr::new(x.clone(), x.type_of().into())),
                SourceExpr::MemTable(x) => {
                    SourceExprOpt::MemTable(TyExpr::new(x.clone(), Ty::Val(AlgebraicType::Product(x.head.head))))
                }
                SourceExpr::DbTable(x) => {
                    SourceExprOpt::DbTable(TyExpr::new(x.clone(), Ty::Val(AlgebraicType::Product(x.head.head))))
                }
            };

            ExprOpt::Query(Box::new(QueryExprOpt {
                source,
                query: q.query.clone(),
            }))
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
pub fn optimize<P: ProgramVm>(p: &mut P, code: Expr) -> Result<ExprOpt, ErrorType> {
    let result = build_typed(p, code);
    println!("{}", &result);
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

/// Second pass:
///
/// Compiles [Tree<ExprOpt>] into [Code] moving the execution into closures.
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
            let q = match q.source {
                SourceExprOpt::Value(x) => {
                    let data = mem_table(x.of.type_of().into(), vec![x.of]);
                    QueryCode {
                        data: Table::MemTable(data),
                        query: q.query.clone(),
                    }
                }
                SourceExprOpt::MemTable(x) => QueryCode {
                    data: Table::MemTable(x.of),
                    query: q.query.clone(),
                },
                SourceExprOpt::DbTable(x) => QueryCode {
                    data: Table::DbTable(x.of),
                    query: q.query.clone(),
                },
            };

            Code::Query(q)
        }
        x => todo!("{}", x),
    })
}

/// Third pass:
///
/// Execute the code
pub fn eval<P: ProgramVm>(p: &mut P, code: Code, deep: usize) -> Code {
    let key = format!("{:?}", &code);

    println!("{}{}", " ".repeat(deep), &key);

    //p.stats.entry(key).and_modify(|counter| *counter += 1).or_insert(1);

    match code {
        Code::Value(_) => code.clone(),
        Code::CallFn(id, old) => {
            let mut params = Vec::with_capacity(old.len());
            for param in old {
                let p = eval(p, param, deep + 1);
                let param = match p {
                    Code::Value(x) => x,
                    Code::Halt(x) => return Code::Halt(x),
                    Code::Pass => continue,
                    x => unreachable!("{x}"),
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
                let v = eval(p, v, deep + 1);
                p.add_ident(&k, v);
            }
            let r = eval(p, body, deep + 1);
            p.env_mut().pop_scope();

            r
        }
        Code::Ident(name) => p.find_ident(&name).unwrap().clone(),
        Code::If(inner) => {
            let (test, if_true, if_false) = &*inner;
            let test = eval(p, test.clone(), deep + 1);

            match test {
                Code::Value(x) => {
                    if x == AlgebraicValue::from(true) {
                        eval(p, if_true.clone(), deep + 1)
                    } else {
                        eval(p, if_false.clone(), deep + 1)
                    }
                }
                x => unimplemented!("{x}"),
            }
        }
        Code::Block(lines) => {
            let mut last = None;
            for x in lines {
                last = Some(eval(p, x, deep + 1));
            }

            if let Some(x) = last {
                x
            } else {
                Code::Pass
            }
        }
        Code::Query(q) => {
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

pub fn build_query(mut result: Box<IterRows>, query: Vec<Query>) -> Result<Box<IterRows<'_>>, ErrorVm> {
    for q in query {
        result = match q {
            Query::Select(cmp) => {
                let iter = result.select(move |row| Ok(cmp.compare(row)));
                Box::new(iter)
            }
            Query::Project(cols) => {
                if cols.is_empty() {
                    result
                } else {
                    let iter = result.project(&cols.clone(), move |row| Ok(row.project(&cols)?))?;
                    Box::new(iter)
                }
            }
            Query::JoinInner(q) => {
                //Pick the smaller set to be at the left
                let col_lhs = FieldExpr::Name(q.col_lhs);
                let col_rhs = FieldExpr::Name(q.col_rhs);
                let col_key = col_rhs.clone();
                let row_rhs = q.rhs.row_count();

                let rhs = Box::new(RelIter::new(q.rhs.head(), row_rhs, q.rhs)) as Box<IterRows<'_>>;

                let (lhs, rhs) = if result.row_count() < row_rhs {
                    (result, rhs)
                } else {
                    (rhs, result)
                };
                let iter = lhs.join_inner(
                    rhs,
                    move |row| {
                        let f = row.get(&col_key);
                        Ok(match f {
                            Field::Name(x) => x.clone(),
                            Field::Value(x) => x.into(),
                        })
                    },
                    move |lhs, rhs| {
                        let lhs = lhs.get(&col_lhs);
                        let rhs = rhs.get(&col_rhs);
                        Ok(lhs == rhs)
                    },
                )?;
                Box::new(iter)
            }
        };
    }
    Ok(result)
}

/// Optimize & compile the [Expr] for late execution
pub fn build_ast<P: ProgramVm>(p: &mut P, ast: Expr) -> Result<Code, ErrorVm> {
    let ast = optimize(p, ast)?;
    compile(p, ast)
}

/// Optimize, compile & run the [Expr]
pub fn run_ast<P: ProgramVm>(p: &mut P, ast: Expr) -> Code {
    match build_ast(p, ast) {
        Ok(code) => eval(p, code, 0),
        Err(err) => Code::Halt(err.into()),
    }
}

// Used internally for testing recursion
#[doc(hidden)]
pub fn fibo(input: u64) -> Expr {
    let kind = AlgebraicType::Builtin(BuiltinType::U64);

    let less = |val: u64| bin_op(OpMath::Minus, var("n"), scalar(val));

    let f = Function::new(
        "fib",
        &[Param::new("n", kind.clone())],
        kind,
        &[if_(
            bin_op(OpCmp::Less, var("n"), scalar(2u64)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{mem_table, prefix_op, query, value};
    use crate::errors::ErrorVm;
    use crate::program::Program;
    use spacetimedb_sats::product;
    use spacetimedb_sats::product_type::ProductType;
    use spacetimedb_sats::relation::{FieldName, MemTable};

    fn fib(n: u64) -> u64 {
        dbg!(n, n < 2);
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

    #[test]
    fn test_optimize_values() {
        let p = &mut Program::new();
        let zero = scalar(0);

        let x = value(zero);
        let ast = optimize(p, x);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_eval_scalar() {
        let p = &mut Program::new();
        let zero = scalar(0);
        assert_eq!(run_ast(p, zero.clone().into()), Code::Value(zero));
    }

    #[test]
    fn test_optimize_ops() {
        let p = &mut Program::new();

        let zero = scalar(0);
        let one = scalar(1);

        let plus = bin_op(OpMath::Add, zero, one);

        let ast = optimize(p, plus);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_math() {
        let p = &mut Program::new();
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
        let nums = prefix_op(OpMath::Add, (0..9i64).into_iter().map(value));
        let result = run_ast(p, nums);

        let total = (0..9i64).into_iter().reduce(|a, b| a + b).unwrap();
        assert_eq!(result, Code::Value(scalar(total)), "+ range");
    }

    #[test]
    fn test_logic() {
        let p = &mut Program::new();

        let a = scalar(true);
        let b = scalar(false);

        let result = run_bin_op(p, OpCmp::Eq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "Eq");

        let result = run_bin_op(p, OpCmp::NotEq, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "NotEq");

        let result = run_bin_op(p, OpCmp::Less, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "Less");

        let result = run_bin_op(p, OpCmp::LessThan, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(false)), "LessThan");

        let result = run_bin_op(p, OpCmp::Greater, a.clone(), b.clone());
        assert_eq!(result, Code::Value(scalar(true)), "Greater");

        let result = run_bin_op(p, OpCmp::GreaterThan, a.clone(), b.clone());
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
        let p = &mut Program::new();

        let a = scalar(1);
        let b = scalar(2);

        let check = if_(bin_op(OpCmp::Eq, scalar(false), scalar(false)), a.clone(), b);
        let result = run_ast(p, check);
        assert_eq!(result, Code::Value(a), "if false = false then 1 else 2");
    }

    #[test]
    fn test_fun() {
        let p = &mut Program::new();
        let kind = AlgebraicType::Builtin(BuiltinType::U64);
        let f = Function::new(
            "sum",
            &[Param::new("a", kind.clone()), Param::new("b", kind.clone())],
            kind,
            &[bin_op(OpMath::Add, var("a"), var("b"))],
        );

        let f = Expr::Fun(f);

        let check = Expr::Block(vec![f, call_fn("sum", &[("a", scalar(1u64)), ("b", scalar(2u64))])]);
        let result = run_ast(p, check);
        let a = scalar(3u64);
        dbg!(&result);
        assert_eq!(result, Code::Value(a), "Sum");
    }

    #[test]
    fn test_fibonacci() {
        let p = &mut Program::new();
        let input = 2;
        let check = fibo(input);
        let result = run_ast(p, check);
        let a = scalar(fib(input));

        assert_eq!(result, Code::Value(a), "Fib");
    }

    #[test]
    fn test_select() {
        let p = &mut Program::new();
        let input = scalar(1);
        let q = query(input.clone()).with_select(OpCmp::Eq, 0, input.clone());

        let head = q.source.head();

        let result = run_ast(p, q.into());
        assert_eq!(result, Code::Table(MemTable::new(&head, &[input.into()])), "Query");
    }

    #[test]
    fn test_project() {
        let p = &mut Program::new();
        let input = scalar(1);
        let source = query(input.clone());
        let q = source.clone().with_project(&[FieldName::Pos(0)]);
        let head = q.source.head();

        let result = run_ast(p, q.into());
        assert_eq!(result, Code::Table(MemTable::new(&head, &[input.into()])), "Project");

        let q = source.with_project(&[FieldName::Pos(1)]);

        let result = run_ast(p, q.into());
        assert_eq!(
            result,
            Code::Halt(ErrorVm::FieldNotFound(FieldName::Pos(1)).into()),
            "Bad Project"
        );
    }

    #[test]
    fn test_join_inner() {
        let p = &mut Program::new();
        let input = scalar(1);
        let q = query(input).with_join_inner(scalar(1), FieldName::Pos(0), FieldName::Pos(0));
        let result = run_ast(p, q.into());

        //The expected result
        let inv = ProductType::from_iter([BuiltinType::I32, BuiltinType::I32]);
        let row = product!(scalar(1), scalar(1));
        let input = mem_table(inv, vec![row]);

        assert_eq!(result, Code::Table(input), "Project");
    }

    #[test]
    fn test_query_logic() {
        let p = &mut Program::new();

        let inv = ProductType::from_iter([("id", BuiltinType::U64), ("name", BuiltinType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let inv = input.clone();

        let q = query(input.clone()).with_select(OpLogic::And, scalar(true), scalar(true));

        let result = run_ast(p, q.into());

        assert_eq!(result, Code::Table(inv.clone()), "Query And");

        let q = query(input).with_select(OpLogic::Or, scalar(true), scalar(false));

        let result = run_ast(p, q.into());

        assert_eq!(result, Code::Table(inv), "Query Or");
    }

    #[test]
    /// Inventory
    /// | id: u64 | name : String |
    fn test_query() {
        let p = &mut Program::new();

        let inv = ProductType::from_iter([("id", BuiltinType::U64), ("name", BuiltinType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);

        let q = query(input).with_join_inner(scalar(1u64), FieldName::Pos(0), FieldName::Pos(0));

        let result = run_ast(p, q.into());

        //The expected result
        let inv = ProductType::from_iter([
            (None, BuiltinType::U64),
            (Some("id"), BuiltinType::U64),
            (Some("name"), BuiltinType::String),
        ]);
        let row = product!(scalar(1u64), scalar(1u64), scalar("health"));
        let input = mem_table(inv, vec![row]);

        assert_eq!(result, Code::Table(input), "Project");
    }

    #[test]
    /// Inventory
    /// | inventory_id: u64 | name : String |
    /// Player
    /// | entity_id: u64 | inventory_id : u64 |
    /// Location
    /// | entity_id: u64 | x : f32 | z : f32 |
    fn test_query_game() {
        let p = &mut Program::new();

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row = product!(1u64, "health");
        let inv = mem_table(head, [row]);

        let head = ProductType::from_iter([("entity_id", BuiltinType::U64), ("inventory_id", BuiltinType::U64)]);
        let row1 = product!(100u64, 1u64);
        let row2 = product!(200u64, 1u64);
        let row3 = product!(300u64, 1u64);
        let player = mem_table(head, [row1, row2, row3]);

        let head = ProductType::from_iter([
            ("entity_id", BuiltinType::U64),
            ("x", BuiltinType::F32),
            ("z", BuiltinType::F32),
        ]);
        let row1 = product!(100u64, 0.0f32, 32.0f32);
        let row2 = product!(100u64, 1.0f32, 31.0f32);
        let location = mem_table(head, [row1, row2]);

        let entity_id = FieldName::Name("entity_id".into());
        let inventory_id = FieldName::Name("inventory_id".into());
        let name = FieldName::Name("name".into());
        let x = FieldName::Name("x".into());
        let z = FieldName::Name("z".into());
        // SELECT
        // Player.*
        //     FROM
        // Player
        // JOIN Location
        // ON Location.entity_id = Player.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        // let q = query(player.clone())
        //     .with_join_inner(location.clone(), entity_id.clone(), entity_id.clone())
        //     .with_select(OpCmp::Greater, x.clone(), scalar(0.0))
        //     .with_select(OpCmp::LessThan, x.clone(), scalar(32.0))
        //     .with_select(OpCmp::Greater, z.clone(), scalar(0.0))
        //     .with_select(OpCmp::LessThan, z.clone(), scalar(32.0))
        //     .with_project(&[entity_id.clone(), inventory_id.clone()]);
        //
        // let result = run_ast(p, q.into());
        //
        // let head = ProductType::from_iter([("entity_id", BuiltinType::U64), ("inventory_id", BuiltinType::U64)]);
        // let row1 = product!(100u64, 1u64);
        // let input = mem_table(head, [row1]);
        //
        // assert_eq!(result, Code::Table(input), "Player");

        // SELECT
        // Inventory.*
        //     FROM
        // Inventory
        // JOIN Player
        // ON Inventory.Inventory_id = Player.inventory_id
        // JOIN Location
        // ON Location.entity_id = Player.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        let q = query(inv)
            .with_join_inner(player, inventory_id.clone(), inventory_id.clone())
            .with_join_inner(location, entity_id.clone(), entity_id)
            .with_select(OpCmp::Greater, x.clone(), scalar(0.0f32))
            .with_select(OpCmp::LessThan, x, scalar(32.0f32))
            .with_select(OpCmp::Greater, z.clone(), scalar(0.0f32))
            .with_select(OpCmp::LessThan, z, scalar(32.0f32))
            .with_project(&[inventory_id, name]);

        let result = run_ast(p, q.into());

        let head = ProductType::from_iter([("inventory_id", BuiltinType::U64), ("name", BuiltinType::String)]);
        let row1 = product!(1u64, "health");
        let input = mem_table(head, [row1]);

        assert_eq!(result, Code::Table(input), "Inventory");
    }
}
