use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use spacetimedb::host::scheduler::Scheduler;
use spacetimedb::Identity;
use tempdir::TempDir;

use spacetimedb::address::Address;
use spacetimedb::database_instance_context::DatabaseInstanceContext;
use spacetimedb::hash::hash_bytes;
use spacetimedb::host::instance_env::InstanceEnv;
use spacetimedb::host::tracelog::replay::replay_report;

pub fn main() {
    let args: Vec<_> = std::env::args().collect(); // get all arguments passed to app
    if args.len() != 2 {
        println!("{} <trace-file>", args[0]);
        return;
    }
    let replay_file = Path::new(args[1].as_str());
    let tmp_dir = TempDir::new("stdb_test").expect("establish tmpdir");
    let db_path = tmp_dir.path();
    let logger_path = tmp_dir.path();
    let scheduler_path = tmp_dir.path().join("scheduler");

    let identity = Identity::from_byte_array(hash_bytes(b"This is a fake identity.").data);
    let address = Address::from_slice(&identity.as_bytes()[..16]);

    let in_memory = false;

    let dbic = DatabaseInstanceContext::new(
        in_memory,
        0,
        0,
        false,
        identity,
        address,
        db_path.to_path_buf(),
        logger_path,
    );

    let iv = InstanceEnv::new(dbic, Scheduler::dummy(&scheduler_path), None);

    let tx = iv.dbic.relational_db.begin_tx();
    let trace_log = File::open(replay_file.to_str().unwrap()).unwrap();
    eprintln!("Replaying trace log: {:?}", trace_log);
    let mut reader = BufReader::new(trace_log);
    let (_, resp) = iv.tx.set(tx, || replay_report(&iv, &mut reader).unwrap());

    serde_json::to_writer(std::io::stdout().lock(), &resp).unwrap();
}
