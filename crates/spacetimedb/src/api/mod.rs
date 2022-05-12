pub fn sign_up(username: String, password: String) {}

pub fn sign_in(username: String, password: String) {}

pub mod database {
    use crate::{db::persistent_object_db::odb, hash::Hash, wasm_host::HOST};
    use std::error::Error;

    pub async fn init_module(
        identity: String,
        name: String,
        wasm_bytes: Vec<u8>,
    ) -> Result<Hash, Box<dyn Error + Send + Sync>> {
        let host = { HOST.lock().unwrap().clone() };
        let address = host.init_module(identity, name, wasm_bytes.clone()).await?;

        // If the module successfully initialized add it to the object database
        odb::add(&wasm_bytes).await;

        Ok(address)
    }

    pub fn update(namespace: String, name: String, wasm_bytecode: impl AsRef<[u8]>) {}

    pub fn logs(namespace: String, name: String) {}

    pub fn revert_ts(namespace: String, name: String, timestamp: u64) {}

    pub fn revert_hash(namespace: String, name: String, hash: Hash) {}

    pub fn query(namespace: String, name: String, query: String) {}

    pub async fn call(
        namespace: String,
        name: String,
        reducer: String,
        arg_data: Vec<u8>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    pub fn address(namespace: String, name: String) -> String {
        unimplemented!()
    }

    pub fn metrics(namespace: String, name: String) {}

    pub mod energy {
        pub fn info(namespace: String, name: String) {}

        pub fn buy(namespace: String, name: String, amount: u64) {}
    }
}
