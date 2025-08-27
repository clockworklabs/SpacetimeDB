use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st};
use std::{borrow::Borrow, fmt, ops::Deref, str::FromStr};

use spacetimedb_lib::Identity;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum InsertDomainResult {
    Success {
        domain: DomainName,
        database_identity: Identity,
    },

    /// The top level domain for the database name is not registered. For example:
    ///
    ///  - `clockworklabs/bitcraft`
    ///
    /// if `clockworklabs` is not registered, this error is returned.
    TldNotRegistered { domain: DomainName },

    /// The top level domain for the database name is registered, but the identity that you provided does
    /// not have permission to insert the given database name. For example:
    ///
    /// - `clockworklabs/bitcraft`
    ///
    /// If you were trying to insert this database name, but the tld `clockworklabs` is
    /// owned by an identity other than the identity that you provided, then you will receive
    /// this error.
    PermissionDenied { domain: DomainName },

    /// Some unspecified error occurred.
    OtherError(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SetDomainsResult {
    Success,

    /// The top level domain for the database name is registered, but the identity that you provided does
    /// not have permission to insert the given database name. For example:
    ///
    /// - `clockworklabs/bitcraft`
    ///
    /// If you were trying to insert this database name, but the tld `clockworklabs` is
    /// owned by an identity other than the identity that you provided, then you will receive
    /// this error.
    ///
    /// In order to set the domains for a database, you must also be the owner of that database.
    PermissionDenied {
        domain: DomainName,
    },

    /// Workaround for cloud, which can't extract the exact failing domain from
    /// reducer errors.
    PermissionDeniedOnAny {
        domains: Box<[DomainName]>,
    },

    /// The database name or identity you provided does not exist.
    DatabaseNotFound,

    /// The caller doesn't own the database.
    NotYourDatabase {
        database: Identity,
    },

    /// Some unspecified error occurred.
    OtherError(String),
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublishOp {
    Created,
    Updated,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PublishResult {
    Success {
        /// `Some` if publish was given a domain name to operate on, `None`
        /// otherwise.
        ///
        /// In other words, this echoes back a domain name if one was given. If
        /// the database name given was in fact a database identity, this will be
        /// `None`.
        domain: Option<DatabaseName>,
        /// The identity of the published database.
        ///
        /// Always set, regardless of whether publish resolved a domain name first
        /// or not.
        database_identity: Identity,
        op: PublishOp,
    },

    /// The top level domain for the database name is registered, but the identity that you provided does
    /// not have permission to insert the given database name. For example:
    ///
    /// - `clockworklabs/bitcraft`
    ///
    /// If you were trying to insert this database name, but the tld `clockworklabs` is
    /// owned by an identity other than the identity that you provided, then you will receive
    /// this error.
    PermissionDenied { name: DatabaseName },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DnsLookupResponse {
    /// The lookup was successful and the domain and identity are returned.
    Success { domain: DomainName, identity: Identity },

    /// There was no domain registered with the given domain name
    Failure { domain: DomainName },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RegisterTldResult {
    Success {
        domain: Tld,
    },
    /// The domain is already registered to the calling identity
    AlreadyRegistered {
        domain: Tld,
    },
    /// The domain is already registered to another identity
    Unauthorized {
        domain: Tld,
    },
    // TODO(jdetter): Insufficient funds error here
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SetDefaultDomainResult {
    Success {
        domain: DomainName,
    },
    /// The identity doesn't own the domain they tried to set as their default.
    Unauthorized {
        domain: DomainName,
    },
    /// No identity owns this domain so it cannot be set as the default domain for an identity.
    NotRegistered {
        domain: DomainName,
    },
}

/// A simplified version of [`DomainName`] that allows a limited set of characters.
///
/// Must match the regex `^[a-z0-9]+(-[a-z0-9]+)*$`
#[derive(Clone, Debug, serde_with::DeserializeFromStr, serde_with::SerializeDisplay)]
pub struct DatabaseName(pub String);

impl AsRef<str> for DatabaseName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<DatabaseName> for String {
    fn from(name: DatabaseName) -> Self {
        name.0
    }
}

impl fmt::Display for DatabaseName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(thiserror::Error, Clone, Copy, Debug)]
pub enum DatabaseNameError {
    #[error("database names cannot be identities")]
    Identity,
    #[error("database names cannot be empty")]
    Empty,
    #[error("invalid hyphen in database name")]
    Hyphen,
    #[error("invalid characters in database name")]
    Invalid,
}

pub fn parse_database_name(s: &str) -> Result<&str, DatabaseNameError> {
    use DatabaseNameError::*;

    if is_identity(s) {
        return Err(Identity);
    }

    let mut chrs = s.chars();
    let mut next = || chrs.next();

    let is_az09 = |c: char| matches!(c, 'a'..='z' | '0'..='9');

    let c = next().ok_or(Empty)?;
    if c == '-' {
        return Err(Hyphen);
    } else if !is_az09(c) {
        return Err(Invalid);
    }

    while let Some(c) = next() {
        if c == '-' {
            // can't have a hyphen at the end
            let c = next().ok_or(Hyphen)?;
            // can't have 2 hyphens in a row
            if !is_az09(c) {
                return Err(Hyphen);
            }
        } else if !is_az09(c) {
            return Err(Invalid);
        }
    }

    Ok(s)
}

impl TryFrom<String> for DatabaseName {
    type Error = DatabaseNameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        parse_database_name(&s)?;
        Ok(Self(s))
    }
}

impl FromStr for DatabaseName {
    type Err = DatabaseNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse_database_name(s)?.to_owned()))
    }
}

impl From<DatabaseName> for Tld {
    fn from(name: DatabaseName) -> Self {
        Tld(name.0)
    }
}

impl From<DatabaseName> for DomainName {
    fn from(name: DatabaseName) -> Self {
        Tld::from(name).into()
    }
}

/// The top level domain part of a [`DomainName`].
///
/// This newtype witnesses that the TLD is well-formed as per the parsing rules
/// of a full [`DomainName`]. A [`Tld`] is also a valid [`DomainName`], and can
/// be converted to this type.
///
/// Note that the SpacetimeDB DNS registry may apply additional restrictions on
/// what TLDs can be registered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Tld(String);

impl Tld {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_lowercase(&self) -> String {
        self.as_str().to_lowercase()
    }
}

impl AsRef<str> for Tld {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<TldRef> for Tld {
    fn as_ref(&self) -> &TldRef {
        TldRef::new(&self.0)
    }
}

impl fmt::Display for Tld {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<DomainName> for Tld {
    fn from(value: DomainName) -> Self {
        let mut name = value.domain_name;
        name.truncate(value.tld_offset);
        Self(name)
    }
}

impl_st!([] Tld, spacetimedb_lib::AlgebraicType::String);
impl_serialize!([] Tld, (self, ser) => spacetimedb_sats::ser::Serialize::serialize(&self.0, ser));
impl_deserialize!([] Tld, de => {
    let s: String = spacetimedb_sats::de::Deserialize::deserialize(de)?;
    ensure_domain_tld(&s).map_err(spacetimedb_sats::de::Error::custom)?;
    Ok(Self(s))
});

impl<'de> serde::Deserialize<'de> for Tld {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        ensure_domain_tld(&s).map_err(serde::de::Error::custom)?;
        Ok(Self(s))
    }
}

/// A slice of a [`Tld`], akin to [`str`].
#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct TldRef(str);

impl TldRef {
    // Private to enforce parsing
    fn new(s: &str) -> &Self {
        // SAFETY: `TldRef` is just a wrapper around `str` with the same memory
        // representation (`repr(transparent)`), therefore converting `&str` to
        // `&TldRef` is safe.
        unsafe { &*(s as *const str as *const TldRef) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<TldRef> for TldRef {
    fn as_ref(&self) -> &TldRef {
        self
    }
}

impl Deref for TldRef {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<TldRef> for Tld {
    fn borrow(&self) -> &TldRef {
        TldRef::new(&self.0)
    }
}

impl ToOwned for TldRef {
    type Owned = Tld;

    fn to_owned(&self) -> Self::Owned {
        Tld(self.0.to_owned())
    }
}

impl fmt::Display for TldRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A [`DomainName`] is the name of a database.
///
/// A database name is usually in one of the two following forms:
///
///  my_database_name
///
/// or
///
///  my_domain/mypath
///
/// You can also have as many path segments as you want (as long as it's less
/// than 256):
///
///  my_domain/a/b/c/d
///
/// Database names must NOT end or start in a slash and cannot have 2 slashes in
/// a row. These are all invalid:
///
///  my_domain/a//c/d
///  /my_domain
///  my_domain/
///
/// Each segment in a database name can contain any UTF-8 character, except for
/// whitespace and '/'. The maximum segment length is 64 characters.
///
/// The first path segment is also referred to as the "top-level domain", or
/// [`Tld`]. The concatenation of all segments after the first '/' is also
/// referred as the "subdomain".
///
/// Note that [`PartialEq`] compares the exact string representation of a
/// [`DomainName`], as one would expect, but the SpacetimeDB registry compares
/// the lowercase representation of it.
///
/// To construct a valid [`DomainName`], use [`parse_domain_name`] or the
/// [`FromStr`] impl.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DomainName {
    // Iff there is a subdomain, next char in `domain_name` is '/'.
    tld_offset: usize,
    domain_name: String,
}

impl DomainName {
    /// Returns a string slice with the domain name.
    pub fn as_str(&self) -> &str {
        &self.domain_name
    }

    /// Get the top-level domain, as a reference.
    pub fn tld(&self) -> &TldRef {
        TldRef::new(&self.domain_name[..self.tld_offset])
    }

    /// Get the top-level domain, as an owned [`Tld`].
    pub fn to_tld(&self) -> Tld {
        self.tld().to_owned()
    }

    /// Get the subdomain, if any.
    pub fn sub_domain(&self) -> Option<&str> {
        if self.tld_offset + 1 < self.domain_name.len() {
            Some(&self.domain_name[self.tld_offset + 1..])
        } else {
            None
        }
    }

    /// Render the name as a lower-case, '/'-separated string, suitable for use
    /// as a unique constrained field in a database.
    pub fn to_lowercase(&self) -> String {
        self.as_str().to_lowercase()
    }
}

impl AsRef<str> for DomainName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<DomainName> for String {
    fn from(name: DomainName) -> Self {
        name.domain_name
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.domain_name)
    }
}

impl FromStr for DomainName {
    type Err = DomainParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_domain_name(s)
    }
}

impl From<Tld> for DomainName {
    fn from(tld: Tld) -> Self {
        let domain_name = tld.0;
        Self {
            tld_offset: domain_name.len(),
            domain_name,
        }
    }
}

impl_st!([] DomainName, spacetimedb_lib::AlgebraicType::String);
impl_serialize!([] DomainName, (self, ser) => spacetimedb_sats::ser::Serialize::serialize(self.as_str(), ser));
impl_deserialize!([] DomainName, de => {
    let s: String = spacetimedb_sats::de::Deserialize::deserialize(de)?;
    parse_domain_name(s).map_err(spacetimedb_sats::de::Error::custom)
});

mod serde_impls {
    use super::*;

    use serde::{
        de::{self, value::MapAccessDeserializer, MapAccess},
        Deserialize, Deserializer, Serialize, Serializer,
    };

    impl Serialize for DomainName {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Serialize::serialize(self.as_str(), serializer)
        }
    }

    /// Version 1 of [`DomainName`] which is represented as a map in JSON.
    #[derive(serde::Deserialize)]
    #[cfg_attr(test, derive(serde::Serialize))]
    pub(super) struct DomainNameV1<'a> {
        pub(super) tld: &'a str,
        pub(super) sub_domain: &'a str,
    }

    /// [`de::Visitor`] for deserializing [`DomainName`].
    ///
    /// Due to the ubiquitous use of [`DomainName`], this must ensure all past
    /// and future `serde` representations can be deserialized.
    struct DomainNameVisitor;

    impl<'de> de::Visitor<'de> for DomainNameVisitor {
        type Value = DomainName;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_domain_name(v).map_err(de::Error::custom)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            parse_domain_name(v).map_err(de::Error::custom)
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let v1: DomainNameV1 = Deserialize::deserialize(MapAccessDeserializer::new(map))?;
            parse_domain_name([v1.tld, "/", v1.sub_domain].concat()).map_err(de::Error::custom)
        }
    }

    impl<'de> Deserialize<'de> for DomainName {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(DomainNameVisitor)
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetNamesResponse {
    pub names: Vec<DatabaseName>,
}

/// Returns whether a hex string is a valid identity.
///
/// Any string that is a valid identity is an invalid database name.
pub fn is_identity(hex: &str) -> bool {
    Identity::from_hex(hex).is_ok()
}

#[derive(thiserror::Error, Debug)]
#[error("Error when parsing a domain, reason: {0}")]
pub struct DomainParsingError(#[from] ParseError);

// XXX(kim): not sure it is a good idea to return the full input, but keeping it
// for now to not break any upstream expectations
#[derive(Debug, thiserror::Error)]
enum ParseError {
    #[error("Database names cannot be empty")]
    Empty,
    #[error("Identities cannot be database names: `{part}`")]
    Identity { part: String },
    #[error("Database names must not start with a slash: `{input}`")]
    StartsSlash { input: String },
    #[error("Database names must not end with a slash: `{input}`")]
    EndsSlash { input: String },
    #[error("Database names must not have 2 consecutive slashes: `{input}`")]
    SlashSlash { input: String },
    #[error("Domain name parts must not contain slashes: `{part}`")]
    ContainsSlash { part: String },
    #[error("Database names must not contain whitespace: `{input}`")]
    Whitespace { input: String },
    #[error("Domain name parts must be shorter than {MAX_CHARS_PART} characters: `{part}`")]
    TooLong { part: String },
    #[error("Domains cannot have more the {MAX_SUBDOMAINS} subdomains: `{input}`")]
    TooManySubdomains { input: String },
}

/// Maximum number of unicode characters a [`DomainName`] component can have.
pub const MAX_CHARS_PART: usize = 64;

/// Maximum number of subdomains a [`DomainName`] can have.
pub const MAX_SUBDOMAINS: usize = 256;

/// Parses a [`DomainName`].
///
/// For more information, see the documentation of [`DomainName`].
pub fn parse_domain_name<S>(domain: S) -> Result<DomainName, DomainParsingError>
where
    S: AsRef<str> + Into<String>,
{
    let input = domain.as_ref();
    if input.is_empty() {
        return Err(ParseError::Empty.into());
    }
    let mut parts = input.split('/');

    let tld = parts.next().ok_or(ParseError::Empty)?;
    // Check len for refined error.
    if tld.is_empty() {
        return Err(ParseError::StartsSlash { input: domain.into() }.into());
    }
    ensure_domain_tld(tld)?;
    let tld_offset = tld.len();

    let mut parts = parts.peekable();
    for (i, part) in parts.by_ref().enumerate() {
        if i + 1 > MAX_SUBDOMAINS {
            return Err(ParseError::TooManySubdomains { input: domain.into() }.into());
        }
        if part.is_empty() {
            // no idea why borrowchk accepts this lol
            let err = if parts.peek().is_some() {
                ParseError::SlashSlash { input: domain.into() }
            } else {
                ParseError::EndsSlash { input: domain.into() }
            };
            return Err(err.into());
        }
        ensure_domain_segment(part)?;
    }

    Ok(DomainName {
        tld_offset,
        domain_name: domain.into(),
    })
}

fn ensure_domain_segment(input: &str) -> Result<(), ParseError> {
    DomainSegment::try_from(input).map(|_| ())
}

fn ensure_domain_tld(input: &str) -> Result<(), ParseError> {
    let DomainSegment(input) = DomainSegment::try_from(input)?;
    if input.contains('/') {
        Err(ParseError::ContainsSlash { part: input.to_owned() })
    } else if is_identity(input) {
        Err(ParseError::Identity { part: input.to_owned() })
    } else {
        Ok(())
    }
}

/// Parsing helper to validate (path) segments of a [`DomainName`], without
/// consuming the input.
struct DomainSegment<'a>(&'a str);

impl<'a> TryFrom<&'a str> for DomainSegment<'a> {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(ParseError::Empty)
        } else if value.chars().count() > MAX_CHARS_PART {
            Err(ParseError::TooLong { part: value.to_owned() })
        } else if value.contains(|c: char| c.is_whitespace()) {
            Err(ParseError::Whitespace {
                input: value.to_string(),
            })
        } else {
            Ok(Self(value))
        }
    }
}
