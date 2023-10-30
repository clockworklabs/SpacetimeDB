use anyhow::Context as _;
use spacetimedb_sats::buffer::{BufReader, BufWriter};
use std::fmt;

use super::write::Write;

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use proptest_derive::Arbitrary;

/// A transaction, consisting of one or more [`Write`]s.
///
/// Encoding:
///
/// ```text
/// <n(4)>[<write_0(6-38)...<write_n(6-38)>]
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Transaction {
    #[cfg_attr(test, proptest(strategy = "arbitrary::writes()"))]
    pub writes: Vec<Write>,
}

#[cfg(test)]
mod arbitrary {
    use super::*;

    // Limit to 64 for performance reasons.
    pub fn writes() -> impl Strategy<Value = Vec<Write>> {
        prop::collection::vec(any::<Write>(), 1..64)
    }
}

/// Error context for [`Transaction::decode`].
enum Context {
    Len,
    Write(u32),
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Failed to decode `Transaction`: ")?;
        match self {
            Self::Len => f.write_str("number of writes"),
            Self::Write(n) => f.write_str(&format!("write {n}")),
        }
    }
}

// tx: [<write>...(dedupped and sorted_numerically)]*
impl Transaction {
    pub fn decode<'a>(reader: &mut impl BufReader<'a>) -> anyhow::Result<Self> {
        if reader.remaining() == 0 {
            return Ok(Self::default());
        }

        let n = reader.get_u32().context(Context::Len)?;
        let mut writes = Vec::with_capacity(n as usize);
        for i in 0..n {
            let write = Write::decode(reader).with_context(|| Context::Write(i))?;
            writes.push(write);
        }

        Ok(Self { writes })
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = 4;
        for write in &self.writes {
            count += write.encoded_len();
        }
        count
    }

    pub fn encode(&self, writer: &mut impl BufWriter) {
        writer.put_u32(self.writes.len() as u32);
        for write in &self.writes {
            write.encode(writer);
        }
    }
}
