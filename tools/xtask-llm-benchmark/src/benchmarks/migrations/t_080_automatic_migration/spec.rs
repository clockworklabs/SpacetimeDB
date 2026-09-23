use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);
        let product = table_name("product", lang);
        let product_name = ident("name", casing);
        scorers.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {product} WHERE {product_name}='legacy'"),
            1,
            "existing_data_preserved",
            Duration::from_secs(10),
        ));
        let category = table_name("category", lang);
        let id = ident("id", casing);
        let label = ident("label", casing);
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "create_category".into(),
                args: vec![json!(7), json!("general")],
                select_query: format!("SELECT {id}, {label} FROM {category}"),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "new_schema_usable",
            },
        ));
        scorers
    })
}
