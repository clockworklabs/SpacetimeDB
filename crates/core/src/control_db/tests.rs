use std::str::FromStr;

use crate::messages::control_db::HostType;
use once_cell::sync::Lazy;
use spacetimedb_lib::error::ResultTest;
use spacetimedb_lib::Hash;
use tempfile::TempDir;

use super::*;

static ALICE: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("alice"));
static BOB: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("bob"));

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

    let addr = Address::zero();
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

    let registered_addr = cdb.spacetime_dns(&domain)?;
    assert_eq!(registered_addr, Some(addr));

    // Try lowercase, too
    let registered_addr = cdb.spacetime_dns(&domain_lower)?;
    assert_eq!(registered_addr, Some(addr));

    // Reverse should yield the original domain (in mixed-case)
    let reverse_lookup = cdb.spacetime_reverse_dns(&addr)?;
    assert_eq!(
        reverse_lookup.first().map(ToString::to_string),
        Some(domain.to_string())
    );
    assert_eq!(reverse_lookup, vec![domain]);
    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}

#[test]
fn test_decode() -> ResultTest<()> {
    let path = TempDir::with_prefix("decode")?;

    let cdb = ControlDb::at(path)?;

    let id = cdb.alloc_spacetime_identity()?;

    let db = Database {
        id: 0,
        address: Default::default(),
        identity: id,
        host_type: HostType::Wasm,
        num_replicas: 0,
        program_bytes_address: Hash::ZERO,
        publisher_address: Some(Address::zero()),
    };

    cdb.insert_database(db.clone())?;

    let dbs = cdb.get_databases()?;

    assert_eq!(dbs.len(), 1);
    assert_eq!(dbs[0].identity, id);

    let mut new_database_instance = DatabaseInstance {
        id: 0,
        database_id: 1,
        node_id: 0,
        leader: true,
    };

    let id = cdb.insert_database_instance(new_database_instance.clone())?;
    new_database_instance.id = id;

    let dbs = cdb.get_database_instances()?;

    assert_eq!(dbs.len(), 1);
    assert_eq!(dbs[0].id, id);

    Ok(())
}
