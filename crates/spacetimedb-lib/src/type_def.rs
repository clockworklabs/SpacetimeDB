use serde::{Deserialize, Serialize};

// () -> Tuple or enum?
// (0: 1) -> Tuple or enum?
// (0: 1, x: (1: 2 | 0: 2))
// (0: 1 | 1: 2)

// Types
// () -> 0-tuple or void?
// (0: u32) -> 1-tuple or 1-enum or monuple?
// (0: u32, 1: (0: 1 | 0: 2)) -> 2-tuple with enum for second type
// (0: 1 | 0: 2) -> 2-enum

// Proposed Types?
// () -> 0-tuple (either + or * operator)
// (1: u32) -> 1-tuple (either + or * operator)
// (1: u32, 2: u32) -> 2-tuple (* operator)
// (1: u32 | 2: u32) -> 2-tuple (+ operator)

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ElementDef {
    // In the case of tuples, this is the id of the column
    // In the case of enums, this is the id of the variant
    pub tag: u8,
    pub name: Option<String>,
    pub element_type: TypeDef,
}

impl ElementDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Result<Self, String>, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() <= 0 {
            return (Err("ElementDef::decode: Byte array has invalid length.".to_string()), 0);
        }

        let tag = bytes[num_read];
        num_read += 1;

        let name_len = bytes[num_read];
        num_read += 1;

        let name = if name_len == 0 {
            None
        } else {
            let name_bytes = &bytes[num_read..num_read + name_len as usize];
            num_read += name_len as usize;
            Some(String::from_utf8(name_bytes.to_vec()).expect("Yeah this should really return a result."))
        };

        let (element_type, nr) = TypeDef::decode(&bytes[num_read..]);
        num_read += nr;

        return match element_type {
            Ok(element_type) => (
                Ok(ElementDef {
                    tag,
                    element_type,
                    name,
                }),
                num_read,
            ),
            Err(e) => (Err(e), 0),
        };
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.tag);

        if let Some(name) = &self.name {
            bytes.push(name.len() as u8);
            bytes.extend(name.as_bytes())
        } else {
            bytes.push(0);
        }

        self.element_type.encode(bytes);
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TupleDef {
    pub elements: Vec<ElementDef>,
}

impl TupleDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Result<Self, String>, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return (Err("TupleDef::decode: byte array has invalid length.".to_string()), 0);
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut elements = Vec::new();
        for _ in 0..len {
            let (element, nr) = ElementDef::decode(&bytes[num_read..]);
            match element {
                Ok(element) => {
                    elements.push(element);
                    num_read += nr;
                }
                Err(e) => {
                    return (Err(e), 0);
                }
            }
        }
        (Ok(TupleDef { elements }), num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.elements.len() as u8);
        for item in &self.elements {
            item.encode(bytes);
        }
    }
}

// TODO: probably implement this with a tuple but store whether the tuple
// is a sum tuple or a product tuple, then we have uniformity over types
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EnumDef {
    pub variants: Vec<ElementDef>,
}

impl EnumDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Result<Self, String>, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() <= 0 {
            return (Err("EnumDef::decode: bytes array length is invalid.".to_string()), 0);
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut items = Vec::new();
        for _ in 0..len {
            let (item, nr) = ElementDef::decode(&bytes[num_read..]);
            match item {
                Ok(item) => {
                    items.push(item);
                    num_read += nr;
                }
                Err(e) => {
                    return (Err(e), 0);
                }
            }
        }
        (Ok(EnumDef { variants: items }), num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.variants.len() as u8);
        for item in &self.variants {
            item.encode(bytes);
        }
    }
}

/// Type definitions
///
/// WARNING:
///
/// Is important the order in this enum so sorting work correctly, and it must match
/// [TypeWideValue]/[TypeValue]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TypeDef {
    Primitive(PrimitiveType),

    Enum(EnumDef),
    Tuple(TupleDef),

    Vec { element_type: Box<TypeDef> },
}

impl From<PrimitiveType> for TypeDef {
    fn from(prim: PrimitiveType) -> Self {
        TypeDef::Primitive(prim)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrimitiveType {
    Unit,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    I128,
    U128,
    F32,
    F64,
    String,
    Bytes,
}

impl TypeDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Result<Self, String>, usize) {
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return (Err("TypeDef::decode: byte array length is invalid.".to_string()), 0);
        }

        let res = match bytes[0] {
            0 => {
                let (tuple_def, bytes_read) = TupleDef::decode(&bytes[1..]);
                match tuple_def {
                    Ok(tuple_def) => (TypeDef::Tuple(tuple_def), bytes_read + 1),
                    Err(e) => {
                        return (Err(e), 0);
                    }
                }
            }
            1 => {
                let (enum_def, bytes_read) = EnumDef::decode(&bytes[1..]);
                match enum_def {
                    Ok(enum_def) => (TypeDef::Enum(enum_def), bytes_read + 1),
                    Err(e) => {
                        return (Err(e), 0);
                    }
                }
            }
            2 => {
                let (type_def, bytes_read) = TypeDef::decode(&bytes[1..]);
                match type_def {
                    Ok(type_def) => (
                        TypeDef::Vec {
                            element_type: Box::new(type_def),
                        },
                        bytes_read + 1,
                    ),
                    Err(e) => {
                        return (Err(e), 0);
                    }
                }
            }
            4 => (TypeDef::Primitive(PrimitiveType::U16), 1),
            3 => (TypeDef::Primitive(PrimitiveType::U8), 1),
            5 => (TypeDef::Primitive(PrimitiveType::U32), 1),
            6 => (TypeDef::Primitive(PrimitiveType::U64), 1),
            7 => (TypeDef::Primitive(PrimitiveType::U128), 1),
            8 => (TypeDef::Primitive(PrimitiveType::I8), 1),
            9 => (TypeDef::Primitive(PrimitiveType::I16), 1),
            10 => (TypeDef::Primitive(PrimitiveType::I32), 1),
            11 => (TypeDef::Primitive(PrimitiveType::I64), 1),
            12 => (TypeDef::Primitive(PrimitiveType::I128), 1),
            13 => (TypeDef::Primitive(PrimitiveType::Bool), 1),
            14 => (TypeDef::Primitive(PrimitiveType::F32), 1),
            15 => (TypeDef::Primitive(PrimitiveType::F64), 1),
            16 => (TypeDef::Primitive(PrimitiveType::String), 1),
            17 => (TypeDef::Primitive(PrimitiveType::Bytes), 1),
            18 => (TypeDef::Primitive(PrimitiveType::Bytes), 1),
            b => panic!("Unknown {}", b),
        };

        (Ok(res.0), res.1)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            TypeDef::Tuple(t) => {
                bytes.push(0);
                t.encode(bytes);
            }
            TypeDef::Enum(e) => {
                bytes.push(1);
                e.encode(bytes);
            }
            TypeDef::Vec { element_type } => {
                bytes.push(2);
                element_type.encode(bytes);
            }
            TypeDef::Primitive(prim) => bytes.push(match prim {
                PrimitiveType::U8 => 3,
                PrimitiveType::U16 => 4,
                PrimitiveType::U32 => 5,
                PrimitiveType::U64 => 6,
                PrimitiveType::U128 => 7,
                PrimitiveType::I8 => 8,
                PrimitiveType::I16 => 9,
                PrimitiveType::I32 => 10,
                PrimitiveType::I64 => 11,
                PrimitiveType::I128 => 12,
                PrimitiveType::Bool => 13,
                PrimitiveType::F32 => 14,
                PrimitiveType::F64 => 15,
                PrimitiveType::String => 16,
                PrimitiveType::Bytes => 17,
                PrimitiveType::Unit => 18,
            }),
        }
    }
}
