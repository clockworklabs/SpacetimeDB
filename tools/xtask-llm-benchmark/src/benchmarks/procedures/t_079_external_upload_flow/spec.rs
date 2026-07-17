use crate::eval::defaults::{default_schema_parity_scorers, make_call_output_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_call_output_parity_scorer(
            host_url, file!(), route_tag, "upload_and_register",
            vec![json!(host_url), json!([1, 2, 3, 4])], "upload_return_url",
        ));
        let table = table_name("uploaded_asset", lang);
        let url = ident("url", casing_for_lang(lang));
        let size = ident("size", casing_for_lang(lang));
        scorers.push(make_sql_count_only_scorer(
            host_url, file!(), route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {url}='https://files.local/object-1' AND {size}=4"),
            1, "upload_metadata_stored", Duration::from_secs(10),
        ));
        scorers
    })
}
