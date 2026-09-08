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
    let wrong_target = authenticate(source, Identity::from_u256(99u64.into()), SystemTime::now());
    db.hosted_admission().begin().unwrap().complete().unwrap();
    let caller = InvocationCaller::from(&auth);
    assert_eq!(caller.flags_for(target, &module(true)).unwrap(), 0);
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        assert!(check_hosted_admission(tx, &db, proof).is_err());
        install_container_fence(&db, tx, &fence(source, 3, 7, true))?;
        assert!(!db.hosted_admission().is_open());
        db.hosted_admission().begin()?.complete()?;
        check_hosted_admission(tx, &db, proof)?;
        assert!(check_hosted_admission(tx, &db, wrong_target.hosted.as_ref()).is_err());
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
        assert!(check_hosted_admission(tx, &db, proof).is_err());
        check_hosted_admission(tx, &db, None)?;
        // Another generation does not reactivate a copied credential.
        install_container_fence(&db, tx, &fence(source, 5, 9, true))?;
        assert!(check_hosted_admission(tx, &db, proof).is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn reopened_database_requires_a_fresh_sweep_despite_a_replayed_allowed_fence() {
    let db = TestDB::durable().unwrap();
    let source = Identity::ONE;
    let target = db.database_identity();
    let auth = authenticate(source, target, SystemTime::now());
    db.with_auto_commit(Workload::ForTests, |tx| {
        install_container_fence(&db, tx, &fence(source, 3, 7, true))
    })
    .unwrap();
    let old_sweep = db.hosted_admission().begin().unwrap();
    let db = db.reopen().unwrap();
    // Shutdown seals the old object, including any late coordinator ticket.
    assert!(old_sweep.complete().is_err());
    assert!(!db.hosted_admission().is_open());
    db.with_read_only(Workload::ForTests, |tx| {
        crate::db::deployment::check_container_fence(tx, source, 3, 7).unwrap();
        let error = check_hosted_admission(tx, &db, auth.hosted.as_ref()).unwrap_err();
        assert!(error.to_string().contains("has not reconciled"));
        check_hosted_admission(tx, &db, None).unwrap();
    });
    db.hosted_admission().begin().unwrap().complete().unwrap();
    db.with_read_only(Workload::ForTests, |tx| {
        check_hosted_admission(tx, &db, auth.hosted.as_ref()).unwrap();
    });
}

#[test]
fn expired_verified_proof_is_rejected_at_both_call_and_transaction_admission() {
    let db = TestDB::in_memory().unwrap();
    let target = db.database_identity();
    let source = Identity::ONE;
    db.hosted_admission().begin().unwrap().complete().unwrap();
    // Valid when received, expired before execution, without sleeps or forged proofs.
    let auth = authenticate(source, target, SystemTime::now() - Duration::from_secs(60));
    assert!(InvocationCaller::from(&auth).flags_for(target, &module(true)).is_err());
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
        install_container_fence(&db, tx, &fence(source, 3, 7, true))?;
        assert!(check_hosted_admission(tx, &db, auth.hosted.as_ref()).is_err());
        Ok(())
    })
    .unwrap();
}

#[tokio::test]
async fn shutdown_seals_hosted_admission_even_for_retained_memory_database_handles() {
    let db = TestDB::in_memory().unwrap();
    let pending = db.hosted_admission().begin().unwrap();
    assert_eq!(db.shutdown().await, None);
    assert!(pending.complete().is_err());
    assert!(!db.hosted_admission().is_open());
    assert!(db.hosted_admission().begin().is_err());
    assert_eq!(db.shutdown().await, None);
}
