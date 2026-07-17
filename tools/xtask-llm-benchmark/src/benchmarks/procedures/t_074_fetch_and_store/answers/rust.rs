use spacetimedb::{procedure, table, ProcedureContext, Table};

#[table(accessor = fetched_record, public)]
pub struct FetchedRecord { #[primary_key] pub id: u64, pub status: u16, pub valid_body: bool }

#[procedure]
pub fn fetch_and_store(ctx: &mut ProcedureContext, url: String) {
    let response = ctx.http.get(url).expect("page request failed");
    let status = response.status().as_u16();
    let valid_body = response.into_body().into_string_lossy().contains("Example Domain");
    ctx.with_tx(|tx| { tx.db.fetched_record().insert(FetchedRecord { id: 1, status, valid_body }); });
}
