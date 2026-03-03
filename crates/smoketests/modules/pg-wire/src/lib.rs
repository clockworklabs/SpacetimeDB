use spacetimedb::sats::{i256, u256};
use spacetimedb::{ConnectionId, Identity, ReducerContext, SpacetimeType, Table, Timestamp, TimeDuration, Uuid};

#[derive(Copy, Clone)]
#[spacetimedb::table(accessor = t_ints, public)]
pub struct TInts {
    i8: i8,
    i16: i16,
    i32: i32,
    i64: i64,
    i128: i128,
    i256: i256,
}

#[spacetimedb::table(accessor = t_ints_tuple, public)]
pub struct TIntsTuple {
    tuple: TInts,
}

#[derive(Copy, Clone)]
#[spacetimedb::table(accessor = t_uints, public)]
pub struct TUints {
    u8: u8,
    u16: u16,
    u32: u32,
    u64: u64,
    u128: u128,
    u256: u256,
}

#[spacetimedb::table(accessor = t_uints_tuple, public)]
pub struct TUintsTuple {
    tuple: TUints,
}

#[derive(Clone)]
#[spacetimedb::table(accessor = t_others, public)]
pub struct TOthers {
    bool: bool,
    f32: f32,
    f64: f64,
    str: String,
    bytes: Vec<u8>,
    identity: Identity,
    connection_id: ConnectionId,
    timestamp: Timestamp,
    duration: TimeDuration,
    uuid: Uuid,
}

#[spacetimedb::table(accessor = t_others_tuple, public)]
pub struct TOthersTuple {
    tuple: TOthers
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub enum Action {
    Inactive,
    Active,
}

#[derive(SpacetimeType, Debug, Clone, Copy)]
pub enum Color {
    Gray(u8),
}

#[derive(Copy, Clone)]
#[spacetimedb::table(accessor = t_simple_enum, public)]
pub struct TSimpleEnum {
    id: u32,
    action: Action,
}

#[spacetimedb::table(accessor = t_enum, public)]
pub struct TEnum {
    id: u32,
    color: Color,
}

#[spacetimedb::table(accessor = t_nested, public)]
pub struct TNested {
   en: TEnum,
   se: TSimpleEnum,
   ints: TInts,
}

#[derive(Clone)]
#[spacetimedb::table(accessor = t_enums)]
pub struct TEnums {
    bool_opt: Option<bool>,
    bool_result: Result<bool, String>,
    action: Action,
}

#[spacetimedb::table(accessor = t_enums_tuple)]
pub struct TEnumsTuple {
    tuple: TEnums,
}

#[spacetimedb::reducer]
pub fn test(ctx: &ReducerContext) {
    let tuple = TInts {
        i8: -25,
        i16: -3224,
        i32: -23443,
        i64: -2344353,
        i128: -234434897853,
        i256: (-234434897853i128).into(),
    };
    let ints = tuple;
    ctx.db.t_ints().insert(tuple);
    ctx.db.t_ints_tuple().insert(TIntsTuple { tuple });

    let tuple = TUints {
        u8: 105,
        u16: 1050,
        u32: 83892,
        u64: 48937498,
        u128: 4378528978889,
        u256: 4378528978889u128.into(),
    };
    ctx.db.t_uints().insert(tuple);
    ctx.db.t_uints_tuple().insert(TUintsTuple { tuple });

    let tuple = TOthers {
        bool: true,
        f32: 594806.58906,
        f64: -3454353.345389043278459,
        str: "This is spacetimedb".to_string(),
        bytes: vec!(1, 2, 3, 4, 5, 6, 7),
        identity: Identity::ONE,
        connection_id: ConnectionId::ZERO,
        timestamp: Timestamp::UNIX_EPOCH,
        duration: TimeDuration::from_micros(1000 * 10000),
        uuid: Uuid::NIL,
    };
    ctx.db.t_others().insert(tuple.clone());
    ctx.db.t_others_tuple().insert(TOthersTuple { tuple });

    ctx.db.t_simple_enum().insert(TSimpleEnum { id: 1, action: Action::Inactive });
    ctx.db.t_simple_enum().insert(TSimpleEnum { id: 2, action: Action::Active });

    ctx.db.t_enum().insert(TEnum { id: 1, color: Color::Gray(128) });

    ctx.db.t_nested().insert(TNested {
        en: TEnum { id: 1, color: Color::Gray(128) },
        se: TSimpleEnum { id: 2, action: Action::Active },
        ints,
    });

    let tuple = TEnums {
        bool_opt: Some(true),
        bool_result: Ok(false),
        action: Action::Active,
    };

    ctx.db.t_enums().insert(tuple.clone());
    ctx.db.t_enums_tuple().insert(TEnumsTuple { tuple });
}
