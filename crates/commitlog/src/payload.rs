use std::sync::Arc;

use spacetimedb_sats::buffer::{BufReader, BufWriter, DecodeError};

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

    /// Decode one [`Self::Record`] from the given buffer.
    ///
    /// The `version` argument corresponds to the log format version of the
    /// current segment (see [`segment::Header::log_format_version`]).
    fn decode_record<'a, R: BufReader<'a>>(&self, version: u8, reader: &mut R) -> Result<Self::Record, DecodeError>;
    // TODO: Assuming `Decoder` is stateful, we could also update the log
    // format version only when it changes, instead of passing it on every
    // `decode_record` call.
}

impl<const N: usize> Encode for [u8; N] {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self[..])
    }
}

pub struct ArrayDecoder<const N: usize>;

impl<const N: usize> Decoder for ArrayDecoder<N> {
    type Record = [u8; N];

    fn decode_record<'a, R: BufReader<'a>>(&self, _version: u8, reader: &mut R) -> Result<Self::Record, DecodeError> {
        reader.get_array()
    }
}
