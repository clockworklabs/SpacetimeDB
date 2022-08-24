use super::{
    messages::{transaction::Transaction, write::DataKey},
    transactional_db::{ScanIter, TransactionalDB, Tx},
};
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use spacetimedb_bindings::{ElementDef, EqTypeValue, PrimaryKey, RangeTypeValue};
pub use spacetimedb_bindings::{TupleDef, TupleValue, TypeDef, TypeValue};
use std::{
    ops::{Range, RangeBounds},
    path::Path,
};

pub const ST_TABLES_ID: u32 = u32::MAX;
pub const ST_COLUMNS_ID: u32 = u32::MAX - 1;

pub struct RelationalDB {
    pub txdb: TransactionalDB,
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
        let mut tx = txdb.begin_tx();

        // Create the st_tables table and insert the information about itself into itself
        // schema: (table_id: u32)
        let row = TupleValue {
            elements: vec![TypeValue::U32(ST_TABLES_ID)],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);

        // Create the st_columns table
        // schema: (table_id: u32, col_id: u32, col_type: u32)
        let row = TupleValue {
            elements: vec![TypeValue::U32(ST_COLUMNS_ID)],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);

        // Insert information about st_tables into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![TypeValue::U32(ST_TABLES_ID), TypeValue::U32(0), TypeValue::Bytes(bytes)],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        // Insert information about st_columns into st_columns
        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(0),
                TypeValue::Bytes(bytes),
            ],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::U32.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(1),
                TypeValue::Bytes(bytes),
            ],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        let mut bytes = Vec::new();
        TypeDef::Bytes.encode(&mut bytes);
        let row = TupleValue {
            elements: vec![
                TypeValue::U32(ST_COLUMNS_ID),
                TypeValue::U32(2),
                TypeValue::Bytes(bytes),
            ],
        };
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        txdb.commit_tx(tx);

        RelationalDB { txdb }
    }

    pub fn reset_hard(&mut self) -> Result<(), anyhow::Error> {
        self.txdb.reset_hard()?;
        Ok(())
    }

    pub fn pk_for_row(row: &TupleValue) -> PrimaryKey {
        let mut bytes = Vec::new();
        row.encode(&mut bytes);
        let data_key = DataKey::from_data(bytes);
        PrimaryKey { data_key }
    }

    pub fn encode_row(row: &TupleValue, bytes: &mut Vec<u8>) {
        // TODO: large file storage the row elements
        row.encode(bytes);
    }

    pub fn decode_row(schema: &TupleDef, bytes: impl AsRef<[u8]>) -> Result<TupleValue, &'static str> {
        // TODO: large file storage the row elements
        let (tuple_value, _) = TupleValue::decode(schema, bytes);
        tuple_value
    }

    pub fn schema_for_table(&self, tx: &mut Tx, table_id: u32) -> Option<TupleDef> {
        let mut columns = Vec::new();
        for bytes in self.txdb.scan(tx, ST_COLUMNS_ID) {
            let schema = TupleDef {
                elements: vec![
                    ElementDef {
                        tag: 0,
                        element_type: TypeDef::U32,
                    },
                    ElementDef {
                        tag: 1,
                        element_type: TypeDef::U32,
                    },
                    ElementDef {
                        tag: 2,
                        element_type: TypeDef::Bytes,
                    },
                ],
            };
            let row = Self::decode_row(&schema, bytes);
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
            let (col_type, _) = TypeDef::decode(bytes);

            if let Err(e) = col_type {
                log::error!("schema_for_table: Table has invalid schema: {} Err: {}", table_id, e);
                return None;
            }

            columns.push(ElementDef {
                tag: col_id as u8, // TODO?
                element_type: col_type.unwrap(),
            });
        }
        columns.sort_by(|a, b| a.tag.cmp(&b.tag));
        if columns.len() > 0 {
            Some(TupleDef { elements: columns })
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

    pub fn commit_tx(&mut self, tx: Tx) -> Option<Transaction> {
        self.txdb.commit_tx(tx)
    }

    pub fn create_table(&mut self, tx: &mut Tx, table_id: u32, schema: TupleDef) -> Result<(), String> {
        // Scan st_tables for this id

        // TODO: allocations remove with fixes to ownership
        for row in self.iter(tx, ST_TABLES_ID).unwrap() {
            let t_id = &row.elements[0];
            let t_id = match t_id {
                TypeValue::U32(t_id) => *t_id,
                _ => panic!("Woah ur columns r messed up."),
            };
            if t_id == table_id {
                return Err("Table exists.".into());
            }
        }

        // Insert the table row into st_tables
        let row = TupleValue {
            elements: vec![TypeValue::U32(table_id)],
        };
        Self::insert_row_raw(&mut self.txdb, tx, ST_TABLES_ID, row);

        let mut i = 0;
        for col in schema.elements {
            let mut bytes = Vec::new();
            col.element_type.encode(&mut bytes);
            let row = TupleValue {
                elements: vec![TypeValue::U32(table_id), TypeValue::U32(i), TypeValue::Bytes(bytes)],
            };
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
            i += 1;
        }

        Ok(())
    }

    pub fn drop_table(&mut self, tx: &mut Tx, table_id: u32) -> Result<(), String> {
        let t = self.delete_range(
            tx,
            ST_TABLES_ID,
            0,
            RangeTypeValue::U32(table_id)..RangeTypeValue::U32(table_id),
        );
        let t = t.expect("ST_TABLES_ID should exist");
        if t == 0 {
            return Err("No such table.".into());
        }
        self.delete_range(
            tx,
            ST_COLUMNS_ID,
            0,
            RangeTypeValue::U32(table_id)..RangeTypeValue::U32(table_id),
        );
        Ok(())
    }

    pub fn insert(&mut self, tx: &mut Tx, table_id: u32, row: TupleValue) {
        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
    }

    pub fn iter_pk<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<PrimaryKeyTableIter<'a>> {
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

    pub fn iter<'a>(&'a self, tx: &'a mut Tx, table_id: u32) -> Option<TableIter<'a>> {
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

    pub fn filter_pk<'a>(&'a self, tx: &'a mut Tx, table_id: u32, primary_key: PrimaryKey) -> Option<TupleValue> {
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

    // AKA: scan
    pub fn filter<'a>(&'a self, tx: &'a mut Tx, table_id: u32, f: fn(&TupleValue) -> bool) -> Option<FilterIter<'a>> {
        if let Some(table_iter) = self.iter(tx, table_id) {
            return Some(FilterIter { table_iter, filter: f });
        }
        None
    }

    // AKA: seek
    pub fn filter_eq<'a>(&'a self, tx: &'a mut Tx, table_id: u32, col_id: u32, value: EqTypeValue) -> Vec<TupleValue> {
        if let Some(table_iter) = self.iter(tx, table_id) {
            for row in table_iter {
                // TODO: more than one row can have this value if col_id
                // is not the primary key
                let col_value = &row.elements[col_id as usize];
                // TODO: This should not unwrap because it will crash the server
                let eq_col_value: EqTypeValue = col_value.try_into().unwrap();
                if eq_col_value == value {
                    return vec![row];
                }
            }
        }
        Vec::new()
    }

    // AKA: seek_range
    pub fn filter_range<'a, R: RangeBounds<RangeTypeValue>>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Option<RangeIter<'a, R>>
    where
        R: RangeBounds<RangeTypeValue>,
    {
        if let Some(table_iter) = self.iter(tx, table_id) {
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
        if let Some(_) = self.filter_pk(tx, table_id, primary_key) {
            self.txdb.delete(tx, table_id, primary_key.data_key);
            return Some(true);
        }
        None
    }

    pub fn delete_filter(&mut self, tx: &mut Tx, table_id: u32, f: fn(row: &TupleValue) -> bool) -> Option<usize> {
        if let Some(filter) = self.filter(tx, table_id, f) {
            let mut data_keys = Vec::new();
            for x in filter {
                let mut bytes = Vec::new();
                Self::encode_row(&x, &mut bytes);
                data_keys.push(DataKey::from_data(bytes));
            }
            let len = data_keys.len();
            for value in data_keys {
                self.txdb.delete(tx, table_id, value);
            }
            return Some(len);
        }
        None
    }

    pub fn delete_eq(&mut self, tx: &mut Tx, table_id: u32, col_id: u32, value: EqTypeValue) -> Option<usize> {
        for x in self.filter_eq(tx, table_id, col_id, value) {
            let mut data_keys = Vec::new();
            let mut bytes = Vec::new();
            Self::encode_row(&x, &mut bytes);
            data_keys.push(DataKey::from_data(bytes));
            let len = data_keys.len();
            for value in data_keys {
                self.txdb.delete(tx, table_id, value);
            }
            return Some(len);
        }
        None
    }

    pub fn delete_range(
        &mut self,
        tx: &mut Tx,
        table_id: u32,
        col_id: u32,
        range: Range<RangeTypeValue>,
    ) -> Option<usize> {
        if let Some(filter) = self.filter_range(tx, table_id, col_id, range) {
            let mut values = Vec::new();
            for x in filter {
                let mut bytes = Vec::new();
                Self::encode_row(&x, &mut bytes);
                values.push(DataKey::from_data(&bytes));
            }
            let len = values.len();
            for value in values {
                self.txdb.delete(tx, table_id, value);
            }
            return Some(len);
        }
        None
    }

    // pub fn from(&self, tx: &mut Transaction, table_name: &str) -> Option<&TableQuery> {
    //     self.tables.iter().find(|t| t.schema.name == table_name)
    // }

    // pub fn from_mut(&mut self, tx: &mut Transaction, table_name: &str) -> Option<&mut TableQuery> {
    //     self.tables.iter_mut().find(|t| t.schema.name == table_name)
    // }
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

pub enum RangeIter<'a, R: RangeBounds<RangeTypeValue>> {
    Scan(ScanRangeIter<'a, R>),
}

impl<'a, R: RangeBounds<RangeTypeValue>> Iterator for RangeIter<'a, R> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RangeIter::Scan(range) => range.next(),
        }
    }
}
pub struct ScanRangeIter<'a, R: RangeBounds<RangeTypeValue>> {
    table_iter: TableIter<'a>,
    col_index: u32,
    range: R,
}

impl<'a, R: RangeBounds<RangeTypeValue>> Iterator for ScanRangeIter<'a, R> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.table_iter.next() {
            let value = &row.elements[self.col_index as usize];
            // TODO: This should not unwrap
            let range_value: RangeTypeValue = value.try_into().unwrap();
            if self.range.contains(&range_value) {
                return Some(row);
            }
        }
        None
    }
}

pub struct FilterIter<'a> {
    table_iter: TableIter<'a>,
    filter: fn(&TupleValue) -> bool,
}

impl<'a> Iterator for FilterIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.table_iter.next() {
            if (self.filter)(&row) {
                return Some(row);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::RelationalDB;
    use spacetimedb_bindings::{ElementDef, RangeTypeValue, TupleDef, TupleValue, TypeDef, TypeValue};
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
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
    }

    #[test]
    fn test_create_table_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        let result = stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        );
        assert!(matches!(result, Err(_)));
    }

    #[test]
    fn test_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)],
            },
        );

        let mut rows = stdb
            .iter(&mut tx, 0)
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
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)],
            },
        );
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .iter(&mut tx, 0)
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
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)],
            },
        );

        let mut rows = stdb
            .filter_range(&mut tx, 0, 0, RangeTypeValue::I32(0)..)
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
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)],
            },
        );
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .filter_range(&mut tx, 0, 0, RangeTypeValue::I32(0)..)
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
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        drop(tx);

        let mut tx = stdb.begin_tx();
        let result = stdb.drop_table(&mut tx, 0);
        assert!(matches!(result, Err(_)));
    }

    #[test]
    fn test_rollback() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            TupleDef {
                elements: vec![ElementDef {
                    tag: 0,
                    element_type: TypeDef::I32,
                }],
            },
        )
        .unwrap();
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(-1)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(0)],
            },
        );
        stdb.insert(
            &mut tx,
            0,
            TupleValue {
                elements: vec![TypeValue::I32(1)],
            },
        );
        drop(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .iter(&mut tx, 0)
            .unwrap()
            .map(|r| *r.elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        let expected: Vec<i32> = Vec::new();
        assert_eq!(rows, expected);
    }
}
