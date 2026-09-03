use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_data_parity_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        for (id, balance) in [(1, 100), (2, 25)] {
            scorers.push(make_reducer_call_both_scorer(
                host_url,
                file!(),
                route_tag,
                "create_account",
                vec![json!(id), json!(balance)],
                "create_account",
            ));
        }
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let account = table_name("account", lang);
        let columns = sql.cols(&["id", "balance"]).join(", ");
        let id = ident("id", casing_for_lang(lang));
        let query = format!("SELECT {columns} FROM {account} WHERE {id}=1 OR {id}=2");

        for scorer_id in ["transfer_once", "duplicate_transfer_is_noop"] {
            scorers.push(make_reducer_data_parity_scorer(
                host_url,
                ReducerDataParityConfig {
                    src_file: file!(),
                    route_tag,
                    reducer: "transfer".into(),
                    args: vec![json!("request-1"), json!(1), json!(2), json!(40)],
                    select_query: query.clone(),
                    id_str: scorer_id,
                    collapse_ws: true,
                    timeout: Duration::from_secs(10),
                },
            ));
        }
        scorers
    })
}
