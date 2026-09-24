use spacetimedb_lib::Identity;
use spacetimedb_primitives::ViewId;
use spacetimedb_sats::u256;

/// A hash for uniquely identifying subscription plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryHash {
    data: [u8; 32],
}

impl From<QueryHash> for u256 {
    fn from(hash: QueryHash) -> Self {
        u256::from_le_bytes(hash.data)
    }
}

impl QueryHash {
    /// The zero value of a QueryHash.
    pub const NONE: Self = Self { data: [0; 32] };

    /// The min value of a QueryHash.
    pub const MIN: Self = Self::NONE;

    /// The max value of a QueryHash.
    pub const MAX: Self = Self { data: [0xFFu8; 32] };

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: blake3::hash(bytes).into(),
        }
    }

    /// Generate a hash from a query string.
    pub fn from_string(sql: &str, identity: Identity, has_param: bool) -> Self {
        if has_param {
            return Self::from_string_and_identity(sql, identity);
        }
        Self::from_bytes(sql.as_bytes())
    }

    /// Parameterized queries must include the caller identity in their hash.
    pub fn from_string_and_identity(sql: &str, identity: Identity) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(sql.as_bytes());
        hasher.update(&identity.to_byte_array());
        Self {
            data: hasher.finalize().into(),
        }
    }

    /// Queries over scoped views must include the caller's scopes in their hash.
    ///
    /// Queries whose results also depend on the caller's identity pass it as `identity`,
    /// in which case they are not shared with other callers in the same scopes.
    ///
    /// `view_scopes` are the arg hashes of the caller's scopes, keyed and sorted by view id.
    pub fn from_string_and_view_scopes(sql: &str, identity: Option<Identity>, view_scopes: &[(ViewId, u256)]) -> Self {
        let mut hasher = blake3::Hasher::new();
        // Separate these hashes from those of `from_bytes` and `from_string_and_identity`.
        hasher.update(b"spacetimedb::subscription::scoped::v1\0");
        hasher.update(&(sql.len() as u64).to_le_bytes());
        hasher.update(sql.as_bytes());
        match identity {
            Some(identity) => {
                hasher.update(&[1]);
                hasher.update(&identity.to_byte_array());
            }
            None => {
                hasher.update(&[0]);
            }
        }
        for (view_id, arg_hash) in view_scopes {
            hasher.update(&view_id.0.to_le_bytes());
            hasher.update(&arg_hash.to_le_bytes());
        }
        Self {
            data: hasher.finalize().into(),
        }
    }
}
