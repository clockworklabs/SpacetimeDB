use spacetimedb_sats::buffer::BufWriter;

/// A commitlog payload consisting of uninterpreted bytes.
pub struct Payload(Box<[u8]>);

impl Payload {
    pub fn new(bytes: impl Into<Box<[u8]>>) -> Self {
        Self(bytes.into())
    }
}

impl spacetimedb_commitlog::Encode for Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        writer.put_u64(self.0.len() as _);
        writer.put_slice(&self.0[..]);
    }
}

impl spacetimedb_commitlog::Encode for &Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        (*self).encode_record(writer)
    }
}
