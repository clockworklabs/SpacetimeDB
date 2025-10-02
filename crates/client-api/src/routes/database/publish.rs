use std::{borrow::Cow, env, num::NonZeroU8};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    response::ErrorResponse,
    Extension,
};
use http::StatusCode;
use serde::Deserialize;
use spacetimedb::{host::UpdateDatabaseResult, messages::control_db::HostType, Identity};
use spacetimedb_client_api_messages::name::{self, DatabaseName, MigrationPolicy, PublishOp, PublishResult};
use spacetimedb_lib::Hash;
use spacetimedb_schema::auto_migrate::MigrationPolicy as SchemaMigrationPolicy;

use crate::{auth::SpacetimeAuth, log_and_500, util::NameOrIdentity, ControlStateDelegate, DatabaseDef, NodeDelegate};

fn require_spacetime_auth_for_creation() -> bool {
    env::var("TEMP_REQUIRE_SPACETIME_AUTH").is_ok_and(|v| !v.is_empty())
}

// A hacky function to let us restrict database creation on maincloud.
fn allow_creation(auth: &SpacetimeAuth) -> Result<(), ErrorResponse> {
    if !require_spacetime_auth_for_creation() {
        return Ok(());
    }
    if auth.claims.issuer.trim_end_matches('/') == "https://auth.spacetimedb.com" {
        Ok(())
    } else {
        log::trace!(
            "Rejecting creation request because auth issuer is {}",
            auth.claims.issuer
        );
        Err((
            StatusCode::UNAUTHORIZED,
            "To create a database, you must be logged in with a SpacetimeDB account.",
        )
            .into())
    }
}

#[derive(Deserialize)]
pub struct PublishDatabaseParams {
    name_or_identity: Option<NameOrIdentity>,
}

#[derive(Deserialize)]
pub struct PublishDatabaseQueryParams {
    #[serde(default)]
    clear: bool,
    num_replicas: Option<usize>,
    /// [`Hash`] of [`MigrationToken`]` to be checked if `MigrationPolicy::BreakClients` is set.
    ///
    /// Users obtain such a hash via the `/database/:name_or_identity/pre-publish POST` route.
    /// This is a safeguard to require explicit approval for updates which will break clients.
    token: Option<Hash>,
    #[serde(default)]
    policy: MigrationPolicy,
    parent: Option<NameOrIdentity>,
}

pub async fn publish<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Path(PublishDatabaseParams { name_or_identity }): Path<PublishDatabaseParams>,
    Query(PublishDatabaseQueryParams {
        clear,
        num_replicas,
        token,
        policy,
        parent,
    }): Query<PublishDatabaseQueryParams>,
    Extension(auth): Extension<SpacetimeAuth>,
    body: Bytes,
) -> axum::response::Result<axum::Json<PublishResult>> {
    let (database_identity, db_name) = get_or_create_identity_and_name(&ctx, &auth, name_or_identity.as_ref()).await?;

    log::trace!("Publishing to the identity: {}", database_identity.to_hex());

    // Check if the database already exists.
    let exists = ctx
        .get_database_by_identity(&database_identity)
        .map_err(log_and_500)?
        .is_some();
    // If not, check that the we caller is sufficiently authenticated.
    if !exists {
        allow_creation(&auth)?;
    }
    // If the `clear` flag was given, clear the database if it exists.
    // NOTE: The `clear_database` method has to check authorization.
    if clear && exists {
        ctx.clear_database(&auth.claims.identity, &database_identity)
            .await
            .map_err(log_and_500)?;
    }
    // Indicate in the response whether we created or updated the database.
    let publish_op = if exists { PublishOp::Updated } else { PublishOp::Created };
    // Check that the replication factor looks somewhat sane.
    let num_replicas = num_replicas
        .map(|n| {
            let n = u8::try_from(n).map_err(|_| bad_request(format!("Replication factor {n} out of bounds").into()))?;
            Ok::<_, ErrorResponse>(NonZeroU8::new(n))
        })
        .transpose()?
        .flatten();
    // If a parent is given, resolve to an existing database.
    let parent = if let Some(name_or_identity) = parent {
        let identity = name_or_identity
            .resolve(&ctx)
            .await
            .map_err(|_| bad_request(format!("Parent database {name_or_identity} not found").into()))?;
        Some(identity)
    } else {
        None
    };

    let schema_migration_policy = schema_migration_policy(policy, token)?;
    let maybe_updated = ctx
        .publish_database(
            &auth.claims.identity,
            DatabaseDef {
                database_identity,
                program_bytes: body.into(),
                num_replicas,
                host_type: HostType::Wasm,
                parent,
            },
            schema_migration_policy,
        )
        .await
        .map_err(log_and_500)?;

    match maybe_updated {
        Some(UpdateDatabaseResult::AutoMigrateError(errs)) => {
            Err(bad_request(format!("Database update rejected: {errs}").into()))
        }
        Some(UpdateDatabaseResult::ErrorExecutingMigration(err)) => Err(bad_request(
            format!("Failed to create or update the database: {err}").into(),
        )),
        None
        | Some(
            UpdateDatabaseResult::NoUpdateNeeded
            | UpdateDatabaseResult::UpdatePerformed
            | UpdateDatabaseResult::UpdatePerformedWithClientDisconnect,
        ) => Ok(axum::Json(PublishResult::Success {
            domain: db_name.cloned(),
            database_identity,
            op: publish_op,
        })),
    }
}

/// Try to resolve `name_or_identity` to an [Identity] and [DatabaseName].
///
/// - If the database exists and has a name registered for it, return that.
/// - If the database does not exist, but `name_or_identity` is a name,
///   try to register the name and return alongside a newly allocated [Identity]
/// - Otherwise, if the database does not exist and `name_or_identity` is `None`,
///   allocate a fresh [Identity] and no name.
///
async fn get_or_create_identity_and_name<'a>(
    ctx: &(impl ControlStateDelegate + NodeDelegate),
    auth: &SpacetimeAuth,
    name_or_identity: Option<&'a NameOrIdentity>,
) -> axum::response::Result<(Identity, Option<&'a DatabaseName>)> {
    match name_or_identity {
        Some(noi) => match noi.try_resolve(ctx).await.map_err(log_and_500)? {
            Ok(resolved) => Ok((resolved, noi.name())),
            Err(name) => {
                // `name_or_identity` was a `NameOrIdentity::Name`, but no record
                // exists yet. Create it now with a fresh identity.
                allow_creation(auth)?;
                let database_auth = SpacetimeAuth::alloc(ctx).await?;
                let database_identity = database_auth.claims.identity;
                create_name(ctx, auth, &database_identity, name).await?;
                Ok((database_identity, Some(name)))
            }
        },
        None => {
            let database_auth = SpacetimeAuth::alloc(ctx).await?;
            let database_identity = database_auth.claims.identity;
            Ok((database_identity, None))
        }
    }
}

/// Try to register `name` for database `database_identity`.
async fn create_name(
    ctx: &(impl NodeDelegate + ControlStateDelegate),
    auth: &SpacetimeAuth,
    database_identity: &Identity,
    name: &DatabaseName,
) -> axum::response::Result<()> {
    let tld: name::Tld = name.clone().into();
    let tld = match ctx
        .register_tld(&auth.claims.identity, tld)
        .await
        .map_err(log_and_500)?
    {
        name::RegisterTldResult::Success { domain } | name::RegisterTldResult::AlreadyRegistered { domain } => domain,
        name::RegisterTldResult::Unauthorized { .. } => {
            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(PublishResult::PermissionDenied { name: name.clone() }),
            )
                .into())
        }
    };
    let res = ctx
        .create_dns_record(&auth.claims.identity, &tld.into(), database_identity)
        .await
        .map_err(log_and_500)?;
    match res {
        name::InsertDomainResult::Success { .. } => Ok(()),
        name::InsertDomainResult::TldNotRegistered { .. } | name::InsertDomainResult::PermissionDenied { .. } => {
            Err(log_and_500("impossible: we just registered the tld"))
        }
        name::InsertDomainResult::OtherError(e) => Err(log_and_500(e)),
    }
}

fn schema_migration_policy(
    policy: MigrationPolicy,
    token: Option<Hash>,
) -> axum::response::Result<SchemaMigrationPolicy> {
    const MISSING_TOKEN: &str = "Migration policy is set to `BreakClients`, but no migration token was provided.";

    match policy {
        MigrationPolicy::BreakClients => token
            .map(SchemaMigrationPolicy::BreakClients)
            .ok_or_else(|| bad_request(MISSING_TOKEN.into())),
        MigrationPolicy::Compatible => Ok(SchemaMigrationPolicy::Compatible),
    }
}

fn bad_request(message: Cow<'static, str>) -> ErrorResponse {
    (StatusCode::BAD_REQUEST, message).into()
}
