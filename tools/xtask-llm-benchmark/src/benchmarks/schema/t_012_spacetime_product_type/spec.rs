use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing_for_lang(lang));
        let result_table = table_name("result", lang);

        let reducer = ident("SetScore", casing);

        // Compare the full row (including the product-typed column) across golden/llm
        let select = sb.select_by_id(&result_table, &["id","value"], "id", 1);

        v.push(make_reducer_data_parity_scorer(host_url, ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.into(),
            args: vec![
                Value::from(1),
                Value::from(2),
                Value::from(3),
            ],
            select_query: select.clone(),
            id_str: "product_type_row_parity",
            collapse_ws: true,
            timeout: Duration::from_secs(10),
        }));

        // Absolute sanity: exactly one row with id=1 exists
        let count = sb.count_by_id(&result_table, "id", 1);
        v.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            &count,
            1,
            "product_type_row_count",
            Duration::from_secs(10),
        ));

        v
    })
}