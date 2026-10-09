//! Tests of the SDK's file API against <../../../../modules/sdk-test-files>.
//!
//! Uploads and downloads are async, so unlike the other test clients these run as one sequence
//! of steps, waiting on channels for the callbacks they need.

use crate::module_bindings::*;

use anyhow::{anyhow, bail, ensure, Context};
use spacetimedb_sdk::{DbContext, Error, FileRef, Table};
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

fn status(result: Result<impl std::fmt::Debug, Error>) -> anyhow::Result<u16> {
    match result {
        Err(Error::FileRequest {
            status: Some(status), ..
        }) => Ok(status),
        other => bail!("expected a failed file request, got {other:?}"),
    }
}

async fn exec_upload_and_download(db_name: &str) -> anyhow::Result<()> {
    let (conn, inserted) = connect(db_name)?;
    let files = conn.files();

    let contents = b"hello, files";
    let uploaded = files.upload("text/plain", &contents[..]).await?;
    ensure!(uploaded.size == contents.len() as u64, "wrong size {}", uploaded.size);

    // A client passes the reference to a reducer, which stores it.
    let result = outcome(|cb| conn.reducers.attach_then("greeting".into(), uploaded.file_ref, cb))?;
    result.map_err(|e| anyhow!("attach failed: {e}"))?;
    let document = recv(&inserted, "the document")?;
    // The generated row type holds a `FileRef`, not a `u256`.
    let file: FileRef = document.file;
    ensure!(file == uploaded.file_ref, "the row holds a different reference");
    ensure!(document.size == contents.len() as u64);

    // Any subscriber can download it.
    ensure!(files.download(file).await? == contents[..], "wrong contents");
    ensure!(files.download_range(file, 7..12).await? == b"files"[..], "wrong range");
    let metadata = files.metadata(file).await?;
    ensure!(metadata.file_ref == file);
    ensure!(metadata.size == contents.len() as u64);
    ensure!(metadata.content_hash == uploaded.content_hash);
    ensure!(metadata.content_type.as_deref() == Some("text/plain"));
    ensure!(
        files.url(file).starts_with(server_url()) && files.url(file).ends_with(&file.to_string()),
        "unexpected URL {}",
        files.url(file)
    );

    // Retrying with the same idempotency key returns the same file. The anonymous connection
    // authenticates with the token the server issued it.
    let first = files.upload_idempotent("text/plain", &b"once"[..], "key-1").await?;
    let retry = files.upload_idempotent("text/plain", &b"once"[..], "key-1").await?;
    ensure!(first.file_ref == retry.file_ref, "the retry created another file");

    // A reference whose read secret is wrong finds nothing.
    let forged = FileRef::new(file.id(), file.read_secret() ^ 1);
    ensure!(status(files.download(forged).await)? == 404);
    let result = outcome(|cb| conn.reducers.attach_then("forged".into(), forged, cb))?;
    ensure!(result.is_err(), "the module accepted a forged reference");

    // Once deleted, the file's metadata is gone.
    let result = outcome(|cb| conn.reducers.remove_then("greeting".into(), cb))?;
    result.map_err(|e| anyhow!("remove failed: {e}"))?;
    ensure!(status(files.metadata(file).await)? == 404);

    conn.disconnect()?;
    Ok(())
}

async fn exec_reducer_created_files(db_name: &str) -> anyhow::Result<()> {
    let (conn, inserted) = connect(db_name)?;
    let text = "written by a reducer";
    let result = outcome(|cb| conn.reducers.write_note_then("note".into(), text.into(), cb))?;
    result.map_err(|e| anyhow!("write_note failed: {e}"))?;
    let document = recv(&inserted, "the note")?;
    let downloaded = conn
        .files()
        .download(document.file)
        .await
        .context("downloading the note")?;
    ensure!(downloaded == text.as_bytes(), "wrong contents {downloaded:?}");
    conn.disconnect()?;
    Ok(())
}

async fn exec_rejected_uploads(db_name: &str) -> anyhow::Result<()> {
    let (conn, _inserted) = connect(db_name)?;
    // The module's allow_upload reducer refuses this content type.
    let result = conn.files().upload("application/x-forbidden", &b"no"[..]).await;
    ensure!(status(result)? == 403);
    conn.disconnect()?;
    Ok(())
}
