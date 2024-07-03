use core::hash::{Hash, Hasher};
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::relation::{ColExpr, ColExprRef, Header};
use spacetimedb_sats::bsatn::ser::BsatnError;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::{bsatn, impl_serialize, AlgebraicValue};
use spacetimedb_table::read_column::ReadColumn;
use spacetimedb_table::table::RowRef;
use std::borrow::Cow;
use std::sync::Arc;

/// RelValue represents either a reference to a row in a table,
/// a reference to an inserted row,
/// or an ephemeral row constructed during query execution.
///
/// A `RelValue` is the type generated/consumed by queries.
#[derive(Debug, Clone)]
pub enum RelValue<'a> {
    /// A reference to a row in a table.
    Row(RowRef<'a>),
    /// An ephemeral row made during query execution.
    Projection(ProductValue),
    /// A row coming directly from a collected update.
    ///
    /// This is really a row in a table, and not an actual projection.
    /// However, for (lifetime) reasons, we cannot (yet) keep it as a `RowRef<'_>`
    /// and must convert that into a `ProductValue`.
    ProjRef(&'a ProductValue),
}

impl Eq for RelValue<'_> {}

impl PartialEq for RelValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Projection(x), Self::Projection(y)) => x == y,
            (Self::ProjRef(x), Self::ProjRef(y)) => x == y,
            (Self::Row(x), Self::Row(y)) => x == y,
            (Self::Projection(x), Self::ProjRef(y)) | (Self::ProjRef(y), Self::Projection(x)) => x == *y,
            (Self::Row(x), Self::Projection(y)) | (Self::Projection(y), Self::Row(x)) => x == y,
            (Self::Row(x), Self::ProjRef(y)) | (Self::ProjRef(y), Self::Row(x)) => x == *y,
        }
    }
}

impl Hash for RelValue<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            // `x.hash(state)` and `x.to_product_value().hash(state)`
            // have the same effect on `state`.
            Self::Row(x) => x.hash(state),
            Self::Projection(x) => x.hash(state),
            Self::ProjRef(x) => x.hash(state),
        }
    }
}

impl_serialize!(['a] RelValue<'a>, (self, ser) => match self {
    Self::Row(row) => row.serialize(ser),
    Self::Projection(row) => row.serialize(ser),
    Self::ProjRef(row) => row.serialize(ser),
});

impl<'a> RelValue<'a> {
    /// Converts `self` into a [`ProductValue`]
    /// either by reading a value from a table,
    /// cloning the reference to a `ProductValue`,
    /// or consuming the owned product.
    pub fn into_product_value(self) -> ProductValue {
        match self {
            Self::Row(row) => row.to_product_value(),
            Self::Projection(row) => row,
            Self::ProjRef(row) => row.clone(),
        }
    }

    /// Converts `self` into a `Cow<'a, ProductValue>`
    /// either by reading a value from a table,
    /// passing the reference to a `ProductValue`,
    /// or consuming the owned product.
    pub fn into_product_value_cow(self) -> Cow<'a, ProductValue> {
        match self {
            Self::Row(row) => Cow::Owned(row.to_product_value()),
            Self::Projection(row) => Cow::Owned(row),
            Self::ProjRef(row) => Cow::Borrowed(row),
        }
    }

    /// Computes the number of columns in this value.
    pub fn num_columns(&self) -> usize {
        match self {
            Self::Row(row_ref) => row_ref.row_layout().product().elements.len(),
            Self::Projection(row) => row.elements.len(),
            Self::ProjRef(row) => row.elements.len(),
        }
    }

    /// Extends `self` with the columns in `other`.
    ///
    /// This will always cause `RowRef<'_>`s to be read out into [`ProductValue`]s.
    pub fn extend(self, other: RelValue<'a>) -> RelValue<'a> {
        let mut x: Vec<_> = self.into_product_value().elements.into();
        x.extend(other.into_product_value());
        RelValue::Projection(x.into())
    }

    /// Read the column at index `col`.
    ///
    /// Use `read_or_take_column` instead if you have ownership of `self`.
    pub fn read_column(&self, col: usize) -> Option<Cow<'_, AlgebraicValue>> {
        match self {
            Self::Row(row_ref) => AlgebraicValue::read_column(*row_ref, col).ok().map(Cow::Owned),
            Self::Projection(pv) => pv.elements.get(col).map(Cow::Borrowed),
            Self::ProjRef(pv) => pv.elements.get(col).map(Cow::Borrowed),
        }
    }

    /// Returns a column either at the index specified in `col`,
    /// or the column is the value that `col` holds.
    ///
    /// Panics if, for `ColExprRef::Col(col)`, the `col` is out of bounds of `self`.
    pub fn get(&'a self, col: ColExprRef<'a>) -> Cow<'a, AlgebraicValue> {
        match col {
            ColExprRef::Col(col) => self.read_column(col.idx()).unwrap(),
            ColExprRef::Value(x) => Cow::Borrowed(x),
        }
    }

    /// Reads or takes the column at `col`.
    /// Calling this method consumes the column at `col` for a `RelValue::Projection`,
    /// so it should not be called again for the same input.
    ///
    /// Panics if `col` is out of bounds of `self`.
    pub fn read_or_take_column(&mut self, col: usize) -> Option<AlgebraicValue> {
        match self {
            Self::Row(row_ref) => AlgebraicValue::read_column(*row_ref, col).ok(),
            Self::Projection(pv) => pv.elements.get_mut(col).map(AlgebraicValue::take),
            Self::ProjRef(pv) => pv.elements.get(col).cloned(),
        }
    }

    /// Turns `cols` into a product
    /// where a value in `cols` is taken directly from it and indices are taken from `self`.
    ///
    /// Panics on an index that is out of bounds of `self`.
    pub fn project_owned(mut self, cols: &[ColExpr]) -> ProductValue {
        cols.iter()
            .map(|col| match col {
                ColExpr::Col(col) => self.read_or_take_column(col.idx()).unwrap(),
                ColExpr::Value(x) => x.clone(),
            })
            .collect()
    }

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf` as if by [`Vec::extend`].
    ///
    /// This method will use a [`spacetimedb_table::bflatn_to_bsatn_fast_path::StaticBsatnLayout`]
    /// if one is available, and may therefore be faster than calling [`bsatn::to_writer`].
    pub fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        match self {
            RelValue::Row(row_ref) => row_ref.to_bsatn_extend(buf),
            RelValue::Projection(row) => bsatn::to_writer(buf, row),
            RelValue::ProjRef(row) => bsatn::to_writer(buf, row),
        }
    }
}

/// An in-memory table
// TODO(perf): Remove `Clone` impl.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemTable {
    pub head: Arc<Header>,
    pub data: Vec<ProductValue>,
    pub table_access: StAccess,
}

impl MemTable {
    pub fn new(head: Arc<Header>, table_access: StAccess, data: Vec<ProductValue>) -> Self {
        assert_eq!(
            head.fields.len(),
            data.first()
                .map(|pv| pv.elements.len())
                .unwrap_or_else(|| head.fields.len()),
            "number of columns in `header.len() != data.len()`"
        );
        Self {
            head,
            data,
            table_access,
        }
    }

    pub fn from_iter(head: Arc<Header>, data: impl IntoIterator<Item = ProductValue>) -> Self {
        Self {
            head,
            data: data.into_iter().collect(),
            table_access: StAccess::Public,
        }
    }
}
