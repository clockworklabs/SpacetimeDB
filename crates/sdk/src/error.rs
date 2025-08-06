use std::sync::Arc;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("Connection is already disconnected or has terminated normally")]
    Disconnected,

    #[error("Failed to connect: {source}")]
    FailedToConnect {
        #[source]
        source: InternalError,
    },

    #[error("Host returned error when processing subscription query: {error}")]
    SubscriptionError { error: String },

    #[error("Subscription has already ended")]
    AlreadyEnded,

    #[error("Unsubscribe already called on subscription")]
    AlreadyUnsubscribed,

    #[error(transparent)]
    Internal(#[from] InternalError),
}

#[derive(Debug, Clone)]
pub struct InternalError {
    message: String,
    cause: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

impl InternalError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }

    #[doc(hidden)]
    /// Called by codegen. Not part of this library's stable API.
    pub fn with_cause(self, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            cause: Some(Arc::new(cause)),
            ..self
        }
    }

    #[doc(hidden)]
    /// Called by codegen. Not part of this library's stable API.
    pub fn failed_parse(ty: &'static str, container: &'static str) -> Self {
        Self::new(format!(
            "Failed to parse {ty} from {container}.

This is often caused by outdated bindings; try re-running `spacetime generate`."
        ))
    }

    #[doc(hidden)]
    /// Called by codegen. Not part of this library's stable API.
    pub fn unknown_name(category: &'static str, name: impl std::fmt::Display, container: &'static str) -> Self {
        Self::new(format!(
            "Unknown {category} {name} in {container}

This is often caused by outdated bindings; try re-running `spacetime generate`."
        ))
    }
}

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)?;
        if let Some(cause) = &self.cause {
            write!(f, ": {cause}")?;
        }
        Ok(())
    }
}

impl std::error::Error for InternalError {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        // `self.cause.as_deref()`,
        // except that formulation is unable to convert our `dyn Error + Send + Sync`
        // into a `dyn Error`.
        // See [this StackOverflow answer](https://stackoverflow.com/questions/63810977/strange-behavior-when-adding-the-send-trait-to-a-boxed-trait-object).
        if let Some(cause) = &self.cause {
            Some(cause)
        } else {
            None
        }
    }
    fn description(&self) -> &str {
        &self.message
    }
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub type Result<T> = std::result::Result<T, Error>;
