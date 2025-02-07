use crate::auth::{anon_auth_middleware, SpacetimeAuth};
use crate::util::EmptyBody;
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Extension;
use http::StatusCode;
use serde::Deserialize;
use spacetimedb_client_api_messages::name::{self, DomainName};

pub(crate) struct DomainParsingRejection;

impl IntoResponse for DomainParsingRejection {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, "Unable to parse domain name").into_response()
    }
}

#[derive(Deserialize)]
pub struct RegisterTldParams {
    domain: String,
}

pub async fn register_tld<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(RegisterTldParams { domain }): Path<RegisterTldParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    // require an empty body in case we want to add json params in the future
    EmptyBody: EmptyBody,
) -> axum::response::Result<impl IntoResponse> {
    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence not using get_or_create

    let tld = domain.parse::<DomainName>().map_err(|_| DomainParsingRejection)?.into();
    let result = ctx.register_tld(&auth.identity, tld).await.map_err(log_and_500)?;
    let code = match result {
        name::RegisterTldResult::Success { .. } => StatusCode::CREATED,
        name::RegisterTldResult::AlreadyRegistered { .. } => StatusCode::OK,
        name::RegisterTldResult::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
    };
    Ok((code, axum::Json(result)))
}

pub fn router<S>(ctx: S) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::put;
    let domain_router = axum::Router::new().route("/", put(register_tld::<S>));
    axum::Router::new()
        .nest("/:domain", domain_router)
        .route_layer(axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>))
}
