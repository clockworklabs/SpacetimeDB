use crate::buffer::{BufReader, BufWriter, DecodeError};
use crate::hash::HASH_SIZE;
use crate::Hash;
use std::fmt::Debug;

pub trait Arguments: Debug + Send {
    fn encoded_size(&self) -> usize;
    fn encode<W: BufWriter>(&self, writer: &mut W);
}

// Represents the arguments to a reducer.
#[derive(Clone, Debug)]
pub struct ReducerArguments {
    pub identity: Hash,
    pub timestamp: u64,
    pub argument_bytes: Vec<u8>,
}

impl ReducerArguments {
    pub fn new(identity: Hash, timestamp: u64, argument_bytes: Vec<u8>) -> Self {
        Self {
            identity,
            timestamp,
            argument_bytes,
        }
    }
}

impl ReducerArguments {
    pub fn decode(r: &mut impl BufReader) -> Result<Self, DecodeError> {
        let identity = Hash::from_slice(r.get_slice(HASH_SIZE)?);
        let timestamp = r.get_u64()?;
        let args_length = r.get_u32()?;
        let argument_bytes = Vec::from(r.get_slice(args_length as usize)?);

        Ok(Self {
            identity,
            timestamp,
            argument_bytes,
        })
    }
}

impl Arguments for ReducerArguments {
    fn encoded_size(&self) -> usize {
        std::mem::size_of::<u64>() + HASH_SIZE + std::mem::size_of::<usize>() + self.argument_bytes.len()
    }

    fn encode<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self.identity.data[..]);
        writer.put_u64(self.timestamp);
        writer.put_u32(self.argument_bytes.len() as u32);
        writer.put_slice(self.argument_bytes.as_slice());
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
    fn encoded_size(&self) -> usize {
        std::mem::size_of_val(&self.timestamp) + std::mem::size_of_val(&self.delta_time)
    }

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
    fn encoded_size(&self) -> usize {
        std::mem::size_of::<u64>() + HASH_SIZE
    }

    fn encode<W: BufWriter>(&self, writer: &mut W) {
        writer.put_slice(&self.identity.data[..]);
        writer.put_u64(self.timestamp);
    }
}

#[cfg(test)]
mod tests {
    use crate::args::{Arguments, ReducerArguments, RepeatingReducerArguments};
    use crate::hash::HASH_SIZE;
    use crate::Hash;
    use bytes::BufMut;
    use rand::{Rng, RngCore};

    const TEST_IDENTITY_VAL: [u64; 4] = [0xCAFEBABE, 0xDEADBEEF, 0xBAADF00D, 0xF00DBABE];
    const NUM_RAND_ITERATIONS: u32 = 32;

    fn test_identity() -> Hash {
        let mut hash_bytes = Vec::with_capacity(HASH_SIZE);
        hash_bytes.put_u64(TEST_IDENTITY_VAL[0]);
        hash_bytes.put_u64(TEST_IDENTITY_VAL[1]);
        hash_bytes.put_u64(TEST_IDENTITY_VAL[2]);
        hash_bytes.put_u64(TEST_IDENTITY_VAL[3]);
        Hash::from_slice(hash_bytes.as_slice())
    }

    fn random_payload() -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let size: usize = rng.gen_range(32..1024);
        let mut result = Vec::with_capacity(size);
        for _i in 0..size {
            result.push(rng.gen::<u8>());
        }
        result
    }

    fn make_random_args() -> (ReducerArguments, Vec<u8>) {
        let mut rng = rand::thread_rng();

        let argument_bytes = random_payload();
        assert!(!argument_bytes.is_empty());
        let ra = ReducerArguments {
            identity: test_identity(),
            timestamp: rng.next_u64(),
            argument_bytes,
        };
        let mut writer = Vec::new();
        ra.encode(&mut writer);

        (ra, writer)
    }

    fn make_random_repeating_args() -> (RepeatingReducerArguments, Vec<u8>) {
        let mut rng = rand::thread_rng();

        let ra = RepeatingReducerArguments {
            timestamp: rng.next_u64(),
            delta_time: rng.next_u64(),
        };
        let mut writer = Vec::new();
        ra.encode(&mut writer);

        (ra, writer)
    }

    #[test]
    fn test_encode_decode_reducer_args() {
        for _i in 0..NUM_RAND_ITERATIONS {
            let (ra, output_vec) = make_random_args();
            let ra2 = ReducerArguments::decode(&mut output_vec.as_slice()).unwrap();
            assert_eq!(ra.timestamp, ra2.timestamp);
            assert_eq!(ra.identity.data, ra2.identity.data);
            assert!(!ra2.argument_bytes.is_empty());
            assert_eq!(ra.argument_bytes.len(), ra2.argument_bytes.len());
            assert_eq!(ra.argument_bytes, ra2.argument_bytes);
        }
    }

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
