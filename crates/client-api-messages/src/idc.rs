//! HTTP responses shared by the IDC receiver and sender.
//!
//! Only recorded outcomes finish a delivery. Authentication, routing, and other
//! failures leave it pending; their response bodies are diagnostics, not outcomes.

use bytes::Bytes;
use http::StatusCode;
use std::fmt;

/// A receiver result or an HTTP error that leaves delivery pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdcResponse {
    /// Empty for a newly committed call; replays may contain BSATN return bytes.
    Success(Bytes),
    /// A reducer or validation failure stored by the receiver. Retrying the same
    /// message returns the same error. Hence, should be considered terminal.
    /// The payload is UTF-8 error text, not BSATN.
    TerminalError(Bytes),
    /// A recorded energy failure, with a UTF-8 description, not BSATN.
    BudgetExceeded(Bytes),
    /// The sender already acknowledged this sequence. There is no result to replay.
    AlreadyAcked,

    /// A delivery failure the sender should retry. It may resolve on a later attempt.
    DeliveryError(IdcDeliveryError),
}

impl IdcResponse {
    pub fn into_http_parts(self) -> (StatusCode, Bytes) {
        match self {
            Self::Success(body) => (StatusCode::OK, body),
            Self::TerminalError(body) => (StatusCode::UNPROCESSABLE_ENTITY, body),
            Self::BudgetExceeded(body) => (StatusCode::PAYMENT_REQUIRED, body),
            Self::AlreadyAcked => (StatusCode::ALREADY_REPORTED, Bytes::new()),
            Self::DeliveryError(error) => error.into_http_parts(),
        }
    }

    /// Decode a complete response body. Anything outside the IDC outcome contract
    /// is a delivery failure, including authentication failures and redirects.
    pub fn from_http_parts(status: StatusCode, body: Bytes) -> Self {
        match status {
            StatusCode::OK => Self::Success(body),
            StatusCode::UNPROCESSABLE_ENTITY => Self::TerminalError(body),
            StatusCode::PAYMENT_REQUIRED => Self::BudgetExceeded(body),
            StatusCode::ALREADY_REPORTED => Self::AlreadyAcked,
            _ => Self::DeliveryError(IdcDeliveryError { status, body }),
        }
    }
}

/// An HTTP response without a usable IDC outcome. Retry without acknowledging
/// the sequence or calling `on_result`; the body must not control retry policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct IdcDeliveryError {
    pub status: StatusCode,
    /// Uninterpreted HTTP body, displayed as lossy UTF-8 for diagnostics only.
    pub body: Bytes,
}

impl IdcDeliveryError {
    pub fn into_http_parts(self) -> (StatusCode, Bytes) {
        (self.status, self.body)
    }
}

impl fmt::Display for IdcDeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}: {}", self.status, String::from_utf8_lossy(&self.body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_round_trip_without_changing_payloads() {
        for (response, status) in [
            (IdcResponse::Success(Bytes::new()), StatusCode::OK),
            (IdcResponse::Success(Bytes::from_static(b"\x00\xff")), StatusCode::OK),
            (
                IdcResponse::TerminalError(Bytes::from_static(b"unauthorized sender")),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                IdcResponse::BudgetExceeded(Bytes::from_static(b"out of energy")),
                StatusCode::PAYMENT_REQUIRED,
            ),
            (IdcResponse::AlreadyAcked, StatusCode::ALREADY_REPORTED),
        ] {
            let (actual_status, body) = response.clone().into_http_parts();
            assert_eq!(actual_status, status);
            assert_eq!(IdcResponse::from_http_parts(actual_status, body), response);
        }
    }

    #[test]
    fn other_responses_leave_delivery_pending_regardless_of_body() {
        for status in [
            201, 204, 301, 307, 400, 401, 403, 404, 408, 415, 425, 429, 500, 502, 503, 504, 530,
        ] {
            for body in [
                Bytes::from_static(b"Ok"),
                Bytes::from_static(b"permanent error"),
                Bytes::new(),
            ] {
                let status = StatusCode::from_u16(status).unwrap();
                let response = IdcResponse::from_http_parts(status, body.clone());
                assert_eq!(
                    response,
                    IdcResponse::DeliveryError(IdcDeliveryError {
                        status,
                        body: body.clone()
                    })
                );
                let (encoded_status, encoded_body) = response.clone().into_http_parts();
                assert_eq!(encoded_status, status);
                assert_eq!(encoded_body, body);
                assert_eq!(IdcResponse::from_http_parts(encoded_status, encoded_body), response);
            }
        }
    }
}
