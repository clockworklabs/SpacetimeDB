use spacetimedb_lib::bsatn;
use spacetimedb_smoketests::Smoketest;

fn idc_path(database_identity: &str, reducer: &str, sender_identity: &str, msg_id: u64) -> String {
    format!(
        "/v1/database/{database_identity}/call-from-database/{reducer}?sender_identity={sender_identity}&msg_id={msg_id}"
    )
}

fn post_idc(test: &Smoketest, reducer: &str, sender_identity: &str, msg_id: u64, body: &[u8]) -> (u16, String) {
    let database_identity = test.database_identity.as_ref().expect("No database published");
    let response = test
        .api_call_with_body_and_headers(
            "POST",
            &idc_path(database_identity, reducer, sender_identity, msg_id),
            Some(body),
            "Content-Type: application/octet-stream\r\n",
        )
        .expect("IDC HTTP call failed");
    let text = response.text().expect("IDC response body should be valid UTF-8");
    (response.status_code, text)
}

#[test]
fn test_call_from_database_deduplicates_successful_reducer() {
    let module_code = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = deliveries, public)]
pub struct Delivery {
    #[primary_key]
    name: String,
    count: u64,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.deliveries().insert(Delivery {
        name: "remote".to_string(),
        count: 0,
    });
}

#[spacetimedb::reducer]
pub fn accept(ctx: &ReducerContext) {
    let mut row = ctx
        .db
        .deliveries()
        .name()
        .find("remote".to_string())
        .expect("delivery row should exist");
    row.count += 1;
    ctx.db.deliveries().name().update(row);
}
"#;

    let test = Smoketest::builder().module_code(module_code).build();
    let body = bsatn::to_vec(&()).expect("BSATN encoding should succeed");
    let sender_identity = "00000000000000000000000000000000000000000000000000000000000000aa";

    let first = post_idc(&test, "accept", sender_identity, 7, &body);
    let second = post_idc(&test, "accept", sender_identity, 7, &body);

    assert_eq!(first.0, 200, "first IDC call should succeed: {first:?}");
    assert_eq!(first.1, "");
    assert_eq!(second.0, 200, "duplicate IDC call should still succeed: {second:?}");
    assert_eq!(second.1, "");

    let output = test.sql("SELECT name, count FROM deliveries").unwrap();
    assert!(
        output.contains(r#""remote" | 1"#),
        "expected the deduplicated reducer to leave a single delivery row, got:\n{output}",
    );
}

#[test]
fn test_call_from_database_replays_stored_reducer_error_without_rerunning() {
    let module_code = r#"
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer]
pub fn always_fail(_ctx: &ReducerContext) -> Result<(), String> {
    log::info!("IDC failing reducer executed");
    Err("boom".to_string())
}
"#;

    let test = Smoketest::builder().module_code(module_code).build();
    let body = bsatn::to_vec(&()).expect("BSATN encoding should succeed");
    let sender_identity = "00000000000000000000000000000000000000000000000000000000000000bb";

    let first = post_idc(&test, "always_fail", sender_identity, 9, &body);
    let second = post_idc(&test, "always_fail", sender_identity, 9, &body);

    assert_eq!(first.0, 422, "first IDC call should surface reducer error: {first:?}");
    assert_eq!(first.1.trim(), "boom");
    assert_eq!(
        second.0, 422,
        "duplicate IDC call should replay reducer error: {second:?}"
    );
    assert_eq!(second.1.trim(), "boom");

    let logs = test.logs(100).unwrap();
    let executions = logs
        .iter()
        .filter(|line| line.contains("IDC failing reducer executed"))
        .count();
    assert_eq!(executions, 1, "duplicate IDC request should not rerun the reducer");
}
