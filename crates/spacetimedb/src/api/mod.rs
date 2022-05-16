use crate::{hash::Hash, identity::{alloc_spacetime_identity, encode_token, decode_token}, postgres};

pub async fn spacetime_identity() -> Result<(Hash, String), anyhow::Error> {
    let identity = alloc_spacetime_identity().await?;
    let identity_token = encode_token(identity)?;
    Ok((identity, identity_token))
}

pub async fn spacetime_identity_associate_email(email: &str, identity_token: &str) -> Result<(), anyhow::Error> {
    let token = decode_token(identity_token)?;
    let hex_identity = token.claims.hex_identity;
    let client = postgres::get_client().await;
    client.query("INSERT INTO registry.email (st_identity, email) VALUES ($1)", &[&hex_identity, &email]).await?;
    Ok(())
}

pub mod database {
    use crate::{
        db::persistent_object_db::odb,
        hash::Hash,
        postgres,
        wasm_host::{self, get_host}, logs::init_log,
    };

    // TODO: Verify identity?
    pub async fn init_module(identity: String, name: String, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let client = postgres::get_client().await;
        let result = client
            .query(
                "SELECT * from registry.module WHERE actor_name = $1 AND st_identity = $2",
                &[&name, &identity],
            )
            .await;

        if let Ok(rows) = result {
            if rows.len() > 0 {
                return Err(anyhow::anyhow!("Cannot init existing actor."));
            }
        }

        let host = wasm_host::get_host();
        let address = host.init_module(wasm_bytes.clone()).await?;

        // If the module successfully initialized add it to the object database
        odb::add(&wasm_bytes).await;

        // Store this module metadata in postgres
        client.query(
            "INSERT INTO registry.module (actor_name, st_identity, module_version, module_address) VALUES ($1, $2, $3, $4)", 
            &[&name, &identity, &0_i32, &hex::encode(address)]
        ).await?;

        init_log(address);

        Ok(address)
    }

    pub fn update(_identity: String, name: String, wasm_bytecode: impl AsRef<[u8]>) {
        unimplemented!()
    }

    pub async fn call(
        identity: String,
        name: String,
        reducer: String,
        _arg_data: Vec<u8>, // TODO
    ) -> Result<(), anyhow::Error> {
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

        get_host().call_reducer(hash, reducer).await?;

        Ok(())
    }
    
    pub fn query(_identity: String, name: String, query: String) {
        unimplemented!()
    }

    pub fn logs(_identity: String, name: String) -> String {
        unimplemented!()
    }

    // Optional
    pub fn revert_ts(_identity: String, name: String, timestamp: u64) {
        unimplemented!()
    }

    pub fn revert_hash(_identity: String, name: String, hash: Hash) {
        unimplemented!()
    }

    pub fn address(_identity: String, name: String) -> String {
        unimplemented!()
    }

    pub fn metrics(_identity: String, name: String) {
        unimplemented!()
    }

    pub mod energy {
        pub fn info(_identity: String, name: String) {
            unimplemented!()
        }

        pub fn buy(_identity: String, name: String, amount: u64) {
            unimplemented!()
        }
    }
}
