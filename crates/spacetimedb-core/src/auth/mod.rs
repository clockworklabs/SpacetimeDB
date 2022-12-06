pub(crate) mod identity;

use crate::hash::Hash;
use crate::nodes::worker_node::control_node_connection::ControlNodeClient;
use anyhow::anyhow;
use gotham::handler::HandlerError;
use hyper::{header::HeaderValue, Body, Response, StatusCode};
use identity::decode_token;

pub fn invalid_token_res() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .unwrap()
}

/// Given an authorization header we will try to get the identity and token from the auth header (as JWT).
/// If there is no JWT in the auth header and [create] is set to [true] we will create a new
/// identity and token and return it as [Ok(Some((identity, token)))]. If there is an identity and
/// token in the authorization, we will verify its authenticity and return an [Err] if we cannot
/// verify it. If it can be verified it is returned as [Ok(Some((identity, token)))]
pub async fn get_or_create_creds_from_header(
    auth_header: Option<&HeaderValue>,
    create: bool,
) -> Result<Option<(Hash, String)>, HandlerError> {
    if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(creds) => Ok(Some(creds)),
            Err(_) => Err(
                HandlerError::from(anyhow!("Authorization is invalid - malformed token."))
                    .with_status(StatusCode::BAD_REQUEST),
            ),
        }
    } else {
        if create {
            Ok(Some(ControlNodeClient::get_shared().get_new_identity().await.unwrap()))
        } else {
            Ok(None)
        }
    }
}

pub fn get_creds_from_header(auth_header: &HeaderValue) -> Result<(Hash, String), ()> {
    // Yes, this is using basic auth. See the below issues.
    // The current form is: Authorization: Basic base64("token:<token>")
    // FOOLS, the lot of them!
    // If/when they fix this issue, this should be changed from
    // basic auth, to a `Authorization: Bearer <token>` header
    // https://github.com/whatwg/websockets/issues/16
    // https://github.com/sta/websocket-sharp/pull/22

    let auth_header = auth_header.to_str().unwrap_or_default().to_string();
    let encoded_token = auth_header.split("Basic ").collect::<Vec<&str>>().get(1).copied();
    let token_string = encoded_token
        .and_then(|encoded_token| base64::decode(encoded_token).ok())
        .and_then(|token_buf| String::from_utf8(token_buf).ok());
    let token_string = token_string.as_deref();
    let token = match token_string {
        Some(token_str) => {
            let split = token_str.split(':').collect::<Vec<&str>>();
            if split.first().copied() != Some("token") {
                None
            } else {
                split.get(1).copied()
            }
        }
        None => None,
    };

    let token_str = if let Some(token) = token {
        token
    } else {
        return Err(());
    };

    let token = decode_token(token_str);
    let token = match token {
        Ok(token) => token,
        Err(error) => {
            log::info!("Deny upgrade. Invalid token: {}", error);
            return Err(());
        }
    };

    let hex_identity = token.claims.hex_identity;
    let identity = Hash::from_hex(hex_identity.as_str()).expect("If this happens we gave out invalid claims");
    Ok((identity, token_str.to_string()))
}
