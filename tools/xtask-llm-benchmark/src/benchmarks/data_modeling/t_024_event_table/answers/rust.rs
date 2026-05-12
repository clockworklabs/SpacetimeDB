use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = damage_event, public, event)]
pub struct DamageEvent {
    pub entity_id: u64,
    pub damage: u32,
    pub source: String,
}

#[reducer]
fn deal_damage(ctx: &ReducerContext, entity_id: u64, damage: u32, source: String) {
    ctx.db.damage_event().insert(DamageEvent {
        entity_id,
        damage,
        source,
    });
}
