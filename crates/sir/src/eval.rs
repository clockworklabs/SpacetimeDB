use crate::ast::{Sir, SirResult};
use crate::program::{DbCtx, Program};

/// Optimize, compile & run the [Expr]
#[tracing::instrument(skip_all)]
pub fn eval_sir<Db: DbCtx>(p: &mut Program<Db>, ast: Sir) -> SirResult {
    match ast {
        Sir::Value(x) => SirResult::Value(x),
        Sir::Pass => SirResult::Pass,
        Sir::Qir(query) => p.db.eval_query(query).into(),
        x => todo!("{x:?}"),
    }
}
