//! Smoke run of the datastore DST target, so the harness keeps working.
//! Long campaigns run through the `spacetimedb-dst` binary instead.
//!
//! Set `DST_REPLAY=<file>` to replay a failure from a campaign.

use spacetimedb_dst_lib::datastore::Datastore;
use spacetimedb_dst_lib::harness::{run, Replay, RunOptions};

#[test]
fn datastore_smoke() {
    if let Ok(path) = std::env::var("DST_REPLAY") {
        let replay = Replay::read(path.as_ref()).expect("readable replay file");
        if let Err(failure) = run::<Datastore>(&RunOptions::from_replay(&replay)) {
            panic!("{failure}");
        }
        return;
    }

    for seed in [1, 2] {
        let options = RunOptions {
            check_determinism: true,
            ..RunOptions::new(seed, 200)
        };
        if let Err(failure) = run::<Datastore>(&options) {
            panic!("{failure}");
        }
    }
}
