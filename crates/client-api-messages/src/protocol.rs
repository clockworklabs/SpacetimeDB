#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum ProtocolEncoding {
    Text,
    Binary,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum ProtocolCompression {
    None,
    Brotli,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct Protocol {
    pub encoding: ProtocolEncoding,
    pub binary_compression: ProtocolCompression,
}
