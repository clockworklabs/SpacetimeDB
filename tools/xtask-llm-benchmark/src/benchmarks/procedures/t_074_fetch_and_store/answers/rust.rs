use spacetimedb::{procedure, table, ProcedureContext, Table};

#[table(accessor = fetched_record, public)]
pub struct FetchedRecord { #[primary_key] pub id: u64, pub status: u16, pub valid_schema: bool }

#[procedure]
pub fn fetch_and_store(ctx: &mut ProcedureContext, server_url: String) {
    let url = format!("{}/v1/database/{}/schema?version=9", server_url.trim_end_matches('/'), ctx.database_identity());
    let response = ctx.http.get(url).expect("schema request failed");
    let status = response.status().as_u16();
    let valid_schema = response.into_body().into_string_lossy().contains("\"tables\"");
    ctx.with_tx(|tx| { tx.db.fetched_record().insert(FetchedRecord { id: 1, status, valid_schema }); });
}
