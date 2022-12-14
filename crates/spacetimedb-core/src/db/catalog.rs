//! # System Catalog
//!
//! The system catalog tracks the metadata for the database engine, such as information about tables, sequences, indexes, and internal bookkeeping information.
//!
//! System catalogs are regular tables. You can drop and recreate the tables, add columns, insert and update values, and **severely mess up your system that way**
//!
//! ## Internal structure
//!
//! The current design is being described at [Notion](https://www.notion.so/clockworklabs/0010-System-Catalog-669518af2e874f8da441bccf6e60ddab).
//!
use crate::db::index::IndexCatalog;
use std::collections::HashMap;

use crate::db::relational_db::ST_TABLE_ID_START;
use crate::db::sequence::{Sequence, SequenceDef, SequenceId};
use crate::db::table::TableCatalog;
use crate::error::DBError;

pub const ST_SEQUENCE_SEQ: &str = "st_seq_sequences";
pub const ST_TABLE_SEQ: &str = "st_seq_tables";
pub const ST_INDEX_SEQ: &str = "st_index_sequences";

/// Fixed ID for internal sequence generator for [Sequence]
pub const ST_SEQUENCE_ID: i64 = 0;
/// Fixed ID for internal sequence generator for [spacetimedb_lib::type_def::TableDef]
pub const ST_TABLE_ID: i64 = 1;
/// Fixed ID for internal sequence generator for [crate::db::index::IndexDef]
pub const ST_INDEX_ID: i64 = 2;

// TODO: This is still in progress, another PR will add/remove things until the design is unified for the database metadata
/// Manage the database metadata.
pub struct Catalog {
    sequences: HashMap<SequenceId, Sequence>,
    pub(crate) tables: TableCatalog,
    pub(crate) indexes: IndexCatalog,
    /// Cache the [SequenceId] of the main sequence generator, for generating `ids` for others [Sequence] objects.
    seq_id: SequenceId,
    /// Cache the [SequenceId] for the generator of tables ([spacetimedb_lib::type_def::TableDef] objects).
    seq_table_id: SequenceId,
    /// Cache the [SequenceId]  for the generator of [crate::db::index::IndexDef] objects.
    index_seq_id: SequenceId,
}

impl Catalog {
    pub fn new() -> Result<Self, DBError> {
        // Initialize the internal sequences for the schema
        let seq_id = ST_SEQUENCE_ID.into();
        let seq = SequenceDef::new(ST_SEQUENCE_SEQ);
        let mut sequences = HashMap::new();
        sequences.insert(seq_id, Sequence::from_def(seq_id, seq)?);

        let seq_table_id = ST_TABLE_ID.into();
        let seq = SequenceDef::new(ST_TABLE_SEQ).with_min_value((ST_TABLE_ID_START + 1) as i64);
        sequences.insert(seq_table_id, Sequence::from_def(seq_table_id, seq)?);

        let index_seq_id = ST_INDEX_ID.into();
        let seq = SequenceDef::new(ST_INDEX_SEQ);
        sequences.insert(index_seq_id, Sequence::from_def(index_seq_id, seq)?);

        Ok(Self {
            sequences,
            tables: TableCatalog::new(),
            indexes: IndexCatalog::new(0),
            seq_id,
            seq_table_id,
            index_seq_id,
        })
    }

    pub fn clear(&mut self) {
        self.sequences.clear();
        self.indexes.clear();
    }

    /// Returns an iterator for all the [Sequence] in the [Catalog]
    pub fn sequences_iter(&self) -> impl Iterator<Item = &Sequence> {
        self.sequences.iter().map(|(_, seq)| seq)
    }

    // TODO: We should verify if the table/column are valid!
    /// Insert a new [Sequence]. Overwrite the last one if exist with the same [SequenceId]
    pub fn add_sequence(&mut self, seq: Sequence) -> SequenceId {
        let idx = seq.sequence_id;
        self.sequences.insert(idx, seq);

        idx
    }

    pub fn get_sequence_mut(&mut self, seq_id: SequenceId) -> Option<&mut Sequence> {
        self.sequences.get_mut(&seq_id)
    }

    pub fn get_sequence(&mut self, seq_id: SequenceId) -> Option<&Sequence> {
        self.sequences.get(&seq_id)
    }

    /// Returns the [SequenceId] for generating `ids` for [Sequence] objects
    pub fn seq_id(&self) -> SequenceId {
        self.seq_id
    }

    /// Returns the [SequenceId] for generating `ids` for table objects
    pub fn seq_table_id(&self) -> SequenceId {
        self.seq_table_id
    }

    /// Returns the [SequenceId] for generating `ids` for [crate::db::index::IndexDef] objects
    pub fn seq_index(&self) -> SequenceId {
        self.index_seq_id
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{ElementDef, TupleDef, TypeDef};

    use crate::db::relational_db::tests_utils::make_test_db;
    use spacetimedb_lib::error::ResultTest;

    #[test]
    fn test_tables() -> ResultTest<()> {
        let (mut stdb, _) = make_test_db()?;

        //dbg!(stdb.catalog.tables.iter().map(|(_, x)| &x.name).collect::<Vec<_>>());
        let total_system_tables = 4;
        assert_eq!(
            stdb.catalog.tables.iter_system_tables().count(),
            total_system_tables,
            "Not loaded the system tables?"
        );
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            TupleDef {
                name: None,
                elements: vec![ElementDef {
                    tag: 0,
                    name: Some("my_i32".into()),
                    element_type: TypeDef::I32,
                }],
            },
        )?;

        assert_eq!(
            stdb.catalog.tables.len(),
            total_system_tables + 1,
            "Not added the user table?"
        );
        let system_count = stdb.catalog.tables.iter_system_tables().count();
        let user_count = stdb.catalog.tables.iter_user_tables().count();
        assert_eq!(system_count, total_system_tables, "Not loaded the system tables?");
        assert_eq!(user_count, 1, "Not added the user table?");

        let table = stdb.catalog.tables.get(table_id);
        assert!(table.is_some(), "Not found the table by ID");

        let table = stdb.catalog.tables.find_name_for_id(table_id);
        assert_eq!(Some("MyTable"), table, "Not found the table by name");

        let table = stdb.catalog.tables.find_id_by_name("MyTable");
        assert_eq!(Some(table_id), table, "Not found the table by ID");

        Ok(())
    }
}
