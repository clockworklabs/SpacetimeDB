// pub mod encoding;
pub mod satn;

use crate::algebraic_value::AlgebraicValue;
use crate::product_type::ProductType;

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub struct ProductValue {
    pub elements: Vec<AlgebraicValue>,
}

#[macro_export]
macro_rules! product {
    [$($elems:expr),*$(,)?] => {
        $crate::ProductValue {
            elements: vec![$($crate::AlgebraicValue::from($elems)),*]
        }
    }
}

impl FromIterator<AlgebraicValue> for ProductValue {
    fn from_iter<T: IntoIterator<Item = AlgebraicValue>>(iter: T) -> Self {
        let elements = iter.into_iter().collect();
        Self { elements }
    }
}

impl crate::Value for ProductValue {
    type Type = ProductType;
}

#[derive(thiserror::Error, Debug, Clone)]
#[error("Field {0}({1:?}) not found or has a invalid type")]
pub struct InvalidFieldError(usize, Option<&'static str>);

impl ProductValue {
    pub fn get_field(&self, index: usize, named: Option<&'static str>) -> Result<&AlgebraicValue, InvalidFieldError> {
        self.elements.get(index).ok_or(InvalidFieldError(index, named))
    }

    pub fn field_as_bool(&self, index: usize, named: Option<&'static str>) -> Result<bool, InvalidFieldError> {
        let f = self.get_field(index, named)?;
        let r = f.as_bool().ok_or(InvalidFieldError(index, named))?;
        Ok(*r)
    }

    pub fn field_as_u32(&self, index: usize, named: Option<&'static str>) -> Result<u32, InvalidFieldError> {
        let f = self.get_field(index, named)?;
        let r = f.as_u32().ok_or(InvalidFieldError(index, named))?;
        Ok(*r)
    }

    pub fn field_as_i64(&self, index: usize, named: Option<&'static str>) -> Result<i64, InvalidFieldError> {
        let f = self.get_field(index, named)?;
        let r = f.as_i64().ok_or(InvalidFieldError(index, named))?;
        Ok(*r)
    }

    pub fn field_as_str(&self, index: usize, named: Option<&'static str>) -> Result<&str, InvalidFieldError> {
        let f = self.get_field(index, named)?;
        let r = f.as_string().ok_or(InvalidFieldError(index, named))?;
        Ok(r)
    }

    pub fn field_as_bytes(&self, index: usize, named: Option<&'static str>) -> Result<&[u8], InvalidFieldError> {
        let f = self.get_field(index, named)?;
        let r = f.as_bytes().ok_or(InvalidFieldError(index, named))?;
        Ok(r)
    }
}
