use crate::dsl::mem_table;
use crate::errors::ErrorVm;
use crate::expr::{Code, CrudCode, CrudExpr, QueryCode, QueryExpr, SourceExpr};
use crate::expr::{Expr, Query};
use crate::iterators::RelIter;
use crate::program::ProgramVm;
use crate::rel_ops::RelOps;
use crate::relation::{MemTable, RelValue, Table};
use spacetimedb_sats::relation::{FieldExpr, Relation};
use spacetimedb_sats::{product, AlgebraicType, ProductType};

fn compile_query(q: QueryExpr) -> QueryCode {
    match q.source {
        SourceExpr::MemTable(x) => QueryCode {
            table: Table::MemTable(x),
            query: q.query.clone(),
        },
        SourceExpr::DbTable(x) => QueryCode {
            table: Table::DbTable(x),
            query: q.query.clone(),
        },
    }
}

fn compile_query_expr(q: CrudExpr) -> Code {
    match q {
        CrudExpr::Query(q) => Code::Crud(CrudCode::Query(compile_query(q))),
        CrudExpr::Insert { source, rows } => {
            let q = match source {
                SourceExpr::MemTable(x) => CrudCode::Insert {
                    table: Table::MemTable(x),
                    rows,
                },
                SourceExpr::DbTable(x) => CrudCode::Insert {
                    table: Table::DbTable(x),
                    rows,
                },
            };
            Code::Crud(q)
        }
        CrudExpr::Update { delete, assignments } => {
            let delete = compile_query(delete);
            Code::Crud(CrudCode::Update { delete, assignments })
        }
        CrudExpr::Delete { query } => {
            let query = compile_query(query);
            Code::Crud(CrudCode::Delete { query })
        }
        CrudExpr::CreateTable { table } => Code::Crud(CrudCode::CreateTable { table }),
        CrudExpr::Drop {
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

pub type IterRows<'a> = dyn RelOps<'a> + 'a;

#[tracing::instrument(skip_all)]
pub fn build_query<'a>(mut result: Box<IterRows<'a>>, query: Vec<Query>) -> Result<Box<IterRows<'a>>, ErrorVm> {
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
                    let iter = result.project(cols, move |cols, row| {
                        Ok(RelValue::Projection(row.project_owned(cols, &header)?))
                    })?;
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
                    move |row| Ok(row.get(&key_lhs, &key_lhs_header)?.into_owned().into()),
                    move |row| Ok(row.get(&key_rhs, &key_rhs_header)?.into_owned().into()),
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

/// Optimize & compile the [CrudExpr] for late execution
#[tracing::instrument(skip_all)]
fn build_ast(ast: CrudExpr) -> Code {
    compile_query_expr(ast)
}

/// Execute the code
#[tracing::instrument(skip_all)]
fn eval<P: ProgramVm>(p: &mut P, code: Code) -> Code {
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
        Code::Table(_) => code,
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
        Expr::Value(x) => Code::Value(x),
        Expr::Halt(err) => Code::Halt(err),
        Expr::Ident(x) => Code::Halt(ErrorVm::Unsupported(format!("Ident {x}")).into()),
    };
    eval(p, code)
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
    use crate::dsl::{query, scalar};
    use crate::program::Program;
    use crate::relation::MemTable;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::operator::{OpCmp, OpLogic};
    use spacetimedb_sats::db::auth::StAccess;
    use spacetimedb_sats::db::error::RelationError;
    use spacetimedb_sats::relation::FieldName;

    fn run_query(p: &mut Program, ast: Expr) -> MemTable {
        match run_ast(p, ast) {
            Code::Table(x) => x,
            x => panic!("Unexpected result on query: {x}"),
        }
    }

    #[test]
    fn test_select() {
        let p = &mut Program::new(AuthCtx::for_testing());
        let input = MemTable::from_value(scalar(1));
        let field = input.get_field_pos(0).unwrap().clone();

        let q = query(input).with_select_cmp(OpCmp::Eq, field, scalar(1));

        let head = q.source.head().clone();

        let result = run_ast(p, q.into());
        let row = scalar(1).into();
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
        let row = input.into();
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
