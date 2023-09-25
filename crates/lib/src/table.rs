use crate::ColumnIndexAttribute;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, SatsString};

#[derive(Clone)]
pub struct ColumnDef {
    pub column: ProductTypeElement,
    pub attr: ColumnIndexAttribute,
    pub pos: usize,
}

/// Describe the columns + meta attributes
/// TODO(cloutiertyler): This type should be deprecated and replaced with
/// ColumnDef or ColumnSchema where appropriate
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProductTypeMeta {
    pub columns: Vec<ProductTypeElement>,
    pub attr: Vec<ColumnIndexAttribute>,
}

impl ProductTypeMeta {
    pub fn new(columns: ProductType) -> Self {
        Self {
            attr: vec![ColumnIndexAttribute::UNSET; columns.elements.len()],
            columns: columns.elements.into(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            attr: Vec::with_capacity(capacity),
            columns: Vec::with_capacity(capacity),
        }
    }

    pub fn clear(&mut self) {
        self.columns.clear();
        self.attr.clear();
    }

    pub fn push(&mut self, name: SatsString, ty: AlgebraicType, attr: ColumnIndexAttribute) {
        self.columns.push(ProductTypeElement::new(ty, Some(name)));
        self.attr.push(attr);
    }

    /// Removes the data at position `index` and returns it.
    ///
    /// # Panics
    ///
    /// If `index` is out of bounds.
    pub fn remove(&mut self, index: usize) -> (ProductTypeElement, ColumnIndexAttribute) {
        (self.columns.remove(index), self.attr.remove(index))
    }

    /// Return mutable references to the data at position `index`, or `None` if
    /// the index is out of bounds.
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut ProductTypeElement, &mut ColumnIndexAttribute)> {
        self.columns
            .get_mut(index)
            .and_then(|pte| self.attr.get_mut(index).map(|attr| (pte, attr)))
    }

    pub fn with_attributes(iter: impl Iterator<Item = (ProductTypeElement, ColumnIndexAttribute)>) -> Self {
        let mut columns = Vec::new();
        let mut attrs = Vec::new();
        for (col, attr) in iter {
            columns.push(col);
            attrs.push(attr);
        }
        Self { attr: attrs, columns }
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = ColumnDef> + '_ {
        self.columns
            .iter()
            .zip(self.attr.iter())
            .enumerate()
            .map(|(pos, (column, attr))| ColumnDef {
                column: column.clone(),
                attr: *attr,
                pos,
            })
    }

    pub fn with_defaults<'a>(
        &'a self,
        row: &'a mut ProductValue,
    ) -> impl Iterator<Item = (ColumnDef, &'a mut AlgebraicValue)> + 'a {
        self.iter()
            .zip(row.elements.iter_mut())
            .filter(|(col, _)| col.attr.is_autoinc())
    }
}
