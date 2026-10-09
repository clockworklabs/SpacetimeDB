mod remote_bindings;

use remote_bindings::game_world;

fn assert_remote_reducer<R: spacetimedb::rt::RemoteReducer>() {}

#[spacetimedb::reducer]
pub fn enqueue_ping(_ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, body: String) {
    let _target = game_world::Identity(target);
    let _args = game_world::ReceivePingArgs { target, body };
    assert_remote_reducer::<game_world::receive_ping>();
}
