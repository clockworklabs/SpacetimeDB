use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let columns = sql.cols(&["category", "total_amount", "sale_count"]).join(", ");
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "exercise".into(),
                args: vec![],
                select_query: format!("SELECT {columns} FROM {}", table_name("category_summary", lang)),
                id_str: "aggregate_is_synchronized",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers.push(crate::eval::scenario::scenario(
            file!(),
            route_tag,
            host_url,
            lang,
            |s| {
                use serde_json::json;
                s.call("set_sale", json!([2, "games", 17]))?;
                s.rows(
                    "category_summary",
                    &["category", "total_amount", "sale_count"],
                    json!([["games", 17, 1]]),
                )?;
                s.call("set_sale", json!([4, "games", -17]))?;
                s.rows(
                    "category_summary",
                    &["category", "total_amount", "sale_count"],
                    json!([["games", 0, 2]]),
                )?;
                s.call("remove_sale", json!([2]))?;
                s.call("remove_sale", json!([2]))?;
                s.rows(
                    "category_summary",
                    &["category", "total_amount", "sale_count"],
                    json!([["games", -17, 1]]),
                )?;
                s.call("remove_sale", json!([4]))?;
                s.rows(
                    "category_summary",
                    &["category", "total_amount", "sale_count"],
                    json!([]),
                )?;
                Ok(())
            },
        ));
        scorers
    })
}
