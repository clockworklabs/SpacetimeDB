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
        let domain = parse_domain_name(s);
        prop_assert!(matches!(domain, Ok(_)), "expected ok, got err: {domain:?}")
    }

    #[test]
    fn prop_domain_name_displays_input(s in gen_valid_domain_name()) {
        let domain = DomainName::from_str(&s).unwrap();
        prop_assert_eq!(s, domain.to_string())
    }

    #[test]
    fn prop_domain_name_into_parse(
        tld in "[\\S&&[^/]]{1,64}",
        sub in prop::option::of(gen_valid_domain_name())
    ) {
        let domain = parse_domain_name(iter::once(tld.as_str()).chain(sub.as_deref()).join("/")).unwrap();
        prop_assert_eq!(&tld, domain.tld().as_str());
        prop_assert_eq!(sub.as_deref(), domain.sub_domain());
        let domain_tld = Tld::from(domain);
        prop_assert_eq!(&tld, domain_tld.as_str());
    }

    #[test]
    fn prop_domain_name_serde(s in gen_valid_domain_name()) {
        let a = parse_domain_name(s).unwrap();
        let js = serde_json::to_string(&a).unwrap();
        eprintln!("json: `{js}`");
        let b: DomainName = serde_json::from_str(&js).unwrap();
        prop_assert_eq!(a, b)
    }

    #[test]
    fn prop_domain_name_sats(s in gen_valid_domain_name()) {
        let a = parse_domain_name(s).unwrap();
        let bsatn = bsatn::to_vec(&a).unwrap();
        let b: DomainName = bsatn::from_slice(&bsatn).unwrap();
        prop_assert_eq!(a, b)
    }

    #[test]
    fn prop_domain_name_inequality(a in gen_valid_domain_name(), b in gen_valid_domain_name()) {
        prop_assume!(a != b);
        let a = parse_domain_name(a).unwrap();
        let b = parse_domain_name(b).unwrap();
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
    fn prop_tld_cannot_be_address(addr_bytes in any::<[u8; 16]>()) {
        let addr = hex::encode(addr_bytes);
        assert!(matches!(
            parse_domain_name(addr),
            Err(DomainParsingError(ParseError::Address { .. }))
        ))
    }

    #[test]
    fn prop_but_tld_can_be_some_other_hex_value(bytes in any::<[u8; 32]>()) {
        let addr = hex::encode(bytes);
        prop_assert!(matches!(parse_domain_name(addr), Ok(_)))
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
