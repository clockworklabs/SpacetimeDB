use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use std::time::Duration;
pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let case = casing_for_lang(lang);
        let sb   = SqlBuilder::new(case);

        let seed = ident("Seed", case);
        let step = ident("Step", case);

        let entity_id = ident("entity_id", sb.case);
        let x = ident("x", sb.case);
        let y = ident("y", sb.case);

        let base = |reducer: &str| ReducerSqlCountConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.to_string(),
            args: vec![],
            sql_count_query: String::new(),
            expected_count: 0,
            id_str: "",
            timeout: Duration::from_secs(10),
        };

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            sql_count_query: "SELECT COUNT(*) AS n FROM positions".into(),
            expected_count: 2,
            id_str: "ecs_seed_positions_count",
            ..base(&seed) // or base("seed") if it's a &str
        }));

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            sql_count_query: "SELECT COUNT(*) AS n FROM next_positions".into(),
            expected_count: 2,
            id_str: "ecs_step_next_positions_count",
            ..base(&step) // or base("step")
        }));

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM next_positions WHERE {eid}=1 AND {x}=1 AND {y}=0",
                eid = entity_id, x = x, y = y
            ),
            expected_count: 1,
            id_str: "ecs_next_pos_entity1",
            ..base(&step)
        }));

        v.push(make_reducer_sql_count_scorer(ReducerSqlCountConfig {
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM next_positions WHERE {eid}=2 AND {x}=8 AND {y}=3",
                eid = entity_id, x = x, y = y
            ),
            expected_count: 1,
            id_str: "ecs_next_pos_entity2",
            ..base(&step)
        }));

        v
    })
}
