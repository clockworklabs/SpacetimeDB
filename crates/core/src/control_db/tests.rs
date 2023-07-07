use std::str::FromStr;

use once_cell::sync::Lazy;
use tempdir::TempDir;

use super::*;

static ALICE: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("alice"));
static BOB: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("bob"));

#[test]
fn test_register_tld() -> anyhow::Result<()> {
    let tmp = TempDir::new("register-tld")?;

    let domain: DomainName = "amaze".parse()?;
    let cdb = ControlDb::at(tmp.path())?;

    cdb.spacetime_register_tld(domain.as_tld(), *ALICE)?;
    let owner = cdb.spacetime_lookup_tld(&domain.as_tld())?;
    assert_eq!(owner, Some(*ALICE));

    let unauthorized = cdb.spacetime_register_tld(domain.as_tld(), *BOB)?;
    assert!(matches!(unauthorized, RegisterTldResult::Unauthorized { .. }));
    let already_registered = cdb.spacetime_register_tld(domain.as_tld(), *ALICE)?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let domain = DomainName::from_str("amAZe")?;
    let already_registered = cdb.spacetime_register_tld(domain.as_tld(), *ALICE)?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}

#[test]
fn test_domain() -> anyhow::Result<()> {
    let tmp = TempDir::new("insert-domain")?;
    let domain: DomainName = "this/hASmiXed/case".parse()?;
    let domain_lower: DomainName = domain.to_lowercase().parse()?;

    let cdb = ControlDb::at(tmp.path())?;

    let addr = Address::from_arr(&[0; 16]);
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

    let tld_owner = cdb.spacetime_lookup_tld(&domain.as_tld())?;
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
