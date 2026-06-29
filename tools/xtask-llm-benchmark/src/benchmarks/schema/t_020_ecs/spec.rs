use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_call_both_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, SqlBuilder};
use std::time::Duration;
pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let case = casing_for_lang(lang);
        let sb = SqlBuilder::new(case);

        let seed = ident("Seed", crate::eval::Casing::Snake);
        let step = ident("Step", crate::eval::Casing::Snake);

        let entity_id = ident("entity_id", sb.case);
        let x = ident("x", sb.case);
        let y = ident("y", sb.case);

        let position_table = table_name("position", lang);
        let next_position_table = table_name("next_position", lang);

        // Seed once
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &seed,
            vec![],
            "ecs_seed",
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {position_table}"),
            2,
            "ecs_seed_position_count",
            Duration::from_secs(10),
        ));

        // Step once
        v.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            &step,
            vec![],
            "ecs_step",
        ));

        // Then just query
        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {next_position_table}"),
            2,
            "ecs_step_next_position_count",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {next_position_table} WHERE {entity_id}=1 AND {x}=1 AND {y}=0",),
            1,
            "ecs_next_pos_entity1",
            Duration::from_secs(10),
        ));

        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {next_position_table} WHERE {entity_id}=2 AND {x}=8 AND {y}=3",),
            1,
            "ecs_next_pos_entity2",
            Duration::from_secs(10),
        ));

        v
    })
}
