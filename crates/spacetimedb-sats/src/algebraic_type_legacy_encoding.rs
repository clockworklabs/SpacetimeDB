use crate::{
    algebraic_type::AlgebraicType, builtin_type::BuiltinType, product_type::ProductType,
    product_type_element::ProductTypeElement, sum_type::SumType, sum_type_variant::SumTypeVariant,
};

const TAG_SUM: u8 = 0x0;
const TAG_PRODUCT: u8 = 0x1;
const TAG_BOOL: u8 = 0x02;
const TAG_I8: u8 = 0x03;
const TAG_U8: u8 = 0x04;
const TAG_I16: u8 = 0x05;
const TAG_U16: u8 = 0x06;
const TAG_I32: u8 = 0x07;
const TAG_U32: u8 = 0x08;
const TAG_I64: u8 = 0x09;
const TAG_U64: u8 = 0x0a;
const TAG_I128: u8 = 0x0b;
const TAG_U128: u8 = 0x0c;
const TAG_F32: u8 = 0x0d;
const TAG_F64: u8 = 0x0e;
const TAG_STRING: u8 = 0x0f;
const TAG_ARRAY: u8 = 0x10;
const TAG_MAP: u8 = 0x11;
const TAG_REF: u8 = 0x12;

impl AlgebraicType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array length is invalid.".to_string());
        }
        match bytes[0] {
            TAG_PRODUCT => {
                let (ty, bytes_read) = ProductType::decode(&bytes[1..])?;
                Ok((AlgebraicType::Product(ty), bytes_read + 1))
            }
            TAG_SUM => {
                let (ty, bytes_read) = SumType::decode(&bytes[1..])?;
                Ok((AlgebraicType::Sum(ty), bytes_read + 1))
            }
            _ => {
                let (ty, bytes_read) = BuiltinType::decode(&bytes[0..])?;
                Ok((AlgebraicType::Builtin(ty), bytes_read))
            }
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            AlgebraicType::Product(ty) => {
                bytes.push(TAG_PRODUCT);
                ty.encode(bytes);
            }
            AlgebraicType::Sum(ty) => {
                bytes.push(TAG_SUM);
                ty.encode(bytes);
            }
            AlgebraicType::Builtin(ty) => {
                ty.encode(bytes);
            }
            AlgebraicType::Ref(ty) => {
                bytes.push(TAG_REF);
                bytes.extend(ty.0.to_le_bytes());
            }
        }
    }
}

impl BuiltinType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array length is invalid.".to_string());
        }
        match bytes[0] {
            TAG_BOOL => Ok((Self::Bool, 1)),
            TAG_I8 => Ok((Self::I8, 1)),
            TAG_U8 => Ok((Self::U8, 1)),
            TAG_I16 => Ok((Self::I16, 1)),
            TAG_U16 => Ok((Self::U16, 1)),
            TAG_I32 => Ok((Self::I32, 1)),
            TAG_U32 => Ok((Self::U32, 1)),
            TAG_I64 => Ok((Self::I64, 1)),
            TAG_U64 => Ok((Self::U64, 1)),
            TAG_I128 => Ok((Self::I128, 1)),
            TAG_U128 => Ok((Self::U128, 1)),
            TAG_F32 => Ok((Self::F32, 1)),
            TAG_F64 => Ok((Self::F64, 1)),
            TAG_STRING => Ok((Self::String, 1)),
            TAG_ARRAY => {
                let (ty, num_read) = AlgebraicType::decode(bytes)?;
                Ok((Self::Array { ty: Box::new(ty) }, num_read))
            }
            TAG_MAP => {
                let mut num_read = 0;
                let (key_ty, nr) = AlgebraicType::decode(bytes)?;
                num_read += nr;
                let (ty, nr) = AlgebraicType::decode(bytes)?;
                num_read += nr;
                Ok((
                    Self::Map {
                        key_ty: Box::new(key_ty),
                        ty: Box::new(ty),
                    },
                    num_read,
                ))
            }
            b => panic!("Unknown {}", b),
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            BuiltinType::Bool => bytes.push(TAG_BOOL),
            BuiltinType::I8 => bytes.push(TAG_I8),
            BuiltinType::U8 => bytes.push(TAG_U8),
            BuiltinType::I16 => bytes.push(TAG_I16),
            BuiltinType::U16 => bytes.push(TAG_U16),
            BuiltinType::I32 => bytes.push(TAG_I32),
            BuiltinType::U32 => bytes.push(TAG_U32),
            BuiltinType::I64 => bytes.push(TAG_I64),
            BuiltinType::U64 => bytes.push(TAG_U64),
            BuiltinType::I128 => bytes.push(TAG_I128),
            BuiltinType::U128 => bytes.push(TAG_U128),
            BuiltinType::F32 => bytes.push(TAG_F32),
            BuiltinType::F64 => bytes.push(TAG_F64),
            BuiltinType::String => bytes.push(TAG_STRING),
            BuiltinType::Array { ty } => {
                bytes.push(TAG_ARRAY);
                ty.encode(bytes);
            }
            BuiltinType::Map { key_ty, ty } => {
                bytes.push(TAG_MAP);
                key_ty.encode(bytes);
                ty.encode(bytes);
            }
        }
    }
}

impl ProductType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() == 0 {
            return Err("Byte array has invalid length.".to_string());
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut elements = Vec::new();
        for _ in 0..len {
            let (element, nr) = ProductTypeElement::decode(&bytes[num_read..])?;
            elements.push(element);
            num_read += nr;
        }
        Ok((ProductType { elements }, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.elements.len() as u8);
        for item in &self.elements {
            item.encode(bytes);
        }
    }
}

impl SumType {
    pub fn decode(bytes: impl AsRef<[u8]>) -> Result<(Self, usize), String> {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        if bytes.len() <= 0 {
            return Err("Bytes array length is invalid.".to_string());
        }

        let len = bytes[num_read];
        num_read += 1;

        let mut items = Vec::new();
        for _ in 0..len {
            let (item, nr) = SumTypeVariant::decode(&bytes[num_read..])?;
            items.push(item);
            num_read += nr;
        }
        Ok((SumType { variants: items }, num_read))
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.variants.len() as u8);
        for item in &self.variants {
            item.encode(bytes);
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

impl SumTypeVariant {
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

        Ok((SumTypeVariant { algebraic_type, name }, num_read))
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
