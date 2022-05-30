use super::{
    messages::write::Value,
    transactional_db::{ScanIter, TransactionalDB, Tx},
};
pub use spacetimedb_bindings::{ColType, ColValue, Column, Schema};
use std::{
    ops::{Range, RangeBounds},
    path::Path,
};

const ST_TABLES_ID: u32 = u32::MAX;
const ST_COLUMNS_ID: u32 = u32::MAX - 1;

pub struct RelationalDB {
    pub txdb: TransactionalDB,
}

impl RelationalDB {
    pub fn open(root: impl AsRef<Path>) -> Self {
        // Create tables that must always exist
        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        let root = root.as_ref();

        let mut txdb = TransactionalDB::open(&root.to_path_buf().join("txdb")).unwrap();
        let mut tx = txdb.begin_tx();

        // Create the st_tables table and insert the information about itself into itself
        // schema: (table_id: u32)
        let row = vec![ColValue::U32(ST_TABLES_ID)];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);

        // Create the st_columns table
        // schema: (table_id: u32, col_id: u32, col_type: u32)
        let row = vec![ColValue::U32(ST_COLUMNS_ID)];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);

        // Insert information about st_tables into st_columns
        let row = vec![
            ColValue::U32(ST_TABLES_ID),
            ColValue::U32(0),
            ColValue::U32(ColType::U32.to_u32()),
        ];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        // Insert information about st_columns into st_columns
        let row = vec![
            ColValue::U32(ST_COLUMNS_ID),
            ColValue::U32(0),
            ColValue::U32(ColType::U32.to_u32()),
        ];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        let row = vec![
            ColValue::U32(ST_COLUMNS_ID),
            ColValue::U32(1),
            ColValue::U32(ColType::U32.to_u32()),
        ];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        let row = vec![
            ColValue::U32(ST_COLUMNS_ID),
            ColValue::U32(2),
            ColValue::U32(ColType::U32.to_u32()),
        ];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        txdb.commit_tx(tx);

        RelationalDB { txdb }
    }

    pub fn encode_row(row: Vec<ColValue>, bytes: &mut Vec<u8>) {
        for col in row {
            bytes.extend(col.to_data());
        }
    }

    pub fn decode_row(columns: &Vec<Column>, bytes: &[u8]) -> Vec<ColValue> {
        let mut row = Vec::new();
        let mut bytes_read: usize = 0;
        for col in columns {
            row.push(ColValue::from_data(&col.col_type, &bytes[bytes_read..]));
            bytes_read += col.col_type.size() as usize;
        }
        row
    }

    pub fn schema_for_table(&self, tx: &mut Tx, table_id: u32) -> Option<Vec<Column>> {
        let mut columns = Vec::new();
        for bytes in self.txdb.scan(tx, ST_COLUMNS_ID) {
            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[0..4]);
            let t_id = u32::from_le_bytes(dst);

            if t_id != table_id {
                continue;
            }

            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[4..8]);
            let col_id = u32::from_le_bytes(dst);

            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[8..12]);
            let col_type = ColType::from_u32(u32::from_le_bytes(dst));

            columns.push(Column { col_id, col_type })
        }
        columns.sort_by(|a, b| a.col_id.cmp(&b.col_id));
        if columns.len() > 0 {
            Some(columns)
        } else {
            None
        }
    }

    fn insert_row_raw(txdb: &mut TransactionalDB, tx: &mut Tx, table_id: u32, row: Vec<ColValue>) {
        let mut bytes = Vec::new();
        Self::encode_row(row, &mut bytes);
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

    pub fn commit_tx(&mut self, tx: Tx) {
        self.txdb.commit_tx(tx);
    }

    pub fn create_table(&mut self, tx: &mut Tx, table_id: u32, schema: Schema) -> Result<(), String> {
        // Scan st_tables for this id

        // TODO: allocations remove with fixes to ownership
        for row in self.iter(tx, ST_TABLES_ID).unwrap() {
            let t_id = row[0];
            let t_id = match t_id {
                ColValue::U32(t_id) => t_id,
                _ => panic!("Woah ur columns r messed up."),
            };
            if t_id == table_id {
                return Err("Table exists.".into());
            }
        }

        // Insert the table row into st_tables
        let row = vec![ColValue::U32(table_id)];
        Self::insert_row_raw(&mut self.txdb, tx, ST_TABLES_ID, row);

        let mut i = 0;
        for col in schema.columns {
            let row = vec![
                ColValue::U32(table_id),
                ColValue::U32(i),
                ColValue::U32(col.col_type.to_u32()),
            ];
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
            i += 1;
        }

        Ok(())
    }

    pub fn drop_table(&mut self, tx: &mut Tx, table_id: u32) -> Result<(), String> {
        let t = self.delete_range(tx, ST_TABLES_ID, 0, ColValue::U32(table_id)..ColValue::U32(table_id));
        let t = t.expect("ST_TABLES_ID should exist");
        if t == 0 {
            return Err("No such table.".into());
        }
        self.delete_range(tx, ST_COLUMNS_ID, 0, ColValue::U32(table_id)..ColValue::U32(table_id));
        Ok(())
    }

    pub fn insert(&mut self, tx: &mut Tx, table_id: u32, row: Vec<ColValue>) {
        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
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

    // AKA: scan
    pub fn filter<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        f: fn(&Vec<ColValue>) -> bool,
    ) -> Option<FilterIter<'a>> {
        if let Some(table_iter) = self.iter(tx, table_id) {
            return Some(FilterIter { table_iter, filter: f });
        }
        None
    }

    // AKA: seek
    pub fn filter_eq<'a>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        value: ColValue,
    ) -> Option<Vec<ColValue>> {
        if let Some(table_iter) = self.iter(tx, table_id) {
            for row in table_iter {
                // TODO: more than one row can have this value if col_id
                // is not the primary key
                if row[col_id as usize] == value {
                    return Some(row);
                }
            }
        }
        None
    }

    // AKA: seek_range
    pub fn filter_range<'a, R: RangeBounds<ColValue>>(
        &'a self,
        tx: &'a mut Tx,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Option<RangeIter<'a, R>>
    where
        R: RangeBounds<ColValue>,
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

    pub fn delete_filter(&mut self, tx: &mut Tx, table_id: u32, f: fn(row: &Vec<ColValue>) -> bool) -> Option<usize> {
        if let Some(filter) = self.filter(tx, table_id, f) {
            let mut values = Vec::new();
            for x in filter {
                let mut bytes = Vec::new();
                Self::encode_row(x, &mut bytes);
                values.push(Value::from_data(bytes));
            }
            let len = values.len();
            for value in values {
                self.txdb.delete(tx, table_id, value);
            }
            return Some(len);
        }
        None
    }

    pub fn delete_eq(&mut self, tx: &mut Tx, table_id: u32, col_id: u32, value: ColValue) -> Option<usize> {
        if let Some(x) = self.filter_eq(tx, table_id, col_id, value) {
            let mut values = Vec::new();
            let mut bytes = Vec::new();
            Self::encode_row(x, &mut bytes);
            values.push(Value::from_data(bytes));
            let len = values.len();
            for value in values {
                self.txdb.delete(tx, table_id, value);
            }
            return Some(len);
        }
        None
    }

    pub fn delete_range(&mut self, tx: &mut Tx, table_id: u32, col_id: u32, range: Range<ColValue>) -> Option<usize> {
        if let Some(filter) = self.filter_range(tx, table_id, col_id, range) {
            let mut values = Vec::new();
            for x in filter {
                let mut bytes = Vec::new();
                Self::encode_row(x, &mut bytes);
                values.push(Value::from_data(&bytes));
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

pub struct TableIter<'a> {
    schema: Vec<Column>,
    txdb_iter: ScanIter<'a>,
}

impl<'a> Iterator for TableIter<'a> {
    type Item = Vec<ColValue>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(bytes) = self.txdb_iter.next() {
            let row = RelationalDB::decode_row(&self.schema, &mut &bytes[..]);
            return Some(row);
        }
        return None;
    }
}

pub enum RangeIter<'a, R: RangeBounds<ColValue>> {
    Scan(ScanRangeIter<'a, R>),
}

impl<'a, R: RangeBounds<ColValue>> Iterator for RangeIter<'a, R> {
    type Item = Vec<ColValue>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RangeIter::Scan(range) => range.next(),
        }
    }
}
pub struct ScanRangeIter<'a, R: RangeBounds<ColValue>> {
    table_iter: TableIter<'a>,
    col_index: u32,
    range: R,
}

impl<'a, R: RangeBounds<ColValue>> Iterator for ScanRangeIter<'a, R> {
    type Item = Vec<ColValue>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.table_iter.next() {
            if self.range.contains(&row[self.col_index as usize]) {
                return Some(row);
            }
        }
        None
    }
}

pub struct FilterIter<'a> {
    table_iter: TableIter<'a>,
    filter: fn(&Vec<ColValue>) -> bool,
}

impl<'a> Iterator for FilterIter<'a> {
    type Item = Vec<ColValue>;

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
    use crate::db::Schema;
    use spacetimedb_bindings::{ColType, ColValue, Column};
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
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
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
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        let result = stdb.create_table(
            &mut tx,
            0,
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
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
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);

        let mut rows = stdb.iter(&mut tx, 0).unwrap().map(|r| r[0]).collect::<Vec<ColValue>>();
        rows.sort();

        assert_eq!(rows, vec![ColValue::I32(-1), ColValue::I32(0), ColValue::I32(1)]);
    }

    #[test]
    fn test_post_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb.iter(&mut tx, 0).unwrap().map(|r| r[0]).collect::<Vec<ColValue>>();
        rows.sort();

        assert_eq!(rows, vec![ColValue::I32(-1), ColValue::I32(0), ColValue::I32(1)]);
    }

    #[test]
    fn test_filter_range_pre_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);

        let mut rows = stdb
            .filter_range(&mut tx, 0, 0, ColValue::I32(0)..)
            .unwrap()
            .map(|r| r[0])
            .collect::<Vec<ColValue>>();
        rows.sort();

        assert_eq!(rows, vec![ColValue::I32(0), ColValue::I32(1)]);
    }

    #[test]
    fn test_filter_range_post_commit() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb
            .filter_range(&mut tx, 0, 0, ColValue::I32(0)..)
            .unwrap()
            .map(|r| r[0])
            .collect::<Vec<ColValue>>();
        rows.sort();

        assert_eq!(rows, vec![ColValue::I32(0), ColValue::I32(1)]);
    }

    #[test]
    fn test_create_table_rollback() {
        let tmp_dir = TempDir::new("stdb_test").unwrap();
        let mut stdb = RelationalDB::open(tmp_dir.path());
        let mut tx = stdb.begin_tx();
        stdb.create_table(
            &mut tx,
            0,
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
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
            Schema {
                columns: vec![Column {
                    col_id: 0,
                    col_type: ColType::I32,
                }],
            },
        )
        .unwrap();
        stdb.commit_tx(tx);

        let mut tx = stdb.begin_tx();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);
        drop(tx);

        let mut tx = stdb.begin_tx();
        let mut rows = stdb.iter(&mut tx, 0).unwrap().map(|r| r[0]).collect::<Vec<ColValue>>();
        rows.sort();

        assert_eq!(rows, vec![]);
    }
}
