use core::fmt;
use std::{ops::Deref, str::FromStr};

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublishOp {
    Created,
    Updated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsLookupResponse {
    /// The lookup was successful and the domain and address are returned.
    Success { domain: DomainName, address: String },

    /// There was no domain registered with the given domain name
    Failure { domain: DomainName },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// A part (component) of a [`DomainName`].
///
/// [`DomainPart`]s are compared case-insensitively using their Unicode
/// lowercase mapping. The original string is used for [`Display`] and
/// [`Serialize`] purposes.
///
/// **Note**: case-insensitive comparison is not the same as unicode case
/// folding. For example, using case folding, "MASSE" and "Maße" compare as
/// equal, while lower-casing each doesn't. Using case folding here would be
/// preferable, as it can detect some instances of words which contain similar-
/// looking, but distinct characters. This would, however, require support from
/// SATS or some other way to allow custom collations in STDB.
///
/// Currently, both casings are retained (even if they are the same), as we will
/// likely need both for storage. This may change in the future.
#[derive(Debug, Clone)]
pub struct DomainPart {
    lower: String,
    mixed: String,
}

impl DomainPart {
    pub fn as_lowercase(&self) -> &str {
        &self.lower
    }

    pub fn as_str(&self) -> &str {
        &self.mixed
    }

    pub fn is_empty(&self) -> bool {
        self.mixed.is_empty()
    }

    /// Length of the original string, in bytes.
    pub fn len(&self) -> usize {
        self.mixed.len()
    }
}

impl PartialEq for DomainPart {
    fn eq(&self, other: &Self) -> bool {
        self.lower.eq(&other.lower)
    }
}

impl Serialize for DomainPart {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.mixed.as_ref())
    }
}

impl<'de> Deserialize<'de> for DomainPart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DomainPart::try_from(s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for DomainPart {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.mixed)
    }
}

impl TryFrom<String> for DomainPart {
    type Error = DomainParsingError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(ParseError::Empty.into())
        } else if value.contains(|c: char| c.is_whitespace()) {
            Err(ParseError::Whitespace { input: value }.into())
        } else {
            Ok(Self {
                lower: value.to_lowercase(),
                mixed: value,
            })
        }
    }
}

impl FromStr for DomainPart {
    type Err = DomainParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

/// The top level domain part of a [`DomainName`].
///
/// This newtype witnesses that the TLD is well-formed as per the parsing rules
/// of a full [`DomainName`]. A [`Tld`] is also a valid [`DomainPart`] and valid
/// [`DomainName`], and can be converted to these types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tld(DomainPart);

impl TryFrom<DomainPart> for Tld {
    type Error = DomainParsingError;

    fn try_from(part: DomainPart) -> Result<Self, Self::Error> {
        if part.as_str().chars().count() > MAX_CHARS_PART {
            Err(ParseError::TooLong { part }.into())
        } else if is_address(part.as_str()) {
            Err(ParseError::Address { part }.into())
        } else {
            Ok(Self(part))
        }
    }
}

impl Deref for Tld {
    type Target = DomainPart;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Tld {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Tld> for DomainName {
    fn from(tld: Tld) -> Self {
        Self { tld, sub_domain: None }
    }
}

impl From<Tld> for DomainPart {
    fn from(tld: Tld) -> Self {
        tld.0
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
/// [`DomainName`]s consist of [`DomainPart`]s, and are compared case-
/// insensitively. Note, however, that the [`fmt::Display`] and [`Serialize`]
/// impls will use the original, (potentially) mixed-case representation.
///
/// To construct a valid [`DomainName`], use [`parse_domain_name`] or the
/// [`FromStr`] impl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainName {
    /// The top level domain for the domain name. For example:
    ///
    ///  * `clockworklabs/bitcraft`
    ///
    /// Here, `clockworklabs` is the tld.
    tld: Tld,
    /// The part after the top level domain, this is not required. For example:
    ///
    ///  * `clockworklabs/bitcraft`
    ///
    /// Here, `bitcraft` is the subdomain.
    sub_domain: Option<DomainPart>,
}

impl DomainName {
    pub fn tld(&self) -> &Tld {
        &self.tld
    }

    /// Drop subdomain, if any, and return only the TLD
    pub fn into_tld(self) -> Tld {
        self.tld
    }

    pub fn sub_domain(&self) -> Option<&DomainPart> {
        self.sub_domain.as_ref()
    }

    /// Render the name as a lower-case, '/'-separated string, suitable for use
    /// as a unique constrained field in a database.
    pub fn to_lowercase(&self) -> String {
        let mut s = String::with_capacity(
            self.tld.lower.len() + self.sub_domain.as_ref().map(|part| part.lower.len() + 1).unwrap_or(0),
        );
        s.push_str(&self.tld.lower);
        if let Some(sub) = &self.sub_domain {
            s.push('/');
            s.push_str(&sub.lower);
        }
        s
    }

    pub fn into_parts(self) -> (Tld, Option<DomainPart>) {
        (self.tld, self.sub_domain)
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tld)?;
        if let Some(sub) = &self.sub_domain {
            write!(f, "/{}", sub)?;
        }

        Ok(())
    }
}

impl FromStr for DomainName {
    type Err = DomainParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_domain_name(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    Address { part: DomainPart },
    #[error("Database names must not start with a slash: `{input}`")]
    StartsSlash { input: String },
    #[error("Database names must not end with a slash: `{input}`")]
    EndsSlash { input: String },
    #[error("Database names must not have 2 consecutive slashes: `{input}`")]
    SlashSlash { input: String },
    #[error("Database names must not contain whitespace: `{input}`")]
    Whitespace { input: String },
    #[error("Domain name parts must be shorter than {MAX_CHARS_PART} characters: `{part}`")]
    TooLong { part: DomainPart },
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
pub fn parse_domain_name(domain: &str) -> Result<DomainName, DomainParsingError> {
    if domain.is_empty() {
        return Err(ParseError::Empty.into());
    }
    let mut parts = domain.split('/');

    let tld = parts.next().ok_or(ParseError::Empty)?;
    if tld.is_empty() {
        return Err(ParseError::StartsSlash {
            input: domain.to_owned(),
        }
        .into());
    }
    let tld = DomainPart::from_str(tld).and_then(Tld::try_from)?;

    let mut sub_domain = String::with_capacity(domain.len() - tld.len());
    let mut parts = parts.peekable();
    for (i, part) in parts.by_ref().enumerate() {
        if i + 1 > MAX_SUBDOMAINS {
            return Err(ParseError::TooManySubdomains {
                input: domain.to_owned(),
            }
            .into());
        }
        if part.is_empty() {
            // no idea why borrowchk accepts this lol
            let err = if parts.peek().is_some() {
                ParseError::SlashSlash {
                    input: domain.to_owned(),
                }
            } else {
                ParseError::EndsSlash {
                    input: domain.to_owned(),
                }
            };
            return Err(err.into());
        }
        if part.chars().count() > MAX_CHARS_PART {
            return Err(ParseError::TooLong { part: tld.into() }.into());
        }

        if i > 0 {
            sub_domain.push('/');
        }
        sub_domain.push_str(part);
    }

    let sub_domain = if sub_domain.is_empty() {
        None
    } else {
        Some(DomainPart::try_from(sub_domain)?)
    };

    Ok(DomainName { tld, sub_domain })
}
