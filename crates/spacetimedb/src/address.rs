#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address(u128);

impl Address {
    const ABBREVIATION_LEN: usize = 16;

    pub fn from_arr(arr: &[u8; 16]) -> Self {
        Self(u128::from_be_bytes(*arr))
    }

    pub fn from_hex(hex: &str) -> Result<Self, anyhow::Error> {
        let data = hex::decode(hex)?;
        let data: [u8; 16] = data
            .try_into()
            .expect("hex representation of hash decoded to incorrect number of bytes");
        Ok(Self::from_arr(&data))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_slice())
    }

    pub fn to_abbreviated_hex(&self) -> String {
        self.to_hex()[0..Self::ABBREVIATION_LEN].to_owned()
    }

    pub fn from_slice(slice: impl AsRef<[u8]>) -> Self {
        let slice = slice.as_ref();
        let mut dst = [0u8; 16];
        dst.copy_from_slice(slice);
        Self(u128::from_be_bytes(dst))
    }

    pub fn as_slice(&self) -> [u8; 16] {
        self.0.to_be_bytes()
    }
}
