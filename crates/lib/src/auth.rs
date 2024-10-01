use std::fmt::{self, Debug};

use blake3;
use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_sats::hex::HexString;

// This should replace the original Identity. I'm adding a new type for now instead of renaming in case it makes refactoring easier.
#[derive(Default, Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize)]
pub struct Identifier {
    __identifier_bytes: [u8; 32],
}

impl Identifier {
    pub fn to_hex(&self) -> HexString<32> {
        spacetimedb_sats::hex::encode(&self.__identifier_bytes)
    }
}

impl Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identity").field(&self.to_hex()).finish()
    }
}

// A thin wrapper around a JWT iss.
// TODO: This should have a constructor that takes a string and validates it (must be a url, length is (0,128]).
pub struct Issuer(String);
// A thin wrapper around a JWT sub.
// TODO: This should have a constructor that takes a string and validates it (length is (0,128]).
pub struct Subject(String);
// A thin wrapper around a JWT aud.
pub struct Audience(Vec<String>);

// This is the validated auth information that we can get out of a client token.
// These are the functions which need to be exposed to the public ReducerContext API.
pub trait AuthInfo {
    // This is a globally unique id made by hashing the Issuer and Subject.
    fn id(&self) -> Identifier;
    fn issuer(&self) -> Issuer;
    fn subject(&self) -> Subject;
    fn audience(&self) -> Audience;
}

// Claims from a token for which we have already verified the signature.
// TODO: this should use the types with validation built in.
pub struct ValidatedTokenClaims {
    pub issuer: String,
    pub subject: String,
    pub audience: Vec<String>,
}

impl AuthInfo for ValidatedTokenClaims {
    // For details on how we hash, see:
    // https://github.com/clockworklabs/SpacetimeDBPrivate/blob/master/proposals/0024-identities-and-identifiers/0024-identities-and-identifiers.md#identifier
    fn id(&self) -> Identifier {
        let input = format!("{}|{}", self.issuer, self.subject);
        let first_hash = blake3::hash(input.as_bytes());
        let id_hash = &first_hash.as_bytes()[..26];
        let mut checksum_input = [0u8; 28];
        // TODO: double check this gets the right number...
        checksum_input[2..].copy_from_slice(id_hash);
        checksum_input[0] = 0xc2;
        checksum_input[1] = 0x00;
        let checksum_hash = &blake3::hash(&checksum_input);

        let mut final_bytes = [0u8; 32];
        final_bytes[0] = 0xc2;
        final_bytes[1] = 0x00;
        final_bytes[2..6].copy_from_slice(&checksum_hash.as_bytes()[..4]);
        final_bytes[6..].copy_from_slice(id_hash);
        Identifier {
            __identifier_bytes: final_bytes,
        }
    }

    fn issuer(&self) -> Issuer {
        Issuer(self.issuer.clone())
    }

    fn subject(&self) -> Subject {
        Subject(self.subject.clone())
    }

    fn audience(&self) -> Audience {
        Audience(self.audience.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::{AuthInfo, ValidatedTokenClaims};
    use rand::{distributions::Alphanumeric, Rng, SeedableRng};
    use rand_chacha::ChaCha20Rng;
    use std::iter;

    // Make sure the checksum is valid.
    fn validate_checksum(id: &[u8; 32]) -> bool {
        let checksum_input = &id[6..];
        let mut checksum_input_with_prefix = [0u8; 28];
        checksum_input_with_prefix[2..].copy_from_slice(checksum_input);
        checksum_input_with_prefix[0] = 0xc2;
        checksum_input_with_prefix[1] = 0x00;
        let checksum_hash = &blake3::hash(&checksum_input_with_prefix);
        checksum_hash.as_bytes()[0..4] == id[2..6]
    }

    // Generates a random string of length between 1 and 128.
    fn generate_random_string(rng: &mut ChaCha20Rng) -> String {
        let string_length = rng.gen_range(1..=128);
        iter::repeat(())
            .map(|()| rng.sample(Alphanumeric))
            .map(char::from)
            .take(string_length)
            .collect()
    }

    #[test]
    fn test_checksum() {
        // Generate a few random tokens and check that the checksum is valid.
        let mut rng = ChaCha20Rng::seed_from_u64(10);
        for _ in 0..20 {
            let issuer = generate_random_string(&mut rng);
            let subject = generate_random_string(&mut rng);
            let id = ValidatedTokenClaims {
                issuer,
                subject,
                audience: vec![],
            }
            .id();
            assert!(validate_checksum(&id.__identifier_bytes));
        }
    }

    #[test]
    fn test_audience_isnt_hashed() {
        let c1 = ValidatedTokenClaims {
            issuer: "test".to_string(),
            subject: "test".to_string(),
            audience: vec!["test".to_string()],
        };
        let c2 = ValidatedTokenClaims {
            issuer: "test".to_string(),
            subject: "test".to_string(),
            audience: vec!["test2".to_string()],
        };
        assert_eq!(c1.id(), c2.id());
    }

    #[test]
    fn test_sub_is_hashed() {
        let c1 = ValidatedTokenClaims {
            issuer: "test".to_string(),
            subject: "test".to_string(),
            audience: vec!["test".to_string()],
        };
        let c2 = ValidatedTokenClaims {
            issuer: "test".to_string(),
            subject: "test2".to_string(),
            audience: vec!["test".to_string()],
        };
        assert_ne!(c1.id(), c2.id());
    }

    #[test]
    fn test_iss_is_hashed() {
        let c1 = ValidatedTokenClaims {
            issuer: "test".to_string(),
            subject: "test".to_string(),
            audience: vec!["test".to_string()],
        };
        let c2 = ValidatedTokenClaims {
            issuer: "test2".to_string(),
            subject: "test".to_string(),
            audience: vec!["test".to_string()],
        };
        assert_ne!(c1.id(), c2.id());
    }
}
