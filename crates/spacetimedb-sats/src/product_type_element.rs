use crate::algebraic_type::AlgebraicType;
use serde::{Deserialize, Serialize};

/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ProductTypeElement {
    pub algebraic_type: AlgebraicType,
    pub name: Option<String>,
}

impl ProductTypeElement {
    pub fn new(algebraic_type: AlgebraicType, name: Option<String>) -> Self {
        Self { algebraic_type, name }
    }

    pub fn new_named(algebraic_type: AlgebraicType, name: String) -> Self {
        Self {
            algebraic_type,
            name: Some(name),
        }
    }
}

impl ProductTypeElement {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() <= 0 {
            return Err("Byte array has invalid length.".to_string());
        }

        let name_len = bytes[num_read];
        num_read += 1;

        let name = if name_len == 0 {
            None
        } else {
            let name_bytes = &bytes[num_read..num_read + name_len as usize];
            num_read += name_len as usize;
            Some(String::from_utf8(name_bytes.to_vec()).expect("Yeah this should really return a result."))
        };

        let (algebraic_type, nr) = AlgebraicType::decode(&bytes[num_read..])?;
        num_read += nr;

        Ok((ProductTypeElement { algebraic_type, name }, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        if let Some(name) = &self.name {
            bytes.push(name.len() as u8);
            bytes.extend(name.as_bytes())
        } else {
            bytes.push(0);
        }

        self.algebraic_type.encode(bytes);
    }
}
