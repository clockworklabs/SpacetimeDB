use crate::ast::{Qir, QirOp, RelId, SirResult};
use crate::env::Env;
use crate::errors::ErrorVm;
use crate::memdb::MemDb;
use crate::table::TableGenerator;
use spacetimedb_lib::identity::AuthCtx;

pub trait DbCtx {
    fn auth(&self) -> AuthCtx;
    fn table_generator(&self, source: RelId, root: QirOp) -> Result<TableGenerator<'_>, ErrorVm>;

    fn eval_query(&mut self, query: Qir) -> Result<SirResult, ErrorVm>;
}

/// The program executor
pub struct Program<Db: DbCtx> {
    pub(crate) env: Env,
    pub(crate) db: Db,
}

impl<Db: DbCtx> Program<Db> {
    pub fn new(db: Db) -> Self {
        Self { env: Env::new(), db }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ast::{QirOp, RelId};
    use crate::eval::eval_sir;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::{product, AlgebraicType, ProductType};
    use spacetimedb_slang::dsl::{mem_table, scalar};

    #[test]
    fn test_db_query() -> ResultTest<()> {
        let mut db = MemDb::new();
        let inv = ProductType::from([(None, AlgebraicType::I32), (Some("0_0"), AlgebraicType::I32)]);

        let t = mem_table(inv, product!(scalar(1), scalar(1)));
        db.add_table(t);

        let mut p = Program::new(db);

        let q = Qir::new(RelId::MemTable(0.into()), QirOp::Scan);

        let result = eval_sir(&mut p, q.into());

        Ok(())
    }
}
