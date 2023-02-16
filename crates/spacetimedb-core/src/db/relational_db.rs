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
use crate::db::index::{IndexId, IndexIter};
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::sequence::{
    read_sled_i64, write_sled_i64, Sequence, SequenceDef, SequenceError, SequenceId, SequenceIter,
};
use crate::db::table::{
    columns_schema, decode_columns_schema, decode_table_schema, table_schema, ColumnFields, TableDef, TableFields,
};
use crate::db::{index, sequence};
use crate::error::{DBError, IndexError, TableError};
use crate::util::prometheus_handle::HistogramVecHandle;
use crate::{db::catalog::Catalog, util::ResultInspectExt};
use fs2::FileExt;
use spacetimedb_lib::{
    buffer::{BufReader, DecodeError},
    data_key::ToDataKey,
    ElementDef, PrimaryKey,
};
use spacetimedb_lib::{TupleDef, TupleValue, TypeDef, TypeValue};
use spacetimedb_sats::product;
use std::fs::File;
use std::{
    ops::{Deref, DerefMut, RangeBounds},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

pub const ST_TABLES_NAME: &str = "st_table";
pub const ST_COLUMNS_NAME: &str = "st_columns";
pub const ST_SEQUENCES_NAME: &str = "st_sequence";
pub const ST_INDEXES_NAME: &str = "st_indexes";

/// The static ID of the table that defines tables
pub const ST_TABLES_ID: u32 = 0;
/// The static ID of the table that defines columns
pub const ST_COLUMNS_ID: u32 = 1;
/// The ID that we can start use to generate user tables that will not conflict with the bootstrapped ones.
pub const ST_TABLE_ID_START: u32 = 2;

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
    pub(crate) seqdb: sled::Db,
    pub(crate) catalog: Catalog,
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
            .map_err(|err| DBError::DatabasedOpened(root.clone(), err.into()))?;

        let seqdb = sled::open(root.join("seq"))?;

        let mut txdb = TransactionalDB::open(message_log, odb).unwrap();

        Self::bootstrap(&mut txdb)?;

        let mut db = RelationalDB {
            txdb,
            seqdb,
            catalog: Catalog::new()?,
            lock,
        };
        //Add the system tables to the catalog
        db.catalog
            .tables
            .insert(TableDef::new(ST_TABLES_ID, ST_TABLES_NAME, table_schema(), true));
        db.catalog
            .tables
            .insert(TableDef::new(ST_COLUMNS_ID, ST_COLUMNS_NAME, columns_schema(), true));
        //Re-load the database objects
        db.bootstrap_sequences()?;
        db.bootstrap_indexes()?;
        Ok(db)
    }

    fn bootstrap(txdb: &mut TransactionalDB) -> Result<(), DBError> {
        let mut tx_ = TxWrapper::begin(txdb);
        let (tx, txdb) = tx_.get();

        // Create the st_tables table and insert the information about itself into itself
        // schema: (table_id: u32, table_name: String)
        let row = product![
            TypeValue::U32(ST_TABLES_ID),
            TypeValue::String(ST_TABLES_NAME.to_string()),
        ];
        Self::insert_row_raw(txdb, tx, ST_TABLES_ID, row);

        // Insert the st_columns table into st_tables
        // schema: (table_id: u32, col_id: u32, col_type: Bytes, col_name: String)
        let row = product![
            TypeValue::U32(ST_COLUMNS_ID),
            TypeValue::String(ST_COLUMNS_NAME.to_string()),
        ];
        Self::insert_row_raw(txdb, tx, ST_TABLES_ID, row);

        // Insert information about st_tables into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_TABLES_ID),
            TypeValue::U32(0),
            TypeValue::Bytes(bytes),
            TypeValue::String("table_id".to_string()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::String.encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_TABLES_ID),
            TypeValue::U32(1),
            TypeValue::Bytes(bytes),
            TypeValue::String("table_name".to_string()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        // Insert information about st_columns into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_COLUMNS_ID),
            TypeValue::U32(ColumnFields::TableId as u32),
            TypeValue::Bytes(bytes),
            TypeValue::String(ColumnFields::TableId.into()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_COLUMNS_ID),
            TypeValue::U32(ColumnFields::ColId as u32),
            TypeValue::Bytes(bytes),
            TypeValue::String(ColumnFields::ColId.into()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::bytes().encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_COLUMNS_ID),
            TypeValue::U32(ColumnFields::ColType as u32),
            TypeValue::Bytes(bytes),
            TypeValue::String(ColumnFields::ColType.into()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::String.encode(&mut bytes);
        let row = product![
            TypeValue::U32(ST_COLUMNS_ID),
            TypeValue::U32(ColumnFields::ColName as u32),
            TypeValue::Bytes(bytes),
            TypeValue::String(ColumnFields::ColName.into()),
        ];
        Self::insert_row_raw(txdb, tx, ST_COLUMNS_ID, row);

        tx_.commit()?;
        Ok(())
    }

    fn bootstrap_sequences(&mut self) -> Result<(), DBError> {
        let mut tx_ = self.begin_tx();
        let (tx, stdb) = tx_.get();

        if stdb.table_id_from_name(tx, ST_SEQUENCES_NAME)?.is_none() {
            stdb.create_system_table(tx, ST_SEQUENCES_NAME, sequence::internal_schema())?;
        };
        tx_.commit()?;

        let mut tx_ = self.begin_tx();
        let (tx, stdb) = tx_.get();
        // Re-fill
        let mut sequences = Vec::from_iter(stdb.catalog.sequences_iter().cloned());
        for seq in stdb.scan_sequences(tx)? {
            sequences.push(seq);
        }

        for seq in sequences {
            stdb.load_sequence(seq)?;
        }

        Ok(())
    }

    /// Loads the indexes into the [Catalog], reloading all the indexes
    fn bootstrap_indexes(&mut self) -> Result<(), DBError> {
        let mut tx_ = self.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = if let Some(table_id) = stdb.table_id_from_name(tx, ST_INDEXES_NAME)? {
            table_id
        } else {
            stdb.create_system_table(tx, ST_INDEXES_NAME, index::internal_schema())?
        };
        tx_.commit()?;

        let mut tx_ = self.begin_tx();
        let (tx, stdb) = tx_.get();

        let mut indexes = index::IndexCatalog::new(table_id);
        indexes.index_all(stdb, tx)?;

        stdb.catalog.indexes = indexes;

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
        // TODO: large file storage of the row elements
        row.encode(bytes);
    }

    pub fn decode_row<'a>(schema: &TupleDef, bytes: &mut impl BufReader<'a>) -> Result<TupleValue, DecodeError> {
        // TODO: large file storage of the row elements
        TupleValue::decode(schema, bytes)
    }

    pub fn schema_for_table(&self, tx: &mut Tx, table_id: u32) -> Result<Option<TupleDef>, DBError> {
        let mut columns = Vec::new();
        let schema = columns_schema();
        for bytes in self.txdb.scan(tx, ST_COLUMNS_ID) {
            let row = match Self::decode_row(&schema, &mut &bytes[..]) {
                Ok(row) => row,
                Err(e) => {
                    log::error!("schema_for_table: Table has invalid schema: {} Err: {}", table_id, e);
                    return Ok(None);
                }
            };
            let col = decode_columns_schema(&row)?;
            if col.table_id != table_id {
                continue;
            }

            let element = ElementDef {
                name: Some(col.col_name.into()),
                algebraic_type: col.col_type,
            };
            columns.push((col.col_id, element))
        }
        // TODO: better way to do this?
        columns.sort_by_key(|(col_id, _)| *col_id);
        let elements: Vec<_> = columns.into_iter().map(|(_, el)| el).collect();

        Ok(if !elements.is_empty() {
            Some(TupleDef { elements })
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

    /// NOTE! This does not store any actual data to disk. It only verifies the
    /// transaction can be committed and returns a commit result if it can
    /// or a None if you should retry the transaction again.
    fn commit_tx(&mut self, tx: Tx) -> Result<Option<CommitResult>, DBError> {
        self.txdb.commit_tx(tx)
    }
}

impl RelationalDB {
    /// Persist to disk the [Tx] result into the [MessageLog].
    ///
    /// Returns `true` if `commit_result` was persisted, `false` if it not has `bytes` to write or was `None`.
    pub fn persist_tx(mlog: &Mutex<MessageLog>, commit_result: Option<CommitResult>) -> Result<bool, DBError> {
        if let Some(commit_result) = commit_result {
            if let Some(bytes) = commit_result.commit_bytes {
                let mut mlog = mlog.lock()?;
                mlog.append(bytes)?;
                mlog.sync_all()?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn internal_create_table(
        &mut self,
        tx: &mut Tx,
        table_name: &str,
        schema: TupleDef,
        is_system_table: bool,
    ) -> Result<u32, DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_CREATE_TABLE_TIME, vec![String::from(table_name)]);
        measure.start();
        // Scan st_tables for this id
        for row in self.scan(tx, ST_TABLES_ID)? {
            let table = decode_table_schema(&row)?;
            if table.table_name == table_name {
                return Err(TableError::Exist(table_name.into()).into());
            }
        }

        let table_id = self.next_sequence(self.catalog.seq_table_id())? as u32;
        assert!(
            table_id > ST_TABLE_ID_START,
            "Attempted to create a table that overrides the ID of a system catalog!"
        );

        // Insert the table row into st_tables
        let row = product![TypeValue::U32(table_id), TypeValue::String(table_name.to_string())];
        Self::insert_row_raw(&mut self.txdb, tx, ST_TABLES_ID, row);

        // Insert the columns into st_columns
        for (i, col) in schema.elements.iter().enumerate() {
            let mut bytes = Vec::new();
            col.algebraic_type.encode(&mut bytes);
            let col_name = col.name.clone().ok_or_else(|| {
                // TODO: Maybe we should support Options as a special type
                // in TypeValue? Theoretically they could just be enums, but
                // that is quite a pain to use.
                TableError::ColumnWithoutName(table_name.into(), i as u32)
            })?;
            let row = product![
                TypeValue::U32(table_id),
                TypeValue::U32(i as u32),
                TypeValue::Bytes(bytes),
                TypeValue::String(col_name.clone()),
            ];
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
        }
        self.catalog
            .tables
            .insert(TableDef::new(table_id, table_name, schema, is_system_table));

        Ok(table_id)
    }

    pub fn create_system_table(&mut self, tx: &mut Tx, table_name: &str, schema: TupleDef) -> Result<u32, DBError> {
        assert!(table_name.starts_with("st_"), "Is not a system table");
        self.internal_create_table(tx, table_name, schema, true)
    }

    pub fn create_table(&mut self, tx: &mut Tx, table_name: &str, schema: TupleDef) -> Result<u32, DBError> {
        if table_name.starts_with("st_") {
            return Err(TableError::System(table_name.into()).into());
        }
        self.internal_create_table(tx, table_name, schema, false)
    }

    pub fn drop_table(&mut self, tx: &mut Tx, table_id: u32) -> Result<(), DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_DROP_TABLE_TIME, vec![format!("{}", table_id)]);
        measure.start();

        let range = self
            .range_scan(
                tx,
                ST_TABLES_ID,
                TableFields::TableId as u32,
                TypeValue::U32(table_id)..TypeValue::U32(table_id),
            )
            .map_err(|_err| TableError::NotFound("ST_TABLES_ID".into()))?
            .collect::<Vec<_>>();
        if self.delete_in(tx, table_id, range)?.is_none() {
            return Err(TableError::IdNotFound(table_id).into());
        }

        let range = self
            .range_scan(
                tx,
                ST_COLUMNS_ID,
                ColumnFields::TableId as u32,
                TypeValue::U32(table_id)..TypeValue::U32(table_id),
            )
            .map_err(|_err| TableError::NotFound("ST_COLUMNS_ID".into()))?
            .collect::<Vec<_>>();
        if self.delete_in(tx, table_id, range)?.is_none() {
            return Err(TableError::NotFound("ST_COLUMNS_ID".into()).into());
        }
        self.catalog.tables.remove(table_id);
        Ok(())
    }

    pub fn table_id_from_name(&self, tx: &mut Tx, table_name: &str) -> Result<Option<u32>, DBError> {
        for row in self.scan(tx, ST_TABLES_ID)? {
            let table = decode_table_schema(&row)?;

            if table.table_name == table_name {
                return Ok(Some(table.table_id));
            }
        }
        Ok(None)
    }

    pub fn table_name_from_id(&self, tx: &mut Tx, table_id: u32) -> Result<Option<String>, DBError> {
        for row in self.scan(tx, ST_TABLES_ID)? {
            let table = decode_table_schema(&row)?;

            if table.table_id == table_id {
                return Ok(Some(table.table_name.into()));
            }
        }
        Ok(None)
    }

    pub fn column_id_from_name(&self, tx: &mut Tx, table_id: u32, col_name: &str) -> Result<Option<u32>, DBError> {
        for row in self.scan(tx, ST_COLUMNS_ID)? {
            let col = decode_columns_schema(&row)?;
            if col.table_id != table_id {
                continue;
            }

            if col.col_name == col_name {
                return Ok(Some(col.col_id));
            }
        }
        Ok(None)
    }

    /// Adds the [index::BTreeIndex] into the [ST_INDEXES_NAME] table
    ///
    /// Returns the `index_id`
    ///
    /// NOTE: It loads the data from the table into it before returning
    pub fn create_index(&mut self, tx: &mut Tx, index: index::IndexDef) -> Result<IndexId, DBError> {
        for row in self.scan_indexes_schema(tx)? {
            if row.name == index.name {
                return Err(IndexError::IndexAlreadyExists(index, row.name).into());
            }
        }

        let index_id = self.next_sequence(self.catalog.seq_index())? as u32;

        // Insert the index row into st_indexes
        let row = product![
            TypeValue::U32(index_id),
            TypeValue::U32(index.table_id),
            TypeValue::U32(index.col_id),
            TypeValue::String(index.name.clone()),
            TypeValue::Bool(index.is_unique),
        ];
        Self::insert_row_raw(&mut self.txdb, tx, self.catalog.indexes.table_idx_id, row);

        let mut index = index::BTreeIndex::from_def(index_id.into(), index);

        index.index_full_column(self, tx)?;
        self.catalog.indexes.insert(index);

        Ok(index_id.into())
    }

    /// Removes the [index::BTreeIndex] from the database by their `index_id`
    pub fn drop_index(&mut self, tx: &mut Tx, index_id: IndexId) -> Result<(), DBError> {
        let index_table_id = self.catalog.indexes.table_idx_id;
        let iter = self.seek(
            tx,
            index_table_id,
            index::IndexFields::IndexId as u32,
            TypeValue::U32(index_id.0).into(),
        )?;

        let row = iter.collect::<Vec<_>>();
        if (self.delete_in(tx, index_table_id, row)?).is_none() {
            return Err(IndexError::NotFound(index_id).into());
        }

        self.catalog.indexes.remove_by_id(index_id);

        Ok(())
    }

    /// Returns an [Iterator] of the indexes stored in the database
    ///
    /// WARNING: The index data is not loaded
    /// NOTE(tyler): This only iterates over the data in the index, not in the table.
    pub fn scan_indexes_schema<'a>(&'a self, tx: &'a mut Tx) -> Result<IndexIter<'a>, DBError> {
        let columns = self
            .schema_for_table(tx, self.catalog.indexes.table_idx_id)?
            .ok_or_else(|| TableError::NotFound("ST_INDEX_NAME".into()))?;

        let table_iter = TableIter {
            txdb_iter: self.txdb.scan(tx, self.catalog.indexes.table_idx_id),
            schema: columns,
        };

        Ok(index::IndexIter { table_iter })
    }

    /// Returns a [index::btree::TuplesIter] over *all the tuples* in the [index::BTreeIndex]
    /// NOTE(tyler): This only iterates over the data in the index, not in the table.
    pub fn scan_index<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<index::TuplesIter<'a>> {
        if let Some(idx) = self.catalog.indexes.get_table_id(table_id) {
            return Some(idx.iter(self, tx));
        }
        None
    }

    /// Returns a [index::btree::ValuesIter] over the tuples that match `value`
    /// NOTE(tyler): This only iterates over the data in the index, not in the table.
    pub fn seek_index_value<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        value: &'a TypeValue,
    ) -> Option<index::btree::ValuesIter> {
        if let Some(idx) = self.catalog.indexes.get_table_column_id(table_id, col_id) {
            return Some(idx.get(self, tx, value));
        }
        None
    }

    /// Returns a [index::btree::TuplesRangeIter] over the tuples that match the [std::ops::Range] of `values`
    /// NOTE(tyler): This only iterates over the data in the index, not in the table.
    pub fn range_scan_index<'a, R: RangeBounds<TypeValue> + 'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Option<index::TuplesRangeIter> {
        if let Some(idx) = self.catalog.indexes.get_table_column_id(table_id, col_id) {
            return Some(idx.iter_range(self, tx, range));
        }
        None
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
            Err(TableError::IdNotFound(table_id).into())
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
            Err(TableError::IdNotFound(table_id).into())
        }
    }

    /// Return the tables from [ST_TABLES_NAME], filtering out the system tables.
    pub fn scan_table_names<'a>(&'a self, tx: &'a mut Tx) -> Result<impl Iterator<Item = (u32, String)> + 'a, DBError> {
        let iter = self.scan(tx, ST_TABLES_ID)?;

        let ids_system = self
            .catalog
            .tables
            .iter_system_tables()
            .map(|(table_id, _)| table_id)
            .collect::<Vec<_>>();

        Ok(iter.filter_map(move |x| {
            let table_id = x.field_as_u32(0, None).unwrap();

            if ids_system.contains(&table_id) {
                None
            } else {
                let name = x.field_as_str(1, None).unwrap().to_string();

                Some((table_id, name))
            }
        }))
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
            Err(TableError::IdNotFound(table_id).into())
        }
    }

    pub fn pk_seek<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        primary_key: PrimaryKey,
    ) -> Result<Option<TupleValue>, DBError> {
        let schema = self
            .schema_for_table(tx, table_id)?
            .ok_or(TableError::IdNotFound(table_id))?;
        let Some(bytes) = self.txdb.seek(tx, table_id, primary_key.data_key) else { return Ok(None) };
        let row = RelationalDB::decode_row(&schema, &mut &bytes[..])
            .map_err(TableError::RowDecodeError)
            .inspect_err_(|e| log::error!("filter_pk: {e}"))?;
        Ok(Some(row))
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

    /// Execute all the triggers and check before inserting a row into the database
    ///
    /// 1- Update the indexes
    ///
    /// Check if we need to update the indexes for the `table_id`.
    ///
    /// Because the index is in-memory is ok to insert here and crash before insert into the DB.
    ///
    /// It also checks for violations to UNIQUE constrain
    fn before_insert(&mut self, table_id: u32, tx: &mut Tx, row: &TupleValue) -> Result<(), DBError> {
        //NOTE: This logic is split to avoid conflicts with mutable/immutable references...
        self.catalog.indexes.check_unique_keys(self, tx, table_id, row)?;
        self.catalog.indexes.update_row(table_id, row)?;
        Ok(())
    }

    pub fn insert(&mut self, tx: &mut Tx, table_id: u32, row: TupleValue) -> Result<(), DBError> {
        let mut measure = HistogramVecHandle::new(&RDB_INSERT_TIME, vec![format!("{}", table_id)]);
        measure.start();

        // TODO: verify schema
        self.before_insert(table_id, tx, &row)?;
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

    pub fn scan_sequences<'a>(&'a mut self, tx: &'a mut Tx) -> Result<SequenceIter<'a>, DBError> {
        let table_id = self
            .table_id_from_name(tx, ST_SEQUENCES_NAME)?
            .ok_or_else(|| TableError::NotFound("ST_SEQUENCES_NAME".into()))?;

        let iter = self.scan(tx, table_id)?;
        Ok(SequenceIter { iter, table_id })
    }

    /// Restores the [Sequence] from disk if it already exists,
    /// and insert it into the [Catalog]
    pub fn load_sequence(&mut self, seq: Sequence) -> Result<SequenceId, DBError> {
        let mut seq = seq;

        if let Some(value) = read_sled_i64(&mut self.seqdb, &seq.sequence_name)? {
            seq.set_val(value)?;
        } else {
            write_sled_i64(&mut self.seqdb, &seq.sequence_name, seq.start)?;
        };

        let idx = seq.sequence_id;
        self.catalog.add_sequence(seq);

        Ok(idx)
    }

    /// Generated the next value for the [SequenceId]
    pub fn next_sequence(&mut self, seq_id: SequenceId) -> Result<i64, DBError> {
        if let Some(seq) = self.catalog.get_sequence_mut(seq_id) {
            let next = seq.next_val();
            if seq.need_store() {
                write_sled_i64(&mut self.seqdb, &seq.sequence_name, Sequence::next_prefetch(next))?;
            }
            Ok(next)
        } else {
            Err(SequenceError::NotFound(seq_id).into())
        }
    }

    /// Add a [Sequence] into the database instance, generates a stable [SequenceId] for it that will persist on restart.
    pub fn create_sequence(&mut self, seq: SequenceDef, tx: &mut Tx) -> Result<SequenceId, DBError> {
        let iter = self.scan_sequences(tx)?;
        let table_id = iter.table_id;

        for x in iter {
            if x.sequence_name == seq.sequence_name {
                return Err(SequenceError::Exist(seq.sequence_name).into());
            }
        }

        let sequence_id: SequenceId = self.next_sequence(self.catalog.seq_id())?.into();
        let seq = Sequence::from_def(sequence_id, seq)?;

        self.insert(tx, table_id, (&seq).into())?;
        self.load_sequence(seq)?;
        Ok(sequence_id)
    }

    ///Removes the [Sequence] from database instance
    pub fn drop_sequence(&mut self, seq_id: SequenceId, tx: &mut Tx) -> Result<(), DBError> {
        let table_id = self
            .table_id_from_name(tx, ST_SEQUENCES_NAME)?
            .ok_or_else(|| TableError::NotFound("ST_SEQUENCES_NAME".into()))?;
        if let Some(seq) = self.catalog.get_sequence(seq_id) {
            let row = TupleValue::from(seq);
            match self.delete_in(tx, table_id, [row])? {
                None => {
                    return Err(SequenceError::NotFound(seq_id).into());
                }
                Some(x) => {
                    if x == 0 {
                        return Err(SequenceError::NotFound(seq_id).into());
                    }
                }
            }
        }
        Ok(())
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

pub fn make_default_ostorage(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>, DBError> {
    Ok(Box::new(HashMapObjectDB::open(path)?))
}

pub fn open_db(path: impl AsRef<Path>) -> Result<RelationalDB, DBError> {
    let path = path.as_ref();
    let mlog = Arc::new(Mutex::new(MessageLog::open(path.join("mlog"))?));
    let odb = Arc::new(Mutex::new(make_default_ostorage(path.join("odb"))?));
    let stdb = RelationalDB::open(path, mlog, odb)?;

    Ok(stdb)
}

pub fn open_log(path: impl AsRef<Path>) -> Result<Arc<Mutex<MessageLog>>, DBError> {
    let path = path.as_ref().to_path_buf();
    Ok(Arc::new(Mutex::new(MessageLog::open(path.join("mlog"))?)))
}

#[cfg(test)]
pub(crate) mod tests_utils {
    use super::*;
    use tempdir::TempDir;

    //Utility for creating a database on a TempDir
    pub(crate) fn make_test_db() -> Result<(RelationalDB, TempDir), DBError> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let stdb = open_db(&tmp_dir)?;
        Ok((stdb, tmp_dir))
    }

    //Utility for creating a database on the same TempDir, for checking behaviours after shutdown
    pub(crate) fn make_test_db_reopen(tmp_dir: &TempDir) -> Result<RelationalDB, DBError> {
        open_db(&tmp_dir)
    }
}

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex};

    use crate::db::message_log::MessageLog;

    use super::RelationalDB;
    use crate::db::relational_db::make_default_ostorage;
    use crate::db::relational_db::open_log;
    use crate::error::DBError;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::{TupleDef, TypeDef, TypeValue};
    use spacetimedb_sats::product;
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
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();
        stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;
        tx_.commit_into_db()?;
        let log = open_log(&tmp_dir)?;

        log.lock().unwrap().sync_all()?;

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

        let schema = TupleDef::from_iter([("my_col", TypeDef::I32)]);

        stdb.create_table(tx, "MyTable", schema)?;

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
        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;
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
        stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;
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
        stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;
        let result = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]));
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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        stdb.insert(tx, table_id, product![TypeValue::I32(-1)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(0)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(1)])?;

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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        stdb.insert(tx, table_id, product![TypeValue::I32(-1)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(0)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(1)])?;
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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        stdb.insert(tx, table_id, product![TypeValue::I32(-1)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(0)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(1)])?;

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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        stdb.insert(tx, table_id, product![TypeValue::I32(-1)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(0)])?;
        stdb.insert(tx, table_id, product![TypeValue::I32(1)])?;
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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;
        tx_.commit()?;

        let mut tx_ = stdb_.begin_tx();
        let (tx, stdb) = tx_.get();
        stdb.insert(tx, 0, product![TypeValue::I32(-1)])?;
        stdb.insert(tx, 0, product![TypeValue::I32(0)])?;
        stdb.insert(tx, 0, product![TypeValue::I32(1)])?;
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
