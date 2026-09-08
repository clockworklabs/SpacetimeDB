use super::*;
use crate::{host::extract_schema, messages::control_db::HostType};
use spacetimedb_lib::{
    bsatn,
    db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section},
    RawModuleDef,
};
use spacetimedb_schema::def::RawModuleDefVersion;

#[test]
fn bundled_bytes_match_current_v10_schema_wire_format() {
    let expected = RawModuleDef::V10(RawModuleDefV10 {
        sections: vec![
            RawModuleDefV10Section::Typespace(Default::default()),
            RawModuleDefV10Section::Capabilities(vec!["hosted_auth_v1".into()]),
        ],
    });
    assert_eq!(bsatn::to_vec(&expected).unwrap(), include_bytes!("v1.schema.bsatn"));
    let program = program(VERSION_1).unwrap();
    assert_eq!(program.hash, hash_bytes(include_bytes!("v1.wasm")));
    assert_eq!(program.hash, VERSION_1_PROGRAM_HASH);
    assert!(matches_program(VERSION_1, &program));
}

#[test]
fn recognition_requires_exact_version_kind_hash_and_bytes() {
    assert!(program(0).is_none());
    assert!(program(2).is_none());
    let mut candidate = program(VERSION_1).unwrap();
    assert!(!matches_program(2, &candidate));
    candidate.kind = ModuleKind::JS;
    assert!(!matches_program(VERSION_1, &candidate));
    candidate.kind = ModuleKind::WASM;
    candidate.hash = Hash::ZERO;
    assert!(!matches_program(VERSION_1, &candidate));
    candidate = program(VERSION_1).unwrap();
    candidate.bytes[0] ^= 1;
    assert!(!matches_program(VERSION_1, &candidate));
    candidate.hash = hash_bytes(&candidate.bytes);
    assert!(!matches_program(VERSION_1, &candidate));
    assert!(!matches_program(VERSION_1, &Program::empty(ModuleKind::WASM)));
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_wasm_host_loads_the_bundled_empty_v10_schema() {
    let module = extract_schema(program(VERSION_1).unwrap().bytes, HostType::Wasm)
        .await
        .unwrap();
    assert_eq!(module.raw_module_def_version(), RawModuleDefVersion::V10);
    assert!(module.supports_hosted_auth_v1());
    assert!(module.tables().next().is_none());
    assert!(module.reducers().next().is_none());
    assert!(module.procedures().next().is_none());
    assert!(module.views().next().is_none());
    assert!(module.typespace().types.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn actual_host_initializes_a_database_with_the_bundled_program() {
    use crate::{
        db::{persistence::LocalPersistenceProvider, Config, Storage},
        energy::NullEnergyMonitor,
        host::{FunctionArgs, HostController, HostRuntimeConfig, ProgramStorage},
        messages::control_db::Database,
        util::jobs::JobCores,
    };
    use spacetimedb_lib::Identity;
    use spacetimedb_paths::{server::ServerDataDir, FromPathUnchecked};
    use std::{sync::Arc, time::Duration};

    let directory = tempfile::tempdir().unwrap();
    let data_dir = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let storage: ProgramStorage = Arc::new(|hash| async move {
        Ok(program(VERSION_1)
            .filter(|program| program.hash == hash)
            .map(|program| program.bytes))
    });
    let controller = HostController::new(
        data_dir.clone(),
        Config {
            storage: Storage::Memory,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        storage,
        Arc::new(NullEnergyMonitor),
        Arc::new(LocalPersistenceProvider::new(data_dir)),
        JobCores::without_pinned_cores(),
    );
    let database = Database {
        id: 0xe001,
        database_identity: Identity::from_u256(0xe001_u32.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Wasm,
        initial_program: v1_hash(),
    };
    let module = controller
        .get_or_launch_module_host(database.clone(), 0xe001)
        .await
        .unwrap();
    let stored = module
        .relational_db()
        .program()
        .unwrap()
        .expect("host initialization must persist the actual program");
    assert!(matches_program(VERSION_1, &stored));
    let metadata = module
        .relational_db()
        .metadata()
        .unwrap()
        .expect("database must be initialized");
    assert_eq!(metadata.program_hash, v1_hash());
    assert_eq!(metadata.database_identity, database.database_identity);
    assert_eq!(metadata.owner_identity, database.owner_identity);
    assert!(module.info.module_def.supports_hosted_auth_v1());
    assert!(module.info.module_def.tables().next().is_none());
    assert!(module
        .call_reducer(Identity::ONE, None, None, None, None, "init", FunctionArgs::Nullary)
        .await
        .is_err());
    drop(module);
    controller
        .exit_module_host(0xe001, Duration::from_secs(5))
        .await
        .unwrap();
}
