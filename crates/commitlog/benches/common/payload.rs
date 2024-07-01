use std::ops::Range;

use rand::{distributions::Standard, Rng as _, RngCore};
use spacetimedb_commitlog::{Decoder, Encode};
use spacetimedb_sats::buffer::{BufReader, BufWriter};

/// A commitlog payload consisting of uninterpreted bytes.
#[derive(Debug)]
pub struct Payload(Box<[u8]>);

impl Payload {
    /// Generate a random [`Payload`] with a size in the given range.
    pub fn random<R: RngCore>(rng: &mut R, size: Range<usize>) -> Self {
        let len = rng.gen_range(size);
        let data: Box<_> = rng.sample_iter(Standard).take(len).collect();
        Self(data)
    }
}

impl Encode for Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        writer.put_u64(self.0.len() as _);
        writer.put_slice(&self.0[..]);
    }
}

impl Encode for &Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        Encode::encode_record(*self, writer)
    }
}

/// A [`Decoder`] for [`Payload`]s.
pub struct PayloadDecoder;

impl Decoder for PayloadDecoder {
    type Error = anyhow::Error;
    type Record = Payload;

    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        _version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> Result<Self::Record, Self::Error> {
        let len = reader.get_u64()?;
        let data = reader.get_slice(len as _)?;

        Ok(Payload(data.into()))
    }

    fn consume_record<'a, R: BufReader<'a>>(
        &self,
        _version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> Result<(), Self::Error> {
        let len = reader.get_u64()?;
        reader.get_slice(len as _)?;

        Ok(())
    }

    fn skip_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> Result<(), Self::Error> {
        self.consume_record(version, tx_offset, reader)
    }
}
