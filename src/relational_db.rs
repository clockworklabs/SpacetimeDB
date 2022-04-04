use crate::{hash::Hash, transactional_db::{Transaction, TransactionalDB}};

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}
    
struct Table {
    id: u32,
    name: String,
}

struct TableRow<T> {
    table_id: u32,
    row: T,
}

impl<T> TableRow<T> {
    fn decode(bytes: &mut &[u8]) -> Self {
        assert!(bytes[0] == ObjType::Row as u8);
        *bytes = &bytes[1..];

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];
        let table_id = u32::from_be_bytes(dst);

        let row = unsafe { std::ptr::read(bytes.as_ptr() as *const _) };
        TableRow {
            table_id,
            row,
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(ObjType::Row as u8);
        let row_bytes = unsafe { any_as_u8_slice(&self.row) };
        bytes.extend(self.table_id.to_be_bytes());
        bytes.extend_from_slice(row_bytes);
    }
}

struct Schema {
    tables: Vec<Table>
}

impl Schema {
    fn decode(bytes: &mut &[u8]) -> Self {
        assert!(bytes[0] == ObjType::Schema as u8);
        *bytes = &bytes[1..];

        let mut tables: Vec<Table> = Vec::new();
        while bytes.len() > 0 {
            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[0..4]);
            *bytes = &bytes[4..];
            let table_id = u32::from_be_bytes(dst);

            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[0..4]);
            *bytes = &bytes[4..];
            let name_len = u32::from_be_bytes(dst);

            let name = std::str::from_utf8(&bytes[0..(name_len as usize)]).unwrap().to_owned();
            *bytes = &bytes[(name_len as usize)..];
            tables.push(Table {
                id: table_id, 
                name,
            });
        }

        Self {
            tables,
        }
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(ObjType::Schema as u8);
        for table in &self.tables {
            bytes.extend(table.id.to_be_bytes());
            bytes.extend((table.name.len() as u32).to_be_bytes());
            bytes.extend(table.name.bytes());
        }
    }
}

#[repr(u8)]
enum ObjType {
    Row,
    Schema,
}

pub struct RelationalDB {
    txdb: TransactionalDB,
    schema_ref: Hash,
    table_id_state: u32,
}

impl RelationalDB {
    pub fn new() -> Self {
        let mut txdb = TransactionalDB::new();
        let mut tx = txdb.begin_tx();
        let schema = Schema {
            tables: Vec::new(),
        };
        let mut bytes = Vec::new();
        schema.encode(&mut bytes);
        let schema_ref = txdb.insert(&mut tx, bytes);
        txdb.commit_tx(tx);
        Self {
            txdb,
            schema_ref,
            table_id_state: 0,
        }
    }

    pub fn begin_tx(&mut self) -> Transaction {
        self.txdb.begin_tx()
    }

    pub fn commit_tx(&mut self, tx: Transaction) {
        self.txdb.commit_tx(tx);
    }

    pub fn create_table<R>(&mut self, tx: &mut Transaction, name: &str) {
        let mut bytes = self.txdb.seek(tx, self.schema_ref).unwrap();
        let mut schema = Schema::decode(&mut bytes);
        for table in &schema.tables {
            if table.name == name {
                // TODO: fail and rollback
                return;
            }
        }
        schema.tables.push(Table {
            id: self.table_id_state,
            name: name.to_owned(),
        });
        let mut bytes = Vec::new();
        schema.encode(&mut bytes);

        self.schema_ref = self.txdb.insert(tx, bytes);
        self.table_id_state += 1;
    }

    // TODO: probably implement with an iterator
    pub fn scan<R, F>(&mut self, tx: &mut Transaction, table_name: &str, mut callback: F)
    where
        Self: Sized,
        F: FnMut(R),
    {
        let schema = Schema::decode(&mut self.txdb.seek(tx, self.schema_ref).unwrap());
        let mut table = None;
        for t in &schema.tables {
            if t.name == table_name {
                table = Some(t);
                break;
            }
        };

        let table = table.unwrap();
        self.txdb.scan(tx, |mut bytes| {
            if bytes.len() == 0 {
                return;
            }

            let obj_type = bytes[0];
            if obj_type != ObjType::Row as u8 {
                return;
            }

            let mut dst = [0u8; 4];
            dst.copy_from_slice(&bytes[1..5]);
            let table_id = u32::from_be_bytes(dst);

            if table_id == table.id {
                let table_row: TableRow<R> = TableRow::decode(&mut bytes);
                callback(table_row.row);
            }
        });
    }

    pub fn select<R, F>(&mut self, tx: &mut Transaction, table_name: &str, predicate: F) -> Vec<R>
    where
        Self: Sized,
        F: Fn(&R) -> bool,
    {
        let mut result = Vec::new(); // TODO: Remove this allocation
        self.scan(tx, table_name, |row: R| {
            if predicate(&row) {
                result.push(row);
            }
        });
        return result;
    }
    
    pub fn insert<R>(&mut self, tx: &mut Transaction, table_name: &str, row: R) -> Option<Hash> {
        // only safe when packed
        let schema = Schema::decode(&mut self.txdb.seek(tx, self.schema_ref).unwrap());
        for table in &schema.tables {
            if table.name == table_name {
                // TODO: Verify schema
                let table_row = TableRow {
                    table_id: table.id,
                    row,
                };
                let mut bytes = Vec::new();
                table_row.encode(&mut bytes);

                // TODO: update indexes and so forth
                return Some(self.txdb.insert(tx, bytes));
            }
        }
                
        // TODO: fail and rollback
        return None;
    }

    pub fn delete(&mut self, tx: &mut Transaction, table: &str, hash: Hash) {

    }

    // aka: cross apply
    pub fn flat_map(&mut self, tx: &mut Transaction, ) {

    }
}


#[cfg(test)]
mod tests {
    use super::RelationalDB;
    #[repr(C, packed)]
    #[derive(Debug, Copy, Clone)]
    struct TestRow {
        a: i32,
        b: u32,
        c: u64
    }

    #[test]
    fn test_scan() {
        let mut db = RelationalDB::new();
        let mut tx = db.begin_tx();
        db.create_table::<TestRow>(&mut tx, "test_row");
        db.insert(&mut tx, "test_row", TestRow {a: 1, b: 2, c: 3});
        db.insert(&mut tx, "test_row", TestRow {a: 1, b: 4, c: 9});
        db.insert(&mut tx, "test_row", TestRow {a: 0, b: 0, c: 0});
        db.insert(&mut tx, "test_row", TestRow {a: 1, b: 1, c: 1});
        db.commit_tx(tx);

        let mut tx = db.begin_tx();
        db.scan(&mut tx, "test_row", |r: TestRow| {
            println!("{:?}", r);
        });
        for r in db.select(&mut tx, "test_row", |r: &TestRow| r.c == 9) {
            println!("{:?}", r);
        }
    }
}
