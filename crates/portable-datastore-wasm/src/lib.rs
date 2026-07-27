//! Wasm adapter for the portable module-test datastore.
//!
//! This crate is intentionally thin: `spacetimedb-portable-datastore` owns the
//! datastore semantics, and this crate only maps those operations to a JS/Wasm ABI.

use std::sync::atomic::{AtomicU32, Ordering};

use js_sys::{Array, Uint8Array};
use spacetimedb_lib::{bsatn, ConnectionId, Identity, RawModuleDef};
use spacetimedb_portable_datastore::{
    CommitMode, PortableDatastore, PortableDatastoreError, PortableTransaction, ValidatedAuth,
};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use wasm_bindgen::prelude::*;

static NEXT_DATASTORE_ID: AtomicU32 = AtomicU32::new(1);

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum WasmCommitMode {
    Normal,
    DropEventTableRows,
}

impl From<WasmCommitMode> for CommitMode {
    fn from(mode: WasmCommitMode) -> Self {
        match mode {
            WasmCommitMode::Normal => Self::Normal,
            WasmCommitMode::DropEventTableRows => Self::DropEventTableRows,
        }
    }
}

#[wasm_bindgen]
pub struct WasmValidatedAuth {
    sender: Identity,
    connection_id: Option<ConnectionId>,
}

#[wasm_bindgen]
impl WasmValidatedAuth {
    #[wasm_bindgen(getter, js_name = senderHex)]
    pub fn sender_hex(&self) -> String {
        self.sender.to_hex().to_string()
    }

    #[wasm_bindgen(getter, js_name = connectionIdHex)]
    pub fn connection_id_hex(&self) -> Option<String> {
        self.connection_id.map(|id| id.to_hex().to_string())
    }
}

impl From<ValidatedAuth> for WasmValidatedAuth {
    fn from(auth: ValidatedAuth) -> Self {
        Self {
            sender: auth.sender,
            connection_id: auth.connection_id,
        }
    }
}

#[wasm_bindgen]
pub struct WasmPortableDatastore {
    id: u32,
    inner: PortableDatastore,
}

#[wasm_bindgen]
impl WasmPortableDatastore {
    #[wasm_bindgen(constructor)]
    pub fn new(raw_module_def_bsatn: &[u8], module_identity_hex: &str) -> Result<WasmPortableDatastore, JsValue> {
        let raw = bsatn::from_slice::<RawModuleDef>(raw_module_def_bsatn).map_err(to_js_error)?;
        let module_identity = Identity::from_hex(module_identity_hex).map_err(to_js_error)?;
        let inner = PortableDatastore::from_module_def(raw, module_identity).map_err(to_js_error)?;
        Ok(Self {
            id: NEXT_DATASTORE_ID.fetch_add(1, Ordering::Relaxed),
            inner,
        })
    }

    #[wasm_bindgen(js_name = tableId)]
    pub fn table_id(&self, table_name: &str) -> Result<u32, JsValue> {
        self.inner.table_id(table_name).map(|id| id.0).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = indexId)]
    pub fn index_id(&self, index_name: &str) -> Result<u32, JsValue> {
        self.inner.index_id(index_name).map(|id| id.0).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = beginMutTx)]
    pub fn begin_mut_tx(&self) -> WasmPortableTransaction {
        WasmPortableTransaction {
            datastore_id: self.id,
            inner: Some(self.inner.begin_mut_tx()),
        }
    }

    #[wasm_bindgen(js_name = commitTx)]
    pub fn commit_tx(&self, tx: &mut WasmPortableTransaction, mode: WasmCommitMode) -> Result<(), JsValue> {
        self.check_tx(tx)?;
        let tx = tx.take()?;
        self.inner.commit_tx(tx, mode.into()).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = rollbackTx)]
    pub fn rollback_tx(&self, tx: &mut WasmPortableTransaction) -> Result<(), JsValue> {
        self.check_tx(tx)?;
        let tx = tx.take()?;
        self.inner.rollback_tx(tx).map_err(to_js_error)
    }

    pub fn reset(&mut self) -> Result<(), JsValue> {
        self.inner.reset().map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = tableRowCount)]
    pub fn table_row_count(&self, table_id: u32) -> Result<f64, JsValue> {
        self.inner
            .table_row_count(None, TableId(table_id))
            .map(|count| count as f64)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = tableRowCountTx)]
    pub fn table_row_count_tx(&self, tx: &WasmPortableTransaction, table_id: u32) -> Result<f64, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .table_row_count(Some(tx.inner_ref()?), TableId(table_id))
            .map(|count| count as f64)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = tableRowsBsatn)]
    pub fn table_rows_bsatn(&self, table_id: u32) -> Result<Array, JsValue> {
        self.inner
            .table_rows_bsatn(None, TableId(table_id))
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = tableRowsBsatnTx)]
    pub fn table_rows_bsatn_tx(&self, tx: &WasmPortableTransaction, table_id: u32) -> Result<Array, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .table_rows_bsatn(Some(tx.inner_ref()?), TableId(table_id))
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = indexScanPointBsatn)]
    pub fn index_scan_point_bsatn(&self, index_id: u32, point: &[u8]) -> Result<Array, JsValue> {
        self.inner
            .index_scan_point_bsatn(None, IndexId(index_id), point)
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = indexScanPointBsatnTx)]
    pub fn index_scan_point_bsatn_tx(
        &self,
        tx: &WasmPortableTransaction,
        index_id: u32,
        point: &[u8],
    ) -> Result<Array, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .index_scan_point_bsatn(Some(tx.inner_ref()?), IndexId(index_id), point)
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = indexScanRangeBsatn)]
    pub fn index_scan_range_bsatn(
        &self,
        index_id: u32,
        prefix: &[u8],
        prefix_elems: u16,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Array, JsValue> {
        self.inner
            .index_scan_range_bsatn(None, IndexId(index_id), prefix, ColId(prefix_elems), rstart, rend)
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = indexScanRangeBsatnTx)]
    pub fn index_scan_range_bsatn_tx(
        &self,
        tx: &WasmPortableTransaction,
        index_id: u32,
        prefix: &[u8],
        prefix_elems: u16,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Array, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .index_scan_range_bsatn(
                Some(tx.inner_ref()?),
                IndexId(index_id),
                prefix,
                ColId(prefix_elems),
                rstart,
                rend,
            )
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = insertBsatnGeneratedCols)]
    pub fn insert_bsatn_generated_cols(
        &self,
        tx: &mut WasmPortableTransaction,
        table_id: u32,
        row: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .insert_bsatn_generated_cols(tx.inner_mut()?, TableId(table_id), row)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = updateBsatnGeneratedCols)]
    pub fn update_bsatn_generated_cols(
        &self,
        tx: &mut WasmPortableTransaction,
        table_id: u32,
        index_id: u32,
        row: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .update_bsatn_generated_cols(tx.inner_mut()?, TableId(table_id), IndexId(index_id), row)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = deleteByRelBsatn)]
    pub fn delete_by_rel_bsatn(
        &self,
        tx: &mut WasmPortableTransaction,
        table_id: u32,
        relation: &[u8],
    ) -> Result<u32, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .delete_by_rel_bsatn(tx.inner_mut()?, TableId(table_id), relation)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = deleteByIndexScanPointBsatn)]
    pub fn delete_by_index_scan_point_bsatn(
        &self,
        tx: &mut WasmPortableTransaction,
        index_id: u32,
        point: &[u8],
    ) -> Result<u32, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .delete_by_index_scan_point_bsatn(tx.inner_mut()?, IndexId(index_id), point)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = deleteByIndexScanRangeBsatn)]
    pub fn delete_by_index_scan_range_bsatn(
        &self,
        tx: &mut WasmPortableTransaction,
        index_id: u32,
        prefix: &[u8],
        prefix_elems: u16,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .delete_by_index_scan_range_bsatn(
                tx.inner_mut()?,
                IndexId(index_id),
                prefix,
                ColId(prefix_elems),
                rstart,
                rend,
            )
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = clearTable)]
    pub fn clear_table(&self, tx: &mut WasmPortableTransaction, table_id: u32) -> Result<f64, JsValue> {
        self.check_tx(tx)?;
        self.inner
            .clear_table(tx.inner_mut()?, TableId(table_id))
            .map(|count| count as f64)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = validateJwtPayload)]
    pub fn validate_jwt_payload(&self, payload: &str, connection_id_hex: &str) -> Result<WasmValidatedAuth, JsValue> {
        let connection_id = ConnectionId::from_hex(connection_id_hex).map_err(to_js_error)?;
        self.inner
            .validate_jwt_payload(payload, connection_id)
            .map(Into::into)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = runQuery)]
    pub fn run_query(&self, sql: &str, database_identity_hex: &str) -> Result<Array, JsValue> {
        let database_identity = Identity::from_hex(database_identity_hex).map_err(to_js_error)?;
        self.inner
            .run_query_bsatn(sql, database_identity)
            .map(rows_to_js)
            .map_err(to_js_error)
    }

    fn check_tx(&self, tx: &WasmPortableTransaction) -> Result<(), JsValue> {
        if tx.datastore_id == self.id {
            Ok(())
        } else {
            Err(JsValue::from_str("transaction belongs to a different datastore"))
        }
    }
}

#[wasm_bindgen]
pub struct WasmPortableTransaction {
    datastore_id: u32,
    inner: Option<PortableTransaction>,
}

impl WasmPortableTransaction {
    fn inner_ref(&self) -> Result<&PortableTransaction, JsValue> {
        self.inner
            .as_ref()
            .ok_or_else(|| JsValue::from_str("transaction already finished"))
    }

    fn inner_mut(&mut self) -> Result<&mut PortableTransaction, JsValue> {
        self.inner
            .as_mut()
            .ok_or_else(|| JsValue::from_str("transaction already finished"))
    }

    fn take(&mut self) -> Result<PortableTransaction, JsValue> {
        self.inner
            .take()
            .ok_or_else(|| JsValue::from_str("transaction already finished"))
    }
}

fn rows_to_js(rows: Vec<Vec<u8>>) -> Array {
    rows.into_iter()
        .map(|row| JsValue::from(Uint8Array::from(row.as_slice())))
        .collect()
}

fn to_js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[allow(dead_code)]
fn _assert_error_is_send_sync(_: &PortableDatastoreError) {}
