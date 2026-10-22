use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::scenario;
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        let mut scorers = default_schema_parity_scorers(host, file!(), route);
        scorers.push(scenario(file!(), route, host, lang, |s| {
            s.call("set_sale", json!([4, "books", 17]))?;
            s.rows(
                "category_total",
                &["category", "total_amount", "sale_count"],
                json!([["books", 17, 1]]),
            )?;
            s.call("set_sale", json!([8, "books", -17]))?;
            s.rows(
                "category_total",
                &["category", "total_amount", "sale_count"],
                json!([["books", 0, 2]]),
            )?;
            s.call("set_sale", json!([4, "games", 23]))?;
            s.rows(
                "category_total",
                &["category", "total_amount", "sale_count"],
                json!([["books", -17, 1], ["games", 23, 1]]),
            )?;
            s.reject("set_sale", json!([8, "", 100]), "invalid category")?;
            s.rows(
                "sale",
                &["id", "category", "amount"],
                json!([[4, "games", 23], [8, "books", -17]]),
            )?;
            s.call("set_sale", json!([8, "games", 0]))?;
            s.call("set_sale", json!([4, "games", 23]))?;
            s.rows(
                "category_total",
                &["category", "total_amount", "sale_count"],
                json!([["games", 23, 2]]),
            )?;
            s.call("remove_sale", json!([4]))?;
            s.call("remove_sale", json!([4]))?;
            s.rows(
                "category_total",
                &["category", "total_amount", "sale_count"],
                json!([["games", 0, 1]]),
            )?;
            s.call("remove_sale", json!([8]))?;
            s.rows("category_total", &["category", "total_amount", "sale_count"], json!([]))?;
            s.rows("sale", &["id"], json!([]))?;
            Ok(())
        }));
        scorers
    })
}
