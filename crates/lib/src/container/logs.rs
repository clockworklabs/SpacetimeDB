//! Bounded process-log pages. One selection follows one exact attempt and
//! capture; it never silently switches to a replacement container.

use super::operations::decimal_u64;
use crate::{deployment::uuid_json, Hash, Identity, Uuid};
use serde::{Deserialize, Serialize};

pub const MAX_LOG_PAGE_BYTES: usize = 512 * 1024;
pub const MAX_LOG_PAGE_RECORDS: usize = 64;
pub const MAX_LOG_RECORD_BYTES: usize = 8192;
pub const MAX_LOG_CURSOR_BYTES: usize = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerLogQuery {
    #[serde(default, with = "optional_decimal_u64")]
    pub generation: Option<u64>,
    pub cursor: Option<String>,
    #[serde(default)]
    pub follow: bool,
}

impl ContainerLogQuery {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.generation == Some(0)
            || self
                .cursor
                .as_ref()
                .is_some_and(|cursor| !valid_cursor(cursor) || self.generation.is_none())
        {
            return Err("invalid container log selection");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogGapReason {
    SourceRotation,
    SourceBackpressure,
    CaptureStartedLate,
}

/// No Debug implementation: process output must not enter platform diagnostics.
/// bytes preserves arbitrary output, including binary data and partial lines.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LogEvent {
    Data {
        #[serde(with = "decimal_i64")]
        timestamp_micros: i64,
        stream: LogStream,
        bytes: Vec<u8>,
    },
    Gap {
        #[serde(with = "decimal_i64")]
        timestamp_micros: i64,
        reason: LogGapReason,
        #[serde(with = "optional_decimal_u64")]
        missed_bytes: Option<u64>,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    #[serde(with = "decimal_u64")]
    pub sequence: u64,
    pub event: LogEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEnd {
    Eof,
    Cancelled,
    SourceFailed,
    OwnerLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLoss {
    SourceUnavailable,
    SourceOpenFailed,
    SourceBindingMismatch,
    SourceRejected,
    AttachmentTimeout,
    DrainFailed,
    DrainTimeout,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerLogPage {
    pub database_identity: Identity,
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    pub deployment_revision: Hash,
    #[serde(with = "uuid_json")]
    pub publication_operation: Uuid,
    #[serde(with = "decimal_u64")]
    pub publication_epoch: u64,
    #[serde(with = "uuid_json")]
    pub capture_id: Uuid,
    pub records: Vec<LogRecord>,
    pub next_cursor: String,
    #[serde(with = "decimal_u64")]
    pub oldest_retained_sequence: u64,
    pub retention_gap: bool,
    /// More records were available when this page was read.
    pub has_more: bool,
    /// Present only on the page reaching the final record. A loss observation
    /// remains incomplete even if the Docker source also reached natural EOF.
    pub end: Option<LogEnd>,
    pub loss: Option<LogLoss>,
}

impl ContainerLogPage {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.generation == 0
            || self.publication_epoch == 0
            || self.capture_id.as_u128() == 0
            || self.publication_operation.as_u128() == 0
            || self.oldest_retained_sequence == 0
            || !valid_cursor(&self.next_cursor)
            || self.records.len() > MAX_LOG_PAGE_RECORDS
            || (self.has_more && (self.records.is_empty() || self.end.is_some()))
        {
            return Err("invalid container log page");
        }
        let mut previous = 0;
        for record in &self.records {
            if record.sequence < self.oldest_retained_sequence
                || record.sequence <= previous
                || matches!(&record.event, LogEvent::Data { bytes, .. } if bytes.len() > MAX_LOG_RECORD_BYTES)
            {
                return Err("invalid container log record");
            }
            previous = record.sequence;
        }
        Ok(())
    }
}

fn valid_cursor(cursor: &str) -> bool {
    (1..=MAX_LOG_CURSOR_BYTES).contains(&cursor.len())
        && cursor
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

mod decimal_i64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(number: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(number)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        let text = String::deserialize(deserializer)?;
        let value: i64 = text.parse().map_err(serde::de::Error::custom)?;
        if value.to_string() != text {
            return Err(serde::de::Error::custom("expected canonical decimal i64"));
        }
        Ok(value)
    }
}

mod optional_decimal_u64 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(number: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        number.map(|value| value.to_string()).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| {
                let value: u64 = text.parse().map_err(serde::de::Error::custom)?;
                if value.to_string() != text {
                    return Err(serde::de::Error::custom("expected canonical decimal u64"));
                }
                Ok(value)
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests;
