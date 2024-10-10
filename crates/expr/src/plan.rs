use crate::{expr::RelExpr, ty::TyCtx};

/// A relation expression [RelExpr] + a typing context [TyCtx]
pub struct LogicalPlan {
    /// The typing context for this expression
    pub ctx: TyCtx,
    /// The logical relational expression
    pub expr: RelExpr,
}
