use crate::{Identity, SpacetimeType, Timestamp};
use std::fmt;
use std::str::FromStr;

/// Header used to propagate distributed reducer transaction ids across remote calls.
pub const TX_ID_HEADER: &str = "X-Spacetime-Tx-Id";

/// A distributed reducer transaction identifier.
///
/// Ordering is primarily by `start_ts`, so this can later support wound-wait.
/// `creator_db` namespaces the id globally, and `nonce` breaks ties for
/// multiple transactions started on the same database at the same timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct GlobalTxId {
    pub start_ts: Timestamp,
    pub creator_db: Identity,
    pub nonce: u32,
}

impl GlobalTxId {
    pub const fn new(start_ts: Timestamp, creator_db: Identity, nonce: u32) -> Self {
        Self {
            start_ts,
            creator_db,
            nonce,
        }
    }
}

impl fmt::Display for GlobalTxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{:08x}",
            self.start_ts.to_micros_since_unix_epoch(),
            self.creator_db.to_hex(),
            self.nonce
        )
    }
}

impl FromStr for GlobalTxId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, ':');
        let start_ts = parts.next().ok_or("missing tx timestamp")?;
        let creator_db = parts.next().ok_or("missing tx creator db")?;
        let nonce = parts.next().ok_or("missing tx nonce")?;
        if parts.next().is_some() {
            return Err("too many tx id components");
        }

        let start_ts = start_ts
            .parse::<i64>()
            .map(Timestamp::from_micros_since_unix_epoch)
            .map_err(|_| "invalid tx timestamp")?;
        let creator_db = Identity::from_hex(creator_db).map_err(|_| "invalid tx creator db")?;
        let nonce = u32::from_str_radix(nonce, 16).map_err(|_| "invalid tx nonce")?;

        Ok(Self::new(start_ts, creator_db, nonce))
    }
}
