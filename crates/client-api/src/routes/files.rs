//! File Storage routes: uploading files, downloading them, and reading their metadata.
//!
//! These routes sit beside the authenticated database routes rather than inside them.
//! A download is authorized by the `FileRef` in its URL, so it needs no identity,
//! and an upload reads its identity, if any, from the `Authorization` header itself.

use crate::auth::SpacetimeAuthHeader;
use crate::routes::database::{find_database_module, ResolvedDatabase};
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::response::{ErrorResponse, IntoResponse, Response};
use axum::Extension;
use futures::TryStreamExt;
use http::{header, HeaderMap, HeaderValue, StatusCode};
use serde::Deserialize;
use spacetimedb::db::files::{self as engine_files, DatabaseFiles};
use spacetimedb::db::relational_db::RelationalDB;
use spacetimedb::host::ReducerOutcome;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{StFileRow, TimestampViaI64};
use spacetimedb_lib::{FileRef, Identity, Timestamp, UploadRequest};
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

/// How long an upload may take, from the first byte of its body to the last.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// The header that carries a new file's reference in an upload response.
const FILE_REF_HEADER: &str = "spacetime-file-ref";

fn error(status: StatusCode, message: impl Into<String>) -> ErrorResponse {
    (status, message.into()).into()
}

/// The module and its database, or a response saying why there isn't one.
async fn database_files<S: ControlStateDelegate + NodeDelegate>(
    ctx: &S,
    database: &spacetimedb::messages::control_db::Database,
) -> axum::response::Result<(Arc<RelationalDB>, Arc<DatabaseFiles>, spacetimedb::host::ModuleHost)> {
    let module = find_database_module(ctx, database).await?;
    let db = module.relational_db().clone();
    let files = db
        .files()
        .cloned()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "this database doesn't support File Storage"))?;
    Ok((db, files, module))
}

#[derive(Deserialize)]
pub struct UploadParams {
    /// A token for `allow_upload` to check, for uploads that aren't authorized by an identity.
    token: Option<String>,
}

/// `POST /v1/database/:name_or_identity/files`
///
/// Uploads one file in a single streaming request. The module's `allow_upload` reducer decides
/// whether to accept it before any bytes are read. The response arrives once the bytes are
/// persisted and the file's `st_file` row is durable.
pub async fn upload<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Extension(ResolvedDatabase(database)): Extension<ResolvedDatabase>,
    auth: SpacetimeAuthHeader,
    Query(UploadParams { token }): Query<UploadParams>,
    headers: HeaderMap,
    body: Body,
) -> axum::response::Result<Response> {
    upload_inner(ctx, database, auth, token, headers, body).await
}

async fn upload_inner<S: ControlStateDelegate + NodeDelegate>(
    ctx: S,
    database: spacetimedb::messages::control_db::Database,
    auth: SpacetimeAuthHeader,
    token: Option<String>,
    headers: HeaderMap,
    body: Body,
) -> axum::response::Result<Response> {
    let declared_size: u64 = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| error(StatusCode::LENGTH_REQUIRED, "an upload must declare its Content-Length"))?;
    let content_type = match headers.get(header::CONTENT_TYPE) {
        None => None,
        Some(value) => match value.to_str() {
            Ok(ty) if spacetimedb_lib::file::is_valid_content_type(ty) => Some(ty.to_owned()),
            _ => return Err(error(StatusCode::BAD_REQUEST, "the Content-Type can't be served back")),
        },
    };
    let sender = auth.get().map(|auth| auth.claims.identity);
    if sender.is_none() && token.is_none() {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "an upload needs an Authorization header or a ?token=",
        ));
    }

    let (_db, files, module) = database_files(&ctx, &database).await?;

    // Decide before reading the body. The file's ID is fixed now, so `allow_upload` can record it.
    let file_ref = engine_files::mint_file_ref();
    let request = UploadRequest {
        file_id: file_ref.id(),
        sender,
        size: declared_size,
        content_type: content_type.clone(),
        token,
    };
    // Token-only uploads have no identity, so `allow_upload` sees the zero identity as its sender.
    let caller = sender.unwrap_or(Identity::ZERO);
    match module.call_allow_upload(caller, &request).await {
        Ok(None) => {
            return Err(error(
                StatusCode::FORBIDDEN,
                "this database doesn't accept uploads: it defines no allow_upload reducer",
            ))
        }
        Ok(Some(result)) => match result.outcome {
            ReducerOutcome::Committed => {}
            ReducerOutcome::Failed(message) => return Err(error(StatusCode::FORBIDDEN, message.to_string())),
            ReducerOutcome::BudgetExceeded => return Err(error(StatusCode::PAYMENT_REQUIRED, "out of energy")),
        },
        Err(e) => return Err(log_and_500(e)),
    }

    // Receive and hash the body, then persist it.
    let spooled = tokio::time::timeout(UPLOAD_TIMEOUT, async {
        let mut spool = files.store().spool().await.map_err(log_and_500)?;
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| error(StatusCode::BAD_REQUEST, format!("failed to read the upload: {e}")))?
        {
            if spool.size() + chunk.len() as u64 > declared_size {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "the body is longer than its Content-Length",
                ));
            }
            spool.write(&chunk).await.map_err(log_and_500)?;
        }
        if spool.size() != declared_size {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "the body is shorter than its Content-Length",
            ));
        }
        spool.finish().await.map_err(log_and_500)
    })
    .await
    .map_err(|_| error(StatusCode::REQUEST_TIMEOUT, "the upload took too long"))??;
    let hash = spooled.hash();
    let size = spooled.size();
    files.persist(spooled).await.map_err(log_and_500)?;

    // Record the file, and wait until the record is durable.
    let row = StFileRow {
        file_id: file_ref.id(),
        read_secret: file_ref.read_secret(),
        content_hash: hash,
        size,
        created_at: TimestampViaI64(Timestamp::now()),
        content_type,
        deleted_at_offset: None,
    };
    // Through the module, so that views that read files' metadata are re-evaluated.
    let recorded = module.insert_uploaded_file(row).await.map_err(log_and_500)?;
    let offset = recorded
        .tx_offset
        .await
        .map_err(|_| log_and_500("the file's record was not committed"))?;
    if let Some(mut durable) = recorded.durable_offset {
        durable.wait_for(offset).await.map_err(log_and_500)?;
    }

    let body = serde_json::json!({
        "file_ref": file_ref.to_string(),
        "size": size,
        "content_hash": hash.to_hex().to_string(),
    });
    let mut response = (StatusCode::CREATED, axum::Json(body)).into_response();
    response.headers_mut().insert(
        FILE_REF_HEADER,
        HeaderValue::from_str(&file_ref.to_string()).expect("hex is a valid header value"),
    );
    Ok(response)
}

#[derive(Deserialize)]
pub struct FileParams {
    file_ref: String,
}

#[derive(Deserialize)]
pub struct ServeParams {
    /// Serve as an attachment with this file name.
    download: Option<String>,
    /// Serve with this content type instead of the stored one.
    #[serde(rename = "type")]
    content_type: Option<String>,
}

/// Which files a lookup may find.
#[derive(Clone, Copy)]
enum Lookup {
    /// Only files that haven't been deleted.
    Live,
    /// Also deleted files whose records history still retains.
    Servable,
}

/// Looks up the `st_file` row a reference names. A malformed or forged reference finds nothing.
async fn find_file(db: &Arc<RelationalDB>, file_ref: &str, lookup: Lookup) -> axum::response::Result<StFileRow> {
    let not_found = || error(StatusCode::NOT_FOUND, "no such file");
    let file_ref: FileRef = file_ref.parse().map_err(|_| not_found())?;
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        db.with_read_only(Workload::Internal, |tx| match lookup {
            Lookup::Live => engine_files::live_file_row_for_ref(tx, file_ref),
            Lookup::Servable => engine_files::servable_file_row_for_ref(tx, file_ref),
        })
    })
    .await
    .map_err(log_and_500)?
    .map_err(log_and_500)?
    .ok_or_else(not_found)
}

/// Parses a single-range `Range` header against an object of `len` bytes.
///
/// `Ok(None)` means serve the whole object; `Err(())` means the range is unsatisfiable.
fn parse_range(headers: &HeaderMap, len: u64) -> Result<Option<Range<u64>>, ()> {
    let Some(value) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    // Multiple ranges and other units aren't supported; serve the whole object instead, as RFC 9110 allows.
    let Some(spec) = value.strip_prefix("bytes=").filter(|spec| !spec.contains(',')) else {
        return Ok(None);
    };
    // RFC 9110 has a server ignore a malformed range and serve the whole object.
    let Some((start, end)) = spec.split_once('-') else {
        return Ok(None);
    };
    let range = match (start.trim(), end.trim()) {
        ("", suffix) => {
            let Ok(suffix) = suffix.parse::<u64>() else {
                return Ok(None);
            };
            len.saturating_sub(suffix)..len
        }
        (start, end) => {
            let Ok(start) = start.parse::<u64>() else {
                return Ok(None);
            };
            let end = match end {
                "" => len,
                end => match end.parse::<u64>() {
                    Ok(end) if end >= start => end.saturating_add(1).min(len),
                    _ => return Ok(None),
                },
            };
            start..end
        }
    };
    // A range that starts at or past the end, or an empty suffix, can't be satisfied.
    if range.start >= range.end {
        return Err(());
    }
    Ok(Some(range))
}

/// `GET` and `HEAD /v1/database/:name_or_identity/files/:file_ref`
///
/// Serves a file's bytes, or a range of them. Holding the reference is the authorization.
pub async fn download<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Extension(ResolvedDatabase(database)): Extension<ResolvedDatabase>,
    Path(FileParams { file_ref }): Path<FileParams>,
    Query(params): Query<ServeParams>,
    headers: HeaderMap,
) -> axum::response::Result<Response> {
    download_inner(ctx, database, file_ref, params, headers).await
}

async fn download_inner<S: ControlStateDelegate + NodeDelegate>(
    ctx: S,
    database: spacetimedb::messages::control_db::Database,
    file_ref: String,
    params: ServeParams,
    headers: HeaderMap,
) -> axum::response::Result<Response> {
    let (db, files, _module) = database_files(&ctx, &database).await?;
    // Deleting a file doesn't revoke downloads while history retains it.
    let row = find_file(&db, &file_ref, Lookup::Servable).await?;
    let etag = format!("\"{}\"", row.content_hash.to_hex());

    // A stored type was validated when the file was created, but fall back rather than fail if not.
    let content_type = params
        .content_type
        .as_deref()
        .or(row.content_type.as_deref())
        .filter(|ty| spacetimedb_lib::file::is_valid_content_type(ty))
        .unwrap_or("application/octet-stream");
    let mut builder = Response::builder()
        .header(header::ETAG, &etag)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, content_type)
        .header(header::LAST_MODIFIED, httpdate(row.created_at.0))
        // Contents never change, but a cached copy can outlive the file's reclamation.
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header(header::CONTENT_SECURITY_POLICY, "sandbox")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    if let Some(name) = &params.download {
        // Keep the name a valid quoted string: visible ASCII and spaces, without quotes or backslashes.
        let name: String = name
            .chars()
            .map(|c| match c {
                ' '..='~' if c != '"' && c != '\\' => c,
                _ => '_',
            })
            .collect();
        builder = builder.header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""));
    }

    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|v| v.as_bytes() == etag.as_bytes())
    {
        return builder
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .map_err(log_and_500);
    }

    let len = row.size;
    let range = match parse_range(&headers, len) {
        Ok(range) => range,
        Err(()) => {
            return Ok((
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{len}"))],
            )
                .into_response())
        }
    };
    let (status, served) = match range {
        Some(range) => {
            builder = builder.header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{len}", range.start, range.end - 1),
            );
            (StatusCode::PARTIAL_CONTENT, range)
        }
        None => (StatusCode::OK, 0..len),
    };
    builder = builder.header(header::CONTENT_LENGTH, served.end - served.start);

    // Serve from the store. A file a reducer just created may not be persisted yet,
    // in which case its bytes are still staged in the database.
    let body = match files.store().read(&row.content_hash, served.clone()).await {
        Ok(stream) => Body::from_stream(stream),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let hash = row.content_hash;
            let staged = tokio::task::spawn_blocking(move || {
                db.with_read_only(Workload::Internal, |tx| engine_files::staged_bytes(tx, hash))
            })
            .await
            .map_err(log_and_500)?
            .map_err(log_and_500)?;
            match staged {
                Some(bytes) => Body::from(bytes[served.start as usize..served.end as usize].to_vec()),
                // The upload may have finished, and its staged row been deleted, since the first read.
                None => match files.store().read(&hash, served.clone()).await {
                    Ok(stream) => Body::from_stream(stream),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        log::error!("file {hash} has an st_file row but no bytes");
                        return Err(error(StatusCode::NOT_FOUND, "no such file"));
                    }
                    Err(e) => return Err(log_and_500(e)),
                },
            }
        }
        Err(e) => return Err(log_and_500(e)),
    };
    builder.status(status).body(body).map_err(log_and_500)
}

/// Formats a timestamp as an HTTP date.
fn httpdate(ts: Timestamp) -> String {
    ts.to_chrono_date_time()
        .map(|dt| dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
        .unwrap_or_default()
}

/// `GET /v1/database/:name_or_identity/files/:file_ref/metadata`
///
/// Returns the file's live record. Unlike a download, this fails as soon as the file is deleted.
pub async fn metadata<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Extension(ResolvedDatabase(database)): Extension<ResolvedDatabase>,
    Path(FileParams { file_ref }): Path<FileParams>,
) -> axum::response::Result<Response> {
    let (db, _files, _module) = database_files(&ctx, &database).await?;
    let row = find_file(&db, &file_ref, Lookup::Live).await?;
    let metadata = row.metadata();
    Ok(axum::Json(serde_json::json!({
        "file_ref": metadata.file_ref.to_string(),
        "size": metadata.size,
        "content_hash": metadata.content_hash.to_hex().to_string(),
        "created_at": metadata.created_at.to_string(),
        "content_type": metadata.content_type,
    }))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(value: &str, len: u64) -> Result<Option<Range<u64>>, ()> {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_str(value).unwrap());
        parse_range(&headers, len)
    }

    #[test]
    fn parses_ranges() {
        assert_eq!(parse_range(&HeaderMap::new(), 10), Ok(None));
        assert_eq!(range("bytes=0-4", 10), Ok(Some(0..5)));
        assert_eq!(range("bytes=5-", 10), Ok(Some(5..10)));
        assert_eq!(range("bytes=-3", 10), Ok(Some(7..10)));
        assert_eq!(range("bytes=8-100", 10), Ok(Some(8..10)));
        assert_eq!(range("bytes=10-", 10), Err(()));
        assert_eq!(range("bytes=-0", 10), Err(()));
        assert_eq!(range("bytes=0-", 0), Err(()));
        // Malformed ranges are ignored.
        assert_eq!(range("bytes=5-4", 10), Ok(None));
        assert_eq!(range("bytes=abc", 10), Ok(None));
        assert_eq!(range("bytes=-", 10), Ok(None));
        // Multiple ranges fall back to the whole file.
        assert_eq!(range("bytes=0-1,3-4", 10), Ok(None));
    }
}
