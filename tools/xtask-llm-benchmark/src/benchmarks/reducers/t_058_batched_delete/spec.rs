use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::scenario::{scenario, update_rows};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;

// Initial v1 subscription rows are named objects; transaction rows can be arrays.
fn work_row(row: &Value, group_column: &str) -> Result<(u64, u64)> {
    let id = row.get("id").or_else(|| row.get("Id")).or_else(|| row.get(0));
    let group = row
        .get("group_id")
        .or_else(|| row.get(group_column))
        .or_else(|| row.get(1));
    Ok((
        id.and_then(Value::as_u64).context("work row lacks an integer ID")?,
        group
            .and_then(Value::as_u64)
            .context("work row lacks an integer group")?,
    ))
}

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(scenario(file!(), route_tag, host_url, lang, |s| {
            s.call("seed_group", json!([7, 5]))?;
            s.call("seed_group", json!([8, 3]))?;
            let (token, _) = s.new_user()?;
            let mut connection = s.connect(&token)?;
            connection.subscribe(1, &format!("SELECT * FROM {}", table_name("work_item", s.lang)))?;
            let initial = connection.next_message()?;
            ensure!(
                initial.get("SubscribeApplied").is_some(),
                "subscription failed: {initial}"
            );
            let initial = update_rows(&initial, "inserts")?;
            let group_column = ident("group_id", casing_for_lang(s.lang));
            let initial = initial
                .iter()
                .map(|row| work_row(row, &group_column))
                .collect::<Result<Vec<_>>>()?;
            ensure!(initial.len() == 8, "seed must create eight rows");
            let mut remaining: BTreeSet<_> = initial.iter().map(|(id, _)| *id).collect();
            ensure!(remaining.len() == 8, "seed produced duplicate rows");
            for (group, count) in [(7, 5), (8, 3)] {
                ensure!(
                    initial.iter().filter(|(_, g)| *g == group).count() == count,
                    "wrong seed group {group}: {initial:?}"
                );
                s.call("request_delete", json!([group]))?;
                s.call("request_delete", json!([group]))?;
                let mut deleted = 0;
                while deleted < count {
                    let update = connection.next_message()?;
                    ensure!(
                        update.get("TransactionUpdate").is_some() || update.get("TransactionUpdateLight").is_some(),
                        "unexpected update: {update}"
                    );
                    ensure!(
                        update_rows(&update, "inserts")?.is_empty(),
                        "cleanup inserted work rows"
                    );
                    let rows = update_rows(&update, "deletes")?;
                    ensure!(
                        (1..=2).contains(&rows.len()),
                        "a cleanup transaction deleted {} rows",
                        rows.len()
                    );
                    for row in rows {
                        let (id, row_group) = work_row(&row, &group_column)?;
                        ensure!(row_group == group, "deleted another group's row: {row}");
                        ensure!(remaining.remove(&id), "deleted an unknown or duplicate row: {row}");
                        deleted += 1;
                    }
                }
                ensure!(deleted == count, "wrong deletion count");
                s.eventually_count("work_item", remaining.len() as i64)?;
            }
            s.call("request_delete", json!([999]))?;
            s.eventually_count("work_item", 0)?;
            Ok(())
        }));
        scorers
    })
}

#[test]
fn initial_and_transaction_rows_have_the_same_identity() {
    for (row, field) in [
        (json!({"id":700,"group_id":7}), "group_id"),
        (json!({"id":700,"groupId":7}), "groupId"),
        (json!({"Id":700,"GroupId":7}), "GroupId"),
        (json!([700, 7]), "group_id"),
    ] {
        assert_eq!(work_row(&row, field).unwrap(), (700, 7));
    }
    assert!(work_row(&json!({"id":700}), "group_id").is_err());
    assert!(work_row(&json!([700, "7"]), "group_id").is_err());
}
