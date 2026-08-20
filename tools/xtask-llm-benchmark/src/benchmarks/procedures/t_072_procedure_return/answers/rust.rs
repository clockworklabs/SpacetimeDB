use spacetimedb::{procedure, ProcedureContext, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Summary {
    pub total: u32,
    pub label: String,
}

#[procedure]
pub fn calculate_summary(_ctx: &mut ProcedureContext, lhs: u32, rhs: u32) -> Summary {
    Summary {
        total: lhs + rhs,
        label: "calculated".into(),
    }
}
