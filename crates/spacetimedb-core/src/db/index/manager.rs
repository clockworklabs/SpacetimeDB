use crate::db::index::{IndexDef, IndexId, IndexKey};
use spacetimedb_lib::{PrimaryKey, TupleValue};
use std::collections::HashMap;

use crate::db::index::btree::BTreeIndex;
use crate::db::relational_db::RelationalDB;
use crate::db::transactional_db::Tx;
use crate::error::*;

#[derive(Debug)]
pub struct IndexCatalog {
    indexes: HashMap<String, BTreeIndex>,
    /// Stores the `table_id` for the [crate::db::relational_db::ST_INDEXES_NAME] table
    pub(crate) table_idx_id: u32,
}

impl IndexCatalog {
    pub fn new(index_table_id: u32) -> Self {
        Self {
            indexes: HashMap::new(),
            table_idx_id: index_table_id,
        }
    }

    /// Delete all the indexes from memory
    pub fn clear(&mut self) {
        self.indexes.clear();
    }

    /// Returns the [BTreeIndex] from the database by their name
    pub fn get(&self, named: &str) -> Option<&BTreeIndex> {
        self.indexes.get(named)
    }

    /// Returns the [BTreeIndex] from the database by their `index_id`
    pub fn get_by_id(&self, index_id: IndexId) -> Option<&BTreeIndex> {
        self.indexes.values().find(|x| x.index_id == index_id)
    }

    /// Returns a mutable reference of the [BTreeIndex] from the database by their name
    ///
    /// NOTE: It returns the index loaded with data from the table
    pub fn get_mut(&mut self, named: &str) -> Option<&mut BTreeIndex> {
        self.indexes.get_mut(named)
    }

    /// Returns the [BTreeIndex] from the database by their name
    ///
    /// NOTE: It returns the index loaded with data from the table
    pub fn name_for_id(&self, index_id: IndexId) -> Option<String> {
        self.iter_by_index_id(index_id).next().map(|x| x.name.clone())
    }

    /// Returns the [BTreeIndex] from the database by their `table_id`/`col_id`
    ///
    /// NOTE: It returns the index loaded with data from the table
    pub fn get_table_column_id(&self, table_id: u32, col_id: u32) -> Option<&BTreeIndex> {
        self.iter_by_table_id(table_id).find(|x| x.col_id == col_id)
    }

    /// Returns the [BTreeIndex] from the database by their `table_id`
    ///
    /// NOTE: It returns the index loaded with data from the table
    pub fn get_table_id(&self, table_id: u32) -> Option<&BTreeIndex> {
        self.iter_by_table_id(table_id).next()
    }

    /// Return `true` if the [BTreeIndex] is removed
    pub fn remove(&mut self, named: &str) -> bool {
        self.indexes.remove(named).is_some()
    }

    /// Return `true` if the [BTreeIndex] is removed
    pub fn remove_by_id(&mut self, index_id: IndexId) -> bool {
        let name = self.name_for_id(index_id);

        if let Some(name) = name {
            self.remove(&name)
        } else {
            false
        }
    }

    pub fn insert(&mut self, index: BTreeIndex) {
        self.indexes.insert(index.name.clone(), index);
    }

    /// Fill the [BTreeIndex] with ALL the data from the database
    pub fn index_all(&mut self, stdb: &RelationalDB, tx: &mut Tx) -> Result<(), DBError> {
        log::debug!("INDEX: RELOAD ALL START...");
        self.indexes.clear();

        let index_from_db = stdb.scan_indexes_schema(tx)?.collect::<Vec<_>>();
        for mut idx in index_from_db {
            idx.index_full_column(stdb, tx)?;
            self.insert(idx);
        }

        log::debug!("INDEX: RELOAD ALL DONE");
        Ok(())
    }

    pub fn iter_by_index_id(&self, index_id: IndexId) -> impl Iterator<Item = &BTreeIndex> {
        self.indexes.values().filter(move |x| x.index_id == index_id)
    }

    pub fn iter_by_table_id(&self, table_id: u32) -> impl Iterator<Item = &BTreeIndex> {
        self.indexes.values().filter(move |x| x.table_id == table_id)
    }

    pub fn iter_mut_by_table_id(&mut self, table_id: u32) -> impl Iterator<Item = &mut BTreeIndex> {
        self.indexes.values_mut().filter(move |x| x.table_id == table_id)
    }

    /// Verify all the indexes that belongs to `table_id` if the row is duplicated.
    ///
    /// It checks the *previous* [spacetimedb_lib::DataKey] if the [BTreeIndex] contains the column and returns the old row
    /// if was duplicated
    pub fn check_unique_keys(
        &self,
        stdb: &RelationalDB,
        tx: &mut Tx,
        table_id: u32,
        row: &TupleValue,
    ) -> Result<(), DBError> {
        let mut keys = Vec::new();
        for idx in self.iter_by_table_id(table_id) {
            if idx.is_unique {
                let key = idx.get_key(row)?;
                let k = IndexKey::from_value(key);
                if let Some(data_key) = idx.idx.get(&k) {
                    keys.push((IndexDef::from(idx), key, *data_key));
                }
            };
        }

        for (idx, key, data_key) in keys {
            if let Some(row) = stdb.pk_seek(tx, table_id, PrimaryKey { data_key })? {
                return Err(IndexError::Duplicated(idx, key.clone(), row).into());
            }
        }
        Ok(())
    }

    /// Updates the row in all the indexes that belongs to `table_id`.
    ///
    /// WARNING: Is expected you call [Self::check_unique_keys] before.
    pub fn update_row(&mut self, table_id: u32, row: &TupleValue) -> Result<(), DBError> {
        for idx in self.iter_mut_by_table_id(table_id) {
            idx.index_row(row)?;
        }
        Ok(())
    }
}
