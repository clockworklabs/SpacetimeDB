use spacetimedb::{procedure, ProcedureContext, SpacetimeType};

#[derive(SpacetimeType)]
pub struct FetchSummary { pub status: u16, pub json_content_type: bool, pub has_tables: bool }

#[procedure]
pub fn fetch_schema_summary(ctx: &mut ProcedureContext, server_url: String) -> FetchSummary {
    let url = format!("{}/v1/database/{}/schema?version=9", server_url.trim_end_matches('/'), ctx.database_identity());
    let response = ctx.http.get(url).expect("schema request failed");
    let status = response.status().as_u16();
    let json_content_type = response.headers().get("content-type")
        .and_then(|value| value.to_str().ok()).is_some_and(|value| value.contains("application/json"));
    let body = response.into_body().into_string_lossy();
    FetchSummary { status, json_content_type, has_tables: body.contains("\"tables\"") }
}
