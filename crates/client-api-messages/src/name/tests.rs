#![allow(clippy::disallowed_macros)]

use std::iter;

use itertools::Itertools as _;
use proptest::prelude::*;

use spacetimedb_sats::bsatn;

use super::*;

fn gen_valid_domain_name() -> impl Strategy<Value = String> {
    "[\\S&&[^/]]{1,64}(/[\\S&&[^/]]{1,64}){0,255}"
}

proptest! {
    #[test]
    fn prop_domain_name_parses(s in gen_valid_domain_name()) {
        parse_domain_name(s)?;
    }

    #[test]
    fn prop_domain_name_displays_input(s in gen_valid_domain_name()) {
        let domain = DomainName::from_str(&s)?;
        prop_assert_eq!(s, domain.to_string())
    }

    #[test]
    fn prop_domain_name_into_parse(
        tld in "[\\S&&[^/]]{1,64}",
        sub in prop::option::of(gen_valid_domain_name())
    ) {
        let domain = parse_domain_name(iter::once(tld.as_str()).chain(sub.as_deref()).join("/"))?;
        prop_assert_eq!(&tld, domain.tld().as_str());
        prop_assert_eq!(sub.as_deref(), domain.sub_domain());
        let domain_tld = Tld::from(domain);
        prop_assert_eq!(&tld, domain_tld.as_str());
    }

    #[test]
    fn prop_domain_name_serde(s in gen_valid_domain_name()) {
        let a = parse_domain_name(s)?;
        let js = serde_json::to_string(&a)?;
        eprintln!("json: `{js}`");
        let b: DomainName = serde_json::from_str(&js)?;
        prop_assert_eq!(a, b)
    }

    #[test]
    fn prop_domain_name_sats(s in gen_valid_domain_name()) {
        let a = parse_domain_name(s)?;
        let bsatn = bsatn::to_vec(&a)?;
        let b: DomainName = bsatn::from_slice(&bsatn)?;
        prop_assert_eq!(a, b)
    }

    #[test]
    fn prop_domain_name_inequality(a in gen_valid_domain_name(), b in gen_valid_domain_name()) {
        prop_assume!(a != b);
        let a = parse_domain_name(a)?;
        let b = parse_domain_name(b)?;
        prop_assert_ne!(a, b);
    }

    #[test]
    fn prop_domain_name_must_not_start_with_slash(s in "/\\S{1,100}") {
        assert!(matches!(
           parse_domain_name(s),
           Err(DomainParsingError(ParseError::StartsSlash { .. })),
        ))
    }

    #[test]
    fn prop_domain_name_must_not_end_with_slash(s in "[\\S&&[^/]]{1,64}/") {
        assert!(matches!(
            parse_domain_name(s),
            Err(DomainParsingError(ParseError::EndsSlash { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_must_not_contain_slashslash(s in "[\\S&&[^/]]{1,25}//[\\S&&[^/]]{1,25}") {
        assert!(matches!(
            parse_domain_name(s),
            Err(DomainParsingError(ParseError::SlashSlash { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_must_not_contain_whitespace(s in "[\\S&&[^/]]{0,10}\\s{1,10}[\\S&&[^/]]{0,10}") {
        assert!(matches!(
            parse_domain_name(s),
            Err(DomainParsingError(ParseError::Whitespace { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_parts_must_not_exceed_max_chars(s in "[\\S&&[^/]]{65}(/[\\S&&[^/]]{65})*") {
        assert!(matches!(
            parse_domain_name(s),
            Err(DomainParsingError(ParseError::TooLong { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_cannot_have_unlimited_subdomains(s in "[\\S&&[^/]]{1,64}(/[\\S&&[^/]]{1,64}){257}") {
        assert!(matches!(
            parse_domain_name(s),
            Err(DomainParsingError(ParseError::TooManySubdomains { .. }))
        ))
    }

    #[test]
    fn prop_tld_cannot_be_identity(addr_bytes in any::<[u8; 32]>()) {
        let addr = hex::encode(addr_bytes);
        assert!(matches!(
            parse_domain_name(addr),
            Err(DomainParsingError(ParseError::Identity { .. }))
        ))
    }

    #[test]
    fn prop_but_tld_can_be_some_other_hex_value(bytes in any::<[u8; 16]>()) {
        let addr = hex::encode(bytes);
        parse_domain_name(addr)?;
    }
}

#[test]
fn test_domain_segment_cannot_be_empty() {
    assert!(matches!(DomainSegment::try_from(""), Err(ParseError::Empty)))
}

#[test]
fn test_domain_name_cannot_be_empty() {
    assert!(matches!(
        parse_domain_name(""),
        Err(DomainParsingError(ParseError::Empty))
    ))
}

#[test]
fn test_tld_is_domain_name() {
    let dom = parse_domain_name("spacetimedb/drop").unwrap();
    let tld = Tld::from(dom);
    let dom = DomainName::from(tld);

    assert_eq!("spacetimedb", dom.tld().as_str());
    assert_eq!(None, dom.sub_domain());
}

#[cfg(feature = "serde")]
mod serde {
    use super::*;

    use crate::name::serde_impls::DomainNameV1;

    #[test]
    fn test_deserialize_domain_name_v1() {
        let js = serde_json::to_string(&DomainNameV1 {
            tld: "clockworklabs",
            sub_domain: "bitcraft-mini",
        })
        .unwrap();
        let de: DomainName = serde_json::from_str(&js).unwrap();

        assert_eq!("clockworklabs/bitcraft-mini", de.as_str());
        assert_eq!("clockworklabs", de.tld().as_str());
        assert_eq!(Some("bitcraft-mini"), de.sub_domain());
    }

    #[test]
    fn test_deserialize_domain_name_v1_validates() {
        let invalid = serde_json::to_string(&DomainNameV1 {
            tld: "eve",
            sub_domain: "bit//craft",
        })
        .unwrap();
        let de: Result<DomainName, serde_json::Error> = serde_json::from_str(&invalid);

        assert!(matches!(de, Err(e) if e.classify() == serde_json::error::Category::Data));
    }

    #[test]
    fn test_deserialize_domain_name_v2() {
        let dn = parse_domain_name("clockworklabs/bitcraft-mini").unwrap();
        let js = serde_json::to_string(&dn).unwrap();
        let de = serde_json::from_str(&js).unwrap();
        assert_eq!(dn, de);
    }
}
