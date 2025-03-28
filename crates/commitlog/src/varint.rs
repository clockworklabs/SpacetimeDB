//! Varint encoding and decoding functions.
//!
//! We use Protobuf's [Base-128 varint] encoding.
//!
//! Unsigned integers are split into 7-bit chunks, with the least significant
//! chunk first.
//! Each chunk is placed in the low 7 bits of a byte.
//! Non-terminal bytes have the high bit set.
//! The final byte in an integer has the high bit zeroed.
//!
//! NOTE: In the current commitlog format, varints are expected to fit into one
//! byte most of the time. Hence, the implementation below is not particularly
//! optimized for larger integers.
//!   Should this change in the future, various crates are available for
//! consideration, including [varint-simd] which provides hardware acceleration
//! on certain architectures.
//!
//! [Base-128 varint]: https://protobuf.dev/programming-guides/encoding/#varints
//! [varint-simd]: https://crates.io/crates/varint-simd

use spacetimedb_sats::buffer::{BufReader, BufWriter, DecodeError};

#[inline]
pub fn encode_varint(mut value: usize, out: &mut impl BufWriter) {
    loop {
        if value < 0x80 {
            out.put_u8(value as u8);
            break;
        } else {
            out.put_u8(((value & 0x7f) | 0x80) as u8);
            value >>= 7;
        }
    }
}

#[inline]
pub fn decode_varint<'a>(reader: &mut impl BufReader<'a>) -> Result<usize, DecodeError> {
    let mut result = 0;
    let mut shift = 0;
    loop {
        let byte = reader.get_u8()?;
        if (byte & 0x80) == 0 {
            result |= (byte as usize) << shift;
            return Ok(result);
        } else {
            result |= ((byte & 0x7F) as usize) << shift;
        }
        shift += 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn varint_roundtrip(val in any::<usize>()) {
            let mut buf = Vec::new();
            encode_varint(val, &mut buf);
            assert_eq!(val, decode_varint(&mut buf.as_slice()).unwrap());
        }
    }
}
