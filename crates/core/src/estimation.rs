use crate::db::relational_db::Tx;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_vm::expr::{Query, QueryExpr, SourceExpr};

/// The estimated number of rows that a query plan will return.
pub fn num_rows(tx: &Tx, expr: &QueryExpr) -> u64 {
    row_est(tx, &expr.source, &expr.query)
}

/// The estimated number of rows that a query sub-plan will return.
fn row_est(tx: &Tx, src: &SourceExpr, ops: &[Query]) -> u64 {
    match ops {
        // The base case is the table row count.
        [] => src.table_id().and_then(|id| tx.get_row_count(id)).unwrap_or(0),
        // Walk in reverse from the end (`op`) to the beginning.
        [input @ .., op] => match op {
            // How selective is an index lookup?
            // We assume a uniform distribution of keys,
            // which implies a selectivity = 1 / NDV,
            // where NDV stands for Number of Distinct Values.
            Query::IndexScan(scan) if scan.is_point() => {
                index_row_est(tx, scan.table.table_id, &scan.columns)
            }
            // We assume projections select 100% of their input rows.
            Query::Project(..)
            // How selective is an arbitrary predicate?
            // If it is not sargable,
            // meaning it cannot be satisfied using an index,
            // we assume the worst-case scenario,
            // that it will select all of its input rows.
            // That is we set the selectivity = 1.
            | Query::Select(_)
            // We do the same for sargable range conditions.
            | Query::IndexScan(_) => {
                row_est(tx, src, input)
            }
            // How selective is an index join?
            // We have an estimate for the number of probe side rows,
            // We have an estimate for the number of rows each index probe will return.
            // Multiplying both estimates together will give us our expectation.
            Query::IndexJoin(join) => {
                let table_id = if join.return_index_rows {
                    src.table_id().unwrap()
                } else {
                    join.probe_side.source.table_id().unwrap()
                };
                row_est(tx, &join.probe_side.source, &join.probe_side.query)
                    * index_row_est(tx, table_id, &join.index_col.into())
            }
            // Since inner join is our most expensive operation,
            // we maximally overestimate its output cardinality,
            // as though each row from the left joins with each row from the right.
            Query::JoinInner(join) => {
                row_est(tx, src, input) * row_est(tx, &join.rhs.source, &join.rhs.query)
            }
        },
    }
}

/// The estimated number of rows that an index probe will return.
/// Note this method is not applicable to range scans.
fn index_row_est(tx: &Tx, table_id: TableId, cols: &ColList) -> u64 {
    tx.num_distinct_values(table_id, cols)
        .map_or(0, |ndv| tx.get_row_count(table_id).unwrap_or(0) / ndv)
}

#[cfg(test)]
mod tests {
    use crate::{
        db::relational_db::{tests_utils::TestDB, RelationalDB},
        error::DBError,
        estimation::num_rows,
        execution_context::ExecutionContext,
        sql::compiler::compile_sql,
    };
    use spacetimedb_lib::AlgebraicType;
    use spacetimedb_sats::product;
    use spacetimedb_vm::expr::CrudExpr;

    fn in_mem_db() -> TestDB {
        TestDB::in_memory().expect("failed to make test db")
    }

    fn num_rows_for(db: &RelationalDB, sql: &str) -> u64 {
        let tx = db.begin_tx();
        match &*compile_sql(db, &tx, sql).expect("Failed to compile sql") {
            [CrudExpr::Query(expr)] => num_rows(&tx, expr),
            exprs => panic!("unexpected result from compilation: {:#?}", exprs),
        }
    }

    const NUM_T_ROWS: u64 = 10;
    const NDV_T: u64 = 5;
    const NUM_S_ROWS: u64 = 2;
    const NDV_S: u64 = 2;

    fn create_table_t(db: &RelationalDB, indexed: bool) {
        let indexes = &[(0.into(), "a")];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        let table_id = db
            .create_table_for_test("T", &["a", "b"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0..NUM_T_ROWS {
                db.insert(tx, table_id, product![i % NDV_T, i])
                    .expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");
    }

    fn create_table_s(db: &RelationalDB, indexed: bool) {
        let indexes = &[(0.into(), "a"), (1.into(), "c")];
        let indexes = if indexed { indexes } else { &[] as &[_] };
        let rhs = db
            .create_table_for_test("S", &["a", "c"].map(|n| (n, AlgebraicType::U64)), indexes)
            .expect("Failed to create table");

        db.with_auto_commit(&ExecutionContext::default(), |tx| -> Result<(), DBError> {
            for i in 0..NUM_S_ROWS {
                db.insert(tx, rhs, product![i, i]).expect("failed to insert into table");
            }
            Ok(())
        })
        .expect("failed to insert into table");
    }

    /// Cardinality estimation for an index lookup depends only on
    /// (1) the total number of rows,
    /// (2) the number of distinct values.
    #[test]
    fn cardinality_estimation_index_lookup() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert_eq!(NUM_T_ROWS / NDV_T, num_rows_for(&db, "select * from T where a = 0"));
    }

    /// We estimate an index range to return all input rows.
    #[test]
    fn cardinality_estimation_index_range() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, "select * from T where a > 0 and a < 2"));
    }

    /// We estimate a selection on a non-indexed column to return all input rows.
    #[test]
    fn select_cardinality_estimation() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, "select * from T where b = 0"));
    }

    /// We estimate a projection to return all input rows.
    #[test]
    fn project_cardinality_estimation() {
        let db = in_mem_db();
        create_table_t(&db, true);
        assert_eq!(NUM_T_ROWS, num_rows_for(&db, "select a from T"));
    }

    /// We estimate an inner join to return the product of its input sizes.
    #[test]
    fn cardinality_estimation_inner_join() {
        let db = in_mem_db();
        create_table_t(&db, false);
        create_table_s(&db, false);
        assert_eq!(
            NUM_T_ROWS * NUM_S_ROWS, // => 20
            num_rows_for(&db, "select T.* from T join S on T.a = S.a where S.c = 0")
        );
    }

    /// An index join estimates its output cardinality in the same way.
    /// As the product of its estimated input cardinalities.
    #[test]
    fn cardinality_estimation_index_join() {
        let db = in_mem_db();
        create_table_t(&db, true);
        create_table_s(&db, true);
        assert_eq!(
            NUM_T_ROWS / NDV_T * NUM_S_ROWS / NDV_S, // => 2
            num_rows_for(&db, "select T.* from T join S on T.a = S.a where S.c = 0")
        );
    }
}
