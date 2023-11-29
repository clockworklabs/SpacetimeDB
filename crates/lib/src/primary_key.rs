use spacetimedb_sats::data_key::DataKey;
use std::fmt;

use crate::buffer::{BufReader, BufWriter, DecodeError};

// TODO(280): Remove PrimaryKey.
// PrimaryKey is a wrapper for DataKey which identifies each row in the database.
// It is not a column and therefore does correspond to a traditional primary key
// in a relational database.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct PrimaryKey {
    pub data_key: DataKey,
}

impl fmt::Debug for PrimaryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PrimaryKey").field(&self.data_key).finish()
    }
}

impl PrimaryKey {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data_key.to_bytes()
    }

    pub fn decode<'a>(bytes: &mut impl BufReader<'a>) -> Result<Self, DecodeError> {
        let data_key = DataKey::decode(bytes)?;
        Ok(PrimaryKey { data_key })
    }

    pub fn encode(&self, bytes: &mut impl BufWriter) {
        self.data_key.encode(bytes)
    }
}
