use core::fmt;
use std::str::FromStr;

use spacetimedb_sats::{
    de::{self, Deserialize, Deserializer},
    ser::{Serialize, Serializer},
    typespace::TypespaceBuilder,
    AlgebraicType, SpacetimeType,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InsertDomainResult {
    Success {
        domain: DomainName,
        address: String,
    },

    /// The top level domain for the database name is not registered. For example:
    ///
    ///  - `clockworklabs/bitcraft`
    ///
    /// if `clockworklabs` is not registered, this error is returned.
    TldNotRegistered {
        domain: DomainName,
    },

    /// The top level domain for the database name is registered, but the identity that you provided does
    /// not have permission to insert the given database name. For example:
    ///
    /// - `clockworklabs/bitcraft`
    ///
    /// If you were trying to insert this database name, but the tld `clockworklabs` is
    /// owned by an identity other than the identity that you provided, then you will receive
    /// this error.
    PermissionDenied {
        domain: DomainName,
    },
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum PublishOp {
    Created,
    Updated,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PublishResult {
    Success {
        /// `Some` if publish was given a domain name to operate on, `None`
        /// otherwise.
        ///
        /// In other words, this echoes back a domain name if one was given. If
        /// the database name given was in fact a database address, this will be
        /// `None`.
        domain: Option<String>,
        /// The address of the published database.
        ///
        /// Always set, regardless of whether publish resolved a domain name first
        /// or not.
        address: String,
        op: PublishOp,
    },

    // TODO: below variants are obsolete with control db module
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
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DnsLookupResponse {
    /// The lookup was successful and the domain and address are returned.
    Success { domain: DomainName, address: String },

    /// There was no domain registered with the given domain name
    Failure { domain: DomainName },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// The top level domain part of a [`DomainName`].
///
/// This newtype witnesses that the TLD is well-formed as per the parsing rules
/// of a full [`DomainName`]. A [`Tld`] is also a valid [`DomainName`], and can
/// be converted to this type.
///
/// Note that [`PartialEq`] compares the exact string representation of
/// the [`Tld`], as one would expect, but the SpacetimeDB registry compares the
/// lowercase representation of it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

impl fmt::Display for Tld {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
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

impl SpacetimeType for Tld {
    fn make_type<S: TypespaceBuilder>(_typespace: &mut S) -> AlgebraicType {
        AlgebraicType::String
    }
}

impl Serialize for Tld {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Tld {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse_domain_tld(&s).map_err(de::Error::custom)?;
        Ok(Self(s))
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Tld {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        parse_domain_tld(&s).map_err(serde::de::Error::custom)?;
        Ok(Self(s))
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
/// Note that [`PartialEq`] compares the exact string representation of a
/// [`DomainName`], as one would expect, but the SpacetimeDB registry compares
/// the lowercase representation of it.
///
/// To construct a valid [`DomainName`], use [`parse_domain_name`] or the
/// [`FromStr`] impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainName {
    // Iff there is a subdomain, next char in `domain_name` is '/'.
    tld_offset: usize,
    domain_name: String,
}

impl DomainName {
    pub fn as_str(&self) -> &str {
        &self.domain_name
    }

    pub fn tld(&self) -> &str {
        &self.domain_name[..self.tld_offset]
    }

    /// Drop subdomain, if any, and return only the TLD
    pub fn into_tld(mut self) -> Tld {
        self.domain_name.truncate(self.tld_offset);
        Tld(self.domain_name)
    }

    pub fn as_tld(&self) -> Tld {
        Tld(self.tld().to_owned())
    }

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

impl SpacetimeType for DomainName {
    fn make_type<S: TypespaceBuilder>(_typespace: &mut S) -> AlgebraicType {
        AlgebraicType::String
    }
}

impl Serialize for DomainName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DomainName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s: String = Deserialize::deserialize(deserializer)?;
        parse_domain_name(s).map_err(de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for DomainName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(self.as_str(), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DomainName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        parse_domain_name(s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReverseDNSResponse {
    pub names: Vec<DomainName>,
}

/// Returns whether a hex string is a valid address.
///
/// Any string that is a valid address is an invalid database name.
pub fn is_address(hex: &str) -> bool {
    hex::decode(hex).map_or(false, |value| value.len() == 16)
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
    #[error("Addresses cannot be database names: `{part}`")]
    Address { part: String },
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
    parse_domain_tld(tld)?;
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
        parse_domain_segment(part)?;
    }

    Ok(DomainName {
        tld_offset,
        domain_name: domain.into(),
    })
}

fn parse_domain_segment(input: &str) -> Result<(), ParseError> {
    DomainSegment::try_from(input).map(|_| ())
}

fn parse_domain_tld(input: &str) -> Result<(), ParseError> {
    DomainTld::try_from(input).map(|_| ())
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

/// Parsing helper to validate a [`DomainSegment`] is a valid [`Tld`], without
/// consuming the input.
struct DomainTld<'a>(&'a str);

impl<'a> TryFrom<&'a str> for DomainTld<'a> {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let DomainSegment(value) = DomainSegment::try_from(value)?;
        if value.contains('/') {
            Err(ParseError::ContainsSlash { part: value.to_owned() })
        } else if is_address(value) {
            Err(ParseError::Address { part: value.to_owned() })
        } else {
            Ok(Self(value))
        }
    }
}
