use crate::buffer::{BufReader, BufWriter, DecodeError};
use crate::hash::HASH_SIZE;
use crate::{Hash, ReducerDef, TupleValue};
use std::fmt::Debug;

// NOTICE!! every time you make a breaking change to the wire format, you MUST
//          bump `SCHEMA_FORMAT_VERSION` in lib.rs!

pub trait Arguments: Debug + Send {
    fn encoded_size(&self) -> usize {
        let mut size = EncodedSize { size: 0 };
        self.encode(&mut size);
        size.size
    }
    fn encode_to_vec(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.encoded_size());
        self.encode(&mut v);
        v
    }
    fn encode<W: BufWriter>(&self, writer: &mut W);
}

struct EncodedSize {
    size: usize,
}
impl BufWriter for EncodedSize {
    fn put_slice(&mut self, slice: &[u8]) {
        self.size += slice.len();
    }
}

// Represents the arguments to a reducer.
#[derive(Clone, Debug)]
pub struct ReducerArguments {
    pub identity: Hash,
    pub timestamp: u64,
    pub arguments: TupleValue,
}

impl<'a> ReducerArguments {
    pub fn new(identity: Hash, timestamp: u64, arguments: TupleValue) -> Self {
        Self {
            identity,
            timestamp,
            arguments,
        }
    }
}

impl ReducerArguments {
    pub fn decode(r: &mut impl BufReader, schema: &ReducerDef) -> Result<Self, DecodeError> {
        let identity = Hash::from_slice(r.get_slice(HASH_SIZE)?);
        let timestamp = r.get_u64()?;
        let arguments = TupleValue::decode_from_elements(&schema.args, r)?;

        Ok(Self {
            identity,
            timestamp,
            arguments,
        })
    }
}

impl Arguments for ReducerArguments {
    fn encode<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self.identity.data[..]);
        writer.put_u64(self.timestamp);
        self.arguments.encode(writer);
    }
}

// Represents the arguments for a repeating reducer.
#[derive(Clone, Debug)]
pub struct RepeatingReducerArguments {
    pub timestamp: u64,
    pub delta_time: u64,
}
impl RepeatingReducerArguments {
    pub fn new(timestamp: u64, delta_time: u64) -> Self {
        Self { timestamp, delta_time }
    }

    pub fn decode(r: &mut impl BufReader) -> Result<Self, DecodeError> {
        let timestamp = r.get_u64()?;
        let delta_time = r.get_u64()?;

        Ok(Self { timestamp, delta_time })
    }
}

impl Arguments for RepeatingReducerArguments {
    fn encode<W: BufWriter>(&self, writer: &mut W) {
        writer.put_u64(self.timestamp);
        writer.put_u64(self.delta_time);
    }
}

// Represents the arguments to a reducer.
#[derive(Clone, Debug)]
pub struct ConnectDisconnectArguments {
    pub identity: Hash,
    pub timestamp: u64,
}

impl ConnectDisconnectArguments {
    pub fn new(identity: Hash, timestamp: u64) -> Self {
        Self { identity, timestamp }
    }
}

impl ConnectDisconnectArguments {
    pub fn decode(r: &mut impl BufReader) -> Result<Self, DecodeError> {
        let identity = Hash::from_slice(r.get_slice(HASH_SIZE)?);
        let timestamp = r.get_u64()?;

        Ok(Self { identity, timestamp })
    }
}

impl Arguments for ConnectDisconnectArguments {
    fn encode<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self.identity.data[..]);
        writer.put_u64(self.timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use rand::distributions::Standard;
    use rand::Rng;

    // const TEST_IDENTITY_VAL: [u64; 4] = [0xCAFEBABE, 0xDEADBEEF, 0xBAADF00D, 0xF00DBABE];
    const NUM_RAND_ITERATIONS: u32 = 32;

    // fn test_identity() -> Hash {
    //     let mut data = [0; HASH_SIZE];
    //     let mut hash_bytes = &mut data[..];
    //     hash_bytes.put_u64(TEST_IDENTITY_VAL[0]);
    //     hash_bytes.put_u64(TEST_IDENTITY_VAL[1]);
    //     hash_bytes.put_u64(TEST_IDENTITY_VAL[2]);
    //     hash_bytes.put_u64(TEST_IDENTITY_VAL[3]);
    //     Hash { data }
    // }

    // fn with_random_args(f: impl FnOnce(ReducerArguments, Vec<u8>)) {
    //     let mut rng = rand::thread_rng();

    //     let size: usize = rng.gen_range(32..1024);
    //     let argument_bytes = &(&mut rng).sample_iter(Standard).take(size).collect::<Vec<u8>>();

    //     let ra = ReducerArguments {
    //         identity: test_identity(),
    //         timestamp: rng.gen(),
    //         arguments,
    //     };
    //     let mut writer = Vec::new();
    //     ra.encode(&mut writer);

    //     f(ra, writer)
    // }

    fn make_random_repeating_args() -> (RepeatingReducerArguments, Vec<u8>) {
        let mut rng = rand::thread_rng();

        let ra = RepeatingReducerArguments {
            timestamp: rng.gen(),
            delta_time: rng.gen(),
        };
        let mut writer = Vec::new();
        ra.encode(&mut writer);

        (ra, writer)
    }

    // #[test]
    // fn test_encode_decode_reducer_args() {
    //     for _i in 0..NUM_RAND_ITERATIONS {
    //         with_random_args(|ra, output_vec| {
    //             let mut rdr = &output_vec[..];
    //             let ra2 = ReducerArguments::decode(&mut rdr).unwrap();
    //             assert_eq!(ra.timestamp, ra2.timestamp);
    //             assert_eq!(ra.identity.data, ra2.identity.data);
    //             assert!(!ra2.argument_bytes.is_empty());
    //             assert_eq!(ra.argument_bytes.len(), ra2.argument_bytes.len());
    //             assert_eq!(ra.argument_bytes, ra2.argument_bytes);
    //         });
    //     }
    // }

    #[test]
    fn test_encode_decode_repeating_reducer_args() {
        for _i in 0..NUM_RAND_ITERATIONS {
            let (ra, output_vec) = make_random_repeating_args();
            let ra2 = RepeatingReducerArguments::decode(&mut output_vec.as_slice()).unwrap();
            assert_eq!(ra.timestamp, ra2.timestamp);
            assert_eq!(ra.delta_time, ra2.delta_time);
        }
    }
}
