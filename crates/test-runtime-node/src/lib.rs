use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use spacetimedb_lib::bsatn::ToBsatn;
use spacetimedb_lib::{bsatn, Identity, ProductValue, RawModuleDef};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_test_datastore::{TestDatastore, TestDatastoreError, TestTransaction};

#[napi]
pub struct NativeContext {
    raw_module_def: Vec<u8>,
    inner: Mutex<NativeContextInner>,
}

struct NativeContextInner {
    datastore: TestDatastore,
    txs: HashMap<u32, TestTransaction>,
    next_tx: u32,
}

#[napi]
impl NativeContext {
    #[napi(js_name = "reset")]
    pub fn reset(&self) -> Result<()> {
        let datastore = datastore_from_raw(&self.raw_module_def)?;
        let mut inner = self.inner.lock().map_err(lock_err)?;
        inner.datastore = datastore;
        inner.txs.clear();
        inner.next_tx = 1;
        Ok(())
    }

    #[napi(js_name = "tableId")]
    pub fn table_id(&self, name: String) -> Result<u32> {
        let inner = self.inner.lock().map_err(lock_err)?;
        Ok(inner.datastore.table_id(&name).map_err(to_napi_err)?.0)
    }

    #[napi(js_name = "indexId")]
    pub fn index_id(&self, name: String) -> Result<u32> {
        let inner = self.inner.lock().map_err(lock_err)?;
        Ok(inner.datastore.index_id(&name).map_err(to_napi_err)?.0)
    }

    #[napi(js_name = "beginTx")]
    pub fn begin_tx(&self) -> Result<u32> {
        let mut inner = self.inner.lock().map_err(lock_err)?;
        let id = inner.next_tx;
        inner.next_tx = inner.next_tx.checked_add(1).ok_or_else(|| Error::from_reason("transaction id overflow"))?;
        let tx = inner.datastore.begin_mut_tx();
        inner.txs.insert(id, tx);
        Ok(id)
    }

    #[napi(js_name = "commitTx")]
    pub fn commit_tx(&self, tx_id: u32) -> Result<()> {
        let mut inner = self.inner.lock().map_err(lock_err)?;
        let tx = inner
            .txs
            .remove(&tx_id)
            .ok_or_else(|| Error::from_reason(format!("unknown transaction id {tx_id}")))?;
        tx.commit().map_err(to_napi_err)
    }

    #[napi(js_name = "abortTx")]
    pub fn abort_tx(&self, tx_id: u32) -> Result<()> {
        let mut inner = self.inner.lock().map_err(lock_err)?;
        if let Some(tx) = inner.txs.remove(&tx_id) {
            tx.rollback().map_err(to_napi_err)?;
        }
        Ok(())
    }

    #[napi(js_name = "tableRowCount")]
    pub fn table_row_count(&self, tx_id: Option<u32>, table_id: u32) -> Result<u32> {
        self.with_target(tx_id, |target| {
            let count = match target {
                Target::Datastore(ds) => ds.table_row_count(TableId(table_id))?,
                Target::Transaction(tx) => tx.table_row_count(TableId(table_id))?,
            };
            u32::try_from(count).map_err(|_| Error::from_reason("row count exceeds u32"))
        })?
    }

    #[napi(js_name = "tableRows")]
    pub fn table_rows(&self, tx_id: Option<u32>, table_id: u32) -> Result<Vec<Buffer>> {
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds.table_rows_bsatn(TableId(table_id)).map(buffers).map_err(to_napi_err),
            Target::Transaction(tx) => tx.table_rows_bsatn(TableId(table_id)).map(buffers).map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "insertBsatn")]
    pub fn insert_bsatn(&self, tx_id: Option<u32>, table_id: u32, row: Buffer) -> Result<Buffer> {
        self.with_target(tx_id, |target| {
            let generated = match target {
                Target::Datastore(ds) => ds.insert_bsatn_generated_cols(TableId(table_id), &row)?,
                Target::Transaction(tx) => tx.insert_bsatn_generated_cols(TableId(table_id), &row)?,
            };
            Ok(Buffer::from(generated))
        })?
    }

    #[napi(js_name = "deleteAllByEqBsatn")]
    pub fn delete_all_by_eq_bsatn(&self, tx_id: Option<u32>, table_id: u32, relation: Buffer) -> Result<u32> {
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds.delete_by_rel_bsatn(TableId(table_id), &relation).map_err(to_napi_err),
            Target::Transaction(tx) => tx.delete_by_rel_bsatn(TableId(table_id), &relation).map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "indexScanPointBsatn")]
    pub fn index_scan_point_bsatn(&self, tx_id: Option<u32>, index_id: u32, point: Buffer) -> Result<Vec<Buffer>> {
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds.index_scan_point_bsatn(IndexId(index_id), &point).map(buffers).map_err(to_napi_err),
            Target::Transaction(tx) => tx.index_scan_point_bsatn(IndexId(index_id), &point).map(buffers).map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "indexScanRangeBsatn")]
    pub fn index_scan_range_bsatn(
        &self,
        tx_id: Option<u32>,
        index_id: u32,
        prefix: Buffer,
        prefix_elems: u32,
        rstart_len: u32,
        rend_len: u32,
    ) -> Result<Vec<Buffer>> {
        let prefix_elems = u16::try_from(prefix_elems)
            .map(ColId)
            .map_err(|_| Error::from_reason("prefix_elems exceeds u16"))?;
        let (prefix, rstart, rend) = split_range_buffer(&prefix, prefix_len, rstart_len, rend_len)?;
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds
                .index_scan_range_bsatn(IndexId(index_id), prefix, prefix_elems, rstart, rend)
                .map(buffers)
                .map_err(to_napi_err),
            Target::Transaction(tx) => tx
                .index_scan_range_bsatn(IndexId(index_id), prefix, prefix_elems, rstart, rend)
                .map(buffers)
                .map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "deleteByIndexScanPointBsatn")]
    pub fn delete_by_index_scan_point_bsatn(&self, tx_id: Option<u32>, index_id: u32, point: Buffer) -> Result<u32> {
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds.delete_by_index_scan_point_bsatn(IndexId(index_id), &point).map_err(to_napi_err),
            Target::Transaction(tx) => tx.delete_by_index_scan_point_bsatn(IndexId(index_id), &point).map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "deleteByIndexScanRangeBsatn")]
    pub fn delete_by_index_scan_range_bsatn(
        &self,
        tx_id: Option<u32>,
        index_id: u32,
        prefix: Buffer,
        prefix_elems: u32,
        rstart_len: u32,
        rend_len: u32,
    ) -> Result<u32> {
        let prefix_elems = u16::try_from(prefix_elems)
            .map(ColId)
            .map_err(|_| Error::from_reason("prefix_elems exceeds u16"))?;
        let (prefix, rstart, rend) = split_range_buffer(&prefix, prefix_len, rstart_len, rend_len)?;
        self.with_target(tx_id, |target| match target {
            Target::Datastore(ds) => ds
                .delete_by_index_scan_range_bsatn(IndexId(index_id), prefix, prefix_elems, rstart, rend)
                .map_err(to_napi_err),
            Target::Transaction(tx) => tx
                .delete_by_index_scan_range_bsatn(IndexId(index_id), prefix, prefix_elems, rstart, rend)
                .map_err(to_napi_err),
        })?
    }

    #[napi(js_name = "updateBsatn")]
    pub fn update_bsatn(&self, tx_id: Option<u32>, table_id: u32, index_id: u32, row: Buffer) -> Result<Buffer> {
        self.with_target(tx_id, |target| {
            let generated = match target {
                Target::Datastore(ds) => ds.update_bsatn_generated_cols(TableId(table_id), IndexId(index_id), &row)?,
                Target::Transaction(tx) => tx.update_bsatn_generated_cols(TableId(table_id), IndexId(index_id), &row)?,
            };
            Ok(Buffer::from(generated))
        })?
    }

    #[napi(js_name = "clearTable")]
    pub fn clear_table(&self, tx_id: Option<u32>, table_id: u32) -> Result<u32> {
        self.with_target(tx_id, |target| {
            let count = match target {
                Target::Datastore(ds) => ds.clear_table(TableId(table_id))?,
                Target::Transaction(tx) => tx.clear_table(TableId(table_id))?,
            };
            u32::try_from(count).map_err(|_| Error::from_reason("clear count exceeds u32"))
        })?
    }

    #[napi(js_name = "runQuery")]
    pub fn run_query(&self, sql: String, database_identity: String) -> Result<Vec<Buffer>> {
        let identity = Identity::from_str(&database_identity).map_err(|err| Error::from_reason(err.to_string()))?;
        let inner = self.inner.lock().map_err(lock_err)?;
        let rows = inner.datastore.run_select_query(&sql, identity).map_err(to_napi_err)?;
        rows.into_iter()
            .map(|row| row.to_bsatn_vec().map(Buffer::from).map_err(|err| Error::from_reason(err.to_string())))
            .collect()
    }

    fn with_target<T>(&self, tx_id: Option<u32>, f: impl FnOnce(Target<'_>) -> Result<T>) -> Result<Result<T>> {
        let inner = self.inner.lock().map_err(lock_err)?;
        let result = match tx_id {
            Some(id) => {
                let tx = inner
                    .txs
                    .get(&id)
                    .ok_or_else(|| Error::from_reason(format!("unknown transaction id {id}")))?;
                f(Target::Transaction(tx))
            }
            None => f(Target::Datastore(&inner.datastore)),
        };
        Ok(result)
    }
}

enum Target<'a> {
    Datastore(&'a TestDatastore),
    Transaction(&'a TestTransaction),
}

#[napi(js_name = "createContext")]
pub fn create_context(module_def: Buffer, module_identity: String) -> Result<NativeContext> {
    let _ = Identity::from_str(&module_identity).map_err(|err| Error::from_reason(err.to_string()))?;
    let raw_module_def = module_def.to_vec();
    let datastore = datastore_from_raw(&raw_module_def)?;
    Ok(NativeContext {
        raw_module_def,
        inner: Mutex::new(NativeContextInner {
            datastore,
            txs: HashMap::new(),
            next_tx: 1,
        }),
    })
}

#[napi(js_name = "validateJwtPayload")]
pub fn validate_jwt_payload(jwt_payload: String) -> Result<String> {
    let claims: serde_json::Value = serde_json::from_str(&jwt_payload).map_err(|err| Error::from_reason(err.to_string()))?;
    validate_test_jwt_claims(&claims).map(|identity| identity.to_hex())
}

fn datastore_from_raw(raw: &[u8]) -> Result<TestDatastore> {
    let raw = bsatn::from_slice::<RawModuleDef>(raw).map_err(|err| Error::from_reason(err.to_string()))?;
    TestDatastore::from_module_def(raw).map_err(to_napi_err)
}

fn validate_test_jwt_claims(claims: &serde_json::Value) -> Result<Identity> {
    let issuer = required_claim(claims, "iss")?;
    let subject = required_claim(claims, "sub")?;

    if issuer.len() > 128 {
        return Err(Error::from_reason(format!("Issuer too long: {issuer:?}")));
    }
    if subject.len() > 128 {
        return Err(Error::from_reason(format!("Subject too long: {subject:?}")));
    }
    if issuer.is_empty() {
        return Err(Error::from_reason("Issuer empty"));
    }
    if subject.is_empty() {
        return Err(Error::from_reason("Subject empty"));
    }

    let computed_identity = Identity::from_claims(issuer, subject);
    if let Some(token_identity) = claims.get("hex_identity") {
        let token_identity: Identity = serde_json::from_value(token_identity.clone())
            .map_err(|err| Error::from_reason(format!("invalid hex_identity claim: {err}")))?;
        if token_identity != computed_identity {
            return Err(Error::from_reason(format!(
                "Identity mismatch: token identity {token_identity:?} does not match computed identity {computed_identity:?}",
            )));
        }
    }

    Ok(computed_identity)
}

fn required_claim<'a>(claims: &'a serde_json::Value, claim: &str) -> Result<&'a str> {
    claims
        .get(claim)
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::from_reason(format!("missing required JWT claim `{claim}`")))
}

fn buffers(rows: Vec<Vec<u8>>) -> Vec<Buffer> {
    rows.into_iter().map(Buffer::from).collect()
}

fn split_range_buffer(
    buf: &[u8],
    prefix_len: u32,
    rstart_len: u32,
    rend_len: u32,
) -> Result<(&[u8], &[u8], &[u8])> {
    let prefix_len = usize::try_from(prefix_len).map_err(|_| Error::from_reason("prefix_len exceeds usize"))?;
    let rstart_len = usize::try_from(rstart_len).map_err(|_| Error::from_reason("rstart_len exceeds usize"))?;
    let rend_len = usize::try_from(rend_len).map_err(|_| Error::from_reason("rend_len exceeds usize"))?;
    let rstart_end = prefix_len
        .checked_add(rstart_len)
        .ok_or_else(|| Error::from_reason("range buffer length overflow"))?;
    let rend_end = rstart_end
        .checked_add(rend_len)
        .ok_or_else(|| Error::from_reason("range buffer length overflow"))?;
    if rend_end > buf.len() {
        return Err(Error::from_reason("range buffer shorter than encoded lengths"));
    }
    Ok((&buf[..prefix_len], &buf[prefix_len..rstart_end], &buf[rstart_end..rend_end]))
}

fn to_napi_err(err: TestDatastoreError) -> Error {
    Error::from_reason(err.to_string())
}

fn lock_err<T>(_: T) -> Error {
    Error::from_reason("native test runtime lock poisoned")
}
