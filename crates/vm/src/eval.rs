use crate::errors::ErrorVm;
use crate::expr::{Code, SourceExpr, SourceSet};
use crate::expr::{Expr, Query};
use crate::iterators::RelIter;
use crate::program::{ProgramVm, Sources};
use crate::rel_ops::RelOps;
use crate::relation::{MemTable, RelValue};
use spacetimedb_sats::relation::{FieldExprRef, Relation};
use std::sync::Arc;

pub type IterRows<'a> = dyn RelOps<'a> + 'a;

/// `sources` should be a `Vec`
/// where the `idx`th element is the table referred to in the `query` as `SourceId(idx)`.
/// While constructing the query, the `sources` will be destructively modified with `Option::take`
/// to extract the sources,
/// so the `query` cannot refer to the same `SourceId` multiple times.
pub fn build_query<'a, const N: usize>(
    mut result: Box<IterRows<'a>>,
    query: &'a [Query],
    sources: Sources<'_, N>,
) -> Result<Box<IterRows<'a>>, ErrorVm> {
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
                let col_lhs = FieldExprRef::Name(&q.col_lhs);
                let col_rhs = FieldExprRef::Name(&q.col_rhs);

                let rhs = build_source_expr_query(sources, &q.rhs.source);
                let rhs = build_query(rhs, &q.rhs.query, sources)?;

                let lhs = result;
                let key_lhs_header = lhs.head().clone();
                let key_rhs_header = rhs.head().clone();
                let col_lhs_header = lhs.head().clone();
                let col_rhs_header = rhs.head().clone();

                if q.semi {
                    let iter = lhs.join_inner(
                        rhs,
                        col_lhs_header.clone(),
                        move |row| Ok(row.get(col_lhs, &key_lhs_header)?.into_owned().into()),
                        move |row| Ok(row.get(col_rhs, &key_rhs_header)?.into_owned().into()),
                        move |l, r| {
                            let l = l.get(col_lhs, &col_lhs_header)?;
                            let r = r.get(col_rhs, &col_rhs_header)?;
                            Ok(l == r)
                        },
                        |l, _| l,
                    )?;
                    Box::new(iter)
                } else {
                    let iter = lhs.join_inner(
                        rhs,
                        Arc::new(col_lhs_header.extend(&col_rhs_header)),
                        move |row| Ok(row.get(col_lhs, &key_lhs_header)?.into_owned().into()),
                        move |row| Ok(row.get(col_rhs, &key_rhs_header)?.into_owned().into()),
                        move |l, r| {
                            let l = l.get(col_lhs, &col_lhs_header)?;
                            let r = r.get(col_rhs, &col_rhs_header)?;
                            Ok(l == r)
                        },
                        move |l, r| l.extend(r),
                    )?;
                    Box::new(iter)
                }
            }
        };
    }
    Ok(result)
}

pub(crate) fn build_source_expr_query<'a, const N: usize>(
    sources: Sources<'_, N>,
    source: &SourceExpr,
) -> Box<IterRows<'a>> {
    let source_id = source.source_id().unwrap_or_else(|| todo!("How pass the db iter?"));
    let head = source.head().clone();
    let rc = source.row_count();
    match sources.take(source_id) {
        None => {
            panic!("Query plan specifies in-mem table for {source_id:?}, but found a `DbTable` or nothing")
        }
        Some(t) => Box::new(RelIter::new(head, rc, t.data.into_iter().map(RelValue::Projection))) as Box<IterRows<'a>>,
    }
}

/// Execute the code
pub fn eval<const N: usize, P: ProgramVm>(p: &mut P, code: Code, sources: Sources<'_, N>) -> Code {
    match code {
        c @ (Code::Value(_) | Code::Halt(_) | Code::Table(_)) => c,
        Code::Block(lines) => {
            let mut result = Vec::with_capacity(lines.len());
            for x in lines {
                let r = eval(p, x, sources);
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
        Code::Crud(q) => p.eval_query(q, sources).unwrap_or_else(|err| Code::Halt(err.into())),
        Code::Pass => Code::Pass,
    }
}

fn to_vec(of: Vec<Expr>) -> Code {
    let mut new = Vec::with_capacity(of.len());
    for ast in of {
        let code = match ast {
            Expr::Block(x) => to_vec(x),
            Expr::Crud(x) => Code::Crud(*x),
            x => Code::Halt(ErrorVm::Unsupported(format!("{x:?}")).into()),
        };
        new.push(code);
    }
    Code::Block(new)
}

/// Optimize, compile & run the [Expr]
pub fn run_ast<const N: usize, P: ProgramVm>(p: &mut P, ast: Expr, mut sources: SourceSet<MemTable, N>) -> Code {
    let code = match ast {
        Expr::Block(x) => to_vec(x),
        Expr::Crud(x) => Code::Crud(*x),
        Expr::Value(x) => Code::Value(x),
        Expr::Halt(err) => Code::Halt(err),
        Expr::Ident(x) => Code::Halt(ErrorVm::Unsupported(format!("Ident {x}")).into()),
    };
    eval(p, code, &mut sources)
}

/// Used internally for testing SQL JOINS.
#[doc(hidden)]
pub mod test_data {
    use crate::{dsl::mem_table, relation::MemTable};
    use spacetimedb_sats::{product, AlgebraicType, ProductType};

    pub struct GameData {
        pub location: MemTable,
        pub inv: MemTable,
        pub player: MemTable,
    }

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
}

#[cfg(test)]
pub mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::test_data::*;
    use super::*;
    use crate::dsl::{mem_table, query, scalar};
    use crate::expr::SourceSet;
    use crate::program::Program;
    use crate::relation::MemTable;
    use spacetimedb_lib::operator::{OpCmp, OpLogic};
    use spacetimedb_sats::db::auth::StAccess;
    use spacetimedb_sats::db::error::RelationError;
    use spacetimedb_sats::relation::FieldName;
    use spacetimedb_sats::{product, AlgebraicType, ProductType};

    fn run_query<const N: usize>(p: &mut Program, ast: Expr, sources: SourceSet<MemTable, N>) -> MemTable {
        match run_ast(p, ast, sources) {
            Code::Table(x) => x,
            x => panic!("Unexpected result on query: {x}"),
        }
    }

    #[test]
    fn test_select() {
        let p = &mut Program;
        let input = MemTable::from_value(scalar(1));
        let field = input.get_field_pos(0).unwrap().clone();
        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input);

        let q = query(source_expr).with_select_cmp(OpCmp::Eq, field, scalar(1));

        let head = q.source.head().clone();

        let result = run_ast(p, q.into(), sources);
        let row = scalar(1).into();
        assert_eq!(
            result,
            Code::Table(MemTable::new(head, StAccess::Public, [row].into())),
            "Query"
        );
    }

    #[test]
    fn test_project() {
        let p = &mut Program;
        let input = scalar(1);
        let table = MemTable::from_value(scalar(1));

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(table.clone());

        let source = query(source_expr);
        let field = table.get_field_pos(0).unwrap().clone();
        let q = source.clone().with_project(&[field.into()], None);
        let head = q.source.head().clone();

        let result = run_ast(p, q.into(), sources);
        let row = input.into();
        assert_eq!(
            result,
            Code::Table(MemTable::new(head.clone(), StAccess::Public, [row].into())),
            "Project"
        );

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(table.clone());

        let source = query(source_expr);
        let field = FieldName::positional(&table.head.table_name, 1);
        let q = source.with_project(&[field.clone().into()], None);

        let result = run_ast(p, q.into(), sources);
        assert_eq!(
            result,
            Code::Halt(RelationError::FieldNotFound(head.clone_for_error(), field).into()),
            "Bad Project"
        );
    }

    #[test]
    fn test_join_inner() {
        let p = &mut Program;
        let table = MemTable::from_value(scalar(1));
        let field = table.get_field_pos(0).unwrap().clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(table.clone());
        let second_source_expr = sources.add_mem_table(table);

        let q = query(source_expr).with_join_inner(second_source_expr, field.clone(), field, false);
        let result = match run_ast(p, q.into(), sources) {
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
    fn test_semijoin() {
        let p = &mut Program;
        let table = MemTable::from_value(scalar(1));
        let field = table.get_field_pos(0).unwrap().clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(table.clone());
        let second_source_expr = sources.add_mem_table(table);

        let q = query(source_expr).with_join_inner(second_source_expr, field.clone(), field, true);
        let result = match run_ast(p, q.into(), sources) {
            Code::Table(x) => x,
            x => panic!("Invalid result {x}"),
        };

        //The expected result
        let inv = ProductType::from([(None, AlgebraicType::I32)]);
        let row = product!(scalar(1));
        let input = mem_table(inv, vec![row]);

        println!("{}", &result.head);
        println!("{}", &input.head);

        assert_eq!(
            result.as_without_table_name(),
            input.as_without_table_name(),
            "Semijoin should not be projected",
        );
    }

    #[test]
    fn test_query_logic() {
        let p = &mut Program;

        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let inv = input.clone();

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input.clone());

        let q = query(source_expr.clone()).with_select_cmp(OpLogic::And, scalar(true), scalar(true));

        let result = run_ast(p, q.into(), sources);

        assert_eq!(result, Code::Table(inv.clone()), "Query And");

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input);

        let q = query(source_expr).with_select_cmp(OpLogic::Or, scalar(true), scalar(false));

        let result = run_ast(p, q.into(), sources);

        assert_eq!(result, Code::Table(inv), "Query Or");
    }

    #[test]
    /// Inventory
    /// | id: u64 | name : String |
    fn test_query_inner_join() {
        let p = &mut Program;

        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let field = input.get_field_pos(0).unwrap().clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(input.clone());
        let second_source_expr = sources.add_mem_table(input);

        let q = query(source_expr).with_join_inner(second_source_expr, field.clone(), field, false);

        let result = match run_ast(p, q.into(), sources) {
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
    /// | id: u64 | name : String |
    fn test_query_semijoin() {
        let p = &mut Program;

        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product!(scalar(1u64), scalar("health"));

        let input = mem_table(inv, vec![row]);
        let field = input.get_field_pos(0).unwrap().clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(input.clone());
        let second_source_expr = sources.add_mem_table(input);

        let q = query(source_expr).with_join_inner(second_source_expr, field.clone(), field, true);

        let result = match run_ast(p, q.into(), sources) {
            Code::Table(x) => x,
            x => panic!("Invalid result {x}"),
        };

        //The expected result
        let inv = ProductType::from([(None, AlgebraicType::U64), (Some("name"), AlgebraicType::String)]);
        let row = product!(scalar(1u64), scalar("health"));
        let input = mem_table(inv, vec![row]);
        assert_eq!(result.data, input.data, "Semijoin should not project");
    }

    #[test]
    /// Inventory
    /// | inventory_id: u64 | name : String |
    /// Player
    /// | entity_id: u64 | inventory_id : u64 |
    /// Location
    /// | entity_id: u64 | x : f32 | z : f32 |
    fn test_query_game() {
        let p = &mut Program;

        let data = create_game_data();

        let location_entity_id = data.location.get_field_named("entity_id").unwrap().clone();
        let inv_inventory_id = data.inv.get_field_named("inventory_id").unwrap().clone();
        let player_inventory_id = data.player.get_field_named("inventory_id").unwrap().clone();
        let player_entity_id = data.player.get_field_named("entity_id").unwrap().clone();

        let inv_name = data.inv.get_field_named("name").unwrap().clone();
        let location_x = data.location.get_field_named("x").unwrap().clone();
        let location_z = data.location.get_field_named("z").unwrap().clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let player_source_expr = sources.add_mem_table(data.player.clone());
        let location_source_expr = sources.add_mem_table(data.location.clone());

        // SELECT
        // Player.*
        //     FROM
        // Player
        // JOIN Location
        // ON Location.entity_id = Player.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        let q = query(player_source_expr)
            .with_join_inner(
                location_source_expr,
                player_entity_id.clone(),
                location_entity_id.clone(),
                true,
            )
            .with_select_cmp(OpCmp::Gt, location_x.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_x.clone(), scalar(32.0f32))
            .with_select_cmp(OpCmp::Gt, location_z.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_z.clone(), scalar(32.0f32));

        let result = run_query(p, q.into(), sources);

        let head = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
        let row1 = product!(100u64, 1u64);
        let input = mem_table(head, [row1]);

        assert_eq!(result.as_without_table_name(), input.as_without_table_name(), "Player");

        let mut sources = SourceSet::<_, 3>::empty();
        let player_source_expr = sources.add_mem_table(data.player);
        let location_source_expr = sources.add_mem_table(data.location);
        let inventory_source_expr = sources.add_mem_table(data.inv);

        // SELECT
        // Inventory.*
        //     FROM
        // Inventory
        // JOIN Player
        // ON Inventory.inventory_id = Player.inventory_id
        // JOIN Location
        // ON Player.entity_id = Location.entity_id
        // WHERE x > 0 AND x <= 32 AND z > 0 AND z <= 32
        let q = query(inventory_source_expr)
            // NOTE: The way this query is set up, the first join must be an inner join, not a semijoin,
            // so that the second join has access to the `Player.entity_id` field.
            // This necessitates a trailing `project` to get just `Inventory.*`.
            .with_join_inner(player_source_expr, inv_inventory_id.clone(), player_inventory_id, false)
            .with_join_inner(location_source_expr, player_entity_id, location_entity_id, true)
            .with_select_cmp(OpCmp::Gt, location_x.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_x, scalar(32.0f32))
            .with_select_cmp(OpCmp::Gt, location_z.clone(), scalar(0.0f32))
            .with_select_cmp(OpCmp::LtEq, location_z, scalar(32.0f32))
            .with_project(&[inv_inventory_id.into(), inv_name.into()], None);

        let result = run_query(p, q.into(), sources);

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
