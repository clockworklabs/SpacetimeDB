use proptest::prelude::*;

use super::*;

proptest! {
    #[test]
    fn prop_domain_part_displays_input(s in "\\S{1,64}") {
        let dp = DomainPart::try_from(s).unwrap();
        prop_assert_eq!(dp.as_str(), &dp.to_string());
    }

    #[test]
    fn prop_domain_part_serdes_input(s in "\\S{1,64}") {
        let dp = DomainPart::try_from(s).unwrap();
        let js = serde_json::to_string(&dp).unwrap();
        let de: DomainPart = serde_json::from_str(&js).unwrap();
        prop_assert_eq!(dp.as_str(), de.as_str());
    }

    #[test]
    fn prop_domain_part_compares_lowercase(s in "\\S{1,64}") {
        let a = DomainPart::try_from(s.to_lowercase()).unwrap();
        let b = DomainPart::try_from(s).unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn prop_domain_part_inequality(a in "\\S{1,64}", b in "\\S{1,64}") {
        prop_assume!(a != b);
        let a = DomainPart::try_from(a).unwrap();
        let b = DomainPart::try_from(b).unwrap();
        prop_assert_ne!(a, b);
    }

    #[test]
    fn prop_domain_name_parser_is_equivalent_to_this_horrifying_regex(
        s in "[\\S&&[^/]]{1,64}(/[\\S&&[^/]]{1,64}){0,255}"
    ) {
        prop_assert!(matches!(parse_domain_name(&s), Ok(_)))
    }

    #[test]
    fn prop_domain_name_must_not_start_with_slash(s in "/\\S{1,100}") {
        assert!(matches!(
           parse_domain_name(&s),
           Err(DomainParsingError(ParseError::StartsSlash { .. })),
        ))
    }

    #[test]
    fn prop_domain_name_must_not_end_with_slash(s in "[\\S&&[^/]]{1,64}/") {
        assert!(matches!(
            parse_domain_name(&s),
            Err(DomainParsingError(ParseError::EndsSlash { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_must_not_contain_slashslash(s in "[\\S&&[^/]]{1,25}//[\\S&&[^/]]{1,25}") {
        assert!(matches!(
            parse_domain_name(&s),
            Err(DomainParsingError(ParseError::SlashSlash { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_must_not_contain_whitespace(s in "[\\S&&[^/]]{0,25}\\s{1,25}[\\S&&[^/]]{0,25}") {
        assert!(matches!(
            parse_domain_name(&s),
            Err(DomainParsingError(ParseError::Whitespace { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_parts_must_not_exceed_max_chars(s in "[\\S&&[^/]]{65}(/[\\S&&[^/]]{65})*") {
        assert!(matches!(
            parse_domain_name(&s),
            Err(DomainParsingError(ParseError::TooLong { .. }))
        ))
    }

    #[test]
    fn prop_domain_name_cannot_have_unlimited_subdomains(s in "[\\S&&[^/]]{1,64}(/[\\S&&[^/]]{1,64}){257}") {
        assert!(matches!(
            parse_domain_name(&s),
            Err(DomainParsingError(ParseError::TooManySubdomains { .. }))
        ))
    }

    #[test]
    fn prop_tld_cannot_be_address(addr_bytes in any::<[u8; 16]>()) {
        let addr = hex::encode(addr_bytes);
        assert!(matches!(
            parse_domain_name(&addr),
            Err(DomainParsingError(ParseError::Address { .. }))
        ))
    }

    #[test]
    fn prop_but_tld_can_be_some_other_hex_value(bytes in any::<[u8; 32]>()) {
        let addr = hex::encode(bytes);
        prop_assert!(matches!(parse_domain_name(&addr), Ok(_)))
    }
}

#[test]
fn test_domain_part_cannot_be_empty() {
    assert!(matches!(
        DomainPart::try_from(String::new()),
        Err(DomainParsingError(ParseError::Empty))
    ))
}

#[test]
fn test_domain_name_cannot_be_empty() {
    assert!(matches!(
        parse_domain_name(""),
        Err(DomainParsingError(ParseError::Empty))
    ))
}
