use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
struct WorldPos {
    x: u32,
    y: u32,
}

#[derive(SpacetimeType)]
enum MoveIntent {
    Path(WorldPos),
    Position(WorldPos),
    Direction(u8),
    ChaseAttack(u32),
}

#[spacetimedb::table(
    accessor = move_state,
    index(accessor = by_intent_and_actor, btree(columns = [intent, actor_id]))
)]
struct MoveState {
    #[index(btree)]
    intent: MoveIntent,
    actor_id: u32,
}

#[spacetimedb::reducer]
fn filter_data_enum_indexes(ctx: &spacetimedb::ReducerContext) {
    let intent = MoveIntent::ChaseAttack(42);

    ctx.db.move_state().intent().filter(&intent).next();
    ctx.db
        .move_state()
        .by_intent_and_actor()
        .filter((&intent, 7u32))
        .next();
}

fn main() {}
