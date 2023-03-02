use crate::errors::ErrorVm;
use crate::expr::SourceExpr;
use crate::rel_ops::RelOps;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{Header, MemTable, RelIter, RelValue, RowCount};

impl RelOps for RelIter<ProductValue> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        Ok(if self.pos == 0 {
            self.pos += 1;
            Some(RelValue::new(&self.head, &self.of))
        } else {
            None
        })
    }
}

impl RelOps for RelIter<SourceExpr> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        match &self.of {
            SourceExpr::Value(x) => Ok(if self.pos == 0 {
                self.pos += 1;
                Some(RelValue::new(&self.head, &(x.into())))
            } else {
                None
            }),
            SourceExpr::MemTable(x) => {
                if self.pos < x.data.len() {
                    self.pos += 1;
                    Ok(Some(RelValue::new(&self.head, &x.data[self.pos - 1])))
                } else {
                    Ok(None)
                }
            }
            SourceExpr::DbTable(_x) => {
                todo!()
            }
        }
    }
}

impl RelOps for RelIter<MemTable> {
    fn head(&self) -> &Header {
        &self.head
    }

    fn row_count(&self) -> RowCount {
        self.row_count
    }

    fn next(&mut self) -> Result<Option<RelValue>, ErrorVm> {
        if self.pos < self.of.data.len() {
            let row = &self.of.data[self.pos];
            self.pos += 1;

            Ok(Some(RelValue::new(self.head(), row)))
        } else {
            Ok(None)
        }
    }
}
