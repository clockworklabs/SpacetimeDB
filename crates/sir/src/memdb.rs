use crate::arena::Arena;
use crate::ast::{Qir, QirOp, RelId, SirResult};
use crate::errors::ErrorVm;
use crate::iterator::{RelIter, RelOps};
use crate::program::DbCtx;
use crate::table::TableGenerator;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::{MemTable, RowCount};

pub struct MemDb {
    tables: Arena<String, MemTable>,
}

impl MemDb {
    pub fn new() -> Self {
        Self { tables: Arena::new() }
    }

    pub fn add_table(&mut self, t: MemTable) -> usize {
        self.tables.add(t.head.table_name.clone(), t)
    }
}

impl DbCtx for MemDb {
    fn auth(&self) -> AuthCtx {
        AuthCtx::for_testing()
    }

    fn table_generator(&self, source: RelId, _root: QirOp) -> Result<TableGenerator<'_>, ErrorVm> {
        let id = match source {
            RelId::DbTable(_) => {
                unreachable!()
            }
            RelId::MemTable(id) => id,
        };
        let t = self.tables.get(id.idx()).unwrap();

        let t = TableGenerator {
            evaluate: None,
            iter: Box::new(RelIter::new(t.head.clone(), RowCount::exact(t.data.len()), t)),
        };

        Ok(t)
    }

    fn eval_query(&mut self, query: Qir) -> Result<SirResult, ErrorVm> {
        let source = self.table_generator(query.source, query.ops.first().clone())?;
        let table_access = *source.access();
        let mut r = build_query(source, query)?;
        let head = r.head().clone();

        let rows = r.collect_vec()?;

        Ok(SirResult::Table(MemTable::new(head, table_access, rows)))
    }
}

#[tracing::instrument(skip_all)]
pub fn build_query(mut result: TableGenerator, query: Qir) -> Result<TableGenerator, ErrorVm> {
    for q in query.ops {
        result = match q {
            QirOp::Scan => result,
            // QirOp::Project(cols) => {}
            // QirOp::ColSeek(_) => {}
            // QirOp::IndexSeek(_, _) => {}
            // QirOp::Join(_) => {}
            x => todo!("{x:?}"),
        }
    }

    Ok(result)
}
