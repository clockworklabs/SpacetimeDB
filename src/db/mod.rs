mod schema;
mod col_value;
mod table;
mod indexes;
mod transaction;
mod object_db;
mod transactional_db;

use crate::db::col_value::ColValue;

use self::{schema::ColType, table::Table, transactional_db::{Transaction, TransactionalDB}};

const ST_TABLES_ID: u32 = 0;
const ST_COLUMNS_ID: u32 = 1;

struct TableQuery<'a> {
    table: &'a Table,
    tx: Transaction
}

pub struct SpacetimeDB {
    txdb: TransactionalDB,
    //tables: Vec<Table>,
}

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
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);

        // Insert information about st_columns into st_columns
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(0), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);
        
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(1), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);
        
        let row = vec![ColValue::U32(ST_COLUMNS_ID), ColValue::U32(2), ColValue::U32(ColType::U32.to_u32())];
        Self::insert_row_raw(&mut txdb, &mut tx, ST_TABLES_ID, row);
        
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

    fn decode_row(txdb: &TransactionalDB, tx: &mut Transaction, table_id: u32, bytes: &mut &[u8]) -> Vec<ColValue> {
        let mut row = Vec::new();

        let mut columns = Vec::new();
        for bytes in &txdb.scan(tx, ST_COLUMNS_ID) {
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
            dst.copy_from_slice(&bytes[4..8]);
            let col_type = ColType::from_u32(u32::from_be_bytes(dst));

            columns.push(Column {
                col_id,
                col_type,
            })
        };

        columns.sort_by(|a, b| a.col_id.cmp(&b.col_id));
        for col in &columns {
            row.push(ColValue::from_data(&col.col_type, bytes));
        }

        row
    }
    
    fn insert_row_raw(txdb: &mut TransactionalDB, tx: &mut Transaction, table_id: u32, row: Vec<ColValue>) {
        let mut bytes = Vec::new();
        Self::encode_row(row, &mut bytes);
        txdb.insert(&mut tx, table_id, bytes);
    }

    pub fn begin_tx(&mut self) -> Transaction {
        self.txdb.begin_tx()
    }

    pub fn commit_tx(&mut self, tx: Transaction) {
        self.txdb.commit_tx(tx);
    }

    pub fn create_table(&mut self, tx: &mut Transaction, table_id: u32, schema: Schema) -> Result<(), String> {
        // Scan st_tables for this id
        for bytes in &self.txdb.scan(tx, ST_TABLES_ID) {
            let row = Self::decode_row(&self.txdb, tx, ST_TABLES_ID, &mut bytes);
            let t_id = row[0];
            let t_id = match t_id {
                ColValue::U32(t_id) => t_id,
                _ => panic!("Woah ur columns r messed up.")
            };
            if t_id != table_id {
                return Err("Table exists.".into());
            }
        }

        // Insert the table row into st_tables
        let row = vec![ColValue::U32(table_id)];
        Self::insert_row_raw(&mut self.txdb, &mut tx, ST_TABLES_ID, row);

        let mut i = 0;
        for col in schema.columns {
            let row = vec![ColValue::U32(table_id), ColValue::U32(i), ColValue::U32(col.col_type.to_u32())];
            Self::insert_row_raw(&mut self.txdb, &mut tx, ST_COLUMNS_ID, row);
            i += 1;
        }

        Ok(())
    }

    pub fn drop_table(&mut self, tx: &mut Transaction, table_id: u32) -> Result<(), String> {
        for bytes in &self.txdb.scan(tx, ST_TABLES_ID) {
            let row = Self::decode_row(&self.txdb, tx, ST_TABLES_ID, &mut bytes);
            let t_id = row[0];
            let t_id = match t_id {
                ColValue::U32(t_id) => t_id,
                _ => panic!("Woah ur columns r messed up.")
            };
            if t_id == table_id {
                // TODO: do the deletion
                return Ok(());
            }
        }
        Err("No such table.".into())
    }
    
    pub fn insert(&mut self, tx: &mut Transaction, table_id: u32, row: Vec<ColValue>) {
        // TODO: verify schema
        Self::insert_row_raw(&mut self.txdb, tx, table_id, row);
    }



    // pub fn from(&self, tx: &mut Transaction, table_name: &str) -> Option<&TableQuery> {
    //     self.tables.iter().find(|t| t.schema.name == table_name)
    // }
    
    // pub fn from_mut(&mut self, tx: &mut Transaction, table_name: &str) -> Option<&mut TableQuery> {
    //     self.tables.iter_mut().find(|t| t.schema.name == table_name)
    // }

    // pub fn from_commit_log(commit_log: Vec<Commit>) -> Self {
    //     let mut db = Self::new();
    //     for commit in commit_log {
    //         db.apply_commit(commit);
    //     }
    //     db
    // }

    // fn apply_commit(&mut self, commit: Commit) {
    //     for write in commit.writes {
    //         match write {
    //             transaction::Write::Insert { table_id, content } => todo!(),
    //             transaction::Write::Delete { table_id, row_key } => todo!(),
    //         }
    //     }
    // }


}


mod tests {
    use std::mem::size_of;

    use crate::db::transaction::Write;

    use super::{SpacetimeDB, col_value::ColValue, schema::Schema};

    #[test]
    fn test_scan() {
        println!("{}", size_of::<Write>());
        // let x = vec![0].iter();
        // let mut stdb = SpacetimeDB::new();
        // let mut tx = stdb.begin_tx();
        // stdb.create_table(&mut tx, Schema {
        //     name: "health".into(),
        //     columns: Vec::new(),
        //     data_layout: super::schema::DataLayout::SOA,
        // });

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