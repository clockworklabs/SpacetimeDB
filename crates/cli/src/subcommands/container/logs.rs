//! Authorized, bounded log pages pinned to one capture across requests.

use super::operations::ContainerClient;
use anyhow::{bail, ensure, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::{header::AUTHORIZATION, StatusCode};
use spacetimedb_lib::{
    container::{
        logs::*,
        operations::{ContainerApiError, ContainerErrorCode},
    },
    Hash, Identity, Uuid,
};
use std::{io::Write, time::Duration};

pub(super) fn cli() -> Command {
    Command::new("logs")
        .about("Read retained stdout and stderr from one container attempt")
        .arg(Arg::new("database").required(true).help("Database name or Identity"))
        .arg(crate::common_args::server())
        .arg(crate::common_args::yes())
        .arg(Arg::new("generation").long("generation").value_parser(clap::value_parser!(u64).range(1..))
            .help("Read this attempt; omitted selects the current generation once"))
        .arg(Arg::new("cursor").long("cursor").requires("generation")
            .help("Resume after an opaque cursor returned by --json"))
        .arg(Arg::new("follow").long("follow").short('f').action(ArgAction::SetTrue)
            .help("Wait for further output from the selected attempt until it ends"))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue)
            .help("Print one JSON page per line, including timestamps, streams, and resume cursors"))
        .after_help("A restart does not change the selected attempt. Without --json, stdout and stderr retain their original bytes and streams. Retention gaps and interrupted capture are reported on stderr.")
}

pub(super) async fn exec(config: &mut crate::Config, args: &ArgMatches) -> Result<()> {
    let selection = args.get_one::<String>("server").map(String::as_str);
    let server = crate::container::publish::client::endpoint(&config.get_host_url(selection)?)?;
    let database = args.get_one::<String>("database").context("database is required")?;
    let mut query = ContainerLogQuery {
        generation: args.get_one::<u64>("generation").copied(),
        cursor: args.get_one::<String>("cursor").cloned(),
        follow: args.get_flag("follow"),
    };
    query.validate().map_err(anyhow::Error::msg)?;
    let auth = crate::util::get_auth_header(config, false, selection, !args.get_flag("force")).await?;
    let client = ContainerClient::new(server, auth.to_header().context("container logs require a login")?)?;
    // Names are resolved only once. All continuation requests use the Identity
    // and exact generation/capture returned by the first authorized page.
    let mut target = database.clone();
    let mut reader = Selection::new(database.parse().ok(), &query);
    let json = args.get_flag("json");
    let mut reported_loss = None;
    loop {
        let page = tokio::select! {
            result = fetch(&client, &target, &query) => result?,
            signal = tokio::signal::ctrl_c() => { signal?; return Ok(()); }
        };
        reader.accept(&page)?;
        {
            let mut out = std::io::stdout().lock();
            let mut err = std::io::stderr().lock();
            write_page(&page, json, &mut out, &mut err, &mut reported_loss)?;
        }
        if let Some(end) = page.end {
            if end != LogEnd::Eof || page.loss.is_some() {
                bail!("container log capture ended with incomplete output; inspect --json for the recorded reason");
            }
            return Ok(());
        }
        if !query.follow && !page.has_more {
            return Ok(());
        }
        target = page.database_identity.to_hex().to_string();
        query.generation = Some(page.generation);
        query.cursor = Some(page.next_cursor);
        // A healthy follow request long-polls. Bound retries even if a server
        // repeatedly returns an immediate heartbeat without new output.
        if page.records.is_empty() {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(200)) => {},
                signal = tokio::signal::ctrl_c() => { signal?; return Ok(()); }
            }
        }
    }
}

async fn fetch(client: &ContainerClient, database: &str, query: &ContainerLogQuery) -> Result<ContainerLogPage> {
    query.validate().map_err(anyhow::Error::msg)?;
    let mut response = client
        .http
        .get(client.url(database, "logs")?)
        .header(AUTHORIZATION, client.authorization.clone())
        .query(query)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("container logs could not be reached on the selected server"))?;
    let status = response.status();
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_LOG_PAGE_BYTES as u64),
        "container log response exceeds its size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("container log response was interrupted"))?
    {
        ensure!(
            chunk.len() <= MAX_LOG_PAGE_BYTES.saturating_sub(bytes.len()),
            "container log response exceeds its size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    if status != StatusCode::OK {
        let error = serde_json::from_slice::<ContainerApiError>(&bytes)
            .ok()
            .map(|value| value.error);
        bail!(
            "{} (HTTP {})",
            match error {
                Some(ContainerErrorCode::AccessDenied) => "database role does not permit reading container logs",
                Some(ContainerErrorCode::NotFound) => "the selected container log history was not found or has expired",
                Some(ContainerErrorCode::InvalidRequest) => "invalid container log selection or cursor",
                Some(ContainerErrorCode::Conflict) => "the selected container log capture is no longer available",
                _ => "container log service is unavailable on the selected server",
            },
            status.as_u16()
        );
    }
    let page: ContainerLogPage =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid container log response"))?;
    page.validate().map_err(anyhow::Error::msg)?;
    Ok(page)
}

#[derive(PartialEq, Eq)]
struct Capture {
    identity: Identity,
    generation: u64,
    revision: Hash,
    publication: Uuid,
    epoch: u64,
    id: Uuid,
}
struct Selection {
    identity: Option<Identity>,
    generation: Option<u64>,
    capture: Option<Capture>,
    cursor: Option<String>,
    sequence: Option<u64>,
    loss: Option<LogLoss>,
}
impl Selection {
    fn new(identity: Option<Identity>, query: &ContainerLogQuery) -> Self {
        Self {
            identity,
            generation: query.generation,
            capture: None,
            cursor: query.cursor.clone(),
            sequence: None,
            loss: None,
        }
    }
    fn accept(&mut self, page: &ContainerLogPage) -> Result<()> {
        page.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            self.identity.is_none_or(|identity| identity == page.database_identity)
                && self.generation.is_none_or(|generation| generation == page.generation),
            "container logs returned another database or generation"
        );
        ensure!(
            self.loss.is_none_or(|loss| page.loss == Some(loss)),
            "container logs changed the recorded capture loss"
        );
        let capture = Capture {
            identity: page.database_identity,
            generation: page.generation,
            revision: page.deployment_revision,
            publication: page.publication_operation,
            epoch: page.publication_epoch,
            id: page.capture_id,
        };
        ensure!(
            self.capture.as_ref().is_none_or(|expected| expected == &capture),
            "container logs changed the selected capture"
        );
        let mut sequence = self.sequence;
        for record in &page.records {
            if let Some(previous) = sequence {
                ensure!(
                    record.sequence > previous,
                    "container logs repeated or reordered records"
                );
                ensure!(
                    page.retention_gap || previous.checked_add(1) == Some(record.sequence),
                    "container logs omitted records without a retention notice"
                );
            }
            sequence = Some(record.sequence);
        }
        ensure!(
            page.records.is_empty() || self.cursor.as_ref() != Some(&page.next_cursor),
            "container logs did not advance their cursor"
        );
        self.identity = Some(page.database_identity);
        self.generation = Some(page.generation);
        self.capture = Some(capture);
        self.cursor = Some(page.next_cursor.clone());
        self.sequence = sequence;
        self.loss = page.loss;
        Ok(())
    }
}

fn write_page(
    page: &ContainerLogPage,
    json: bool,
    out: &mut impl Write,
    err: &mut impl Write,
    reported_loss: &mut Option<LogLoss>,
) -> Result<()> {
    if json {
        serde_json::to_writer(&mut *out, page)?;
        out.write_all(b"\n")?;
    } else {
        if page.retention_gap {
            writeln!(err, "[container logs: earlier records were removed by retention]")?;
        }
        for record in &page.records {
            match &record.event {
                LogEvent::Data {
                    stream: LogStream::Stdout,
                    bytes,
                    ..
                } => out.write_all(bytes)?,
                LogEvent::Data {
                    stream: LogStream::Stderr,
                    bytes,
                    ..
                } => err.write_all(bytes)?,
                LogEvent::Gap {
                    reason, missed_bytes, ..
                } => writeln!(
                    err,
                    "[container logs: capture gap {reason:?}, missing bytes {missed_bytes:?}]"
                )?,
            }
        }
        if let Some(loss) = page.loss
            && Some(loss) != *reported_loss
        {
            writeln!(err, "[container logs: incomplete capture, {loss:?}]")?;
        }
    }
    *reported_loss = page.loss;
    out.flush()?;
    err.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests;
