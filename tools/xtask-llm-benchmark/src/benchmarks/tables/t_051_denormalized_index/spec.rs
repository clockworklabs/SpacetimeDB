use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_data_parity_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "create_category",
            vec![json!(1), json!("old-slug")],
            "create_category",
        ));
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "create_product",
            vec![json!(10), json!(1), json!("Widget")],
            "create_product",
        ));

        let sql = SqlBuilder::new(casing_for_lang(lang));
        let table = table_name("product", lang);
        let columns = sql.cols(&["id", "category_id", "category_slug", "name"]).join(", ");
        let id = ident("id", casing_for_lang(lang));
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "rename_category".into(),
                args: vec![json!(1), json!("new-slug")],
                select_query: format!("SELECT {columns} FROM {table} WHERE {id}=10"),
                id_str: "denormalized_value_stays_synchronized",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
