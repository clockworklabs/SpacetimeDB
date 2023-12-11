use std::sync::Arc;

use anyhow::Context as _;
use std::fmt;

use spacetimedb_sats::buffer::{BufReader, BufWriter, DecodeError};
pub use spacetimedb_sats::DataKey;

#[cfg(test)]
use proptest_derive::Arbitrary;

/// A single write operation within a [`super::transaction::Transaction`].
///
/// Encoding:
///
/// ```text
/// <flags(1)><set_id(4)><value(1-33)>
/// ```
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct Write {
    pub operation: Operation,
    pub set_id: u32, // aka table id
    pub data_key: DataKey,
    #[cfg_attr(test, proptest(value = "None"))]
    pub data: Option<Arc<Vec<u8>>>,
}

/// The operation of a [`Write`], either insert or delete.
///
/// Encoded as a single byte with bits:
///
/// ```text
/// 0   = insert / delete
/// 1-7 = unused
/// ```
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
#[repr(u8)]
pub enum Operation {
    Delete = 0,
    Insert,
}

impl Operation {
    pub fn to_u8(&self) -> u8 {
        match self {
            Operation::Delete => 0,
            Operation::Insert => 1,
        }
    }

    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Delete,
            _ => Self::Insert,
        }
    }

    pub fn decode<'a>(reader: &mut impl BufReader<'a>) -> Result<Self, DecodeError> {
        let flags = reader.get_u8()?;
        let op = (flags & 0b1000_0000) >> 7;

        Ok(Self::from_u8(op))
    }

    pub fn encoded_len(&self) -> usize {
        1
    }

    pub fn encode(&self, writer: &mut impl BufWriter) {
        let mut flags = 0u8;
        flags = if self.to_u8() != 0 { flags | 0b1000_0000 } else { flags };
        writer.put_u8(flags);
    }
}

/// Error context for [`Write::decode`].
enum Context {
    Op,
    SetId,
    DataKey,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Failed to decode `Write`: ")?;
        match self {
            Self::Op => f.write_str("operation flags"),
            Self::SetId => f.write_str("set id"),
            Self::DataKey => f.write_str("data key"),
        }
    }
}

impl Write {
    pub fn decode<'a>(reader: &mut impl BufReader<'a>) -> anyhow::Result<Self> {
        let operation = Operation::decode(reader).context(Context::Op)?;
        let set_id = reader.get_u32().context(Context::SetId)?;
        let data_key = DataKey::decode(reader).context(Context::DataKey)?;
        let len = reader.get_u32()?;
        let data = if len > 0 {
            let bytes = reader.get_slice(len as usize)?;
            Some(Arc::new(bytes.to_vec()))
        } else {
            None
        };

        Ok(Self {
            operation,
            set_id,
            data_key,
            data,
        })
    }

    pub fn encoded_len(&self) -> usize {
        let mut count = self.operation.encoded_len();
        count += 4; // set_id
        count += self.data_key.encoded_len();
        count += 4 + self.data.as_ref().map(|bytes| bytes.len()).unwrap_or_default();
        count
    }

    pub fn encode(&self, writer: &mut impl BufWriter) {
        self.operation.encode(writer);
        writer.put_u32(self.set_id);
        self.data_key.encode(writer);
        match &self.data {
            None => writer.put_u32(0),
            Some(data) => {
                writer.put_u32(data.len() as u32);
                writer.put_slice(data);
            }
        }
    }
}
