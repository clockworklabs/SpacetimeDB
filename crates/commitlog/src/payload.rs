use std::sync::Arc;

use spacetimedb_sats::{
    bsatn,
    buffer::{BufReader, BufWriter, DecodeError},
    ser::Serialize,
    ProductValue,
};
use thiserror::Error;

pub mod txdata;
pub use txdata::Txdata;

/// A **datatype** which can be encoded.
///
/// The transaction payload of the commitlog (i.e. individual records in the log)
/// must satisfy this trait.
pub trait Encode {
    /// Encode `self` to the given buffer.
    fn encode_record<W: BufWriter>(&self, writer: &mut W);
}

impl<T: Encode> Encode for Arc<T> {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        (**self).encode_record(writer)
    }
}

impl Encode for ProductValue {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        self.serialize(bsatn::Serializer::new(writer))
            .expect("bsatn serialize should never fail");
    }
}

impl Encode for () {
    fn encode_record<W: BufWriter>(&self, _writer: &mut W) {}
}

/// A decoder which can decode the transaction (aka record) format of the log.
///
/// Unlike [`Encode`], this is not a datatype: the canonical commitlog format
/// requires to look up row types during log traversal in order to be able to
/// decode (see also [`RowDecoder`]).
pub trait Decoder {
    /// The type of records this decoder can decode.
    /// This is also the type which can be appended to a commitlog, and so must
    /// satisfy [`Encode`].
    type Record: Encode;
    /// The type of decode errors, which must subsume [`DecodeError`].
    type Error: From<DecodeError>;

    /// Decode one [`Self::Record`] from the given buffer.
    ///
    /// The `version` argument corresponds to the log format version of the
    /// current segment (see [`segment::Header::log_format_version`]).
    ///
    /// The `tx_argument` is the transaction offset of the current record
    /// relative to the start of the log.
    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> Result<Self::Record, Self::Error>;

    /// Variant of [`Self::decode_record`] which discards the decoded
    /// [`Self::Record`].
    ///
    /// Useful for folds which don't need to yield or collect record values.
    ///
    /// The default implementation just drops the record returned from
    /// [`Self::decode_record`]. Implementations may want to override this, such
    /// that the record is not allocated in the first place.
    fn consume_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> Result<(), Self::Error> {
        self.decode_record(version, tx_offset, reader).map(drop)
    }
}

impl<const N: usize> Encode for [u8; N] {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self[..])
    }
}

#[derive(Debug, Error)]
pub enum ArrayDecodeError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Traversal(#[from] crate::error::Traversal),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct ArrayDecoder<const N: usize>;

impl<const N: usize> Decoder for ArrayDecoder<N> {
    type Record = [u8; N];
    type Error = ArrayDecodeError;

    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        _version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> Result<Self::Record, Self::Error> {
        Ok(reader.get_array()?)
    }
}
