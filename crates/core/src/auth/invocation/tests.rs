use super::*;
use crate::auth::hosted_tokens::{sign_hosted_token, HostedTokenBinding, HostedTokenValidator};
use crate::auth::JwtKeys;
use crate::db::deployment::install_container_fence;
use crate::db::relational_db::tests_utils::TestDB;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::StContainerFenceRow;
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
use spacetimedb_lib::identity::AuthCtx;
use std::time::Duration;

fn module(hosted_auth: bool) -> ModuleDef {
    let mut builder = RawModuleDefV10Builder::new();
    if hosted_auth {
        builder.add_capability("hosted_auth_v1");
    }
    builder.finish().try_into().unwrap()
}

/// Obtain every proof through the production signer and target-bound verifier.
fn authenticate(source: Identity, target: Identity, now: SystemTime) -> ConnectionAuthCtx {
    let keys = JwtKeys::generate().unwrap();
    let binding = HostedTokenBinding {
        source_database: source,
        target_database: target,
        generation: 3,
        grant_revision: 7,
        lease_expires_at: now + Duration::from_secs(30),
    };
    let token = sign_hosted_token(
        &keys.private,
        "test.platform",
        &binding,
        now,
        now + Duration::from_secs(20),
        "invocation-test",
    )
    .unwrap();
    HostedTokenValidator::new([("test.platform".into(), keys.public)])
        .unwrap()
        .validate_token(&token, target, now, |issuer, requested_source, requested_target| {
            (issuer == "test.platform" && requested_source == source && requested_target == target).then_some(binding)
        })
        .unwrap()
        .into_connection_auth()
        .unwrap()
}

fn fence(source: Identity, generation: u64, grant_revision: u64, allowed: bool) -> StContainerFenceRow {
    StContainerFenceRow {
        source_identity: source.into(),
        generation,
        target_grant_revision: grant_revision,
        target_set_hash: spacetimedb_lib::hash_bytes(b"configured targets"),
        allowed,
    }
}

#[test]
fn internal_requires_verified_self_call_and_updated_bindings() {
    let target = Identity::ONE;
    let foreign = Identity::from_u256(2u64.into());
    let updated = module(true);
    let old_bindings = module(false);
    // This also covers an ordinary connection whose sender equals the database.
    assert_eq!(InvocationCaller::from(target).flags_for(target, &updated).unwrap(), 0);
    for (source, expected_flags) in [(target, 1), (foreign, 0)] {
        let auth = authenticate(source, target, SystemTime::now());
        let caller = InvocationCaller::from(&auth);
        assert_eq!(caller.flags_for(target, &updated).unwrap(), expected_flags);
        assert!(caller.flags_for(target, &old_bindings).is_err());
        assert!(caller.flags_for(Identity::ZERO, &updated).is_err());
    }
}

#[test]
fn authenticated_proof_cannot_be_paired_with_another_sender_or_sql_caller() {
    let source = Identity::ONE;
    let target = Identity::from_u256(2u64.into());
    let owner = Identity::from_u256(3u64.into());
    let mut auth = authenticate(source, target, SystemTime::now());
    assert!(SqlCallAuth::authenticated(AuthCtx::for_current(owner), &auth).is_err());
    // ConnectionAuthCtx has public fields for trusted host code. Defend the
    // boundary against accidentally mixing separately authenticated contexts.
    auth.claims.identity = owner;
    assert!(InvocationCaller::from(&auth).flags_for(target, &module(true)).is_err());
    assert!(SqlCallAuth::authenticated(AuthCtx::for_current(owner), &auth).is_err());
}

#[test]
fn internal_authentication_does_not_grant_owner_sql_permissions() {
    let source = Identity::ONE;
    let owner = Identity::from_u256(3u64.into());
    let auth = authenticate(source, source, SystemTime::now());
    assert_eq!(
        InvocationCaller::from(&auth).flags_for(source, &module(true)).unwrap(),
        1
    );
    let sql = SqlCallAuth::authenticated(AuthCtx::new(owner, source), &auth).unwrap();
    assert_eq!(sql.caller(), source);
    assert!(sql.has_read_access(StAccess::Public));
    assert!(!sql.has_read_access(StAccess::Private));
    assert!(!sql.has_write_access());
    assert!(!sql.bypass_rls());
}

#[test]
fn transaction_admission_rechecks_persisted_fences_after_initial_authentication() {
    let db = TestDB::in_memory().unwrap();
    let target = db.database_identity();
    let source = Identity::ONE;
    let auth = authenticate(source, target, SystemTime::now());
    let proof = auth.hosted.as_ref();
    let caller = InvocationCaller::from(&auth);
    assert_eq!(caller.flags_for(target, &module(true)).unwrap(), 0);
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        assert!(check_hosted_admission(tx, target, proof).is_err());
        install_container_fence(&db, tx, &fence(source, 3, 7, true))?;
        check_hosted_admission(tx, target, proof)?;
        assert!(check_hosted_admission(tx, Identity::from_u256(99u64.into()), proof).is_err());
        Ok(())
    })
    .unwrap();
    // Revocation commits after token verification and before the queued call.
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        install_container_fence(&db, tx, &fence(source, 4, 8, false))?;
        Ok(())
    })
    .unwrap();
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        assert!(check_hosted_admission(tx, target, proof).is_err());
        check_hosted_admission(tx, target, None)?;
        // Another generation does not reactivate a copied credential.
        install_container_fence(&db, tx, &fence(source, 5, 9, true))?;
        assert!(check_hosted_admission(tx, target, proof).is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn expired_verified_proof_is_rejected_at_both_call_and_transaction_admission() {
    let db = TestDB::in_memory().unwrap();
    let target = db.database_identity();
    let source = Identity::ONE;
    // Valid when received, expired before execution, without sleeps or forged proofs.
    let auth = authenticate(source, target, SystemTime::now() - Duration::from_secs(60));
    assert!(InvocationCaller::from(&auth).flags_for(target, &module(true)).is_err());
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        install_container_fence(&db, tx, &fence(source, 3, 7, true))?;
        assert!(check_hosted_admission(tx, target, auth.hosted.as_ref()).is_err());
        Ok(())
    })
    .unwrap();
}
