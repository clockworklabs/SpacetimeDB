//! Tests of the SDK's file API against <../../../../modules/sdk-test-files>.
//!
//! Uploads and downloads are async, so unlike the other test clients these run as one sequence
//! of steps, waiting on channels for the callbacks they need.

use crate::module_bindings::*;

use anyhow::{anyhow, ensure, Context};
use futures::TryStreamExt as _;
use spacetimedb_sdk::{DbContext, FileRef, Files, Table, UploadError, UploadOptions};
use std::sync::mpsc;
use std::time::Duration;
use test_counter::server_url;

const TIMEOUT: Duration = Duration::from_secs(30);

pub async fn dispatch(test: &str, db_name: &str) {
    let result = match test {
        "upload-and-download" => exec_upload_and_download(db_name).await,
        "reducer-created-files" => exec_reducer_created_files(db_name).await,
        "rejected-uploads" => exec_rejected_uploads(db_name).await,
        _ => panic!("Unknown test: {test}"),
    };
    if let Err(e) = result {
        panic!("{test} failed: {e:?}");
    }
}

/// Waits for a message without stalling the runtime's other tasks.
fn recv<T>(rx: &mpsc::Receiver<T>, what: &str) -> anyhow::Result<T> {
    tokio::task::block_in_place(|| rx.recv_timeout(TIMEOUT)).map_err(|_| anyhow!("timed out waiting for {what}"))
}

/// Connects anonymously, subscribes to `document`, and returns once the subscription is applied.
///
/// Documents inserted afterwards arrive on the returned channel.
fn connect(db_name: &str) -> anyhow::Result<(DbConnection, mpsc::Receiver<Document>)> {
    let (connected_tx, connected_rx) = mpsc::channel();
    let conn = DbConnection::builder()
        .with_database_name(db_name)
        .with_uri(server_url())
        .on_connect(move |_, _, _| connected_tx.send(()).unwrap())
        .on_connect_error(|_, error| panic!("Connect errored: {error:?}"))
        .build()?;
    conn.run_threaded();
    recv(&connected_rx, "the connection")?;

    let (applied_tx, applied_rx) = mpsc::channel();
    conn.subscription_builder()
        .on_applied(move |_| applied_tx.send(()).unwrap())
        .on_error(|_, error| panic!("Subscription errored: {error:?}"))
        .subscribe(["SELECT * FROM document"]);
    recv(&applied_rx, "the subscription")?;

    let (inserted_tx, inserted_rx) = mpsc::channel();
    conn.db.document().on_insert(move |_, row| {
        let _ = inserted_tx.send(row.clone());
    });
    Ok((conn, inserted_rx))
}

/// Calls a reducer through `call`, and returns its outcome.
fn outcome(
    call: impl FnOnce(
        Box<
            dyn FnOnce(&ReducerEventContext, Result<Result<(), String>, spacetimedb_sdk::__codegen::InternalError>)
                + Send,
        >,
    ) -> spacetimedb_sdk::Result<()>,
) -> anyhow::Result<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    call(Box::new(move |_, result| {
        let _ = tx.send(result.map_err(|e| e.to_string()));
    }))?;
    recv(&rx, "the reducer's outcome")?.map_err(|e| anyhow!("the reducer call failed: {e}"))
}

/// Downloads the file, or `range` of it, or returns `None` if there's nothing to download.
async fn download(
    files: &Files,
    file: FileRef,
    range: Option<std::ops::Range<u64>>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(stream) = files.download(file, range).await? else {
        return Ok(None);
    };
    let chunks: Vec<_> = stream.try_collect().await?;
    Ok(Some(chunks.concat()))
}

fn typed(content_type: &str) -> UploadOptions {
    UploadOptions {
        content_type: Some(content_type.into()),
        ..Default::default()
    }
}

async fn exec_upload_and_download(db_name: &str) -> anyhow::Result<()> {
    let (conn, inserted) = connect(db_name)?;
    let files = conn.files();

    let contents = b"hello, files";
    let uploaded = files.upload(&contents[..], typed("text/plain")).await?;

    // A client passes the reference to a reducer, which stores it.
    let result = outcome(|cb| conn.reducers.attach_then("greeting".into(), uploaded, cb))?;
    result.map_err(|e| anyhow!("attach failed: {e}"))?;
    let document = recv(&inserted, "the document")?;
    // The generated row type holds a `FileRef`, not a `u256`.
    let file: FileRef = document.file;
    ensure!(file == uploaded, "the row holds a different reference");
    ensure!(document.size == contents.len() as u64);

    // Any subscriber can download it.
    let whole = download(&files, file, None).await?;
    ensure!(whole.as_deref() == Some(&contents[..]), "wrong contents {whole:?}");
    let range = download(&files, file, Some(7..12)).await?;
    ensure!(range.as_deref() == Some(&b"files"[..]), "wrong range {range:?}");
    let metadata = files.metadata(file).await?.context("the file has metadata")?;
    ensure!(metadata.file_ref() == file);
    ensure!(metadata.size() == contents.len() as u64);
    ensure!(metadata.content_hash() == spacetimedb_sdk::__codegen::__lib::file::content_hash(contents));
    ensure!(metadata.content_type().as_deref() == Some("text/plain"));
    let url = files.url(file);
    ensure!(
        url.as_str().starts_with(server_url()) && url.path().ends_with(&file.to_string()),
        "unexpected URL {url}"
    );

    // Retrying with the same idempotency key returns the same file. The anonymous connection
    // authenticates with the token the server issued it.
    let keyed = || UploadOptions {
        content_type: Some("text/plain".into()),
        idempotency_key: Some("key-1".into()),
    };
    let first = files.upload(&b"once"[..], keyed()).await?;
    let retry = files.upload(&b"once"[..], keyed()).await?;
    ensure!(first == retry, "the retry created another file");

    // A reference whose read secret is wrong finds nothing.
    let forged = FileRef::new(file.id_unchecked(), file.read_secret() ^ 1);
    ensure!(download(&files, forged, None).await?.is_none());
    ensure!(files.metadata(forged).await?.is_none());
    let result = outcome(|cb| conn.reducers.attach_then("forged".into(), forged, cb))?;
    ensure!(result.is_err(), "the module accepted a forged reference");

    // Once deleted, the file's metadata is gone, but history keeps its bytes downloadable.
    let result = outcome(|cb| conn.reducers.remove_then("greeting".into(), cb))?;
    result.map_err(|e| anyhow!("remove failed: {e}"))?;
    ensure!(files.metadata(file).await?.is_none());
    ensure!(download(&files, file, None).await?.as_deref() == Some(&contents[..]));

    conn.disconnect()?;
    Ok(())
}

async fn exec_reducer_created_files(db_name: &str) -> anyhow::Result<()> {
    let (conn, inserted) = connect(db_name)?;
    let text = "written by a reducer";
    let result = outcome(|cb| conn.reducers.write_note_then("note".into(), text.into(), cb))?;
    result.map_err(|e| anyhow!("write_note failed: {e}"))?;
    let document = recv(&inserted, "the note")?;
    let downloaded = download(&conn.files(), document.file, None)
        .await
        .context("downloading the note")?;
    ensure!(
        downloaded.as_deref() == Some(text.as_bytes()),
        "wrong contents {downloaded:?}"
    );
    conn.disconnect()?;
    Ok(())
}

async fn exec_rejected_uploads(db_name: &str) -> anyhow::Result<()> {
    let (conn, _inserted) = connect(db_name)?;
    // The module's allow_upload reducer refuses this content type.
    let result = conn.files().upload(&b"no"[..], typed("application/x-forbidden")).await;
    ensure!(
        matches!(result, Err(UploadError::Rejected { .. })),
        "expected a rejection, got {result:?}"
    );
    conn.disconnect()?;
    Ok(())
}
