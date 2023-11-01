use anyhow::{bail, Context as _};
use spacetimedb_sats::buffer::{BufReader, BufWriter};
use std::{fmt, sync::Arc};

use super::transaction::Transaction;
use crate::hash::Hash;

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use proptest_derive::Arbitrary;

/// A commit is one record in the write-ahead log.
///
/// Encoding:
///
/// ```text
/// [0u8 | 1u8<hash(32)>]<commit_offset(8)><min_tx_offset<8>[<transaction>...]
/// ```
#[derive(Debug, Default, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Commit {
    /// The [`Hash`] over the encoded bytes of the previous commit, or `None` if
    /// it is the very first commit.
    pub parent_commit_hash: Option<Hash>,
    /// Counter of all commits in a log.
    pub commit_offset: u64,
    /// Counter of all transactions in a log.
    ///
    /// That is, a per-log value which is incremented by `transactions.len()`
    /// when the [`Commit`] is constructed.
    pub min_tx_offset: u64,
    /// The [`Transaction`]s in this commit, usually only one.
    #[cfg_attr(test, proptest(strategy = "arbitrary::transactions()"))]
    pub transactions: Vec<Arc<Transaction>>,
}

#[cfg(test)]
mod arbitrary {
    use super::*;
    // Custom strategy to apply an upper bound on the number of [`Transaction`]s
    // generated.
    //
    // We only ever commit a single transaction in practice.
    pub fn transactions() -> impl Strategy<Value = Vec<Arc<Transaction>>> {
        prop::collection::vec(any::<Arc<Transaction>>(), 1..8)
    }
}

/// Error context for [`Commit::decode`]
enum Context {
    Parent,
    Hash,
    CommitOffset,
    MinTxOffset,
    Transaction(usize),
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Failed to decode `Commit`: ")?;
        match self {
            Self::Parent => f.write_str("parent commit hash tag"),
            Self::Hash => f.write_str("parent commit hash"),
            Self::CommitOffset => f.write_str("commit offset"),
            Self::MinTxOffset => f.write_str("min transaction offset"),
            Self::Transaction(n) => f.write_str(&format!("transaction {n}")),
        }
    }
}

impl Commit {
    pub fn decode<'a>(reader: &mut impl BufReader<'a>) -> anyhow::Result<Self> {
        if reader.remaining() == 0 {
            return Ok(Self::default());
        }

        let parent_commit_hash = match reader.get_u8().context(Context::Parent)? {
            0 => None,
            1 => reader
                .get_array()
                .map(|data| Hash { data })
                .map(Some)
                .context(Context::Hash)?,
            x => bail!("Invalid tag for `Option<Hash>`: {x}"),
        };
        let commit_offset = reader.get_u64().context(Context::CommitOffset)?;
        let min_tx_offset = reader.get_u64().context(Context::MinTxOffset)?;
        let mut transactions = Vec::new();
        while reader.remaining() > 0 {
            let tx = Transaction::decode(reader)
                .map(Arc::new)
                .with_context(|| Context::Transaction(transactions.len() + 1))?;
            transactions.push(tx);
        }

        Ok(Self {
            parent_commit_hash,
            commit_offset,
            min_tx_offset,
            transactions,
        })
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = 1; // tag for option
        if let Some(hash) = self.parent_commit_hash {
            count += hash.data.len();
        }

        // 8 for commit_offset
        count += 8;

        // 8 for min_tx_offset
        count += 8;

        for tx in &self.transactions {
            count += tx.encoded_len();
        }

        count
    }

    pub fn encode(&self, writer: &mut impl BufWriter) {
        match self.parent_commit_hash {
            Some(hash) => {
                writer.put_u8(1);
                writer.put_slice(&hash.data);
            }
            None => writer.put_u8(0),
        }
        writer.put_u64(self.commit_offset);
        writer.put_u64(self.min_tx_offset);
        for tx in &self.transactions {
            tx.encode(writer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        // Generating arbitrary commits is quite slow, so limit to just a few
        // cases.
        //
        // Note that this config applies to all `#[test]`s within the enclosing
        // `proptest!`.
        #![proptest_config(ProptestConfig::with_cases(64))]


        #[test]
        fn prop_commit_encoding_roundtrip(commit in any::<Commit>()) {
            let mut buf = Vec::new();
            commit.encode(&mut buf);
            let decoded = Commit::decode(&mut buf.as_slice()).unwrap();
            prop_assert_eq!(commit, decoded)
        }

        #[test]
        fn prop_encoded_len_is_encoded_len(commit in any::<Commit>()) {
            let mut buf = Vec::new();
            commit.encode(&mut buf);
            prop_assert_eq!(buf.len(), commit.encoded_len())
        }
    }
}
