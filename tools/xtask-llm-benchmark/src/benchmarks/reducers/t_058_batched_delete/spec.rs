use crate::eval::defaults::{
    default_schema_parity_scorers, make_eventually_sql_count_scorer, make_reducer_call_both_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::{json, Value};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "seed_group",
            vec![json!(7), json!(5)],
            "seed_group",
        ));
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "request_delete",
            vec![json!(7)],
            "request_delete",
        ));
        let table = table_name("work_item", lang);
        let group_id = ident("group_id", casing_for_lang(lang));
        scorers.push(make_eventually_sql_count_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {group_id}=7"),
            0,
            "scheduled_batches_finish",
            Duration::from_secs(10),
        ));
        // The count check alone passes when seeding does nothing, so prove rows existed and other groups survive.
        scorers.push(crate::eval::scenario::scenario(file!(), route_tag, host_url, lang, |s| {
            let count = |group: u64| -> anyhow::Result<Value> {
                let query = format!(
                    "SELECT COUNT(*) AS n FROM \"{}\" WHERE \"{}\" = {group}",
                    s.relation("work_item"),
                    ident("group_id", casing_for_lang(s.lang))
                );
                Ok(s.owner_sql(&query)?[0][0].clone())
            };
            s.call("seed_group", json!([8, 3]))?;
            s.call("seed_group", json!([9, 5]))?;
            anyhow::ensure!(count(8)? == json!(3) && count(9)? == json!(5), "seed_group did not insert the rows");
            s.call("request_delete", json!([9]))?;
            s.eventually(|| {
                anyhow::ensure!(count(9)? == json!(0), "group 9 still has rows");
                Ok(())
            })?;
            anyhow::ensure!(count(8)? == json!(3), "deleting group 9 touched group 8");
            Ok(())
        }));
        scorers
    })
}
