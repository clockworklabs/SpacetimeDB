use std::str::FromStr;

use once_cell::sync::Lazy;
use spacetimedb::messages::control_db::HostType;
use spacetimedb_client_api::auth::LOCALHOST;
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
fn test_delete_database_retains_names() -> anyhow::Result<()> {
    let tmp = TempDir::with_prefix("delete-database-names")?;
    let deleted = Identity::from_claims(LOCALHOST, "deleted-database");
    let retained = Identity::from_claims(LOCALHOST, "retained-database");
    let names = ["Deleted-Name".parse::<DomainName>()?, "deleted-name/alias".parse()?];
    {
        let control = ControlDb::at(tmp.path())?;
        let make_database = |database_identity| Database {
            id: 0,
            database_identity,
            owner_identity: *ALICE,
            host_type: HostType::Wasm,
            initial_program: Hash::ZERO,
            bootstrap_generation: 0,
        };
        let id = control.insert_database(make_database(deleted))?;
        control.insert_database(make_database(retained))?;
        for name in &names {
            control.spacetime_insert_domain(&deleted, name.clone(), *ALICE, true)?;
        }
        control.spacetime_insert_domain(&retained, "retained-name".parse()?, *ALICE, true)?;
        control.spacetime_insert_domain(&deleted, "shared-name/deleted".parse()?, *ALICE, true)?;
        control.spacetime_insert_domain(&retained, "shared-name/retained".parse()?, *ALICE, true)?;
        assert_eq!(control.delete_database(id)?, Some(id));
        assert_eq!(control.delete_database(id)?, None);
    }
    let control = ControlDb::at(tmp.path())?;
    for name in names {
        assert_eq!(control.spacetime_dns(name.as_ref())?, Some(deleted));
    }
    assert_eq!(control.spacetime_reverse_dns(&deleted)?.len(), 3);
    assert!(control.get_database_by_identity(&deleted)?.is_none());
    assert_eq!(control.spacetime_dns("retained-name")?, Some(retained));
    assert_eq!(control.spacetime_dns("shared-name/deleted")?, Some(deleted));
    assert_eq!(control.spacetime_dns("shared-name/retained")?, Some(retained));
    assert_eq!(
        control.spacetime_lookup_tld("shared-name".parse::<DomainName>()?.tld())?,
        Some(*ALICE)
    );
    assert!(control.get_database_by_identity(&retained)?.is_some());
    assert_eq!(
        control.spacetime_lookup_tld("deleted-name".parse::<DomainName>()?.tld())?,
        Some(*ALICE)
    );
    Ok(())
}
