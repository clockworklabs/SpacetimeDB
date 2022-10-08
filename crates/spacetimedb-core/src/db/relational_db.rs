use super::{
    relational_operators::Relation,
    transactional_db::{CommitResult, ScanIter, TransactionalDB, Tx},
};
// use super::relational_operators::Project;
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use spacetimedb_lib::{
    buffer::{BufReader, DecodeError},
    ElementDef, PrimaryKey,
};
pub use spacetimedb_lib::{TupleDef, TupleValue, TypeDef, TypeValue};
use std::{
    ops::{DerefMut, RangeBounds},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

pub const ST_TABLES_NAME: &'static str = "st_table";
pub const ST_COLUMNS_NAME: &'static str = "st_columns";

pub const ST_TABLES_ID: u32 = 0;
pub const ST_COLUMNS_ID: u32 = 1;

pub struct TxWrapper {
    pub tx: Option<Tx>,
    pub relational_db: RelationalDBWrapper,
}

impl Drop for TxWrapper {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let mut relational_db = self.relational_db.lock().unwrap();
            relational_db.rollback_tx(tx)
        }
    }
}

impl std::ops::Deref for TxWrapper {
    type Target = Tx;

    fn deref(&self) -> &Tx {
        self.tx.as_ref().unwrap()
    }
}

impl std::ops::DerefMut for TxWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.tx.as_mut().unwrap()
    }
}

impl From<TxWrapper> for Tx {
    fn from(mut txw: TxWrapper) -> Self {
        txw.tx.take().unwrap()
    }
}

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

    pub fn begin_tx(&self) -> TxWrapper {
        TxWrapper {
            tx: Some(self.inner.lock().unwrap().begin_tx()),
            relational_db: self.clone(),
        }
    }
}

pub struct RelationalDBGuard<'a> {
    inner: MutexGuard<'a, RelationalDB>,
}

impl<'a> RelationalDBGuard<'a> {
    fn new(inner: MutexGuard<'a, RelationalDB>) -> Self {
        log::trace!("LOCKING DB");
        Self { inner }
    }
}

impl<'a> std::ops::Deref for RelationalDBGuard<'a> {
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
}

fn make_default_ostorage(path: &Path) -> Box<dyn ObjectDB + Send> {
    Box::new(HashMapObjectDB::open(path).unwrap())
}

impl RelationalDB {
    pub fn open(root: impl AsRef<Path>) -> Self {
        // Create tables that must always exist
        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        let root = root.as_ref();

        let mut txdb = TransactionalDB::open(&root.to_path_buf().join("txdb"), make_default_ostorage).unwrap();

        Self::bootstrap(&mut txdb);

        RelationalDB { txdb }
    }

    fn bootstrap(txdb: &mut TransactionalDB) {
        let mut tx = txdb.begin_tx();

        // Create the st_tables table and insert the information about itself into itself
        // schema: (table_id: u32, table_name: String)
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_TABLES_ID),
                TypeValue::String(ST_TABLES_NAME.to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, &mut tx, ST_TABLES_ID, row);

        // Insert the st_columns table into st_tables
        // schema: (table_id: u32, col_id: u32, col_type: Bytes, col_name: String)
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::String(ST_COLUMNS_NAME.to_string()),
            ]
            .into(),
        };
        Self::insert_row_raw(txdb, &mut tx, ST_TABLES_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

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
        Self::insert_row_raw(txdb, &mut tx, ST_COLUMNS_ID, row);

        txdb.commit_tx(tx);
    }

    pub fn reset_hard(&mut self) -> Result<(), anyhow::Error> {
        self.txdb.reset_hard()?;
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

    pub fn schema_for_table(&self, tx: &mut Tx, table_id: u32) -> Option<TupleDef> {
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
                ]
                .into(),
            };
            let row = Self::decode_row(&schema, &mut &bytes[..]);
            if let Err(e) = row {
                log::error!("schema_for_table: Table has invalid schema: {} Err: {}", table_id, e);
                return None;
            }
            let row = row.unwrap();

            let col = &row.elements[0];
            let t_id: u32 = *col.as_u32().unwrap();
            if t_id != table_id {
                continue;
            }

            let col = &row.elements[1];
            let col_id: u32 = *col.as_u32().unwrap();

            let col = &row.elements[2];
            let bytes: &Vec<u8> = col.as_bytes().unwrap();
            let col_type = TypeDef::decode(&mut &bytes[..])
                .map_err(|e| log::error!("schema_for_table: Table has invalid schema: {} Err: {}", table_id, e))
                .ok()?;

            let col = &row.elements[3];
            let col_name: &String = col.as_string().unwrap();

            let element = ElementDef {
                // TODO: do we keep col_id's, do we keep tags for tuples?
                tag: col_id as u8,
                name: Some(col_name.clone()),
                element_type: col_type,
            };
            columns.push(element)
        }
        columns.sort_by(|a, b| a.tag.cmp(&b.tag));
        if columns.len() > 0 {
            Some(TupleDef {
                name: None,
                elements: columns,
            })
        } else {
            None
        }
    }

    fn insert_row_raw(txdb: &mut TransactionalDB, tx: &mut Tx, table_id: u32, row: TupleValue) {
        let mut bytes = Vec::new();
        Self::encode_row(&row, &mut bytes);
        txdb.insert(tx, table_id, bytes);

        // https://stackoverflow.com/questions/43581810/how-postgresql-index-deals-with-mvcc
        // https://stackoverflow.com/questions/60361958/how-does-the-btree-index-of-postgresql-achieve-multi-version-concurrency-control
        // https://stackoverflow.com/questions/65053753/how-does-postgres-atomically-updates-secondary-indices
    }

    pub fn begin_tx(&mut self) -> Tx {
        self.txdb.begin_tx()
    }

    pub fn rollback_tx(&mut self, tx: Tx) {
        self.txdb.rollback_tx(tx);
    }

    pub fn commit_tx(&mut self, tx: Tx) -> Option<CommitResult> {
        self.txdb.commit_tx(tx)
    }

    pub fn create_table(&mut self, tx: &mut Tx, table_name: &str, schema: TupleDef) -> Result<u32, String> {
        // Scan st_tables for this id

        let mut table_count = 0;
        for row in self.scan(tx, ST_TABLES_ID).unwrap() {
            table_count += 1;
            let t_name = &row.elements[1];
            let t_name = t_name.as_string().expect("Woah ur columns r messed up.");

            if t_name == table_name {
                return Err(format!("Table with name {} exists.", t_name));
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
        let mut i = 0;
        for col in schema.elements {
            let mut bytes = Vec::new();
            col.element_type.encode(&mut bytes);
            let col_name = if let Some(col_name) = col.name {
                col_name
            } else {
                // TODO: Maybe we should support Options as a special type
                // in TypeValue? Theoretically they could just be enums, but
                // that is quite a pain to use.
                return Err(format!("Column {} is missing a name.", i));
            };
            let row = TupleValue {
                elements: vec![
                    TypeValue::U32(table_id),
                    TypeValue::U32(i),
                    TypeValue::Bytes(bytes),
                    TypeValue::String(col_name),
                ]
                .into(),
            };
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
            i += 1;
        }

        Ok(table_id)
    }

    pub fn drop_table(&mut self, tx: &mut Tx, table_id: u32) -> Result<(), String> {
        let range = self
            .range_scan(tx, ST_TABLES_ID, 0, TypeValue::U32(table_id)..TypeValue::U32(table_id))
            .expect("ST_TABLES_ID should exist")
            .collect::<Vec<_>>();
        if let None = self.delete_in(tx, table_id, range) {
            return Err("No such table.".into());
        }

        let range = self
            .range_scan(tx, ST_COLUMNS_ID, 0, TypeValue::U32(table_id)..TypeValue::U32(table_id))
            .expect("ST_COLUMNS_ID should exist")
            .collect::<Vec<_>>();
        let _count = self.delete_in(tx, table_id, range).expect("ST_COLUMNS_ID should exist");
        Ok(())
    }

    pub fn insert(&mut self, tx: &mut Tx, table_id: u32, row: TupleValue) {
        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
    }

    pub fn table_id_from_name(&self, tx: &mut Tx, table_name: &str) -> Option<u32> {
        for row in self.scan(tx, ST_TABLES_ID).unwrap() {
            let t_id = &row.elements[0];
            let t_id = *t_id.as_u32().expect("Woah ur columns r messed up.");

            let t_name = &row.elements[1];
            let t_name = t_name.as_string().expect("Woah ur columns r messed up.");

            if t_name == table_name {
                return Some(t_id);
            }
        }
        None
    }

    pub fn table_name_from_id(&self, tx: &mut Tx, table_id: u32) -> Option<String> {
        for row in self.scan(tx, ST_TABLES_ID).unwrap() {
            let t_id = &row.elements[0];
            let t_id = *t_id.as_u32().expect("Woah ur columns r messed up.");

            let t_name = &row.elements[1];
            let t_name = t_name.as_string().expect("Woah ur columns r messed up.");

            if t_id == table_id {
                return Some(t_name.clone());
            }
        }
        None
    }

    pub fn column_id_from_name(&self, tx: &mut Tx, table_id: u32, col_name: &str) -> Option<u32> {
        // schema: (table_id: u32, col_id: u32, col_type: Bytes, col_name: String)
        for row in self.scan(tx, ST_COLUMNS_ID).unwrap() {
            let t_id = &row.elements[0];
            let t_id = *t_id.as_u32().expect("Woah ur columns r messed up.");

            if t_id != table_id {
                continue;
            }

            let col_id = &row.elements[1];
            let col_id = *col_id.as_u32().expect("Woah ur columns r messed up.");

            let c_name = &row.elements[3];
            let c_name = c_name.as_string().expect("Woah ur columns r messed up.");

            if c_name == col_name {
                return Some(col_id);
            }
        }
        None
    }

    pub fn scan_pk<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<PrimaryKeyTableIter<'a>> {
        let columns = self.schema_for_table(tx, table_id);
        if let Some(columns) = columns {
            Some(PrimaryKeyTableIter {
                txdb_iter: self.txdb.scan(tx, table_id),
                schema: columns,
            })
        } else {
            None
        }
    }

    // AKA: iter
    pub fn scan<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<TableIter<'a>> {
        let columns = self.schema_for_table(tx, table_id);
        if let Some(columns) = columns {
            Some(TableIter {
                txdb_iter: self.txdb.scan(tx, table_id),
                schema: columns,
            })
        } else {
            None
        }
    }

    pub fn scan_raw<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<TableIterRaw<'a>> {
        let columns = self.schema_for_table(tx, table_id);
        if let Some(_) = columns {
            Some(TableIterRaw {
                txdb_iter: self.txdb.scan(tx, table_id),
            })
        } else {
            None
        }
    }

    pub fn pk_seek<'a>(&'a self, tx: &'a mut Tx, table_id: u32, primary_key: PrimaryKey) -> Option<TupleValue> {
        let schema = self.schema_for_table(tx, table_id);
        if let Some(schema) = schema {
            let bytes = self.txdb.seek(tx, table_id, primary_key.data_key);
            if let Some(bytes) = bytes {
                return match RelationalDB::decode_row(&schema, &mut &bytes[..]) {
                    Ok(row) => Some(row),
                    Err(e) => {
                        log::error!("filter_pk: Row decode failure: {}", e);
                        None
                    }
                };
            }
        }
        return None;
    }

    pub fn seek<'a>(&'a self, tx: &'a mut Tx, table_id: u32, col_id: u32, value: TypeValue) -> Option<SeekIter> {
        if let Some(table_iter) = self.scan(tx, table_id) {
            return Some(SeekIter::Scan(ScanSeekIter {
                table_iter,
                col_index: col_id,
                value,
            }));
        }
        None
    }

    pub fn range_scan<'a, R: RangeBounds<TypeValue> + 'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Option<RangeIter<R>> {
        if let Some(table_iter) = self.scan(tx, table_id) {
            return Some(RangeIter::Scan(ScanRangeIter {
                table_iter,
                col_index: col_id,
                range,
            }));
        }
        None
    }

    pub fn delete_pk(&mut self, tx: &mut Tx, table_id: u32, primary_key: PrimaryKey) -> Option<bool> {
        // TODO: our use of options here doesn't seem correct, I think we might want to double up on options
        if let Some(_) = self.pk_seek(tx, table_id, primary_key) {
            self.txdb.delete(tx, table_id, primary_key.data_key);
            return Some(true);
        }
        None
    }

    pub fn delete_in<R: Relation>(&mut self, tx: &mut Tx, table_id: u32, relation: R) -> Option<usize> {
        if self.schema_for_table(tx, table_id).is_none() {
            return None;
        }
        let mut count = 0;
        for tuple in relation {
            let data_key = tuple.to_data_key();

            // TODO: Think about if we need to verify that the key is in
            // the table before deleting
            if let Some(_) = self.txdb.seek(tx, table_id, data_key) {
                count += 1;
                self.txdb.delete(tx, table_id, data_key);
            }
        }
        Some(count)
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
            let row = RelationalDB::decode_row(&self.schema, &mut &bytes[..]);
            if let Err(e) = row {
                log::error!("PrimaryKeyTableIter::next: Failed to decode row! Err: {}", e);
                return None;
            }

            return Some(RelationalDB::pk_for_row(&row.unwrap()));
        }
        return None;
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
            let row = RelationalDB::decode_row(&self.schema, &mut &bytes[..]);
            if let Err(e) = row {
                log::error!("TableIter::next: Failed to decode row: Err: {}", e);
                return None;
            }
            return Some(row.unwrap());
        }
        return None;
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
        while let Some(row) = self.table_iter.next() {
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
        while let Some(row) = self.table_iter.next() {
            let value = &row.elements[self.col_index as usize];
            if self.range.contains(value) {
                return Some(row);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::RelationalDB;
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
    fn test() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            "MyTable",
            TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )
        .unwrap();
    }

    #[test]
    fn test_table_name() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        let t_id = stdb.table_id_from_name(&mut tx, "MyTable");
        assert_eq!(t_id, Some(table_id))
    }

    #[test]
    fn test_column_name() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            "MyTable",
            TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )
        .unwrap();
        let table_id = stdb.table_id_from_name(&mut tx, "MyTable").unwrap();
        let col_id = stdb.column_id_from_name(&mut tx, table_id, "my_col");
        assert_eq!(col_id, Some(0))
    }

    #[test]
    fn test_create_table_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            "MyTable",
            TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_col".into()),
                    element_type: TypeDef::I32,
                }]
                .into(),
            },
        )
        .unwrap();
        let result = stdb.create_table(
            &mut tx,
            "MyTable",
            TupleDef {
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
    }

    #[test]
    fn test_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        );

        let mut rows = stdb
            .scan(&mut tx, table_id)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
    }

    #[test]
    fn test_post_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        );
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .scan(&mut tx, table_id)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
    }

    #[test]
    fn test_filter_range_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        );

        println!("{}", table_id);

        let mut rows = stdb
            .range_scan(&mut tx, table_id, 0, TypeValue::I32(0)..)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
    }

    #[test]
    fn test_filter_range_post_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            table_id,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        );
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .range_scan(&mut tx, table_id, 0, TypeValue::I32(0)..)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
    }

    #[test]
    fn test_create_table_rollback() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        drop(tx);

        let mut tx = stdb.begin_tx();
        let result = stdb.drop_table(&mut tx, table_id);
        assert!(matches!(result, Err(_)));
    }

    #[test]
    fn test_rollback() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        let table_id = stdb
            .create_table(
                &mut tx,
                "MyTable",
                TupleDef {
                    name: None,
                    elements: vec![ElementDef {
                        tag: 0,
                        name: Some("my_col".into()),
                        element_type: TypeDef::I32,
                    }]
                    .into(),
                },
            )
            .unwrap();
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)].into(),
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)].into(),
            },
        );
        drop(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .scan(&mut tx, table_id)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        let expected: Vec<i32> = Vec::new();
        assert_eq!(rows, expected);
    }
}
