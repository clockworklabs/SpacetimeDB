use std::str::FromStr;

use once_cell::sync::Lazy;
use spacetimedb::auth::identity::ContainerClaim;
use spacetimedb::messages::control_db::HostType;
use spacetimedb_client_api::auth::LOCALHOST;
use spacetimedb_lib::container::{ContainerSpec, ContainerState};
use spacetimedb_lib::error::ResultTest;
use spacetimedb_lib::Hash;
use tempfile::TempDir;

use super::*;

static ALICE: Lazy<Identity> = Lazy::new(|| Identity::from_claims(LOCALHOST, "alice"));
static BOB: Lazy<Identity> = Lazy::new(|| Identity::from_claims(LOCALHOST, "bob"));

#[test]
fn test_register_tld() -> anyhow::Result<()> {
    let tmp = TempDir::with_prefix("register-tld")?;

    let domain: DomainName = "amaze".parse()?;
    let cdb = ControlDb::at(tmp.path())?;

    cdb.spacetime_register_tld(domain.to_tld(), *ALICE)?;
    let owner = cdb.spacetime_lookup_tld(domain.tld())?;
    assert_eq!(owner, Some(*ALICE));

    let unauthorized = cdb.spacetime_register_tld(domain.to_tld(), *BOB)?;
    assert!(matches!(unauthorized, RegisterTldResult::Unauthorized { .. }));
    let already_registered = cdb.spacetime_register_tld(domain.to_tld(), *ALICE)?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let domain = DomainName::from_str("amAZe")?;
    let already_registered = cdb.spacetime_register_tld(domain.to_tld(), *ALICE)?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}

#[test]
fn test_domain() -> anyhow::Result<()> {
    let tmp = TempDir::with_prefix("insert-domain")?;
    let domain: DomainName = "this/hASmiXed/case".parse()?;
    let domain_lower: DomainName = domain.to_lowercase().parse()?;

    let cdb = ControlDb::at(tmp.path())?;

    let addr = Identity::ZERO;
    let res = cdb.spacetime_insert_domain(&addr, domain.clone(), *ALICE, true)?;
    assert!(matches!(res, InsertDomainResult::Success { .. }));

    // Check Alice owns TLD
    let unauthorized = cdb
        .spacetime_insert_domain(&addr, "this/is/bob".parse()?, *BOB, true)
        .unwrap();
    assert!(matches!(unauthorized, InsertDomainResult::PermissionDenied { .. }));

    let already_registered = cdb.spacetime_insert_domain(&addr, domain.clone(), *ALICE, true);
    assert!(matches!(already_registered, Err(Error::RecordAlreadyExists(_))));
    // Cannot register lowercase
    let already_registered = cdb.spacetime_insert_domain(&addr, domain_lower.clone(), *ALICE, true);
    assert!(matches!(already_registered, Err(Error::RecordAlreadyExists(_))));

    let tld_owner = cdb.spacetime_lookup_tld(domain.tld())?;
    assert_eq!(tld_owner, Some(*ALICE));

    let registered_addr = cdb.spacetime_dns(domain.as_ref())?;
    assert_eq!(registered_addr, Some(addr));

    // Try lowercase, too
    let registered_addr = cdb.spacetime_dns(domain_lower.as_ref())?;
    assert_eq!(registered_addr, Some(addr));

    // Reverse should yield the original domain (in mixed-case)
    let reverse_lookup = cdb.spacetime_reverse_dns(&addr)?;
    assert_eq!(
        reverse_lookup.first().map(ToString::to_string),
        Some(domain.to_string())
    );
    assert_eq!(reverse_lookup, vec![domain.clone()]);

    // We can remove the domain records for Alice's database
    let deleted = cdb.spacetime_replace_domains(&addr, &ALICE, &[]);
    assert!(matches!(deleted, Ok(SetDomainsResult::Success)));

    // The domain records are gone
    let registered_addr = cdb.spacetime_dns(domain.as_ref())?;
    assert_eq!(registered_addr, None);

    // Reverse DNS should yield empty
    let reverse_lookup = cdb.spacetime_reverse_dns(&addr)?;
    assert_eq!(reverse_lookup, vec![]);

    // Bob cannot register the TLD
    let unauthorized = cdb
        .spacetime_insert_domain(&addr, "this/is/bob".parse()?, *BOB, true)
        .unwrap();
    assert!(matches!(unauthorized, InsertDomainResult::PermissionDenied { .. }));

    // Alice can add the domain back
    let addr = Identity::ZERO;
    let res = cdb.spacetime_insert_domain(&addr, domain.clone(), *ALICE, true)?;
    assert!(matches!(res, InsertDomainResult::Success { .. }));

    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}

#[test]
fn test_decode() -> ResultTest<()> {
    let path = TempDir::with_prefix("decode")?;

    let cdb = ControlDb::at(path)?;

    // TODO: Use a random identity.
    let id = Identity::ZERO;

    let db = Database {
        id: 0,
        database_identity: Default::default(),
        owner_identity: id,
        host_type: HostType::Wasm,
        initial_program: Hash::ZERO,
        bootstrap_generation: 0,
    };

    cdb.insert_database(db.clone())?;

    let dbs = cdb.get_databases()?;

    assert_eq!(dbs.len(), 1);
    assert_eq!(dbs[0].owner_identity, id);

    let new_replica = Replica {
        id: 0,
        database_id: 1,
        node_id: 0,
        leader: true,
    };

    let id = cdb.insert_replica(new_replica)?;

    let dbs = cdb.get_replicas()?;

    assert_eq!(dbs.len(), 1);
    assert_eq!(dbs[0].id, id);

    Ok(())
}

#[test]
fn test_container_generations() -> anyhow::Result<()> {
    let tmp = TempDir::with_prefix("container")?;
    let cdb = ControlDb::at(tmp.path())?;
    let database = Database {
        id: 0,
        database_identity: *ALICE,
        owner_identity: *BOB,
        host_type: HostType::Wasm,
        initial_program: Hash::ZERO,
        bootstrap_generation: 0,
    };
    let id = cdb.insert_database(database.clone())?;
    let spec = |image: &str| {
        Some(ContainerSpec {
            image: format!("{image}@sha256:{}", "a".repeat(64)),
            command: None,
            env_keys: vec![],
            resources: Default::default(),
            ports: vec![],
            restart: Default::default(),
        })
    };
    let state = || cdb.get_container(id).unwrap().map(|c| (c.generation, c.running));
    let current = |generation| {
        cdb.is_current_container(&ContainerClaim {
            database: *ALICE,
            generation,
        })
        .unwrap()
    };

    // Creating a container starts it.
    cdb.set_container(id, &ALICE, spec("a"))?;
    assert_eq!(state(), Some((1, true)));
    assert!(current(1) && !current(0) && !current(2));

    // Stopping keeps the generation, but no instance is current.
    assert!(cdb.set_container_running(id, false)?);
    assert_eq!(state(), Some((1, false)));
    assert!(!current(1));

    // Replacing a stopped container's spec doesn't start it.
    cdb.set_container(id, &ALICE, spec("b"))?;
    assert_eq!(state(), Some((1, false)));

    // Starting takes a new generation, even when already running.
    assert!(cdb.set_container_running(id, true)?);
    assert!(cdb.set_container_running(id, true)?);
    assert_eq!(state(), Some((3, true)));
    assert!(current(3) && !current(2));

    // So do replacing a running container's spec and resetting its database,
    // which stops the container and starts it again once the new replica runs.
    cdb.set_container(id, &ALICE, spec("c"))?;
    cdb.set_container_running(id, false)?;
    cdb.set_container_running(id, true)?;
    assert_eq!(state(), Some((5, true)));
    assert!(current(5) && !current(4));

    // A removed container keeps its generation for the next one.
    cdb.set_container(id, &ALICE, None)?;
    assert_eq!(state(), None);
    assert!(!current(5));
    assert!(!cdb.set_container_running(id, true)?);
    cdb.set_container(id, &ALICE, spec("a"))?;
    assert_eq!(state(), Some((6, true)));
    assert!(current(6));

    // Deleting the database deletes its container, but keeps the generation
    // for a database republished with the same identity.
    cdb.delete_database(id)?;
    assert!(!current(6));
    for tree in ["container", "container_status"] {
        assert!(cdb.db.open_tree(tree)?.is_empty());
    }
    let id = cdb.insert_database(database)?;
    cdb.set_container(id, &ALICE, spec("a"))?;
    assert_eq!(cdb.get_container(id)?.map(|c| c.generation), Some(7));
    assert!(current(7) && !current(6));
    Ok(())
}

#[test]
fn test_container_status_reports() -> anyhow::Result<()> {
    let tmp = TempDir::with_prefix("container")?;
    let cdb = ControlDb::at(tmp.path())?;
    let id = cdb.insert_database(Database {
        id: 0,
        database_identity: *ALICE,
        owner_identity: *BOB,
        host_type: HostType::Wasm,
        initial_program: Hash::ZERO,
        bootstrap_generation: 0,
    })?;
    let spec = Some(ContainerSpec {
        image: format!("a@sha256:{}", "a".repeat(64)),
        command: None,
        env_keys: vec![],
        resources: Default::default(),
        ports: vec![],
        restart: Default::default(),
    });
    let state = || cdb.get_container(id).unwrap().and_then(|c| c.state);
    let running = || -> Vec<_> {
        let running = cdb.running_containers().unwrap();
        running.into_iter().map(|(db, c)| (db.id, c.generation)).collect()
    };

    // A container whose database is gone is not listed.
    cdb.set_container(id, &ALICE, spec.clone())?;
    cdb.set_container(id + 1, &ALICE, spec.clone())?;
    assert_eq!(running(), [(id, 1)]);

    // Only reports for the current generation are recorded.
    cdb.set_container_status(id, 1, ContainerState::Running)?;
    cdb.set_container_status(id, 0, ContainerState::Exited(1))?;
    assert_eq!(state(), Some(ContainerState::Running));

    // A stopped container is not listed, and reports for it are dropped.
    cdb.set_container_running(id, false)?;
    assert_eq!(running(), []);
    cdb.set_container_status(id, 1, ContainerState::Exited(0))?;
    assert_eq!(state(), None);
    Ok(())
}
