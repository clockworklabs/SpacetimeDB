use std::sync::Arc;

use spacetimedb_lib::bsatn;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("Disconnected normally following a call to `DbContext::disconnect`")]
    Disconnected,

    #[error("Already disconnected in call to `DbContext::disconnect`")]
    AlreadyDisconnected,

    #[error("Host returned error when processing subscription query: {error}")]
    SubscriptionError { error: String },

    #[error("Subscription has already ended")]
    AlreadyEnded,

    #[error("Unsubscribe already called on subscription")]
    AlreadyUnsubscribed,

    #[error("Unknown {kind} {name} in {container}")]
    UnknownName {
        kind: &'static str,
        name: String,
        container: &'static str,
    },

    #[error("Failed to parse {ty} from {container}: {source}")]
    Parse {
        ty: &'static str,
        container: &'static str,
        #[source]
        source: Box<Self>,
    },

    #[error("Failed to parse row of type {ty}")]
    ParseRow {
        ty: &'static str,
        #[source]
        source: bsatn::DecodeError,
    },

    #[error("Failed to parse arguments for reducer {reducer_name} of type {ty}")]
    ParseReducerArgs {
        ty: &'static str,
        reducer_name: &'static str,
        #[source]
        source: bsatn::DecodeError,
    },

    #[error("Failed to serialize arguments for reducer {reducer_name} of type {ty}")]
    SerializeReducerArgs {
        ty: &'static str,
        reducer_name: &'static str,
        #[source]
        source: bsatn::EncodeError,
    },

    #[error("Error in WebSocket connection: {0}")]
    Ws(#[from] crate::websocket::WsError),

    #[error("Failed to create Tokio runtime: {source}")]
    CreateTokioRuntime {
        #[source]
        source: Arc<std::io::Error>,
    },

    #[error("Unexpected error when getting current Tokio runtime: {source}")]
    TokioTryCurrent {
        #[source]
        source: Arc<tokio::runtime::TryCurrentError>,
    },

    #[doc(hidden)]
    #[error("Call to set_client_address after CLIENT_ADDRESS was initialized to a different value")]
    AlreadySetClientAddress,
}

pub type Result<T> = std::result::Result<T, Error>;
