use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InsertDomainResult {
    Success {
        domain: String,
        address: String,
    },

    /// The top level domain for the database name is not registered. For example:
    ///
    ///  - `clockworklabs/bitcraft`
    ///
    /// if `clockworklabs` is not registered, this error is returned.
    TldNotRegistered {
        domain: String,
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
        domain: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PublishResult {
    Success {
        domain: Option<String>,
        address: String,
    },

    /// The top level domain for the database name is not registered. For example:
    ///
    ///  - `clockworklabs/bitcraft`
    ///
    /// if `clockworklabs` is not registered, this error is returned.
    TldNotRegistered {
        domain: String,
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
        domain: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsLookupResponse {
    /// The lookup was successful and the domain and address are returned.
    Success { domain: String, address: String },

    /// There was no domain registered with the given domain name
    Failure { domain: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegisterTldResult {
    Success {
        domain: String,
    },
    /// The domain is already registered to the calling identity
    AlreadyRegistered {
        domain: String,
    },
    /// The domain is already registered to another identity
    Unauthorized {
        domain: String,
    },
    // TODO(jdetter): Insufficient funds error here
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SetDefaultDomainResult {
    Success {
        domain: String,
    },
    /// The identity doesn't own the domain they tried to set as their default.
    Unauthorized {
        domain: String,
    },
    /// No identity owns this domain so it cannot be set as the default domain for an identity.
    NotRegistered {
        domain: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainName {
    /// The top level domain for the domain name. For example:
    ///
    ///  * `clockworklabs/bitcraft`
    ///
    /// Here, `clockworklabs` is the tld.
    pub tld: String,
    /// The part after the top level domain, this is not required. For example:
    ///
    ///  * `clockworklabs/bitcraft`
    ///
    /// Here, `bitcraft` is the subdomain.
    pub sub_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseDNSResponse {
    pub names: Vec<String>,
}

/// Returns whether a hex string is a valid address. Any string that is a valid address is
/// an invalid database name
pub fn is_address(hex: &str) -> bool {
    match hex::decode(hex) {
        Ok(value) => value.len() == 16,
        Err(_) => false,
    }
}

/// Parses a database name. A database name is usually in one of the two following forms:
///  my_database_name
/// or
///  my_domain/mypath
/// You can also have as many path segments as you want:
///  my_domain/a/b/c/d
///
/// Database names must NOT end or start in a slash and cannot have 2 slashes in a row. These
/// are all invalid:
///  my_domain/a//c/d
///  /my_domain
///  my_domain/
pub fn parse_domain_name(domain: &str) -> Result<DomainName, anyhow::Error> {
    if is_address(domain) {
        return Err(anyhow::anyhow!("Database names cannot be a valid address: {}", domain));
    }
    if domain.ends_with('/') {
        return Err(anyhow::anyhow!("Database names must not end with a slash: {}", domain));
    }
    if domain.starts_with('/') {
        return Err(anyhow::anyhow!(
            "Database names must not start with a slash: {}",
            domain
        ));
    }
    if domain.contains("//") {
        return Err(anyhow::anyhow!(
            "Database names must not have 2 consecutive slashes: {}",
            domain
        ));
    }

    if domain.contains('/') {
        let parts: Vec<&str> = domain.split('/').collect();
        let domain_name = parts[0];
        Ok(DomainName {
            tld: domain_name.to_string(),
            sub_domain: Some(
                domain
                    .chars()
                    .skip(domain_name.len() + 1)
                    .take(domain.len() - domain_name.len() + 1)
                    .collect(),
            ),
        })
    } else {
        Ok(DomainName {
            tld: domain.to_string(),
            sub_domain: None,
        })
    }
}
