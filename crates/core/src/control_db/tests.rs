use std::str::FromStr;

use once_cell::sync::Lazy;
use tempdir::TempDir;

use super::*;

static ALICE: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("alice"));
static BOB: Lazy<Identity> = Lazy::new(|| Identity::from_hashing_bytes("bob"));

#[tokio::test]
async fn test_register_tld() -> anyhow::Result<()> {
    let tmp = TempDir::new("register-tld")?;

    let domain: DomainName = "amaze".parse()?;
    let cdb = tokio::task::spawn_blocking({
        let path = tmp.path().to_path_buf();
        move || ControlDb::at(path)
    })
    .await??;

    cdb.spacetime_register_tld(domain.tld().clone(), *ALICE).await?;
    let owner = cdb.spacetime_lookup_tld(domain.tld()).await?;
    assert_eq!(owner, Some(*ALICE));

    let unauthorized = cdb.spacetime_register_tld(domain.tld().clone(), *BOB).await?;
    assert!(matches!(unauthorized, RegisterTldResult::Unauthorized { .. }));
    let already_registered = cdb.spacetime_register_tld(domain.tld().clone(), *ALICE).await?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let (mixed, _) = DomainName::from_str("amAZe")?.into_parts();
    let already_registered = cdb.spacetime_register_tld(mixed, *ALICE).await?;
    assert!(matches!(
        already_registered,
        RegisterTldResult::AlreadyRegistered { .. }
    ));
    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}

#[tokio::test]
async fn test_domain() -> anyhow::Result<()> {
    let tmp = TempDir::new("insert-domain")?;
    let domain: DomainName = "this/hASmiXed/case".parse()?;
    let domain_lower: DomainName = domain.to_lowercase().parse()?;

    let cdb = tokio::task::spawn_blocking({
        let path = tmp.path().to_path_buf();
        move || ControlDb::at(path)
    })
    .await??;

    let addr = Address::from_arr(&[0; 16]);
    let res = cdb.spacetime_insert_domain(&addr, domain.clone(), *ALICE, true).await?;
    assert!(matches!(res, InsertDomainResult::Success { .. }));

    // Check Alice owns TLD
    let unauthorized = cdb
        .spacetime_insert_domain(&addr, domain.tld().clone().into(), *BOB, true)
        .await?;
    assert!(matches!(unauthorized, InsertDomainResult::PermissionDenied { .. }));

    let already_registered = cdb.spacetime_insert_domain(&addr, domain.clone(), *ALICE, true).await;
    assert!(matches!(already_registered, Err(Error::RecordAlreadyExists(_))));
    // Cannot register lowercase
    let already_registered = cdb
        .spacetime_insert_domain(&addr, domain_lower.clone(), *ALICE, true)
        .await;
    assert!(matches!(already_registered, Err(Error::RecordAlreadyExists(_))));

    let tld_owner = cdb.spacetime_lookup_tld(domain.tld()).await?;
    assert_eq!(tld_owner, Some(*ALICE));

    let registered_addr = cdb.spacetime_dns(&domain).await?;
    assert_eq!(registered_addr, Some(addr));

    // Try lowercase, too
    let registered_addr = cdb.spacetime_dns(&domain_lower).await?;
    assert_eq!(registered_addr, Some(addr));

    // Reverse should yield the original domain (in mixed-case)
    let reverse_lookup = cdb.spacetime_reverse_dns(&addr).await?;
    assert_eq!(
        reverse_lookup.first().map(ToString::to_string),
        Some(domain.to_string())
    );
    assert_eq!(reverse_lookup, vec![domain]);
    let _ = tmp.close().ok(); // force tmp to not be dropped until here

    Ok(())
}
