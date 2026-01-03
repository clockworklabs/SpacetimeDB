use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = entities)]
pub struct Entity {
    #[primary_key]
    pub id: i32,
}

#[table(name = positions)]
pub struct Position {
    #[primary_key]
    pub entity_id: i32,
    pub x: i32,
    pub y: i32,
}

#[table(name = velocities)]
pub struct Velocity {
    #[primary_key]
    pub entity_id: i32,
    pub vx: i32,
    pub vy: i32,
}

#[table(name = next_positions)]
pub struct NextPosition {
    #[primary_key]
    pub entity_id: i32,
    pub x: i32,
    pub y: i32,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.entities().insert(Entity { id: 1 });
    ctx.db.entities().insert(Entity { id: 2 });

    ctx.db.positions().insert(Position {
        entity_id: 1,
        x: 1,
        y: 0,
    });
    ctx.db.positions().insert(Position {
        entity_id: 2,
        x: 10,
        y: 0,
    });

    ctx.db.velocities().insert(Velocity {
        entity_id: 1,
        vx: 1,
        vy: 0,
    });
    ctx.db.velocities().insert(Velocity {
        entity_id: 2,
        vx: -2,
        vy: 3,
    });
}

#[spacetimedb::reducer]
pub fn step(ctx: &ReducerContext) {
    for p in ctx.db.positions().iter() {
        if let Some(v) = ctx.db.velocities().entity_id().find(p.entity_id) {
            let np = NextPosition {
                entity_id: p.entity_id,
                x: p.x + v.vx,
                y: p.y + v.vy,
            };

            if ctx.db.next_positions().entity_id().find(p.entity_id).is_some() {
                ctx.db.next_positions().entity_id().update(np);
            } else {
                ctx.db.next_positions().insert(np);
            }
        }
    }
}
