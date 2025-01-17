use reqwest::{Response, StatusCode};
use thiserror::Error;

#[derive(Debug)]
pub enum RequestSource {
    Client,
    Server,
}

#[derive(Error, Debug)]
pub enum CliError {
    #[error("HTTP status {kind:?} error ({status}): {msg}")]
    Request {
        msg: String,
        kind: RequestSource,
        status: StatusCode,
    },
    #[error(transparent)]
    ReqWest(#[from] reqwest::Error),
    #[error("Config error: The option `{key}` not found")]
    Config { key: String },
    #[error("Config error: The option `{key}` is not a `{kind}`, found: `{type}: {value}`",
        type=found.type_name(),
        value=found
    )]
    ConfigType {
        key: String,
        kind: &'static str,
        found: toml_edit::Item,
    },
}

/// Turn a response into an error if the server returned an error.
pub async fn error_for_status(response: Response) -> Result<Response, CliError> {
    let status = response.status();
    if let Some(kind) = status
        .is_client_error()
        .then_some(RequestSource::Client)
        // Anything that is not a success is an error for the client, even a redirect that is not followed.
        .or_else(|| (!status.is_success()).then_some(RequestSource::Server))
    {
        let msg = response.text().await?;
        return Err(CliError::Request { kind, msg, status });
    }

    Ok(response)
}
