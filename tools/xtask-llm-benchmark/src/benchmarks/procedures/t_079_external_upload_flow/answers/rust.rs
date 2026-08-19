use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};
use spacetimedb::{procedure, table, ProcedureContext, Table};

#[table(accessor = uploaded_asset, public)]
pub struct UploadedAsset {
    #[primary_key]
    pub id: u64,
    pub url: String,
    pub size: u64,
    pub status: u16,
    pub response_body_present: bool,
}

#[handler]
fn upload(_ctx: &mut HandlerContext, _request: Request) -> Response {
    Response::builder()
        .status(201)
        .body(Body::from_bytes("https://files.local/object-1"))
        .unwrap()
}

#[router]
fn routes() -> Router {
    Router::new().post("/upload", upload)
}

#[procedure]
pub fn upload_and_register(ctx: &mut ProcedureContext, upload_url: String, data: Vec<u8>) -> String {
    let request = Request::builder()
        .method("POST")
        .uri(upload_url.clone())
        .header("content-type", "application/octet-stream")
        .body(Body::from_bytes(data.clone()))
        .unwrap();
    let response = ctx.http.send(request).expect("upload failed");
    assert!(response.status().is_success(), "upload failed: {}", response.status());
    let status = response.status().as_u16();
    let response_body_present = !response.into_body().into_bytes().is_empty();
    let row_url = upload_url.clone();
    ctx.with_tx(|tx| {
        tx.db.uploaded_asset().insert(UploadedAsset {
            id: 1,
            url: row_url.clone(),
            size: data.len() as u64,
            status,
            response_body_present,
        });
    });
    upload_url
}
