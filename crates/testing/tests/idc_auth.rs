use spacetimedb::auth::identity::ConnectionAuthCtx;
use spacetimedb::db::{Config, Storage};
use spacetimedb::host::{FunctionArgs, IdcReducerCallOutcome, ReducerOutcome};
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};

#[test]
#[serial_test::serial]
fn idc_receiver_authenticates_database_without_module_jwt() {
    CompiledModule::compile("idc-auth-test", CompilationMode::Debug).with_module_async(
        Config {
            storage: Storage::Memory,
            page_pool_max_size: None,
        },
        |module| async move {
            let host = module.client.module();
            let connection = ConnectionId::from_u128(1);
            host.call_identity_connected(
                ConnectionAuthCtx {
                    identity: Identity::ONE,
                    jwt_payload: None,
                },
                connection,
            )
            .await
            .unwrap();
            let result = host
                .call_idc_reducer(
                    Identity::ONE,
                    Some(connection),
                    "receive",
                    FunctionArgs::Nullary,
                    1,
                    1,
                    0,
                )
                .await
                .unwrap();
            let IdcReducerCallOutcome::Applied(result) = result else {
                panic!("first delivery must execute the reducer");
            };
            assert!(matches!(result.result.outcome, ReducerOutcome::Committed));
            host.call_identity_disconnected(Identity::ONE, connection)
                .await
                .unwrap();

            assert!(host
                .call_identity_connected(
                    ConnectionAuthCtx {
                        identity: Identity::ZERO,
                        jwt_payload: None,
                    },
                    ConnectionId::from_u128(2),
                )
                .await
                .is_err());
        },
    );
}
