use super::{
    message_log::MessageLog,
    relational_operators::Relation,
    transactional_db::{CommitResult, ScanIter, TransactionalDB, Tx, TxCtx, TxWrapper},
};
// use super::relational_operators::Project;
use crate::db::db_metrics::{
    RDB_CREATE_TABLE_TIME, RDB_DELETE_IN_TIME, RDB_DELETE_PK_TIME, RDB_DROP_TABLE_TIME, RDB_INSERT_TIME,
    RDB_SCAN_PK_TIME, RDB_SCAN_RAW_TIME, RDB_SCAN_TIME,
};
use crate::db::ostorage::ObjectDB;
use crate::error::{DBError, TableError};
use crate::util::prometheus_handle::HistogramVecHandle;
use fs2::FileExt;
use spacetimedb_lib::{
    buffer::{BufReader, DecodeError},
    ElementDef, PrimaryKey,
};
use spacetimedb_lib::{TupleDef, TupleValue, TypeDef, TypeValue};
use std::fs::File;
use std::{
    ops::{Deref, DerefMut, RangeBounds},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

pub const ST_TABLES_NAME: &str = "st_table";
pub const ST_COLUMNS_NAME: &str = "st_columns";

pub const ST_TABLES_ID: u32 = 0;
pub const ST_COLUMNS_ID: u32 = 1;

#[derive(Clone)]
pub struct RelationalDBWrapper {
    inner: Arc<Mutex<RelationalDB>>,
}

impl RelationalDBWrapper {
    pub fn new(relational_db: RelationalDB) -> Self {
        Self {
            inner: Arc::new(Mutex::new(relational_db)),
        }
    }

    pub fn lock(&self) -> Result<RelationalDBGuard, PoisonError<std::sync::MutexGuard<'_, RelationalDB>>> {
        match self.inner.lock() {
            Ok(inner) => Ok(RelationalDBGuard::new(inner)),
            Err(err) => Err(err),
        }
    }

    pub fn begin_tx(&self) -> TxWrapper<Self> {
        TxWrapper::begin(self.clone())
    }
}

impl TxCtx for RelationalDBWrapper {
    fn begin_tx_raw(&mut self) -> Tx {
        self.lock().unwrap().begin_tx_raw()
    }

    fn rollback_tx(&mut self, tx: Tx) {
        self.lock().unwrap().rollback_tx(tx)
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        self.lock().unwrap().commit_tx(tx)
    }
}

pub type WrapTxWrapper = TxWrapper<RelationalDBWrapper>;

pub struct RelationalDBGuard<'a> {
    inner: MutexGuard<'a, RelationalDB>,
}

impl<'a> RelationalDBGuard<'a> {
    fn new(inner: MutexGuard<'a, RelationalDB>) -> Self {
        log::trace!("LOCKING DB");
        Self { inner }
    }
}

impl<'a> Deref for RelationalDBGuard<'a> {
    type Target = MutexGuard<'a, RelationalDB>;

    fn deref(&self) -> &MutexGuard<'a, RelationalDB> {
        &self.inner
    }
}

impl<'a> DerefMut for RelationalDBGuard<'a> {
    fn deref_mut(&mut self) -> &mut MutexGuard<'a, RelationalDB> {
        &mut self.inner
    }
}

impl<'a> Drop for RelationalDBGuard<'a> {
    fn drop(&mut self) {
        log::trace!("UNLOCKING DB");
    }
}

pub struct RelationalDB {
    pub(crate) txdb: TransactionalDB,
    #[allow(dead_code)]
    lock: File,
}

impl RelationalDB {
    pub fn open(
        root: impl AsRef<Path>,
        message_log: Arc<Mutex<MessageLog>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<Self, DBError> {
        // Create tables that must always exist
        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        // NOTE: This is not used because currently the TransactionalDB doesn't
        // write anything to disk, however because this may happen in the future
        // I'm going to leave this.
        let root = root.as_ref().to_path_buf();
        let lock = File::create(root.join("db.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|err| DBError::DatabasedOpened(root, err.into()))?;

        let mut txdb = TransactionalDB::open(message_log, odb).unwrap();

        Self::bootstrap(&mut txdb)?;

        Ok(RelationalDB { txdb, lock })
    }

    fn bootstrap(txdb: &mut TransactionalDB) -> Result<(), DBError> {
        let mut tx_ = TxWrapper::begin(txdb);
        let (tx, txdb) = tx_.get();

        // Create the st_tables table and insert the information about itself into itself
        // schema: (table_id: u32, table_name: String)
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_TABLES_ID),
                TypeValue::String(ST_TABLES_NAME.to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_TABLES_ID, row);

        // Insert the st_columns table into st_tables
        // schema: (table_id: u32, col_id: u32, col_type: Bytes, col_name: String)
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::String(ST_COLUMNS_NAME.to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_TABLES_ID, row);

        // Insert information about st_tables into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_TABLES_ID),
                TypeValue::U32(0),
                TypeValue::Bytes(bytes),
                TypeValue::String("table_id".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::String.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_TABLES_ID),
                TypeValue::U32(1),
                TypeValue::Bytes(bytes),
                TypeValue::String("table_name".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        // Insert information about st_columns into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(0),
                TypeValue::Bytes(bytes),
                TypeValue::String("table_id".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(1),
                TypeValue::Bytes(bytes),
                TypeValue::String("col_id".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::Bytes.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(2),
                TypeValue::Bytes(bytes),
                TypeValue::String("col_type".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::String.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(3),
                TypeValue::Bytes(bytes),
                TypeValue::String("col_name".to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        tx_.commit()?;
        Ok(())
    }

    pub fn reset_hard(&mut self, message_log: Arc<Mutex<MessageLog>>) -> Result<(), DBError> {
        self.txdb.reset_hard(message_log)?;
        Ok(())
    }

    pub fn pk_for_row(row: &TupleValue) -> PrimaryKey {
        PrimaryKey {
            data_key: row.to_data_key(),
        }
    }

    pub fn encode_row(row: &TupleValue, bytes: &mut Vec<u8>) {
        // TODO: large file storage the row elements
        row.encode(bytes);
    }

    pub fn decode_row(schema: &TupleDef, bytes: &mut impl BufReader) -> Result<TupleValue, DecodeError> {
        // TODO: large file storage the row elements
        TupleValue::decode(schema, bytes)
    }

    pub fn schema_for_table(&self, tx: &mut Tx, table_id: u32) -> Result<Option<TupleDef>, DBError> {
        let mut columns = Vec::new();
        for bytes in self.txdb.scan(tx, ST_COLUMNS_ID) {
            let schema = TupleDef {
                name: None,
                elements: vec![
                    ElementDef {
                        tag: 0,
                        name: None,
                        element_type: TypeDef::U32,
                    },
                    ElementDef {
                        tag: 1,
                        name: None,
                        element_type: TypeDef::U32,
                    },
                    ElementDef {
                        tag: 2,
                        name: None,
                        element_type: TypeDef::Bytes,
                    },
                    ElementDef {
                        tag: 3,
                        name: None,
                        element_type: TypeDef::String,
                    },
                ],
            };

            let row = match Self::decode_row(&schema, &mut &bytes[..]) {
                Ok(row) => row,
                Err(e) => {
                    log::error!("schema_for_table: Table has invalid schema: {} Err: {}", table_id, e);
                    return Ok(None);
                }
            };
            let t_id = row.field_as_u32(0, Some("t_id"))?;
            if t_id != table_id {
                continue;
            }
            let col_id = row.field_as_u32(1, Some("col_id"))?;

            let bytes = row.field_as_bytes(2, Some("col_type"))?;
            let col_type =
                TypeDef::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

            let col_name = row.field_as_str(3, Some("col_name"))?;

            let element = ElementDef {
                // TODO: do we keep col_id's, do we keep tags for tuples?
                tag: col_id as u8,
                name: Some(col_name.into()),
                element_type: col_type,
            };
            columns.push(element)
        }
        columns.sort_by(|a, b| a.tag.cmp(&b.tag));

        Ok(if !columns.is_empty() {
            Some(TupleDef {
                name: None,
                elements: columns,
            })
        } else {
            None
        })
    }

    fn insert_row_raw(txdb: &mut TransactionalDB, tx: &mut Tx, table_id: u32, row: TupleValue) {
        let mut bytes = Vec::new();
        Self::encode_row(&row, &mut bytes);
        txdb.insert(tx, table_id, bytes);

        // https://stackoverflow.com/questions/43581810/how-postgresql-index-deals-with-mvcc
        // https://stackoverflow.com/questions/60361958/how-does-the-btree-index-of-postgresql-achieve-multi-version-concurrency-control
        // https://stackoverflow.com/questions/65053753/how-does-postgres-atomically-updates-secondary-indices
    }

    pub fn begin_tx(&mut self) -> TxWrapper<&mut Self> {
        TxWrapper::begin(self)
    }
}

impl TxCtx for RelationalDB {
    fn begin_tx_raw(&mut self) -> Tx {
        self.txdb.begin_tx_raw()
    }

    fn rollback_tx(&mut self, tx: Tx) {
        self.txdb.rollback_tx(tx)
    }

    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        self.txdb.commit_tx(tx)
    }
}

impl RelationalDB {
    pub fn create_table(&mut self, tx: &mut Tx, table_name: &str, schema: &TupleDef) -> Result<u32, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_CREATE_TABLE_TIME, vec![String::from(table_name)]);
        measure.start();
        // Scan st_tables for this id

        let mut table_count = 0;
        for row in self.scan(tx, ST_TABLES_ID)? {
            table_count += 1;
            let t_name = row.field_as_str(1, Some("name"))?;

            if t_name == table_name {
                return Err(TableError::Exist(t_name.into()).into());
            }
        }

        // TODO: we probably want to replace this with autoincrementing
        let table_id = table_count;

        // Insert the table row into st_tables
        let row = TupleValue {
            elements: vec![TypeValue::U32(table_id), TypeValue::String(table_name.to_string())].into(),
        };
        Self::insert_row_raw(&mut self.txdb, tx, ST_TABLES_ID, row);

        // Insert the columns into st_columns
        for (i, col) in schema.elements.iter().enumerate() {
            let mut bytes = Vec::new();
            col.element_type.encode(&mut bytes);
            let col_name = col.name.clone().ok_or_else(|| {
                // TODO: Maybe we should support Options as a special type
                // in TypeValue? Theoretically they could just be enums, but
                // that is quite a pain to use.
                TableError::ColumnWithoutName(table_name.into(), i as u32)
            })?;
            let row = TupleValue {
                elements: vec![
                    TypeValue::U32(table_id),
                    TypeValue::U32(i as u32),
                    TypeValue::Bytes(bytes),
                    TypeValue::String(col_name),
                ]
                .into(),
            };
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
        }

        Ok(table_id)
    }

    pub fn drop_table(&mut self, tx: &mut Tx, table_id: u32) -> Result<(), DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_DROP_TABLE_TIME, vec![format!("{}", table_id)]);
        measure.start();

        let range = self
            .range_scan(tx, ST_TABLES_ID, 0, TypeValue::U32(table_id)..TypeValue::U32(table_id))
            .map_err(|_err| TableError::NotFound("ST_TABLES_ID".into()))?
            .collect::<Vec<_>>();
        if self.delete_in(tx, table_id, range)?.is_none() {
            return Err(TableError::IdNotFound(table_id).into());
        }

        let range = self
            .range_scan(tx, ST_COLUMNS_ID, 0, TypeValue::U32(table_id)..TypeValue::U32(table_id))
            .map_err(|_err| TableError::NotFound("ST_COLUMNS_ID".into()))?
            .collect::<Vec<_>>();
        if self.delete_in(tx, table_id, range)?.is_none() {
            return Err(TableError::NotFound("ST_COLUMNS_ID".into()).into());
        }
        Ok(())
    }

    pub fn table_id_from_name(&self, tx: &mut Tx, table_name: &str) -> Result<Option<u32>, DBError> {
        for row in self.scan(tx, ST_TABLES_ID)? {
            let t_id = row.field_as_u32(0, Some("id"))?;
            let t_name = row.field_as_str(1, Some("name"))?;

            if t_name == table_name {
                return Ok(Some(t_id));
            }
        }
        Ok(None)
    }

    pub fn table_name_from_id(&self, tx: &mut Tx, table_id: u32) -> Result<Option<String>, DBError> {
        for row in self.scan(tx, ST_TABLES_ID)? {
            let t_id = row.field_as_u32(0, Some("id"))?;
            let t_name = row.field_as_str(1, Some("name"))?;

            if t_id == table_id {
                return Ok(Some(t_name.into()));
            }
        }
        Ok(None)
    }

    pub fn column_id_from_name(&self, tx: &mut Tx, table_id: u32, col_name: &str) -> Result<Option<u32>, DBError> {
        // schema: (table_id: u32, col_id: u32, col_type: Bytes, col_name: String)
        for row in self.scan(tx, ST_COLUMNS_ID)? {
            let t_id = row.field_as_u32(0, Some("table_id"))?;
            if t_id != table_id {
                continue;
            }
            let col_id = row.field_as_u32(1, Some("col_id"))?;

            let c_name = row.field_as_str(3, Some("c_name"))?;
            if c_name == col_name {
                return Ok(Some(col_id));
            }
        }
        Ok(None)
    }

    pub fn scan_pk<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Result<PrimaryKeyTableIter<'a>, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_SCAN_PK_TIME, vec![format!("{}", table_id)]);
        measure.start();
        let columns = self.schema_for_table(tx, table_id)?;
        if let Some(columns) = columns {
            Ok(PrimaryKeyTableIter {
                txdb_iter: self.txdb.scan(tx, table_id),
                schema: columns,
            })
        } else {
            Err(TableError::ScanPkTableIdNotFound(table_id).into())
        }
    }

    // AKA: iter
    pub fn scan<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Result<TableIter<'a>, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_SCAN_TIME, vec![format!("{}", table_id)]);
        measure.start();
        let columns = self.schema_for_table(tx, table_id)?;
        if let Some(columns) = columns {
            Ok(TableIter {
                txdb_iter: self.txdb.scan(tx, table_id),
                schema: columns,
            })
        } else {
            Err(TableError::ScanTableIdNotFound(table_id).into())
        }
    }

    pub fn scan_raw<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Result<TableIterRaw<'a>, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_SCAN_RAW_TIME, vec![format!("{}", table_id)]);
        measure.start();

        let columns = self.schema_for_table(tx, table_id)?;
        if columns.is_some() {
            Ok(TableIterRaw {
                txdb_iter: self.txdb.scan(tx, table_id),
            })
        } else {
            Err(TableError::ScanTableIdNotFound(table_id).into())
        }
    }

    pub fn pk_seek<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        primary_key: PrimaryKey,
    ) -> Result<Option<TupleValue>, DBError> {
        let schema = self.schema_for_table(tx, table_id)?;
        if let Some(schema) = schema {
            let bytes = self.txdb.seek(tx, table_id, primary_key.data_key);
            if let Some(bytes) = bytes {
                return match RelationalDB::decode_row(&schema, &mut &bytes[..]) {
                    Ok(row) => Ok(Some(row)),
                    Err(e) => {
                        log::error!("filter_pk: Row decode failure: {e}");
                        Err(TableError::DecodeSeekTableIdNotFound(table_id, e.into()).into())
                    }
                };
            }
        }
        Ok(None)
    }

    pub fn seek<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        value: TypeValue,
    ) -> Result<SeekIter, DBError> {
        let table_iter = self.scan(tx, table_id)?;
        Ok(SeekIter::Scan(ScanSeekIter {
            table_iter,
            col_index: col_id,
            value,
        }))
    }

    pub fn range_scan<'a, R: RangeBounds<TypeValue> + 'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Result<RangeIter<R>, DBError> {
        let table_iter = self.scan(tx, table_id)?;
        Ok(RangeIter::Scan(ScanRangeIter {
            table_iter,
            col_index: col_id,
            range,
        }))
    }

    pub fn insert(&mut self, tx: &mut Tx, table_id: u32, row: TupleValue) -> Result<(), DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_INSERT_TIME, vec![format!("{}", table_id)]);
        measure.start();

        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
        Ok(())
    }

    pub fn delete_pk(&mut self, tx: &mut Tx, table_id: u32, primary_key: PrimaryKey) -> Result<Option<bool>, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_DELETE_PK_TIME, vec![format!("{}", table_id)]);
        measure.start();

        // TODO: our use of options here doesn't seem correct, I think we might want to double up on options
        if self.pk_seek(tx, table_id, primary_key)?.is_some() {
            self.txdb.delete(tx, table_id, primary_key.data_key);
            return Ok(Some(true));
        }
        Ok(None)
    }

    pub fn delete_in<R: Relation>(&mut self, tx: &mut Tx, table_id: u32, relation: R) -> Result<Option<u32>, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_DELETE_IN_TIME, vec![format!("{}", table_id)]);
        measure.start();
        if self.schema_for_table(tx, table_id)?.is_none() {
            return Ok(None);
        }
        let mut count = 0;
        for tuple in relation {
            let data_key = tuple.to_data_key();

            // TODO: Think about if we need to verify that the key is in
            // the table before deleting
            if self.txdb.seek(tx, table_id, data_key).is_some() {
                count += 1;
                self.txdb.delete(tx, table_id, data_key);
            }
        }
        Ok(Some(count))
    }
}

pub struct PrimaryKeyTableIter<'a> {
    schema: TupleDef,
    txdb_iter: ScanIter<'a>,
}

impl<'a> Iterator for PrimaryKeyTableIter<'a> {
    type Item = PrimaryKey;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(bytes) = self.txdb_iter.next() {
            // TODO: for performance ditch the reading the row and read the primary key directly or something
            return match RelationalDB::decode_row(&self.schema, &mut &bytes[..]) {
                Ok(row) => Some(RelationalDB::pk_for_row(&row)),
                Err(e) => {
                    log::error!("PrimaryKeyTableIter::next: Failed to decode row! Err: {}", e);
                    None
                }
            };
        }
        None
    }
}

pub struct TableIter<'a> {
    schema: TupleDef,
    txdb_iter: ScanIter<'a>,
}

impl<'a> Iterator for TableIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(bytes) = self.txdb_iter.next() {
            return match RelationalDB::decode_row(&self.schema, &mut &bytes[..]) {
                Ok(row) => Some(row),
                Err(e) => {
                    log::error!("TableIter::next: Failed to decode row! Err: {}", e);
                    None
                }
            };
        }
        None
    }
}

pub struct TableIterRaw<'a> {
    txdb_iter: ScanIter<'a>,
}

impl<'a> Iterator for TableIterRaw<'a> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.txdb_iter.next()
    }
}

pub enum SeekIter<'a> {
    Scan(ScanSeekIter<'a>),
}

impl<'a> Iterator for SeekIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SeekIter::Scan(seek) => seek.next(),
        }
    }
}
pub struct ScanSeekIter<'a> {
    table_iter: TableIter<'a>,
    col_index: u32,
    value: TypeValue,
}

impl<'a> Iterator for ScanSeekIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        for row in &mut self.table_iter {
            let value = &row.elements[self.col_index as usize];
            if &self.value == value {
                return Some(row);
            }
        }
        None
    }
}

pub enum RangeIter<'a, R: RangeBounds<TypeValue>> {
    Scan(ScanRangeIter<'a, R>),
}

impl<'a, R: RangeBounds<TypeValue>> Iterator for RangeIter<'a, R> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RangeIter::Scan(range) => range.next(),
        }
    }
}
pub struct ScanRangeIter<'a, R: RangeBounds<TypeValue>> {
    table_iter: TableIter<'a>,
    col_index: u32,
    range: R,
}

impl<'a, R: RangeBounds<TypeValue>> Iterator for ScanRangeIter<'a, R> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        for row in &mut self.table_iter {
            let value = &row.elements[self.col_index as usize];
            if self.range.contains(value) {
                return Some(row);
            }
        }
        None
    }
}

#[cfg(test)]
pub(crate) mod tests_utils {
    use super::*;
    use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;

    pub(crate) fn make_default_ostorage(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>, DBError> {
        Ok(Box::new(HashMapObjectDB::open(path)?))
    }
}

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex};

    use crate::db::message_log::MessageLog;

    use super::RelationalDB;
    use crate::db::relational_db::tests_utils::make_default_ostorage;
    use crate::error::DBError;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::{ElementDef, TupleDef, TupleValue, TypeDef, TypeValue};
    use tempdir::TempDir;

    // let ptr = stdb.from(&mut tx, "health")
    //     .unwrap()
    //     .filter_eq("hp", ColValue::I32(0))
    //     .unwrap();
    // let row = stdb.from(&mut tx, "health").unwrap().row_at_pointer(ptr);

    // stdb.from(&mut tx, "health").unwrap().
    // stdb.commit_tx(tx);

    // let player = stdb.players.where_username_in_range("bob").to_owned();
    // player.username = "asdfadf"
    // stdb.players.update_where_id_is(0, player);

    //stdb.from(&mut tx, "health").join(&mut tx).unwrap().delete_eq("hp", );

    // let health = stdb!(from health where hp = 0 select * as Health);
    // health.set(2332);
    // stdb!(update health where hp = 0 set {health as *});

    #[test]
    fn test() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();
        stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;
        Ok(())
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog.clone(), odb.clone())?;
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let schema = TupleDef {
            name: None,
            elements: vec![ElementDef {
                tag: 0,
                name: Some("my_col".into()),
                element_type: TypeDef::I32,
            }],
        };

        stdb.create_table(tx, "MyTable", &schema)?;

        tx_.commit()?;
        //drop(stdb);
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));

        match RelationalDB::open(tmp_dir.path(), mlog, odb) {
            Ok(_) => {
                panic!("Allowed to open database twice")
            }
            Err(e) => match e {
                DBError::DatabasedOpened(_, _) => {}
                err => {
                    panic!("Failed with error {err}")
                }
            },
        }

        Ok(())
    }

    #[test]

    fn test_table_name() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();
        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;
        let t_id = stdb.table_id_from_name(tx, "MyTable")?;
        assert_eq!(t_id, Some(table_id));
        Ok(())
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();
        stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;
        let table_id = stdb.table_id_from_name(tx, "MyTable")?.unwrap();
        let col_id = stdb.column_id_from_name(tx, table_id, "my_col")?;
        assert_eq!(col_id, Some(0));
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();
        stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;
        let result = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        );
        assert!(matches!(result, Err(_)));
        Ok(())
    }

    #[test]
    fn test_pre_commit() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;

        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        )?;

        let mut rows = stdb
            .scan(tx, table_id)?
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb_ = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx_ = stdb_.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;

        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        )?;
        tx_.commit()?;

        let mut tx = stdb_.begin_tx();
        let (tx, stdb) = tx.get();
        let mut rows = stdb
            .scan(tx, table_id)?
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;

        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        )?;

        println!("{}", table_id);

        let mut rows = stdb
            .range_scan(tx, table_id, 0, TypeValue::I32(0)..)?
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb_ = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx_ = stdb_.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;

        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        )?;
        stdb.insert(
            tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        )?;
        tx_.commit()?;

        let mut tx = stdb_.begin_tx();
        let (tx, stdb) = tx.get();
        let mut rows = stdb
            .range_scan(tx, table_id, 0, TypeValue::I32(0)..)?
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;

        drop(tx);

        let mut tx = stdb.begin_tx();
        let (tx, stdb) = tx.get();
        let result = stdb.drop_table(tx, table_id);
        assert!(matches!(result, Err(_)));
        Ok(())
    }

    #[test]

    fn test_rollback() -> ResultTest<()> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let mut stdb_ = RelationalDB::open(tmp_dir.path(), mlog, odb)?;
        let mut tx_ = stdb_.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            &TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )?;
        tx_.commit()?;

        let mut tx_ = stdb_.begin_tx();
        let (tx, stdb) = tx_.get();
        stdb.insert(
            tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        )?;
        stdb.insert(
            tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        )?;
        stdb.insert(
            tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        )?;
        drop(tx_);

        let mut tx = stdb_.begin_tx();
        let (tx, stdb) = tx.get();
        let mut rows = stdb
            .scan(tx, table_id)?
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        let expected: Vec<i32> = Vec::new();
        assert_eq!(rows, expected);
        Ok(())
    }
}
