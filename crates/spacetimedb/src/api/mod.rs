use crate::{
    db::object_db::ObjectDB,
    hash::Hash,
    identity::{alloc_spacetime_identity, decode_token, encode_token},
    postgres,
};
use lazy_static::lazy_static;
use std::sync::Mutex;
// use rocksdb;

/*
-x SpacetimeDB API
-x indexing of columns
-x atomic transactions (including creation of tables)
-x diff commits
- snapshot commits
-x SQL query -> API call support
-x Smart contract which calls APIs
- Schema migration
- Schema code generation to improve ergonomics of smart contract
- Metrics tables (data size, tx/sec)
- Dashboard for displaying metrics
-x (some way to upload smart contracts and track the versions to our website??)
-x subscription API (subscription queries)
- read permissions
- partial in-memory state
- (client library for syncing data based on queries??)
-x non-primitive type columns (e.g. struct in column)
*/

lazy_static! {
    pub static ref MODULE_ODB: Mutex<ObjectDB> = Mutex::new(ObjectDB::open("/stdb/module_odb").unwrap());
}

pub async fn spacetime_identity() -> Result<(Hash, String), anyhow::Error> {
    let identity = alloc_spacetime_identity().await?;
    let identity_token = encode_token(identity)?;
    Ok((identity, identity_token))
}

pub async fn spacetime_identity_associate_email(email: &str, identity_token: &str) -> Result<(), anyhow::Error> {
    let token = decode_token(identity_token)?;
    let hex_identity = token.claims.hex_identity;
    let client = postgres::get_client().await;
    client
        .query(
            "INSERT INTO registry.email (st_identity, email) VALUES ($1, $2)",
            &[&hex_identity, &email],
        )
        .await?;
    Ok(())
}

pub mod database {
    use crate::{
        hash::Hash,
        logs::{self, init_log},
        postgres,
        wasm_host::{self, get_host},
    };

    use super::MODULE_ODB;

    // TODO: Verify identity?
    pub async fn init_module(hex_identity: &str, name: &str, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let client = postgres::get_client().await;
        let result = client
            .query(
                "SELECT * from registry.module WHERE actor_name = $1 AND st_identity = $2",
                &[&name, &hex_identity],
            )
            .await;

        if let Ok(rows) = result {
            if rows.len() > 0 {
                return Err(anyhow::anyhow!("Cannot init existing actor."));
            }
        }

        let identity = *Hash::from_slice(&hex::decode(hex_identity).unwrap());
        let host = wasm_host::get_host();
        let address = host.init_module(identity, name.into(), wasm_bytes.clone()).await?;

        // If the module successfully initialized add it to the object database
        {
            let mut object_db = MODULE_ODB.lock().unwrap();
            object_db.add(wasm_bytes);
        }

        // Store this module metadata in postgres
        client.query(
            "INSERT INTO registry.module (actor_name, st_identity, module_version, module_address) VALUES ($1, $2, $3, $4)", 
            &[&name, &hex_identity, &0_i32, &hex::encode(address)]
        ).await?;

        init_log(address);

        Ok(address)
    }

    pub fn update(_identity: String, _name: String, _wasm_bytecode: impl AsRef<[u8]>) {
        unimplemented!()
    }

    pub async fn call(identity: &str, name: &str, reducer: String, arg_bytes: Vec<u8>) -> Result<(), anyhow::Error> {
        // TODO: optimize by loading all these into memory
        let client = postgres::get_client().await;
        let result = client
            .query_one(
                r"
            SELECT DISTINCT ON (actor_name, st_identity, module_version)
                actor_name, st_identity, module_version, module_address
            FROM registry.module
                WHERE actor_name = $1
                AND st_identity = $2
            ORDER BY module_version DESC;",
                &[&name, &identity],
            )
            .await;
        let row = result?;
        let module_address: String = row.get(3);
        let hash: Hash = Hash::from_iter(hex::decode(module_address).unwrap());

        get_host().call_reducer(hash, reducer, arg_bytes).await?;

        Ok(())
    }

    pub fn query(_identity: String, _name: String, _query: String) {
        unimplemented!()
    }

    pub async fn logs(identity: &str, name: &str, num_lines: u32) -> String {
        let client = postgres::get_client().await;
        let result = client
            .query(
                "SELECT (module_address) from registry.module WHERE actor_name = $1 AND st_identity = $2",
                &[&name, &identity],
            )
            .await;

        // TODO: actually handle errors
        let rows = result.unwrap();
        let row = rows.first().unwrap();
        let hex_address: String = row.get(0);

        let module_address = Hash::from_iter(hex::decode(hex_address).unwrap());
        logs::read_latest(module_address, num_lines).await
    }

    // Optional
    pub fn revert_ts(_identity: String, _name: String, _timestamp: u64) {
        unimplemented!()
    }

    pub fn revert_hash(_identity: String, _name: String, _hash: Hash) {
        unimplemented!()
    }

    pub fn address(_identity: String, _name: String) -> String {
        unimplemented!()
    }

    pub fn metrics(_identity: String, _name: String) {
        unimplemented!()
    }

    pub mod energy {
        pub fn info(_identity: String, _name: String) {
            unimplemented!()
        }

        pub fn buy(_identity: String, _name: String, _amount: u64) {
            unimplemented!()
        }
    }
}
