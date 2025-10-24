use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer,
};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
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

        v.push(make_reducer_sql_count_scorer(
            file!(),
            route_tag,
            &seed,
            vec![],
            "SELECT COUNT(*) AS n FROM positions",
            2,
            "ecs_seed_positions_count",
            Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(
            file!(),
            route_tag,
            &step,
            vec![],
            "SELECT COUNT(*) AS n FROM next_positions",
            2,
            "ecs_step_next_positions_count",
            Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(
            file!(),
            route_tag,
            &step,
            vec![],
            &format!(
                "SELECT COUNT(*) AS n FROM next_positions WHERE {eid}=1 AND {x}=1 AND {y}=0",
                eid = entity_id, x = x, y = y
            ),
            1,
            "ecs_next_pos_entity1",
            Duration::from_secs(10),
        ));

        v.push(make_reducer_sql_count_scorer(
            file!(),
            route_tag,
            &step,
            vec![],
            &format!(
                "SELECT COUNT(*) AS n FROM next_positions WHERE {eid}=2 AND {x}=8 AND {y}=3",
                eid = entity_id, x = x, y = y
            ),
            1,
            "ecs_next_pos_entity2",
            Duration::from_secs(10),
        ));

        v
    })
}
