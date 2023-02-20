use anyhow::anyhow;
use gotham::handler::HandlerError;
use hyper::http::HeaderValue;
use hyper::StatusCode;
use spacetimedb::auth::get_creds_from_header;
use spacetimedb::auth::identity::encode_token;
use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::hash::Hash;

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
    } else if create {
        let identity = CONTROL_DB.alloc_spacetime_identity().await?;
        let token = encode_token(identity)?;

        Ok(Some((identity, token)))
    } else {
        Ok(None)
    }
}
