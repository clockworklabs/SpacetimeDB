use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use spacetimedb::auth::idc;
use spacetimedb::db::{Config, Storage};
use spacetimedb_client_api::routes::database::DatabaseRoutes;
use spacetimedb_client_api::NodeDelegate;
use spacetimedb_lib::sats::product;
use spacetimedb_lib::{bsatn, Hash, Identity};
use spacetimedb_schema::def::reducer_signature_hash;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};
use tower::ServiceExt;

#[test]
#[serial_test::serial]
fn idc_receiver_runs_connection_lifecycle_without_jwt() {
    CompiledModule::compile("idc-auth-test", CompilationMode::Debug).with_module_async(
        Config {
            storage: Storage::Memory,
            page_pool_max_size: None,
        },
        |module| async move {
            let host = module.client.module();
            let (_, reducer, owning) = host.info.module_def.reducer_by_name_with_module("receive").unwrap();
            let signature_hash = reducer_signature_hash(
                &reducer.params(),
                &reducer.ok_return_type,
                &reducer.err_return_type,
                owning.typespace(),
            );
            let app = DatabaseRoutes::default()
                .into_router(module.env())
                .with_state(module.env());
            let clients_before = host.relational_db().connected_clients().unwrap();

            let call_idc =
                |sender: Identity, seq: u64, fail: bool, hash: Hash, status: StatusCode, error: &'static str| {
                    let app = app.clone();
                    let env = module.env();
                    let db_identity = module.db_identity;
                    async move {
                        let token = idc::sign(env.jwt_auth_provider(), sender, db_identity).unwrap();
                        let request = Request::builder()
                            .method("POST")
                            .uri(format!(
                                "/{}/call-from-database/receive?stream_id=1&seq={seq}&signature_hash={}",
                                db_identity.to_hex(),
                                hash.to_hex(),
                            ))
                            .header("Authorization", format!("Bearer {token}"))
                            .header("Content-Type", "application/octet-stream")
                            .body(Body::from(bsatn::to_vec(&product![fail]).unwrap()))
                            .unwrap();
                        let response = app.oneshot(request).await.unwrap();
                        let actual_status = response.status();
                        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                        let body = String::from_utf8_lossy(&body);
                        assert_eq!(actual_status, status, "{body}");
                        assert!(body.contains(error), "{body}");
                    }
                };

            let assert_counts = async |connected: u64, received: u64, disconnected: u64| {
                let app = app.clone();
                let db_identity = module.db_identity;
                // Use a normal HTTP call to verify ordinary clients still receive a JWT.
                let request = Request::builder()
                    .method("POST")
                    .uri(format!("/{}/call/assert_counts", db_identity.to_hex()))
                    .header("Content-Type", "application/json")
                    .body(Body::from(format!("[{connected},{received},{disconnected}]")))
                    .unwrap();
                let response = app.oneshot(request).await.unwrap();
                let status = response.status();
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
                assert_eq!(host.relational_db().connected_clients().unwrap(), clients_before);
            };

            // Rejection in client_connected must prevent delivery. Accepted requests
            // must disconnect after success, replay, or failure, without exposing a JWT.
            // Untrusted senders are rejected before the target reducer runs.
            call_idc(
                Identity::ZERO,
                1,
                false,
                signature_hash,
                StatusCode::FORBIDDEN,
                "untrusted database",
            )
            .await;
            assert_counts(0, 0, 0).await;

            // A trusted sender runs the reducer and disconnects cleanly afterward.
            call_idc(Identity::ONE, 1, false, signature_hash, StatusCode::OK, "").await;
            assert_counts(1, 1, 1).await;

            // A duplicate delivery replays the stored outcome without rerunning the reducer.
            call_idc(Identity::ONE, 1, false, signature_hash, StatusCode::OK, "").await;
            assert_counts(2, 1, 2).await;

            // A reducer-level rejection is terminal but still runs the connection lifecycle.
            call_idc(
                Identity::ONE,
                2,
                true,
                signature_hash,
                StatusCode::UNPROCESSABLE_ENTITY,
                "delivery rejected",
            )
            .await;
            assert_counts(3, 1, 3).await;

            // Changing the arguments cannot replace an already-recorded failure.
            // The reducer would succeed now, but this sequence must not run again.
            call_idc(
                Identity::ONE,
                2,
                false,
                signature_hash,
                StatusCode::UNPROCESSABLE_ENTITY,
                "delivery rejected",
            )
            .await;
            assert_counts(4, 1, 4).await;

            // A signature mismatch is recorded as a terminal outcome without reducer execution.
            call_idc(
                Identity::ONE,
                3,
                false,
                spacetimedb_lib::Hash::from_byte_array([0; 32]),
                StatusCode::UNPROCESSABLE_ENTITY,
                "",
            )
            .await;
            assert_counts(5, 1, 5).await;
        },
    );
}
