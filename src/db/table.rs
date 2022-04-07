use std::{collections::{BTreeMap, HashMap, btree_map}, ops::Range};
use super::{col_value::ColValue, indexes::{BTreeIndex, HashIndex}, schema::{IndexType, Schema}};

pub type Pointer = usize;

pub struct Table {
    pub schema: Schema,
    row_count: usize,
    soa_data: Vec<u8>,
    aos_data: Vec<u8>,
    hash_indexes: HashMap<usize, HashIndex>,
    btree_indexes: HashMap<usize, BTreeIndex>
}

impl Table {

    pub fn new(schema: Schema) -> Self {
        let mut hash_indexes = HashMap::new();
        let mut btree_indexes = HashMap::new();

        for (i, column) in schema.columns.iter().enumerate() {
            for index_type in &column.indexes {
                match index_type {
                    IndexType::Hash => {
                        hash_indexes.insert(i, HashIndex {
                            col_index: i,
                            hash_map: HashMap::new(),
                        });
                    },
                    IndexType::BTree => {
                        btree_indexes.insert(i, BTreeIndex {
                            col_index: i,
                            btree_map: BTreeMap::new(),
                        });
                    },
                }
            }
        }

        Self {
            soa_data: Vec::new(), 
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
            let col = &self.schema.columns[i];
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

    pub fn iter(&self) -> TableIter {
        TableIter {
            table: self,
            row_pointer: 0,
        }
    }

    // AKA: scan
    pub fn filter(&self, f: fn(&Vec<ColValue>) -> bool) -> FilterIter {
        FilterIter::Filter(TableFilterIterator {
            table: self,
            row_pointer: 0,
            filter: f,
        })
    }

    // AKA: seek
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

    // AKA: seek_range
    pub fn filter_range(&self, col_name: &str, range: Range<ColValue>) -> FilterIter
    {
        let col = self.schema.column_index_by_name(col_name);
        if let Some(btree_index) = self.btree_indexes.get(&col) {
            return FilterIter::BTree(btree_index.btree_map.range(range))
        }
        return FilterIter::Range(TableRangeIterator {
            table: self,
            row_pointer: 0,
            col_index: col,
            range,
        });
    }

    pub fn delete(&self, f: fn(&Vec<ColValue>) -> bool) {
        // TODO
    }

    pub fn delete_eq(&mut self, col_name: &str, key: ColValue) {
        // TODO
    }

    pub fn delete_range(&mut self, col_name: &str, key: ColValue) {
        // TODO
    }

}

impl<'a> IntoIterator for &'a Table {
    type Item = Pointer;
    type IntoIter = TableIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        TableIter {
            table: self,
            row_pointer: 0,
        }
    }
}

pub enum FilterIter<'a> {
    BTree(btree_map::Range<'a, ColValue, Pointer>),
    Filter(TableFilterIterator<'a>),
    Range(TableRangeIterator<'a>),
    All(TableIter<'a>),
}

impl<'a> FilterIter<'a> {
    pub fn with_btree_range(range: btree_map::Range<'a, ColValue, Pointer>) -> Self {
        Self::BTree(range)
    }
}

impl<'a> Iterator for FilterIter<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            FilterIter::BTree(range) => range.next().map(|(_, ptr)| *ptr),
            FilterIter::Filter(filter) => filter.next(),
            FilterIter::Range(range) => range.next(),
            FilterIter::All(iter) => iter.next(),
        }
        
    }
}

pub struct TableIter<'a> {
    table: &'a Table,
    row_pointer: Pointer
}

impl<'a> Iterator for TableIter<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        let pointer = self.row_pointer;
        self.row_pointer += 1;
        if pointer < self.table.row_count {
            return Some(pointer);
        }
        return None;
    }
}

pub struct TableRangeIterator<'a> {
    table: &'a Table,
    row_pointer: Pointer,
    col_index: usize,
    range: Range<ColValue>,
}

impl<'a> Iterator for TableRangeIterator<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.table.row_at_pointer(self.row_pointer) {
            let pointer = self.row_pointer;
            self.row_pointer += 1;
            if self.range.contains(&row[self.col_index]) {
                return Some(pointer);
            }
        }
        None
    }
}

pub struct TableFilterIterator<'a> {
    table: &'a Table,
    row_pointer: Pointer,
    filter: fn(&Vec<ColValue>) -> bool,
}

impl<'a> Iterator for TableFilterIterator<'a> {
    type Item = Pointer;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(row) = self.table.row_at_pointer(self.row_pointer) {
            let pointer = self.row_pointer;
            self.row_pointer += 1;
            if (self.filter)(&row) {
                return Some(pointer);
            }
        }
        None
    }
}