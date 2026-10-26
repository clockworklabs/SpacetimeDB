#![allow(non_snake_case)]

use spacetimedb::spacetimedb_lib::RawModuleDef;

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: spacetimedb::CaseConversionPolicy = spacetimedb::CaseConversionPolicy::None;

#[spacetimedb::reducer]
pub fn receivePing(_: &spacetimedb::ReducerContext, messageBody: String) {
    let _ = messageBody;
}

#[spacetimedb::table(accessor = outboundPing, outbox(crate::receivePing))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: spacetimedb::Identity,
    #[param(name = messageBody)]
    payload: String,
}

#[spacetimedb::reducer(on_result(self::outboundPing))]
pub fn on_ping_result(_: &spacetimedb::ReducerContext, _row: OutboundPing, _result: Result<(), String>) {}

#[test]
fn local_outbox_hash_uses_the_canonical_reducer_signature() {
    let RawModuleDef::V10(raw) = spacetimedb::test_utils::module_def() else {
        panic!("expected V10 module definition");
    };
    assert_eq!(raw.case_conversion_policy(), spacetimedb::CaseConversionPolicy::None);
    assert!(raw.outboxes().is_none());
    let outbox = &raw.local_outboxes().unwrap()[0];
    assert_eq!(outbox.reducer_source_name.as_ref(), "receivePing");
    assert_eq!(outbox.on_result_reducer.as_ref().unwrap().as_ref(), "on_ping_result");
    assert_eq!(outbox.arg_columns.len(), 1);

    let module = spacetimedb_schema::def::ModuleDef::try_from(RawModuleDef::V10(raw)).unwrap();
    let reducer = module.reducer_by_name("receivePing").unwrap().1;
    let outbox = module.tables().find_map(|table| table.outbox.as_ref()).unwrap();
    assert_eq!(
        reducer.params().elements[0].name.as_ref().unwrap().as_ref(),
        "messageBody"
    );
    let expected_hash = spacetimedb_schema::def::reducer_signature_hash(
        &reducer.params(),
        &reducer.ok_return_type,
        &reducer.err_return_type,
        module.typespace(),
    );
    assert_eq!(outbox.signature_hash, expected_hash);
    assert_eq!(outbox.on_result_reducer.as_ref().unwrap().as_ref(), "on_ping_result");
}
