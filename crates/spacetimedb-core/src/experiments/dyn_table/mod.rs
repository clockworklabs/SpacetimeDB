mod schema;
use std::{collections::{BTreeMap, HashMap, btree_map}, ops::Range};
use self::schema::{ColType, IndexType, Schema};

type Pointer = usize;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug, PartialOrd, Ord)]
pub enum ColValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Bool(bool),
    //F32(f32),
    //F64(f64),
}

impl ColValue {
    fn col_type(&self) -> ColType {
        match self {
            ColValue::U8(_) => ColType::U8,
            ColValue::U16(_) => ColType::U16,
            ColValue::U32(_) => ColType::U32,
            ColValue::U64(_) => ColType::U64,
            ColValue::U128(_) => ColType::U128,
            ColValue::I8(_) => ColType::I8,
            ColValue::I16(_) => ColType::I16,
            ColValue::I32(_) => ColType::I32,
            ColValue::I64(_) => ColType::I64,
            ColValue::I128(_) => ColType::I128,
            ColValue::Bool(_) => ColType::Bool,
            //ColValue::F32(_) => ColType::F32,
            //ColValue::F64(_) => ColType::F64,
        }
    }

    fn from_data(col_type: &ColType, data: &[u8]) -> Self {
        match col_type {
            ColType::U8 => {
                ColValue::U8(data[0])
            },
            ColType::U16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(data);
                ColValue::U16(u16::from_le_bytes(dst))
            },
            ColType::U32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(data);
                ColValue::U32(u32::from_le_bytes(dst))
            },
            ColType::U64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(data);
                ColValue::U64(u64::from_le_bytes(dst))
            },
            ColType::U128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(data);
                ColValue::U128(u128::from_le_bytes(dst))
            },
            ColType::I8 => {
                ColValue::I8(data[0] as i8)
            },
            ColType::I16 => {
                let mut dst = [0u8; 2];
                dst.copy_from_slice(data);
                ColValue::I16(i16::from_le_bytes(dst))
            },
            ColType::I32 => {
                let mut dst = [0u8; 4];
                dst.copy_from_slice(data);
                ColValue::I32(i32::from_le_bytes(dst))
            },
            ColType::I64 => {
                let mut dst = [0u8; 8];
                dst.copy_from_slice(data);
                ColValue::I64(i64::from_le_bytes(dst))
            },
            ColType::I128 => {
                let mut dst = [0u8; 16];
                dst.copy_from_slice(data);
                ColValue::I128(i128::from_le_bytes(dst))
            },
            ColType::Bool => {
                ColValue::Bool(if data[0] == 0 {false} else {true})
            },
            // ColType::F32 => {
            //     let mut dst = [0u8; 4];
            //     dst.copy_from_slice(data);
            //     ColValue::F32(f32::from_le_bytes(dst))
            // },
            // ColType::F64 => {
            //     let mut dst = [0u8; 8];
            //     dst.copy_from_slice(data);
            //     ColValue::F64(f64::from_le_bytes(dst))
            // },
        }
    }

    fn to_data(&self) -> Vec<u8> {
        match self {
            ColValue::U8(x) => x.to_le_bytes().to_vec(),
            ColValue::U16(x) => x.to_le_bytes().to_vec(),
            ColValue::U32(x) => x.to_le_bytes().to_vec(),
            ColValue::U64(x) => x.to_le_bytes().to_vec(),
            ColValue::U128(x) => x.to_le_bytes().to_vec(),
            ColValue::I8(x) => x.to_le_bytes().to_vec(),
            ColValue::I16(x) => x.to_le_bytes().to_vec(),
            ColValue::I32(x) => x.to_le_bytes().to_vec(),
            ColValue::I64(x) => x.to_le_bytes().to_vec(),
            ColValue::I128(x) => x.to_le_bytes().to_vec(),
            ColValue::Bool(x) => (if *x { 1 as u8 } else { 0 as u8}).to_le_bytes().to_vec(),
            // ColValue::F32(x) => x.to_le_bytes().to_vec(),
            // ColValue::F64(x) => x.to_le_bytes().to_vec(),
        }
    }
}

pub struct HashIndex {
    _col_index: usize,
    hash_map: HashMap<ColValue, Pointer>
}
pub struct BTreeIndex {
    _col_index: usize,
    btree_map: BTreeMap<ColValue, Pointer>
}

pub struct DynTable {
    schema: Schema,
    row_count: usize,
    _soa_data: Vec<u8>,
    aos_data: Vec<u8>,
    hash_indexes: HashMap<usize, HashIndex>,
    btree_indexes: HashMap<usize, BTreeIndex>
}

impl<'a> IntoIterator for &'a DynTable {
    type Item = Pointer;
    type IntoIter = DynTableIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        DynTableIterator {
            dyn_table: self,
            row_pointer: 0,
        }
    }
}

impl DynTable {
    pub fn new(schema: Schema) -> Self {
        let mut hash_indexes = HashMap::new();
        let mut btree_indexes = HashMap::new();

        for (i, column) in schema.columns.iter().enumerate() {
            for index_type in &column.indexes {
                match index_type {
                    IndexType::Hash => {
                        hash_indexes.insert(i, HashIndex {
                            _col_index: i,
                            hash_map: HashMap::new(),
                        });
                    },
                    IndexType::BTree => {
                        btree_indexes.insert(i, BTreeIndex {
                            _col_index: i,
                            btree_map: BTreeMap::new(),
                        });
                    },
                }
            }
        }

        Self {
            _soa_data: Vec::new(), 
            aos_data: Vec::new(), 
            schema,
            hash_indexes,
            btree_indexes,
            row_count: 0,
        }
    }

    pub fn row_at_pointer(&self, pointer: Pointer) -> Option<Vec<ColValue>> {
        if pointer >= self.row_count {
            return None;
        }
        let mut cols = Vec::new(); // TODO: allocations
        let row_size = self.schema.row_size();
        let data_index = pointer * row_size;
        let mut total_size = 0;
        for c in &self.schema.columns {
            let start = data_index + total_size;
            let end = data_index + total_size + c.col_type.size() as usize;
            let slice = &self.aos_data[start..end];
            cols.push(ColValue::from_data(&c.col_type, slice));
            total_size += end - start;
        }
        Some(cols)
    }

    pub fn column_at_pointer(&self, col_index: usize, pointer: Pointer) -> Option<ColValue> {
        // TODO: SOA
        self.row_at_pointer(pointer).map(|x| x[col_index])
    }

    pub fn insert(&mut self, row: Vec<ColValue>) -> Result<(), &'static str> {
        // Check validity of row
        if row.len() != self.schema.columns.len() {
            return Err("Row length did not match number of columns");
        }

        for i in 0..self.schema.columns.len() {
            let val = &row[i];
            let col = &self.schema.columns[i];
            if val.col_type() != col.col_type {
                return Err("Column type mismatch.");
            }
        }

        for i in 0..self.schema.columns.len() {
            let _col = &self.schema.columns[i];
            let val = &row[i];
            let hash_index = self.hash_indexes.get_mut(&i);
            if let Some(hash_index) = hash_index {
                if let Some(_) = hash_index.hash_map.insert(*val, self.row_count) {
                    unimplemented!("Crashing because duplicate entry for a hash index");
                }
            }
            let btree_index = self.btree_indexes.get_mut(&i);
            if let Some(btree_index) = btree_index {
                if let Some(_) = btree_index.btree_map.insert(*val, self.row_count) {
                    unimplemented!("Crashing because duplicate entry for a btree index");
                }
            }
            self.aos_data.extend(val.to_data());
        }

        self.row_count += 1;
        Ok(())
    }

    pub fn filter(&self, f: fn(&Vec<ColValue>) -> bool) -> FilterResult {
        FilterResult::Filter(DynTableFilterIterator {
            dyn_table: self,
            row_pointer: 0,
            filter: f,
        })
    }

    pub fn filter_eq(&self, col_name: &str, key: ColValue) -> Option<Pointer> {
        let col = self.schema.column_index_by_name(col_name);
        if let Some(hash_index) = self.hash_indexes.get(&col) {
            return hash_index.hash_map.get(&key).map(|x| *x);
        }
        if let Some(btree_index) = self.btree_indexes.get(&col) {
            return btree_index.btree_map.get(&key).map(|x| *x);
        }
        for ptr in self.into_iter() {
            let row = self.row_at_pointer(ptr).unwrap();
            if row[col] == key {
                return Some(ptr)
            }
        }
        return None;
    }

    pub fn filter_range(&self, col_name: &str, range: Range<ColValue>) -> FilterResult
    {
        let col = self.schema.column_index_by_name(col_name);
        if let Some(btree_index) = self.btree_indexes.get(&col) {
            return FilterResult::BTree(btree_index.btree_map.range(range))
        }
        return FilterResult::Range(DynTableRangeIterator {
            dyn_table: self,
            row_pointer: 0,
            col_index: col,
            range,
        });
    }

    pub fn delete(&self, _f: fn(&Vec<ColValue>) -> bool) {
        // TODO
    }

    pub fn delete_eq(&mut self, _col_name: &str, _key: ColValue) {
        // TODO
    }

    pub fn delete_range(&mut self, _col_name: &str, _key: ColValue) {
        // TODO
    }
}

pub enum FilterResult<'a> {
    BTree(btree_map::Range<'a, ColValue, Pointer>),
    Filter(DynTableFilterIterator<'a>),
    Range(DynTableRangeIterator<'a>),
    All(DynTableIterator<'a>),
}

impl<'a> FilterResult<'a> {
    pub fn with_btree_range(range: btree_map::Range<'a, ColValue, Pointer>) -> Self {
        Self::BTree(range)
    }
}

impl<'a> Iterator for FilterResult<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            FilterResult::BTree(range) => range.next().map(|(_, ptr)| *ptr),
            FilterResult::Filter(filter) => filter.next(),
            FilterResult::Range(range) => range.next(),
            FilterResult::All(iter) => iter.next(),
        }
        
    }
}

pub struct DynTableIterator<'a> {
    dyn_table: &'a DynTable,
    row_pointer: Pointer
}

impl<'a> Iterator for DynTableIterator<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        let pointer = self.row_pointer;
        self.row_pointer += 1;
        if pointer < self.dyn_table.row_count {
            return Some(pointer);
        }
        return None;
    }
}

pub struct DynTableRangeIterator<'a> {
    dyn_table: &'a DynTable,
    row_pointer: Pointer,
    col_index: usize,
    range: Range<ColValue>,
}

impl<'a> Iterator for DynTableRangeIterator<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.dyn_table.row_at_pointer(self.row_pointer) {
            let pointer = self.row_pointer;
            self.row_pointer += 1;
            if self.range.contains(&row[self.col_index]) {
                return Some(pointer);
            }
        }
        None
    }
}

pub struct DynTableFilterIterator<'a> {
    dyn_table: &'a DynTable,
    row_pointer: Pointer,
    filter: fn(&Vec<ColValue>) -> bool,
}

impl<'a> Iterator for DynTableFilterIterator<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.dyn_table.row_at_pointer(self.row_pointer) {
            let pointer = self.row_pointer;
            self.row_pointer += 1;
            if (self.filter)(&row) {
                return Some(pointer);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use criterion::black_box;
    use crate::dyn_table::{ColValue, DynTable, schema::{ColType, Column, DataLayout, IndexType, Schema}};

    #[test]
    fn test() {
        let schema = Schema {
            columns: vec![
                Column {
                    col_type: ColType::I32,
                    name: "x".to_owned(),
                    constraints: vec![],
                    indexes: vec![IndexType::BTree]
                },
                Column {
                    col_type: ColType::I32,
                    name: "z".to_owned(),
                    constraints: vec![],
                    indexes: vec![IndexType::BTree]
                },
            ],
            data_layout: DataLayout::SOA,
        };
        let mut table = DynTable::new(schema);

        for i in 0..1000000 {
            let _ = table.insert(vec![ColValue::I32(i), ColValue::I32(-i)]);
        }

        println!("Range");
        let start = std::time::Instant::now();
        for pointer in table.filter_range("x", ColValue::I32(0)..ColValue::I32(75)) {
            let row = table.row_at_pointer(pointer);
            black_box(row);
        }
        let duration = start.elapsed();
        println!("{} us", duration.as_micros());

        // println!("All");
        // for pointer in table.into_iter() {
        //     let row = table.row_at_pointer(pointer);
        //     println!("{:?}", row);
        // }
        println!("Hash");
        let pointer = table.filter_eq("x", ColValue::I32(64));
        if let Some(pointer) = pointer {
            let row = table.row_at_pointer(pointer);
            println!("{:?}", row);
        }
    }
}
