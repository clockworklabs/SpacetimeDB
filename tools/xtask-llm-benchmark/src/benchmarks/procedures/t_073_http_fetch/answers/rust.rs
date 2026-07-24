use spacetimedb::{procedure, ProcedureContext, SpacetimeType};

#[derive(SpacetimeType)]
pub struct FetchSummary {
    pub status: u16,
    pub html_content_type: bool,
    pub has_example_domain: bool,
}

#[procedure]
pub fn fetch_page_summary(ctx: &mut ProcedureContext, url: String) -> FetchSummary {
    let response = ctx.http.get(url).expect("page request failed");
    let status = response.status().as_u16();
    let html_content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/html"));
    let body = response.into_body().into_string_lossy();
    FetchSummary {
        status,
        html_content_type,
        has_example_domain: body.contains("Example Domain"),
    }
}
