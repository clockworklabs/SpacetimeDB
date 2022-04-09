pub mod schema;
mod col_value;
pub mod object_db;
pub mod transactional_db;

use std::ops::{Range, RangeBounds};
use crate::{db::col_value::ColValue, hash::hash_bytes};
use self::{schema::ColType, transactional_db::{ScanIter, Transaction, TransactionalDB}};

const ST_TABLES_ID: u32 = 5555;
const ST_COLUMNS_ID: u32 = 5556;

// struct TableQuery<'a> {
//     table: &'a Table,
//     tx: Transaction
// }

pub struct SpacetimeDB {
    txdb: TransactionalDB,
    //tables: Vec<Table>,
}

#[derive(Debug)]
pub struct Column {
    pub col_id: u32,
    pub col_type: ColType,
}

pub struct Schema {
    pub columns: Vec<Column>,
}

// impl Schema {
//     fn decode(bytes: &mut &[u8]) -> Self {
//         let mut columns: Vec<Column> = Vec::new();
//         while bytes.len() > 0 {
//             let mut dst = [0u8; 4];
//             dst.copy_from_slice(&bytes[0..4]);
//             *bytes = &bytes[4..];
//             let col_type = ColType::from_u32(u32::from_be_bytes(dst));

//             let mut dst = [0u8; 4];
//             dst.copy_from_slice(&bytes[0..4]);
//             *bytes = &bytes[4..];
//             let name_len = u32::from_be_bytes(dst);

//             let name = std::str::from_utf8(&bytes[0..(name_len as usize)]).unwrap().to_owned();
//             *bytes = &bytes[(name_len as usize)..];
//             columns.push(Column {
//                 col_type, 
//                 name,
//             });
//         }
//         Self {
//             columns,
//         }
//     }

//     fn encode(&self, bytes: &mut Vec<u8>) {
//         for column in &self.columns {
//             bytes.extend(column.col_type.to_u32().to_be_bytes());
//             bytes.extend((column.name.len() as u32).to_be_bytes());
//             bytes.extend(column.name.bytes());
//         }
//     }
// }

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

impl SpacetimeDB {

    pub fn new() -> Self {
        // Create tables that must always exist
        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        let mut txdb = TransactionalDB::new();
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
        let row = vec![ColValue::U32(ST_TABLES_ID), ColValue::U32(0), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);

        // Insert information about st_columns into st_columns
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(0), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);
        
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(1), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);
        
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(2), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_COLUMNS_ID, row);
        
        txdb.commit_tx(tx);

        SpacetimeDB {
            txdb,
        }
    }

    fn encode_row(row: Vec<ColValue>, bytes: &mut Vec<u8>) {
        for col in row {
            bytes.extend(col.to_data());
        }
    }

    fn decode_row(columns: &Vec<Column>, bytes: &mut &[u8]) -> Vec<ColValue> {
        let mut row = Vec::new();
        for col in columns {
            row.push(ColValue::from_data(&col.col_type, bytes));
        }
        row
    }

    fn schema_for_table(txdb: &TransactionalDB, tx: &mut Transaction, table_id: u32) -> Vec<Column> {
        let mut columns = Vec::new();
        for bytes in txdb.scan(tx, ST_COLUMNS_ID) {
            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[0..4]);
            let t_id = u32::from_be_bytes(dst);

            if t_id != table_id {
                continue;
            }
            
            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[4..8]);
            let col_id = u32::from_be_bytes(dst);

            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[8..12]);
            let col_type = ColType::from_u32(u32::from_be_bytes(dst));

            columns.push(Column {
                col_id,
                col_type,
            })
        };
        columns.sort_by(|a, b| a.col_id.cmp(&b.col_id));
        columns
    }
    
    fn insert_row_raw(txdb: &mut TransactionalDB, tx: &mut Transaction, table_id: u32, row: Vec<ColValue>) {
        let mut bytes = Vec::new();
        Self::encode_row(row, &mut bytes);
        txdb.insert(tx, table_id, bytes);
    }

    pub fn begin_tx(&mut self) -> Transaction {
        self.txdb.begin_tx()
    }

    pub fn commit_tx(&mut self, tx: Transaction) {
        self.txdb.commit_tx(tx);
    }

    pub fn create_table(&mut self, tx: &mut Transaction, table_id: u32, schema: Schema) -> Result<(), String> {
        // Scan st_tables for this id

        // TODO: allocations remove with fixes to ownership
        for row in self.iter(tx, table_id) {
            let t_id = row[0];
            let t_id = match t_id {
                ColValue::U32(t_id) => t_id,
                _ => panic!("Woah ur columns r messed up.")
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
            let row = vec![ColValue::U32(table_id), ColValue::U32(i), ColValue::U32(col.col_type.to_u32())];
            Self::insert_row_raw(&mut self.txdb, tx, ST_COLUMNS_ID, row);
            i += 1;
        }

        Ok(())
    }

    pub fn drop_table(&mut self, tx: &mut Transaction, table_id: u32) -> Result<(), String> {
        let num_deleted = self.delete_range(tx, ST_TABLES_ID, 0, ColValue::U32(table_id)..ColValue::U32(table_id));
        if num_deleted == 0 {
            return Err("No such table.".into());
        }
        self.delete_range(tx, ST_COLUMNS_ID, 0, ColValue::U32(table_id)..ColValue::U32(table_id));
        Ok(())
    }
    
    pub fn insert(&mut self, tx: &mut Transaction, table_id: u32, row: Vec<ColValue>) {
        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
    }

    pub fn iter<'a>(&'a self, tx: &'a mut Transaction, table_id: u32) -> TableIter<'a> {
        let columns = Self::schema_for_table(&self.txdb, tx, table_id);
        println!("{:?}", columns);
        TableIter {
            txdb_iter: self.txdb.scan(tx, table_id),
            schema: columns,
        }
    }

    // AKA: scan
    pub fn filter<'a>(&'a self, tx: &'a mut Transaction, table_id: u32, f: fn(&Vec<ColValue>) -> bool) -> FilterIter<'a> {
        FilterIter {
            table_iter: self.iter(tx, table_id),
            filter: f,
        }
    }

    // AKA: seek_range
    pub fn filter_range<'a, R: RangeBounds<ColValue>>(&'a self, tx: &'a mut Transaction, table_id: u32, col_id: u32, range: R) -> RangeIter<'a, R>
    where
        R: RangeBounds<ColValue>
    {
        RangeIter::Scan(ScanRangeIter {
            table_iter: self.iter(tx, table_id),
            col_index: col_id,
            range,
        })
    }

    pub fn delete_filter(&mut self, tx: &mut Transaction, table_id: u32, f: fn(row: &Vec<ColValue>) -> bool) -> usize {
        let mut hashes = Vec::new();
        for x in self.filter(tx, table_id, f) {
            let mut bytes = Vec::new();
            Self::encode_row(x, &mut bytes);
            hashes.push(hash_bytes(&bytes));
        }
        let len = hashes.len();
        for hash in hashes {
            self.txdb.delete(tx, table_id, hash);
        }
        len
    }

    pub fn delete_range(&mut self, tx: &mut Transaction, table_id: u32, col_id: u32, range: Range<ColValue>) -> usize {
        let mut hashes = Vec::new();
        for x in self.filter_range(tx, table_id, col_id, range) {
            let mut bytes = Vec::new();
            Self::encode_row(x, &mut bytes);
            hashes.push(hash_bytes(&bytes));
        }
        let len = hashes.len();
        for hash in hashes {
            self.txdb.delete(tx, table_id, hash);
        }
        len
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
            let row = SpacetimeDB::decode_row(&self.schema, &mut &bytes[..]);
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


mod tests {
    use crate::db::{Column, Schema, schema::ColType};
    use super::{SpacetimeDB, col_value::ColValue};

    #[test]
    fn test_scan() {
        let mut stdb = SpacetimeDB::new();
        let mut tx = stdb.begin_tx();
        stdb.create_table(&mut tx, 0, Schema {
            columns: vec![Column {col_id: 0, col_type: ColType::I32}],
        }).unwrap();
        stdb.insert(&mut tx, 0, vec![ColValue::I32(-1)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(0)]);
        stdb.insert(&mut tx, 0, vec![ColValue::I32(1)]);

        for x in stdb.filter_range(&mut tx, 0, 0, ..ColValue::I32(0)) {
            println!("x: {:?}", x);
        }

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
    }
}