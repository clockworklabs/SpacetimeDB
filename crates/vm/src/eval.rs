use crate::errors::ErrorVm;
use crate::expr::{Code, ColumnOp, Expr, JoinExpr, ProjectExpr, SourceSet};
use crate::program::{ProgramVm, Sources};
use crate::rel_ops::RelOps;
use crate::relation::RelValue;
use spacetimedb_sats::ProductValue;

pub type IterRows<'a> = dyn RelOps<'a> + 'a;

pub fn build_select<'a>(base: impl RelOps<'a> + 'a, cmp: &'a ColumnOp) -> Box<IterRows<'a>> {
    Box::new(base.select(move |row| cmp.compare(row)))
}

pub fn build_project<'a>(base: impl RelOps<'a> + 'a, proj: &'a ProjectExpr) -> Box<IterRows<'a>> {
    Box::new(base.project(&proj.cols, move |cols, row| {
        RelValue::Projection(row.project_owned(cols))
    }))
}

pub fn join_inner<'a>(lhs: impl RelOps<'a> + 'a, rhs: impl RelOps<'a> + 'a, q: &'a JoinExpr) -> Box<IterRows<'a>> {
    let col_lhs = q.col_lhs.idx();
    let col_rhs = q.col_rhs.idx();
    let key_lhs = move |row: &RelValue<'_>| row.read_column(col_lhs).unwrap().into_owned();
    let key_rhs = move |row: &RelValue<'_>| row.read_column(col_rhs).unwrap().into_owned();
    let pred = move |l: &RelValue<'_>, r: &RelValue<'_>| l.read_column(col_lhs) == r.read_column(col_rhs);

    if q.inner.is_some() {
        Box::new(lhs.join_inner(rhs, key_lhs, key_rhs, pred, move |l, r| l.extend(r)))
    } else {
        Box::new(lhs.join_inner(rhs, key_lhs, key_rhs, pred, move |l, _| l))
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
pub fn run_ast<const N: usize, P: ProgramVm>(
    p: &mut P,
    ast: Expr,
    mut sources: SourceSet<Vec<ProductValue>, N>,
) -> Code {
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
pub mod test_helpers {
    use crate::relation::MemTable;
    use core::hash::BuildHasher as _;
    use spacetimedb_data_structures::map::DefaultHashBuilder;
    use spacetimedb_primitives::TableId;
    use spacetimedb_sats::relation::{Column, FieldName, Header};
    use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, ProductType, ProductValue};
    use std::sync::Arc;

    pub fn mem_table_without_table_name(mem: &MemTable) -> (&[Column], &[ProductValue]) {
        (&mem.head.fields, &mem.data)
    }

    pub fn header_for_mem_table(table_id: TableId, fields: ProductType) -> Header {
        let hash = DefaultHashBuilder::default().hash_one(&fields);
        let table_name = format!("mem#{:x}", hash).into();

        let cols = Vec::from(fields.elements)
            .into_iter()
            .enumerate()
            .map(|(pos, f)| Column::new(FieldName::new(table_id, pos.into()), f.algebraic_type))
            .collect();

        Header::new(table_id, table_name, cols, Vec::new())
    }

    pub fn mem_table_one_u64(table_id: TableId) -> MemTable {
        let ty = ProductType::from([AlgebraicType::U64]);
        mem_table(table_id, ty, product![1u64])
    }

    pub fn mem_table<T: Into<ProductValue>>(
        table_id: TableId,
        ty: impl Into<ProductType>,
        iter: impl IntoIterator<Item = T>,
    ) -> MemTable {
        let head = header_for_mem_table(table_id, ty.into());
        MemTable::from_iter(Arc::new(head), iter.into_iter().map(Into::into))
    }

    pub fn scalar(of: impl Into<AlgebraicValue>) -> AlgebraicValue {
        of.into()
    }

    pub struct GameData {
        pub location: MemTable,
        pub inv: MemTable,
        pub player: MemTable,
        pub location_ty: ProductType,
        pub inv_ty: ProductType,
        pub player_ty: ProductType,
    }

    pub fn create_game_data() -> GameData {
        let inv_ty = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row = product!(1u64, "health");
        let inv = mem_table(0.into(), inv_ty.clone(), [row]);

        let player_ty = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
        let row1 = product!(100u64, 1u64);
        let row2 = product!(200u64, 1u64);
        let row3 = product!(300u64, 1u64);
        let player = mem_table(1.into(), player_ty.clone(), [row1, row2, row3]);

        let location_ty = ProductType::from([
            ("entity_id", AlgebraicType::U64),
            ("x", AlgebraicType::F32),
            ("z", AlgebraicType::F32),
        ]);
        let row1 = product!(100u64, 0.0f32, 32.0f32);
        let row2 = product!(100u64, 1.0f32, 31.0f32);
        let location = mem_table(2.into(), location_ty.clone(), [row1, row2]);

        GameData {
            location,
            inv,
            player,
            inv_ty,
            player_ty,
            location_ty,
        }
    }
}

#[cfg(test)]
pub mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::test_helpers::*;
    use super::*;
    use crate::expr::{CrudExpr, Query, QueryExpr, SourceExpr, SourceSet};
    use crate::iterators::RelIter;
    use crate::relation::MemTable;
    use spacetimedb_lib::operator::{OpCmp, OpLogic};
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::db::error::RelationError;
    use spacetimedb_sats::relation::{FieldName, Header};
    use spacetimedb_sats::{product, AlgebraicType, ProductType};

    /// From an original source of `result`s, applies `queries` and returns a final set of results.
    fn build_query<'a, const N: usize>(
        mut result: Box<IterRows<'a>>,
        queries: &'a [Query],
        sources: Sources<'_, N>,
    ) -> Box<IterRows<'a>> {
        for q in queries {
            result = match q {
                Query::IndexScan(_) | Query::IndexJoin(_) => panic!("unsupported on memory tables"),
                Query::Select(cmp) => build_select(result, cmp),
                Query::Project(proj) => build_project(result, proj),
                Query::JoinInner(q) => {
                    let rhs = build_source_expr_query(sources, &q.rhs.source);
                    let rhs = build_query(rhs, &q.rhs.query, sources);
                    join_inner(result, rhs, q)
                }
            };
        }
        result
    }

    fn build_source_expr_query<'a, const N: usize>(sources: Sources<'_, N>, source: &SourceExpr) -> Box<IterRows<'a>> {
        let source_id = source.source_id().unwrap();
        let table = sources.take(source_id).unwrap();
        Box::new(RelIter::new(table.into_iter().map(RelValue::Projection)))
    }

    /// A default program that run in-memory without a database
    struct Program;

    impl ProgramVm for Program {
        fn eval_query<const N: usize>(&mut self, query: CrudExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
            match query {
                CrudExpr::Query(query) => {
                    let result = build_source_expr_query(sources, &query.source);
                    let rows = build_query(result, &query.query, sources).collect_vec(|row| row.into_product_value());

                    let head = query.head().clone();

                    Ok(Code::Table(MemTable::new(head, query.source.table_access(), rows)))
                }
                _ => todo!(),
            }
        }
    }

    fn run_query<const N: usize>(ast: Expr, sources: SourceSet<Vec<ProductValue>, N>) -> MemTable {
        match run_ast(&mut Program, ast, sources) {
            Code::Table(x) => x,
            x => panic!("Unexpected result on query: {x}"),
        }
    }

    fn get_field_pos(table: &MemTable, pos: usize) -> FieldName {
        *table.head.fields.get(pos).map(|x| &x.field).unwrap()
    }

    #[test]
    fn test_select() {
        let input = mem_table_one_u64(0.into());
        let field = get_field_pos(&input, 0);
        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input);

        let q = QueryExpr::new(source_expr)
            .with_select_cmp(OpCmp::Eq, field, scalar(1u64))
            .unwrap();

        let head = q.head().clone();

        let result = run_query(q.into(), sources);
        let row = product![1u64];
        assert_eq!(result, MemTable::from_iter(head, [row]), "Query");
    }

    #[test]
    fn test_project() {
        let p = &mut Program;
        let table = mem_table_one_u64(0.into());

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(table.clone());

        let source = QueryExpr::new(source_expr);
        let field = get_field_pos(&table, 0);
        let q = source.clone().with_project([field.into()].into(), None).unwrap();
        let head = q.head().clone();

        let result = run_ast(p, q.into(), sources);
        let row = product![1u64];
        assert_eq!(result, Code::Table(MemTable::from_iter(head.clone(), [row])), "Project");
    }

    #[test]
    fn test_project_out_of_bounds() {
        let table = mem_table_one_u64(0.into());

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(table.clone());

        let source = QueryExpr::new(source_expr);
        // This field is out of bounds of `table`'s header, so `run_ast` will panic.
        let field = FieldName::new(table.head.table_id, 1.into());
        assert!(matches!(
            source.with_project([field.into()].into(), None).unwrap_err(),
            RelationError::FieldNotFound(_, f) if f == field,
        ));
    }

    #[test]
    fn test_join_inner() {
        let table_id = 0.into();
        let table = mem_table_one_u64(table_id);
        let col: ColId = 0.into();
        let field = table.head.fields[col.idx()].clone();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(table.clone());
        let second_source_expr = sources.add_mem_table(table);

        let q = QueryExpr::new(source_expr).with_join_inner(second_source_expr, col, col, false);
        dbg!(&q);
        let result = run_query(q.into(), sources);

        // The expected result.
        let head = Header::new(table_id, "".into(), [field.clone(), field].into(), Vec::new());
        let input = MemTable::from_iter(head.into(), [product!(1u64, 1u64)]);

        println!("{}", &result.head);
        println!("{}", &input.head);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Project"
        );
    }

    #[test]
    fn test_semijoin() {
        let table_id = 0.into();
        let table = mem_table_one_u64(table_id);
        let col = 0.into();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(table.clone());
        let second_source_expr = sources.add_mem_table(table);

        let q = QueryExpr::new(source_expr).with_join_inner(second_source_expr, col, col, true);
        let result = run_query(q.into(), sources);

        // The expected result.
        let inv = ProductType::from([(None, AlgebraicType::U64)]);
        let input = mem_table(table_id, inv, [product![1u64]]);

        println!("{}", &result.head);
        println!("{}", &input.head);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Semijoin should not be projected",
        );
    }

    #[test]
    fn test_query_logic() {
        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product![1u64, "health"];

        let input = mem_table(0.into(), inv, vec![row]);
        let inv = input.clone();

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input.clone());

        let q = QueryExpr::new(source_expr.clone())
            .with_select_cmp(OpLogic::And, scalar(true), scalar(true))
            .unwrap();

        let result = run_query(q.into(), sources);

        assert_eq!(result, inv.clone(), "Query And");

        let mut sources = SourceSet::<_, 1>::empty();
        let source_expr = sources.add_mem_table(input);

        let q = QueryExpr::new(source_expr)
            .with_select_cmp(OpLogic::Or, scalar(true), scalar(false))
            .unwrap();

        let result = run_query(q.into(), sources);

        assert_eq!(result, inv, "Query Or");
    }

    #[test]
    /// Inventory
    /// | id: u64 | name : String |
    fn test_query_inner_join() {
        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product![1u64, "health"];

        let table_id = 0.into();
        let input = mem_table(table_id, inv, [row]);
        let col = 0.into();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(input.clone());
        let second_source_expr = sources.add_mem_table(input);

        let q = QueryExpr::new(source_expr).with_join_inner(second_source_expr, col, col, false);

        let result = run_query(q.into(), sources);

        //The expected result
        let inv = ProductType::from([
            (None, AlgebraicType::U64),
            (Some("id"), AlgebraicType::U64),
            (Some("name"), AlgebraicType::String),
        ]);
        let row = product![1u64, "health", 1u64, "health"];
        let input = mem_table(table_id, inv, vec![row]);
        assert_eq!(result.data, input.data, "Project");
    }

    #[test]
    /// Inventory
    /// | id: u64 | name : String |
    fn test_query_semijoin() {
        let inv = ProductType::from([("id", AlgebraicType::U64), ("name", AlgebraicType::String)]);

        let row = product![1u64, "health"];

        let table_id = 0.into();
        let input = mem_table(table_id, inv, [row]);
        let col = 0.into();

        let mut sources = SourceSet::<_, 2>::empty();
        let source_expr = sources.add_mem_table(input.clone());
        let second_source_expr = sources.add_mem_table(input);

        let q = QueryExpr::new(source_expr).with_join_inner(second_source_expr, col, col, true);

        let result = run_query(q.into(), sources);

        // The expected result.
        let inv = ProductType::from([(None, AlgebraicType::U64), (Some("name"), AlgebraicType::String)]);
        let row = product![1u64, "health"];
        let input = mem_table(table_id, inv, vec![row]);
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
        // See table above.
        let data = create_game_data();
        let inv @ [inv_inventory_id, _] = [0, 1].map(|c| c.into());
        let inv_head = data.inv.head.clone();
        let inv_expr = |col: ColId| inv_head.fields[col.idx()].field.into();
        let [location_entity_id, location_x, location_z] = [0, 1, 2].map(|c| c.into());
        let [player_entity_id, player_inventory_id] = [0, 1].map(|c| c.into());
        let loc_head = data.location.head.clone();
        let loc_field = |col: ColId| loc_head.fields[col.idx()].field;
        let inv_table_id = data.inv.head.table_id;
        let player_table_id = data.player.head.table_id;

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
        let q = QueryExpr::new(player_source_expr)
            .with_join_inner(location_source_expr, player_entity_id, location_entity_id, true)
            .with_select_cmp(OpCmp::Gt, loc_field(location_x), scalar(0.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::LtEq, loc_field(location_x), scalar(32.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::Gt, loc_field(location_z), scalar(0.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::LtEq, loc_field(location_z), scalar(32.0f32))
            .unwrap();

        let result = run_query(q.into(), sources);

        let ty = ProductType::from([("entity_id", AlgebraicType::U64), ("inventory_id", AlgebraicType::U64)]);
        let row1 = product!(100u64, 1u64);
        let input = mem_table(player_table_id, ty, [row1]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Player"
        );

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
        let q = QueryExpr::new(inventory_source_expr)
            // NOTE: The way this query is set up, the first join must be an inner join, not a semijoin,
            // so that the second join has access to the `Player.entity_id` field.
            // This necessitates a trailing `project` to get just `Inventory.*`.
            .with_join_inner(player_source_expr, inv_inventory_id, player_inventory_id, false)
            .with_join_inner(
                location_source_expr,
                (inv_head.fields.len() + player_entity_id.idx()).into(),
                location_entity_id,
                true,
            )
            .with_select_cmp(OpCmp::Gt, loc_field(location_x), scalar(0.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::LtEq, loc_field(location_x), scalar(32.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::Gt, loc_field(location_z), scalar(0.0f32))
            .unwrap()
            .with_select_cmp(OpCmp::LtEq, loc_field(location_z), scalar(32.0f32))
            .unwrap()
            .with_project(inv.map(inv_expr).into(), Some(inv_table_id))
            .unwrap();

        let result = run_query(q.into(), sources);

        let ty = ProductType::from([("inventory_id", AlgebraicType::U64), ("name", AlgebraicType::String)]);
        let row1 = product!(1u64, "health");
        let input = mem_table(inv_table_id, ty, [row1]);

        assert_eq!(
            mem_table_without_table_name(&result),
            mem_table_without_table_name(&input),
            "Inventory"
        );
    }
}
