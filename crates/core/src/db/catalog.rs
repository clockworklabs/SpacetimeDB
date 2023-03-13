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

use crate::db::sequence::SequenceCatalog;
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

#[derive(Debug, Clone, Copy)]
pub enum CatalogKind {
    Table,
    Column,
    Index,
    Sequence,
}

/// Manage the database metadata.
pub struct Catalog {
    pub(crate) sequences: SequenceCatalog,
    pub(crate) tables: TableCatalog,
    pub(crate) indexes: IndexCatalog,
}

impl Catalog {
    pub fn new() -> Result<Self, DBError> {
        Ok(Self {
            sequences: SequenceCatalog::new()?,
            tables: TableCatalog::new(),
            indexes: IndexCatalog::new(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{TupleDef, TypeDef};

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

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_i32", TypeDef::I32)]))?;

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
