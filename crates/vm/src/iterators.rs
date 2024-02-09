use crate::errors::ErrorVm;
use crate::rel_ops::RelOps;
use crate::relation::{MemTable, RelIter, RelValue};
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{Header, RowCount};

impl<'a> RelOps<'a> for RelIter<ProductValue> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        Ok(if self.pos == 0 {
            self.pos += 1;
            Some(RelValue::new(self.of.clone(), None))
        } else {
            None
        })
    }
}

impl<'a> RelOps<'a> for RelIter<MemTable> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue<'a>>, ErrorVm> {
        if self.pos < self.of.data.len() {
            let row = &self.of.data[self.pos];
            self.pos += 1;

            Ok(Some(RelValue::Projection(row.clone())))
        } else {
            Ok(None)
        }
    }
}
