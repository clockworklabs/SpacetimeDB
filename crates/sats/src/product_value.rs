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

impl ProductValue {
    pub fn new(elements: &[AlgebraicValue]) -> Self {
        Self {
            elements: elements.into(),
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
#[error("Field {0}({1:?}) not found or has an invalid type")]
pub struct InvalidFieldError(pub usize, pub Option<&'static str>);

impl ProductValue {
    pub fn get_field(&self, index: usize, named: Option<&'static str>) -> Result<&AlgebraicValue, InvalidFieldError> {
        self.elements.get(index).ok_or(InvalidFieldError(index, named))
    }

    pub fn extract_field<'a, T, F>(
        &'a self,
        index: usize,
        named: Option<&'static str>,
        f: F,
    ) -> Result<T, InvalidFieldError>
    where
        F: Fn(&'a AlgebraicValue) -> Option<T> + 'a,
    {
        let v = self.elements.get(index).ok_or(InvalidFieldError(index, named))?;
        let r = f(v).ok_or(InvalidFieldError(index, named))?;
        Ok(r)
    }

    pub fn field_as_bool(&self, index: usize, named: Option<&'static str>) -> Result<bool, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_bool().copied())
    }

    pub fn field_as_u8(&self, index: usize, named: Option<&'static str>) -> Result<u8, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u8().copied())
    }

    pub fn field_as_u32(&self, index: usize, named: Option<&'static str>) -> Result<u32, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u32().copied())
    }

    pub fn field_as_i64(&self, index: usize, named: Option<&'static str>) -> Result<i64, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_i64().copied())
    }

    pub fn field_as_str(&self, index: usize, named: Option<&'static str>) -> Result<&str, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_string().map(|x| x.as_str()))
    }

    pub fn field_as_bytes(&self, index: usize, named: Option<&'static str>) -> Result<&[u8], InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_bytes().map(|x| x.as_slice()))
    }
}
