#![allow(dead_code)]
type Pointer = usize;

enum IndexType {
    // TODO: DerivedHash (e.g. hashcode from coords)
    Hash,
    BTree,
    GIN
}

enum ConstraintType {
    Unique,
}

enum ColValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(Vec<u8>),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    I256(Vec<u8>),
    Bool(bool),
    F32(f32),
    F64(f64),
    String(String)
}

impl ColValue {
    fn col_type(&self) -> ColType {
        match self {
            ColValue::U8(_) => ColType::U8,
            ColValue::U16(_) => ColType::U16,
            ColValue::U32(_) => ColType::U32,
            ColValue::U64(_) => ColType::U64,
            ColValue::U128(_) => ColType::U128,
            ColValue::U256(_) => ColType::U256,
            ColValue::I8(_) => ColType::I8,
            ColValue::I16(_) => ColType::I16,
            ColValue::I32(_) => ColType::I32,
            ColValue::I64(_) => ColType::I64,
            ColValue::I128(_) => ColType::I128,
            ColValue::I256(_) => ColType::I256,
            ColValue::Bool(_) => ColType::Bool,
            ColValue::F32(_) => ColType::F32,
            ColValue::F64(_) => ColType::F64,
            ColValue::String(_) => ColType::String
        }
    }

    fn to_data(&self) -> Vec<u8> {
        match self {
            ColValue::U8(x) => x.to_le_bytes().to_vec(),
            ColValue::U16(x) => x.to_le_bytes().to_vec(),
            ColValue::U32(x) => x.to_le_bytes().to_vec(),
            ColValue::U64(x) => x.to_le_bytes().to_vec(),
            ColValue::U128(x) => x.to_le_bytes().to_vec(),
            ColValue::U256(x) => x.to_owned(),
            ColValue::I8(x) => x.to_le_bytes().to_vec(),
            ColValue::I16(x) => x.to_le_bytes().to_vec(),
            ColValue::I32(x) => x.to_le_bytes().to_vec(),
            ColValue::I64(x) => x.to_le_bytes().to_vec(),
            ColValue::I128(x) => x.to_le_bytes().to_vec(),
            ColValue::I256(x) => x.to_owned(),
            ColValue::Bool(x) => (if *x { 1 as u8 } else { 0 as u8}).to_le_bytes().to_vec(),
            ColValue::F32(x) => x.to_le_bytes().to_vec(),
            ColValue::F64(x) => x.to_le_bytes().to_vec(),
            ColValue::String(x) => {
                let mut vec = x.as_bytes().to_vec();
                vec.truncate(32); // TODO: this is wrong
                vec
            }
        }
    }
}

#[derive(PartialEq)]
enum ColType {
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    I8,
    I16,
    I32,
    I64,
    I128,
    I256,
    Bool,
    F32,
    F64,
    String
}

impl ColType {
    fn size(&self) -> u8 {
        match self {
            ColType::U8 => 1,
            ColType::U16 => 2,
            ColType::U32 => 4,
            ColType::U64 => 8,
            ColType::U128 => 16,
            ColType::U256 => 32,
            ColType::I8 => 1,
            ColType::I16 => 2,
            ColType::I32 => 4,
            ColType::I64 => 8,
            ColType::I128 => 16,
            ColType::I256 => 32,
            ColType::Bool => 1,
            ColType::F32 => 4,
            ColType::F64 => 8,
            ColType::String => 32,
        }
    }
}

struct Column {
    col_type: ColType,
    name: String,
}

struct Schema {
    columns: Vec<Column>
}

struct DynTable {
    data: Vec<u8>,
    schema: Schema,
}

impl DynTable {
    pub fn new(schema: Schema) -> Self {
        Self {
            data: Vec::new(), 
            schema,
        }
    }

    fn row_size(&self) -> usize {
        let mut size: usize = 0;
        for c in &self.schema.columns {
            size += c.col_type.size() as usize
        }
        size
    }

    fn add_hash_index(&mut self, col_name: &str) {
        // TODO
    }
    
    pub fn add_index(&mut self, col_name: &str, index_type: IndexType) {
        let col = self.schema.columns.iter().find(|c| c.name == col_name).unwrap();
        match index_type {
            IndexType::Hash => self.add_hash_index(col_name),
            IndexType::BTree => unimplemented!(),
            IndexType::GIN => unimplemented!(),
        }
    }
    
    pub fn remove_index(&mut self, col_name: &str, index_type: IndexType) {
        // TODO
    }

    pub fn insert(&mut self, row: Vec<ColValue>) {
        for i in 0..self.schema.columns.len() {
            let val = &row[i];
            let col = &self.schema.columns[i];
            if val.col_type() != col.col_type {
                return;
            }
        }
        for i in 0..self.schema.columns.len() {
            let val = &row[i];
            // TODO: update indexes
            self.data.extend(val.to_data());
        }
    }

    // Iterates all rows in the table
    pub fn iterate(&mut self, f: fn(Vec<ColValue>)) {

    }

    pub fn filter_eq(&mut self, col_name: &str, key: ColValue) -> Vec<ColValue> {
        Vec::new()
    }

    pub fn filter_gt(&mut self, col_name: &str, key: ColValue) -> Vec<Vec<ColValue>> {
        Vec::new()
    }

    pub fn filter_lt(&mut self, col_name: &str, key: ColValue) -> Vec<Vec<ColValue>> {
        Vec::new()
    }
    
    pub fn filter_ge(&mut self, col_name: &str, key: ColValue) -> Vec<Vec<ColValue>> {
        Vec::new()
    }
    
    pub fn filter_le(&mut self, col_name: &str, key: ColValue) -> Vec<Vec<ColValue>> {
        Vec::new()
    }

    pub fn delete_where_eq(&mut self, col_name: &str, key: ColValue) {

    }

    // Need to develop a base API that can implement the below functions using indexes

    // "delete + where"
    // TODO: make this efficient using indexes + macros
    pub fn delete_where(&mut self, f: fn(Vec<ColValue>) -> bool) {

    }

    // "update + where"
    // TODO: make this efficient using indexes + macros
    pub fn update_where(&mut self, f: fn(Vec<ColValue>) -> Vec<ColValue>) {

    }

    // "where"
    // TODO: make this efficient using indexes + macros
    pub fn filter(&mut self, f: fn(Vec<ColValue>) -> bool) -> Vec<Vec<ColValue>> {
        return Vec::new();
    }
    
    // "where on unique constraint"
    // TODO: make this efficient using indexes + macros
    pub fn get(&mut self, f: fn(Vec<ColValue>) -> bool) -> Vec<ColValue> {
        return Vec::new();
    }

    // "select"
    // TODO: make this efficient using indexes + macros
    pub fn map(&mut self, f: fn(Vec<ColValue>) -> Vec<ColValue>) -> Vec<Vec<ColValue>> {
        return Vec::new();
    }

    // "aggregate (generalized sum / min / max)"
    // TODO: make this efficient using indexes + macros
    pub fn reduce(&mut self, start: Vec<ColValue>, f: fn(Vec<ColValue>, Vec<ColValue>) -> Vec<ColValue>) -> Vec<ColValue> {
        return Vec::new();
    }
}

fn test() {
    let mut table = DynTable::new(Schema {
        columns: vec![
            Column { col_type: ColType::F32, name: "x".to_owned() },
            Column { col_type: ColType::F32, name: "z".to_owned() },
            Column { col_type: ColType::U64, name: "id".to_owned() },
            Column { col_type: ColType::String, name: "title".to_owned() },
        ],
    });
    table.add_index("x", IndexType::BTree);
    table.add_index("id", IndexType::Hash);
    table.insert(vec![ColValue::F32(0.0), ColValue::F32(0.0), ColValue::U64(123), ColValue::String("Hello, World!".to_owned())]);
}