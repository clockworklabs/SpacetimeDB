use spacetimedb::delete_range;
use spacetimedb::println;
use spacetimedb::spacetimedb;
use spacetimedb::ReducerContext;
use spacetimedb::Timestamp;
use spacetimedb::TypeValue;

#[spacetimedb(table)]
pub struct TestA {
    pub x: u32,
    pub y: u32,
    pub z: String,
}

#[spacetimedb(tuple)]
pub struct TestB {
    foo: String,
}

// #[spacetimedb(migrate)]
// pub fn migrate() {}

#[spacetimedb(init)]
pub fn init() {
    spacetimedb::schedule!("1000ms", repeating_test(_, Timestamp::now()));
}

#[spacetimedb(reducer, repeat = 1000ms)]
pub fn repeating_test(ctx: ReducerContext, prev_time: Timestamp) {
    let delta_time = prev_time.elapsed();
    println!("Timestamp: {:?}, Delta time: {:?}", ctx.timestamp, delta_time);
}

#[spacetimedb(reducer)]
pub fn test(ctx: ReducerContext, arg: TestA, arg2: TestB) -> anyhow::Result<()> {
    println!("BEGIN");
    println!("sender: {:?}", ctx.sender);
    println!("timestamp: {:?}", ctx.timestamp);
    println!("bar: {:?}", arg2.foo);

    for i in 0..10 {
        TestA::insert(TestA {
            x: i + arg.x,
            y: i + arg.y,
            z: "Yo".to_owned(),
        });
    }

    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }

    println!("Row count before delete: {:?}", row_count);

    delete_range(1, 0, TypeValue::U32(5)..TypeValue::U32(10))?;

    let mut row_count = 0;
    for _row in TestA::iter() {
        row_count += 1;
    }

    println!("Row count after delete: {:?}", row_count);
    println!("END");
    Ok(())
}

#[spacetimedb(connect)]
fn on_connect(_ctx: ReducerContext) {}
