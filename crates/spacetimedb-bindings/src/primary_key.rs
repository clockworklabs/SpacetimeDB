use crate::DataKey;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PrimaryKey {
    pub data_key: DataKey,
}

impl PrimaryKey {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data_key.to_bytes()
    }

    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let (data_key, nr) = DataKey::decode(bytes);
        (PrimaryKey {
            data_key
        }, nr)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) -> usize {
        self.data_key.encode(bytes)
    }
}