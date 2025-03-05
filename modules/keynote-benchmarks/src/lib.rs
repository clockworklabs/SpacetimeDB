use core::ops::AddAssign;
use spacetimedb::{log_stopwatch::LogStopwatch, rand::Rng, reducer, table, DbContext, ReducerContext, Table};

#[derive(Clone, Copy, Debug)]
#[table(name = position, public)]
#[repr(C)]
pub struct Position {
    #[primary_key]
    #[index(direct)]
    id: u32,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy, Debug)]
#[table(name = velocity, public)]
#[repr(C)]
pub struct Velocity {
    #[primary_key]
    #[index(direct)]
    id: u32,
    dx: f32,
    dy: f32,
    dz: f32,
}

impl AddAssign<Velocity> for Position {
    fn add_assign(&mut self, vel: Velocity) {
        self.x += vel.dx;
        self.y += vel.dy;
        self.z += vel.dz;
    }
}

#[reducer(init)]
fn init(ctx: &ReducerContext) {
    let _stopwatch = LogStopwatch::new("init");

    // Insert 10^6 randomized positions and velocities,
    // but with incrementing and corresponding ids.
    let db = ctx.db();
    let mut rng = ctx.rng();
    for id in 0..1_000_000 {
        let (x, y, z) = rng.gen();
        let (dx, dy, dz) = rng.gen();
        db.position().insert(Position { id, x, y, z });
        db.velocity().insert(Velocity { id, dx, dy, dz });
    }
}

#[reducer]
fn update_positions_by_collect(ctx: &ReducerContext) {
    let _stopwatch = LogStopwatch::new("update_positions_by_collect");

    let mut pos_vec = ctx.db.position().iter().collect::<Vec<_>>();
    let mut vel_vec = ctx.db.velocity().iter().collect::<Vec<_>>();

    pos_vec.sort_unstable_by_key(|pos| pos.id);
    vel_vec.sort_unstable_by_key(|vel| vel.id);

    for (pos, vel) in pos_vec.iter_mut().zip(&vel_vec) {
        *pos += *vel;
    }

    for pos in pos_vec {
        ctx.db.position().id().update(pos);
    }
}

#[reducer]
fn roundtrip(ctx: &ReducerContext) {
    // Warmup the index.
    let id = ctx.db().velocity().id();
    for x in 0..10_000 {
        id.find(x);
    }

    // Measures the hot latency.
    let _stopwatch = LogStopwatch::new("index_roundtrip");
    id.find(10_001);
}
