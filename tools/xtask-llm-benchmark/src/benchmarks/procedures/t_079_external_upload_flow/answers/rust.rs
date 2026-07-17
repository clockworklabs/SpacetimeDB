use spacetimedb::{procedure, table, ProcedureContext, Table};
use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};

#[table(accessor = uploaded_asset, public)]
pub struct UploadedAsset { #[primary_key] pub id: u64, pub url: String, pub size: u64 }

#[handler]
fn upload(_ctx: &mut HandlerContext, _request: Request) -> Response {
    Response::builder().status(201).body(Body::from_bytes("https://files.local/object-1")).unwrap()
}

#[router]
fn routes() -> Router { Router::new().post("/upload", upload) }

#[procedure]
pub fn upload_and_register(ctx: &mut ProcedureContext, server_url: String, data: Vec<u8>) -> String {
    let url = format!("{}/v1/database/{}/route/upload", server_url.trim_end_matches('/'), ctx.database_identity());
    let request = Request::builder().method("POST").uri(url).header("content-type", "application/octet-stream")
        .body(Body::from_bytes(data.clone())).unwrap();
    let response = ctx.http.send(request).expect("upload failed");
    let asset_url = response.into_body().into_string_lossy();
    let row_url = asset_url.clone();
    ctx.with_tx(|tx| { tx.db.uploaded_asset().insert(UploadedAsset { id: 1, url: row_url.clone(), size: data.len() as u64 }); });
    asset_url
}
