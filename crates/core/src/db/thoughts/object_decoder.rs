struct Decoder<'de> {
    input: &'de [u8],
    bytes_read: usize,
}

impl<'de> Decoder<'de> {
    fn decode_write() -> Write {

    }
}

pub fn from_slice<'a, T>(s: &'a impl AsRef<[u8]>) -> Result<(T, usize), anyhow::Error>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_slice(s);
    let t = T::deserialize(&mut deserializer)?;
    Ok((t, deserializer.bytes_read))
}
