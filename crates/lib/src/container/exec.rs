//! Versioned messages for one authenticated WebSocket per container exec.
//!
//! Authenticate the upgrade before accepting a Start message. Start must be the
//! first application message and is accepted only once. A Ready response binds
//! the socket to that exact database, generation and session until Exit/Error.
//! Reconnection or replay must never silently start another process. EOF closes
//! only stdin; closing the socket does not establish process termination.
//! Reject duplicate Start, stdin after EOF, and controls after the terminal
//! response. Output precedes exactly one Exit/Error. Bound queued bytes as well
//! as individual messages. Retain the chosen RuntimeHandle throughout the
//! session, even if another instance starts under the same deployment.
//!
//! These codecs bound and validate messages, not authority or session ordering.
//! The server separately checks Admin permission, the complete current runtime
//! binding and its lease, and validates the inherited environment before exec.

use super::{
    operations::{decimal_u64, ContainerErrorCode},
    validate_env_key, validate_exec_size, MAX_ARGV_ENTRIES, MAX_ENV_KEYS,
};
use crate::{deployment::uuid_json, Identity, Uuid};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const SUBPROTOCOL: &str = "v1.container.exec.spacetimedb";
/// JSON escapes may expand a valid 128 KiB argv/environment by up to six times.
pub const MAX_CONTROL_BYTES: usize = 1024 * 1024;
pub const MAX_DATA_BYTES: usize = 64 * 1024;
pub const MAX_BINARY_BYTES: usize = MAX_DATA_BYTES + 1;

/// Fixed diagnostics never reflect argv, environment values or peer input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid container exec message")]
pub struct ProtocolError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSize {
    pub rows: u16,
    pub columns: u16,
}
impl TerminalSize {
    pub fn validate(self) -> Result<(), ProtocolError> {
        if (1..=4096).contains(&self.rows) && (1..=4096).contains(&self.columns) {
            Ok(())
        } else {
            Err(ProtocolError)
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecStart {
    /// Exact control generation selected by status, never a floating "current".
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    /// Literal argv; the server never supplies an implicit shell or user override.
    pub argv: Vec<String>,
    /// None inherits the container's directory. Some must be an absolute path.
    pub working_directory: Option<String>,
    /// Overrides for this process only; platform keys remain reserved.
    pub environment: BTreeMap<String, String>,
    pub stdin: bool,
    /// Some allocates a PTY at this initial size; None keeps stdout/stderr separate.
    pub terminal: Option<TerminalSize>,
}
impl fmt::Debug for ExecStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecStart")
            .field("generation", &self.generation)
            .field("stdin", &self.stdin)
            .field("terminal", &self.terminal)
            .finish_non_exhaustive()
    }
}
impl ExecStart {
    /// The backend must also count its inherited environment after applying
    /// these overrides. This validation cannot see that immutable launch state.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.generation == 0
            || self.argv.is_empty()
            || self.argv[0].is_empty()
            || self.argv.len() > MAX_ARGV_ENTRIES
            || self.environment.len() > MAX_ENV_KEYS
            || self.environment.keys().any(|key| validate_env_key(key).is_err())
            || self
                .working_directory
                .as_ref()
                .is_some_and(|path| !path.starts_with('/') || path.len() > 4096 || path.contains('\0'))
        {
            return Err(ProtocolError);
        }
        if let Some(size) = self.terminal {
            size.validate()?;
        }
        let environment: Vec<_> = self
            .environment
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        validate_exec_size(&self.argv, &environment).map_err(|_| ProtocolError)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientControl {
    Start(ExecStart),
    StdinEof,
    Resize(TerminalSize),
    /// Linux signal number on the supported x86_64/aarch64 guest platforms.
    /// Targets this exec process, never the container main process or a host PID.
    Signal(u8),
}
impl ClientControl {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() > MAX_CONTROL_BYTES {
            return Err(ProtocolError);
        }
        let message: Self = serde_json::from_slice(bytes).map_err(|_| ProtocolError)?;
        message.validate()?;
        Ok(message)
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Start(start) => start.validate(),
            Self::Resize(size) => size.validate(),
            Self::Signal(1..=64) | Self::StdinEof => Ok(()),
            Self::Signal(_) => Err(ProtocolError),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecReady {
    pub database_identity: Identity,
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    #[serde(with = "uuid_json")]
    pub session_id: Uuid,
    pub tty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerControl {
    Ready(ExecReady),
    /// Follows the final output frame and confirms observed process exit.
    Exit {
        exit_code: i64,
    },
    /// Terminal failure. Does not assert that an already started process exited.
    Error {
        error: ContainerErrorCode,
    },
}
impl ServerControl {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        // Responses contain metadata only, so they need much less space than Start.
        if bytes.len() > 4096 {
            return Err(ProtocolError);
        }
        let message: Self = serde_json::from_slice(bytes).map_err(|_| ProtocolError)?;
        if matches!(&message, Self::Ready(ready) if ready.generation == 0 || ready.session_id.as_u128() == 0) {
            return Err(ProtocolError);
        }
        Ok(message)
    }
}

/// PTY output uses Stdout because the guest PTY combines both output streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stream {
    Stdin = 0,
    Stdout = 1,
    Stderr = 2,
}

/// One channel byte followed by arbitrary nonempty process bytes. Binary data
/// need not be UTF-8; empty messages never stand in for an EOF control message.
pub fn encode_data(stream: Stream, bytes: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_DATA_BYTES {
        return Err(ProtocolError);
    }
    let mut frame = Vec::with_capacity(bytes.len() + 1);
    frame.push(stream as u8);
    frame.extend_from_slice(bytes);
    Ok(frame)
}

pub fn decode_stdin(frame: &[u8]) -> Result<&[u8], ProtocolError> {
    let (stream, data) = decode_data(frame)?;
    if stream == Stream::Stdin {
        Ok(data)
    } else {
        Err(ProtocolError)
    }
}

pub fn decode_output(frame: &[u8]) -> Result<(Stream, &[u8]), ProtocolError> {
    let (stream, data) = decode_data(frame)?;
    if stream == Stream::Stdin {
        Err(ProtocolError)
    } else {
        Ok((stream, data))
    }
}

fn decode_data(frame: &[u8]) -> Result<(Stream, &[u8]), ProtocolError> {
    if frame.len() < 2 || frame.len() > MAX_BINARY_BYTES {
        return Err(ProtocolError);
    }
    let stream = match frame[0] {
        0 => Stream::Stdin,
        1 => Stream::Stdout,
        2 => Stream::Stderr,
        _ => return Err(ProtocolError),
    };
    Ok((stream, &frame[1..]))
}

#[cfg(test)]
mod tests;
