use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use tempdir::TempDir;

use spacetimedb::address::Address;
use spacetimedb::hash::hash_bytes;
use spacetimedb::host::host_controller::Scheduler;
use spacetimedb::host::instance_env::InstanceEnv;
use spacetimedb::host::tracelog::replay::replay_report;
use spacetimedb::protobuf::control_db::HostType;
use spacetimedb::worker_database_instance::WorkerDatabaseInstance;

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

    let identity = hash_bytes(b"This is a fake identity.");
    let address = Address::from_slice(&identity.as_slice()[0..16]);

    let wdi = WorkerDatabaseInstance::new(0, 0, HostType::Wasmer, false, identity, address, db_path, logger_path);

    let iv = InstanceEnv::new(wdi, Scheduler::dummy(), Default::default(), None);

    let tx = iv.worker_database_instance.relational_db.begin_tx();
    let trace_log = File::open(replay_file.to_str().unwrap()).unwrap();
    eprintln!("Replaying trace log: {:?}", trace_log);
    let mut reader = BufReader::new(trace_log);
    let (_, resp) = iv.tx.set(tx, || replay_report(&iv, &mut reader).unwrap());

    serde_json::to_writer(std::io::stdout().lock(), &resp).unwrap();
}
