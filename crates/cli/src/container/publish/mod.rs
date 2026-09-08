//! Managed publication uses one immutable operation and a durable local resume
//! directory. Ambiguous HTTP responses never select a new operation or fall
//! back to the legacy module publication endpoint.
pub mod client;
pub mod journal;

#[cfg(test)]
pub(crate) mod tests;

use anyhow::{bail, ensure, Context, Result};
use client::{ArtifactEndpoint, PublisherClient, UploadStatus, UPLOAD_CHUNK_BYTES};
use journal::Journal;
use spacetimedb_lib::deployment::{
    api::{PublicationPhase, PublicationStatus},
    operation_expiry_ms,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum Outcome {
    Complete(PublicationStatus),
    Pending(PublicationStatus),
    Aborted(PublicationStatus),
    /// Database creation succeeded. Never describe a naming failure as a
    /// rollback or send another publication operation to compensate for it.
    NamingUnconfirmed(PublicationStatus),
}

pub async fn run(
    client: &PublisherClient,
    journal: &mut Journal,
    approved_artifact: Option<&str>,
    wait: Duration,
    cancel: CancellationToken,
) -> Result<Outcome> {
    let cancel = cancel.child_token();
    let _cancel = crate::container::CancelOnDrop(cancel.clone());
    ensure!(
        client.server().as_str() == journal.record.server,
        "resume server differs from the original publication endpoint"
    );
    let artifact = client.artifact_endpoint(&journal.record.artifact_endpoint, approved_artifact)?;
    let request = journal.record.request()?;
    let operation = request.manifest.current().envelope.operation_id;
    let permission = client.permission().await?;
    ensure!(
        permission.identity == journal.record.publisher,
        "resume publisher differs from the original publication identity"
    );

    // Observe an already admitted operation before requiring original local
    // object files or applying today's new-publication permission/limits.
    if journal.record.submitted
        && let Some(database) = journal.record.database
    {
        match observe_or_replay(client, journal, database, operation).await {
            Ok(Some(status)) => {
                journal.record.check_status(&request, &status)?;
                journal.record.status = Some(status);
                journal.save()?;
                return wait_and_name(client, journal, wait, &cancel).await;
            }
            Ok(None) => (),
            Err(error) => {
                return Err(error).context("publication observation was not confirmed; resume the same directory")
            }
        }
    }
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    operation_expiry_ms(operation, now)?;
    ensure!(
        !cancel.is_cancelled(),
        "publication cancelled; resume directory retained"
    );
    if journal.record.database.is_none() {
        let reservation = client
            .reserve(
                journal
                    .record
                    .reservation
                    .as_ref()
                    .context("creation reservation is missing")?,
            )
            .await?;
        ensure!(
            reservation.operation_id == operation && reservation.staging_open,
            "reservation was not confirmed for this operation"
        );
        let returned = client.artifact_endpoint(&reservation.artifact_endpoint, approved_artifact)?;
        ensure!(
            returned.as_str() == artifact.as_str(),
            "reservation changed the selected artifact endpoint"
        );
        journal.record.database = Some(reservation.database_identity);
        journal.save()?;
    }
    journal.verify_objects(cancel.clone()).await?;
    for index in 0..journal.record.uploads.len() {
        upload(client, &artifact, journal, index, &cancel).await?;
    }
    ensure!(
        !cancel.is_cancelled(),
        "publication cancelled; resume directory retained"
    );
    // This marker must be durable before sending the admission request. A crash
    // after the server commits but before the response can then observe/replay.
    journal.record.submitted = true;
    journal.save()?;
    let status = client
        .submit_bytes(journal.record.database.unwrap(), journal.record.request_json.as_bytes())
        .await
        .context("publication outcome is not confirmed; resume the same directory")?;
    journal.record.check_status(&request, &status)?;
    journal.record.status = Some(status);
    journal.save()?;
    wait_and_name(client, journal, wait, &cancel).await
}

/// Operation inspection has current database-read authorization. Exact PUT
/// replay instead authenticates the original admitted publisher and immutable
/// request. It must precede local artifact/expiry work after read revocation.
async fn observe_or_replay(
    client: &PublisherClient,
    journal: &Journal,
    database: spacetimedb_lib::Identity,
    operation: spacetimedb_lib::Uuid,
) -> Result<Option<PublicationStatus>> {
    match client.status(database, operation).await {
        Err(error)
            if error
                .downcast_ref::<client::HttpFailure>()
                .is_some_and(|error| error.status == reqwest::StatusCode::FORBIDDEN) =>
        {
            client
                .submit_bytes(database, journal.record.request_json.as_bytes())
                .await
                .map(Some)
                .context("exact publication replay was not confirmed; keep the same resume directory")
        }
        result => result,
    }
}

async fn upload(
    client: &PublisherClient,
    artifact: &ArtifactEndpoint,
    journal: &mut Journal,
    index: usize,
    cancel: &CancellationToken,
) -> Result<()> {
    let database = journal.record.database.context("reservation missing")?;
    let item = journal.record.uploads[index].clone();
    let mut status = if let Some(prior) = item.session {
        match client.upload_status(artifact, database, prior.id, item.object).await {
            Ok(status) => status,
            Err(error)
                if error.downcast_ref::<client::HttpFailure>().is_some_and(|error| {
                    matches!(error.status, reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE)
                }) =>
            {
                client.begin_upload(artifact, database, item.kind, item.object).await?
            }
            Err(error) => return Err(error),
        }
    } else {
        client.begin_upload(artifact, database, item.kind, item.object).await?
    };
    store_upload(journal, index, &status)?;
    if status.complete {
        return Ok(());
    }
    let mut file = tokio::fs::File::open(journal.object_path(&journal.record.uploads[index])).await?;
    let mut ambiguous_attempts = 0;
    while status.offset < status.object.size {
        ensure!(
            !cancel.is_cancelled(),
            "artifact upload cancelled; resume directory retained"
        );
        file.seek(std::io::SeekFrom::Start(status.offset)).await?;
        let len = usize::try_from((status.object.size - status.offset).min(UPLOAD_CHUNK_BYTES as u64))?;
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes)
            .await
            .context("retained artifact was truncated")?;
        let expected = status.offset + len as u64;
        match client.append_upload(artifact, database, &status, bytes).await {
            Ok(next) => {
                ensure!(
                    next.offset == expected,
                    "artifact append acknowledged an unexpected offset"
                );
                status = next;
                ambiguous_attempts = 0;
            }
            Err(error) => {
                if !retryable(&error) {
                    return Err(error);
                }
                ambiguous_attempts += 1;
                ensure!(
                    ambiguous_attempts <= 3,
                    "artifact append outcome remains unknown; resume the same directory"
                );
                let next = client
                    .upload_status(artifact, database, status.id, status.object)
                    .await?;
                ensure!(
                    next.offset == status.offset || next.offset == expected,
                    "artifact offset changed outside this append"
                );
                status = next;
            }
        }
        store_upload(journal, index, &status)?;
    }
    status = match client.complete_upload(artifact, database, &status).await {
        Ok(status) => status,
        Err(error) if retryable(&error) => {
            let observed = client
                .upload_status(artifact, database, status.id, status.object)
                .await?;
            if observed.complete {
                observed
            } else {
                return Err(error).context("artifact completion is not confirmed; resume the same directory");
            }
        }
        Err(error) => return Err(error),
    };
    store_upload(journal, index, &status)
}
fn store_upload(journal: &mut Journal, index: usize, status: &UploadStatus) -> Result<()> {
    journal.record.uploads[index].session = Some(status.clone());
    journal.save()
}
fn retryable(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|error| error.is_timeout() || error.is_connect() || error.is_request() || error.is_body())
        || error.downcast_ref::<client::HttpFailure>().is_some_and(|error| {
            error.status.is_server_error()
                || matches!(
                    error.status,
                    reqwest::StatusCode::CONFLICT | reqwest::StatusCode::TOO_MANY_REQUESTS
                )
        })
}
async fn wait_and_name(
    client: &PublisherClient,
    journal: &mut Journal,
    wait: Duration,
    cancel: &CancellationToken,
) -> Result<Outcome> {
    let deadline = tokio::time::Instant::now() + wait;
    let request = journal.record.request()?;
    loop {
        let status = journal
            .record
            .status
            .clone()
            .context("confirmed publication status missing")?;
        match status.phase {
            PublicationPhase::AbortedBeforeCommit => return Ok(Outcome::Aborted(status)),
            PublicationPhase::Complete => {
                if let Some(name) = journal
                    .record
                    .requested_name
                    .clone()
                    .filter(|_| !journal.record.name_confirmed)
                {
                    // Existing name replacement has no CAS token. Do not retry an
                    // ambiguous name update and overwrite aliases added later.
                    if journal.record.naming_attempted {
                        return Ok(Outcome::NamingUnconfirmed(status));
                    }
                    journal.record.naming_attempted = true;
                    journal.save()?;
                    if client.set_name(status.database_identity, &name).await.is_err() {
                        return Ok(Outcome::NamingUnconfirmed(status));
                    }
                    journal.record.name_confirmed = true;
                    journal.save()?;
                }
                return Ok(Outcome::Complete(status));
            }
            _ => (),
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(Outcome::Pending(status));
        }
        tokio::select! {
            _=cancel.cancelled()=>bail!("publication wait cancelled; its durable operation continues, resume the same directory"),
            _=tokio::time::sleep_until((tokio::time::Instant::now()+Duration::from_secs(1)).min(deadline))=>(),
        }
        let next = tokio::time::timeout_at(
            deadline,
            observe_or_replay(client, journal, status.database_identity, status.operation_id),
        )
        .await;
        match next {
            Ok(Ok(Some(next))) => {
                journal.record.check_status(&request, &next)?;
                journal.record.status = Some(next);
                journal.save()?;
            }
            Ok(Ok(None)) => bail!("confirmed publication disappeared; keep the same operation and resume directory"),
            Ok(Err(error)) => {
                return Err(error).context("publication status is not confirmed; resume the same directory")
            }
            Err(_) => return Ok(Outcome::Pending(status)),
        }
    }
}
