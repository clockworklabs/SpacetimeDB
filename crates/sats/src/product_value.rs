use crate::algebraic_value::AlgebraicValue;
use crate::product_type::ProductType;
use crate::ArrayValue;
use nonempty::NonEmpty;
use spacetimedb_primitives::ColId;

/// A product value is made of a a list of
/// "elements" / "fields" / "factors" of other `AlgebraicValue`s.
///
/// The type of a product value is a [product type](`ProductType`).
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub struct ProductValue {
    /// The values that make up this product value.
    pub elements: Vec<AlgebraicValue>,
}

/// Constructs a product value from a list of fields with syntax `product![v1, v2, ...]`.
///
/// Repeat notation from `vec![x; n]` is not supported.
#[macro_export]
macro_rules! product {
    [$($elems:expr),*$(,)?] => {
        $crate::ProductValue {
            elements: [$($crate::AlgebraicValue::from($elems)),*].into()
        }
    }
}

impl ProductValue {
    /// Returns a product value constructed from the given values in `elements`.
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

/// An error that occurs when a field, of a product value, is accessed that doesn't exist.
#[derive(thiserror::Error, Debug, Copy, Clone)]
#[error("Field {col_pos}({name:?}) not found or has an invalid type")]
pub struct InvalidFieldError {
    /// The claimed col_pos of the field within the product value.
    pub col_pos: usize,
    /// The name of the field, if any.
    pub name: Option<&'static str>,
}

impl ProductValue {
    /// Borrow the value at field of `self` indentified by `index`.
    ///
    /// The `name` is non-functional and is only used for error-messages.
    pub fn get_field(&self, index: usize, name: Option<&'static str>) -> Result<&AlgebraicValue, InvalidFieldError> {
        self.elements
            .get(index)
            .ok_or(InvalidFieldError { col_pos: index, name })
    }

    /// This function is used to project fields based on the provided `indexes`.
    ///
    /// It will raise an [InvalidFieldError] if any of the supplied `indexes` cannot be found.
    ///
    /// The optional parameter `name: Option<&'static str>` serves a non-functional role and is
    /// solely utilized for generating error messages.
    ///
    /// **Important:**
    ///
    /// The resulting [AlgebraicValue] will wrap into a [ProductValue] when projecting multiple
    /// fields, otherwise it will consist of a single [AlgebraicValue].
    ///
    pub fn project(&self, indexes: &[(usize, Option<&'static str>)]) -> Result<AlgebraicValue, InvalidFieldError> {
        let fields = match indexes {
            [(index, name)] => self.get_field(*index, *name)?.clone(),
            indexes => {
                let fields: Result<Vec<_>, _> = indexes
                    .iter()
                    .map(|(index, name)| self.get_field(*index, *name).cloned())
                    .collect();
                AlgebraicValue::Product(ProductValue::new(&fields?))
            }
        };

        Ok(fields)
    }

    /// This utility function is designed to project fields based on the supplied `indexes`.
    ///
    /// **Important:**
    ///
    /// The resulting [AlgebraicValue] will wrap into a [ProductValue] when projecting multiple
    /// fields, otherwise it will consist of a single [AlgebraicValue].
    ///
    /// **Parameters:**
    /// - `indexes`: A [NonEmpty<u32>] containing the indexes of fields to be projected.
    ///
    pub fn project_not_empty(&self, indexes: &NonEmpty<ColId>) -> Result<AlgebraicValue, InvalidFieldError> {
        let indexes: Vec<_> = indexes.iter().map(|x| (x.idx(), None)).collect();
        self.project(&indexes)
    }

    /// Extracts the `value` at field of `self` identified by `index`
    /// and then runs it through the function `f` which possibly returns a `T` derived from `value`.
    pub fn extract_field<'a, T>(
        &'a self,
        index: usize,
        name: Option<&'static str>,
        f: impl 'a + Fn(&'a AlgebraicValue) -> Option<T>,
    ) -> Result<T, InvalidFieldError> {
        f(self.get_field(index, name)?).ok_or(InvalidFieldError { col_pos: index, name })
    }

    /// Interprets the value at field of `self` identified by `index` as a `bool`.
    pub fn field_as_bool(&self, index: usize, named: Option<&'static str>) -> Result<bool, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_bool().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `u8`.
    pub fn field_as_u8(&self, index: usize, named: Option<&'static str>) -> Result<u8, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u8().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `u32`.
    pub fn field_as_u32(&self, index: usize, named: Option<&'static str>) -> Result<u32, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u32().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `u64`.
    pub fn field_as_u64(&self, index: usize, named: Option<&'static str>) -> Result<u64, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u64().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `i64`.
    pub fn field_as_i64(&self, index: usize, named: Option<&'static str>) -> Result<i64, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_i64().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `i128`.
    pub fn field_as_i128(&self, index: usize, named: Option<&'static str>) -> Result<i128, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_i128().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a `u128`.
    pub fn field_as_u128(&self, index: usize, named: Option<&'static str>) -> Result<u128, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_u128().copied())
    }

    /// Interprets the value at field of `self` identified by `index` as a string slice.
    pub fn field_as_str(&self, index: usize, named: Option<&'static str>) -> Result<&str, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_string()).map(|x| &**x)
    }

    /// Interprets the value at field of `self` identified by `index` as a byte slice.
    pub fn field_as_bytes(&self, index: usize, named: Option<&'static str>) -> Result<&[u8], InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_bytes())
    }

    /// Interprets the value at field of `self` identified by `index` as a array.
    pub fn field_as_array(&self, index: usize, named: Option<&'static str>) -> Result<&ArrayValue, InvalidFieldError> {
        self.extract_field(index, named, |f| f.as_array())
    }
}
