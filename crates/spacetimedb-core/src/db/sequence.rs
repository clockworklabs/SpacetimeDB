//! Sequence number generator
//!
//! This involves creating and initializing a new special row in the table [crate::db::relational_db::ST_SEQUENCES_NAME].
//!
//! After a sequence is created, you use the functions `next_val` and `set_val` to operate on the sequence
use crate::db::relational_db::TableIter;
use crate::db::sequence;
use crate::error::DBError;
use spacetimedb_lib::{TupleDef, TupleValue, TypeDef, TypeValue};
use spacetimedb_sats::product;
use std::fmt;
use thiserror::Error;

/// How many values are cached before commit into the database
///
/// In case of a crash, the sequence will at most skip this amount of values
const PREFETCH_LOG: u32 = 32;

/// The `id` for [Sequence]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceId(pub(crate) u32);

impl fmt::Display for SequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for SequenceId {
    fn from(x: usize) -> Self {
        SequenceId(x as u32)
    }
}

impl From<i64> for SequenceId {
    fn from(x: i64) -> Self {
        SequenceId(x as u32)
    }
}

impl From<i32> for SequenceId {
    fn from(x: i32) -> Self {
        SequenceId(x as u32)
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SequenceError {
    #[error("Sequence with name `{0}` already exists.")]
    Exist(String),
    #[error("Sequence `{0}`: The increment is 0, and this means the sequence can't advance.")]
    IncrementIsZero(String),
    #[error("Sequence `{0}`: The min_value {1} must < max_value {2}.")]
    MinMax(String, i64, i64),
    #[error("Sequence `{0}`: The start value {1} must be >= min_value {2}.")]
    MinStart(String, i64, i64),
    #[error("Sequence `{0}`: The start value {1} must be <= min_value {2}.")]
    MaxStart(String, i64, i64),
    #[error("Sequence `{0}` failed to decode value from Sled (not a i64).")]
    SequenceValue(String),
    #[error("Sequence ID `{0}` not found.")]
    NotFound(SequenceId),
}

/// The fields that define the internal table [crate::db::relational_db::ST_SEQUENCES_NAME].
#[derive(Debug)]
pub enum SequenceFields {
    SequenceId = 0,
    SequenceName,
    //Not stored in the DB instance. Is taken from Sled instead
    //Current,
    Start,
    Increment,
    MinValue,
    MaxValue,
    TableId,
    ColId,
}

impl SequenceFields {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            SequenceFields::SequenceId => "sequence_id",
            SequenceFields::SequenceName => "sequence_name",
            SequenceFields::Start => "increment",
            SequenceFields::Increment => "start",
            SequenceFields::MinValue => "min_value",
            SequenceFields::MaxValue => "max_value",
            SequenceFields::TableId => "table_id",
            SequenceFields::ColId => "col_id",
        }
    }
}

impl From<SequenceFields> for Option<&'static str> {
    fn from(x: SequenceFields) -> Self {
        Some(x.name())
    }
}

impl From<SequenceFields> for Option<String> {
    fn from(x: SequenceFields) -> Self {
        Some(x.name().into())
    }
}

/// Builder that define the parameters to create a [Sequence]
#[derive(Debug, Clone)]
pub struct SequenceDef {
    pub(crate) sequence_name: String,
    for_table: Option<(u32, u32)>,
    increment: i64,
    start: Option<i64>,
    min_value: Option<i64>,
    max_value: Option<i64>,
}

impl SequenceDef {
    pub fn new(sequence_name: &str) -> Self {
        Self {
            sequence_name: sequence_name.into(),
            for_table: None,
            increment: 1,
            start: None,
            min_value: None,
            max_value: None,
        }
    }

    /// Specifies which value is added to the current sequence value.
    ///
    /// A positive value will make an ascending sequence, a negative one a descending sequence.
    ///
    /// The default value is 1.
    ///
    /// WARNING:
    ///
    /// If the increment is 0, the sequence can't advance and will fail on [Sequence::from_def]
    pub fn with_increment(self, increment: i64) -> Self {
        let mut x = self;
        x.increment = increment;
        x
    }

    /// Determines the starting point for the sequence.
    ///
    /// The default starting value is min_value for ascending sequences and max_value for descending ones.
    pub fn with_start(self, value: i64) -> Self {
        let mut x = self;
        x.start = Some(value);
        x
    }

    /// Determines the minimum value a sequence can generate.
    ///
    /// The default for an ascending sequence is 1. The default for a descending sequence is [i64::MIN].
    pub fn with_min_value(self, value: i64) -> Self {
        let mut x = self;
        x.min_value = Some(value);
        x
    }

    /// Determines the maximum value for the sequence.
    ///
    /// The default for an ascending sequence is [i64::MAX]. The default for a descending sequence is -1.
    pub fn with_max_value(self, value: i64) -> Self {
        let mut x = self;
        x.max_value = Some(value);
        x
    }

    /// Associated with a specific table/column, such that if that column (or its whole table) is dropped,
    /// the sequence will be automatically dropped as well.
    ///
    /// WARNING: Assumes the `table_id`, `col_id` are valid.
    pub fn with_table(self, table_id: u32, col_id: u32) -> Self {
        let mut x = self;
        x.for_table = Some((table_id, col_id));
        x
    }
}

/// A [Sequence] generator
#[allow(dead_code)] //The compiler mark Debug, Clone dead but the catalog use it
#[derive(Debug, Clone)]
pub struct Sequence {
    pub(crate) sequence_id: SequenceId,
    pub(crate) sequence_name: String,
    current: i64,
    increment: i64,
    pub(crate) start: i64,
    min_value: i64,
    max_value: i64,
    /// Optionally attached to this table
    table_id: Option<u32>,
    col_id: Option<u32>,
    /// Cache this amount of sequences
    cache: u32,
}

impl Sequence {
    /// Create a [Sequence] from a [SequenceDef]
    ///
    /// Validates if the [SequenceDef] is well built.
    ///
    /// WARNING: Assumes the `sequence_id` is valid
    pub fn from_def(sequence_id: SequenceId, seq: SequenceDef) -> Result<Self, SequenceError> {
        if seq.increment == 0 {
            return Err(SequenceError::IncrementIsZero(seq.sequence_name));
        }

        let (table_id, col_id) = if let Some((table_id, col_id)) = seq.for_table {
            (Some(table_id), Some(col_id))
        } else {
            (None, None)
        };

        let (min_value, max_value, start) = if seq.increment > 0 {
            let min_value = seq.min_value.unwrap_or(1);
            (
                min_value,
                seq.max_value.unwrap_or(i64::MAX),
                seq.start.unwrap_or(min_value),
            )
        } else {
            let max_value = seq.max_value.unwrap_or(-1);
            (
                seq.min_value.unwrap_or(i64::MIN),
                max_value,
                seq.start.unwrap_or(max_value),
            )
        };

        if min_value >= max_value {
            return Err(SequenceError::MinMax(seq.sequence_name, min_value, max_value));
        }
        if start < min_value {
            return Err(SequenceError::MinStart(seq.sequence_name, start, min_value));
        }
        if start > max_value {
            return Err(SequenceError::MaxStart(seq.sequence_name, start, max_value));
        }

        Ok(Self {
            sequence_id,
            sequence_name: seq.sequence_name,
            table_id,
            col_id,
            increment: seq.increment,
            min_value,
            start,
            max_value,
            current: start,
            cache: PREFETCH_LOG,
        })
    }

    /// Assigns the current value to the [Sequence] and verify is inside the bounds
    /// of `min_value...max_value`
    pub fn set_val(&mut self, current: i64) -> Result<(), SequenceError> {
        if current < self.min_value {
            return Err(SequenceError::MinStart(
                self.sequence_name.clone(),
                current,
                self.min_value,
            ));
        }
        if current > self.max_value {
            return Err(SequenceError::MaxStart(
                self.sequence_name.clone(),
                current,
                self.max_value,
            ));
        }

        self.current = current;
        Ok(())
    }

    /// Check if the [Sequence] need to be persisted.
    ///
    /// It reads until [PREFETCH_LOG] to be true.
    pub fn need_store(&mut self) -> bool {
        if let Some(next) = self.cache.checked_add(1) {
            if next < PREFETCH_LOG {
                self.cache = next;
                return false;
            }
        }
        self.cache = 0;
        true
    }

    /// Utility that calculates the next value to be persisted in the database
    pub(crate) fn next_prefetch(next: i64) -> i64 {
        next + (PREFETCH_LOG as i64) - 1
    }

    /// Generate the next `value`/`id`.
    ///
    /// **WARNING: It wraps around.**
    ///
    /// # Example
    ///
    /// ```
    /// use spacetimedb::db::sequence::{Sequence, SequenceDef};
    /// let seq = SequenceDef::new("simple").with_min_value(10).with_max_value(12);
    /// let mut seq = Sequence::from_def(0.into(), seq).unwrap();
    ///
    /// assert_eq!(seq.next_val(), 10);
    /// assert_eq!(seq.next_val(), 11);
    /// assert_eq!(seq.next_val(), 12);
    /// assert_eq!(seq.next_val(), 10);
    /// ```
    pub fn next_val(&mut self) -> i64 {
        let current = self.current;
        let mut next = self.current;

        // ascending sequence
        if self.increment > 0 {
            if (self.max_value >= 0 && next > self.max_value - self.increment)
                || (self.max_value < 0 && next + self.increment > self.max_value)
            {
                next = self.min_value
            } else {
                next += self.increment
            };
        } else {
            /* descending sequence */
            if (self.min_value < 0 && next < self.min_value - self.increment)
                || (self.min_value >= 0 && next + self.increment < self.min_value)
            {
                next = self.max_value
            } else {
                next += self.increment;
            }
        }
        self.current = next;
        current
    }
}

/// Table [ST_GENERATOR_NAME]
///
/// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_id |
/// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|
/// | 1           | "seq_customer_id" | 1         | 10    | 10        | 12        | 1        | 1      |
pub(crate) fn internal_schema() -> TupleDef {
    TupleDef::from_iter([
        (SequenceFields::SequenceId.name(), TypeDef::U32),
        (SequenceFields::SequenceName.name(), TypeDef::String),
        (SequenceFields::Increment.name(), TypeDef::I64),
        (SequenceFields::Start.name(), TypeDef::I64),
        (SequenceFields::MinValue.name(), TypeDef::I64),
        (SequenceFields::MaxValue.name(), TypeDef::I64),
        (SequenceFields::TableId.name(), TypeDef::U32),
        (SequenceFields::ColId.name(), TypeDef::U32),
    ])
}

pub fn decode_schema(row: TupleValue) -> Result<Sequence, DBError> {
    let seq_id = row.field_as_u32(SequenceFields::SequenceId as usize, SequenceFields::SequenceId.into())?;
    let sequence_name = row.field_as_str(
        SequenceFields::SequenceName as usize,
        SequenceFields::SequenceName.into(),
    )?;
    let increment = row.field_as_i64(SequenceFields::Increment as usize, SequenceFields::Increment.into())?;
    let start = row.field_as_i64(SequenceFields::Start as usize, SequenceFields::Start.into())?;
    let min_value = row.field_as_i64(SequenceFields::MinValue as usize, SequenceFields::MinValue.into())?;
    let max_value = row.field_as_i64(SequenceFields::MaxValue as usize, SequenceFields::MaxValue.into())?;
    let table_id = row.field_as_u32(SequenceFields::TableId as usize, SequenceFields::TableId.into())?;
    let col_id = row.field_as_u32(SequenceFields::ColId as usize, SequenceFields::ColId.into())?;

    let seq = SequenceDef::new(sequence_name)
        .with_increment(increment)
        .with_start(start)
        .with_min_value(min_value)
        .with_max_value(max_value)
        .with_table(table_id, col_id);

    let seq = Sequence::from_def(SequenceId(seq_id), seq)?;
    Ok(seq)
}

impl From<&Sequence> for TupleValue {
    fn from(x: &Sequence) -> Self {
        product![
            TypeValue::U32(x.sequence_id.0),
            TypeValue::String(x.sequence_name.clone()),
            TypeValue::I64(x.increment),
            TypeValue::I64(x.start),
            TypeValue::I64(x.min_value),
            TypeValue::I64(x.max_value),
            TypeValue::U32(0),
            TypeValue::U32(0),
        ]
    }
}

/// Utility for reading from [sled::Db]
pub(crate) fn read_sled_i64(seqdb: &mut sled::Db, key: &str) -> Result<Option<i64>, DBError> {
    if let Some(val) = seqdb.get(key)? {
        let value = i64::from_be_bytes(
            val.as_ref()
                .try_into()
                .map_err(|_| SequenceError::SequenceValue(key.to_string()))?,
        );
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

/// Utility for writing into [sled::Db]
pub(crate) fn write_sled_i64(seqdb: &mut sled::Db, key: &str, value: i64) -> Result<(), DBError> {
    seqdb.insert(key, &value.to_be_bytes())?;
    Ok(())
}

pub struct SequenceIter<'a> {
    pub(crate) iter: TableIter<'a>,
    pub table_id: u32,
}

impl<'a> Iterator for SequenceIter<'a> {
    type Item = Sequence;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row) = self.iter.next() {
            let seq = sequence::decode_schema(row).unwrap();
            return Some(seq);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::make_default_ostorage;
    use crate::db::transactional_db::TxCtx;
    use crate::db::{
        message_log::MessageLog,
        relational_db::{tests_utils::make_test_db_reopen, RelationalDB},
    };
    use spacetimedb_lib::error::ResultTest;
    use std::sync::{Arc, Mutex};
    use tempdir::TempDir;

    //Utility for creating a database on a TempDir
    pub(crate) fn make_test_db() -> Result<(RelationalDB, TempDir, Arc<Mutex<MessageLog>>), DBError> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));
        let stdb = RelationalDB::open(tmp_dir.path(), mlog.clone(), odb)?;

        Ok((stdb, tmp_dir, mlog))
    }

    #[test]
    fn test_sequence_validations() -> ResultTest<()> {
        let seq = SequenceDef::new("simple");
        assert!(Sequence::from_def(0.into(), seq).is_ok());

        let seq = SequenceDef::new("simple").with_increment(0);
        assert_eq!(
            Sequence::from_def(0.into(), seq).unwrap_err(),
            SequenceError::IncrementIsZero("simple".into())
        );

        let seq = SequenceDef::new("simple").with_min_value(1).with_max_value(0);
        assert_eq!(
            Sequence::from_def(0.into(), seq).unwrap_err(),
            SequenceError::MinMax("simple".into(), 1, 0)
        );

        let seq = SequenceDef::new("simple").with_start(0).with_min_value(1);
        assert_eq!(
            Sequence::from_def(0.into(), seq).unwrap_err(),
            SequenceError::MinStart("simple".into(), 0, 1)
        );

        let seq = SequenceDef::new("simple")
            .with_start(2)
            .with_min_value(1)
            .with_max_value(1);
        assert_eq!(
            Sequence::from_def(0.into(), seq).unwrap_err(),
            SequenceError::MinMax("simple".into(), 1, 1)
        );
        Ok(())
    }

    #[test]
    fn test_sequence_asc() -> ResultTest<()> {
        let seq = SequenceDef::new("simple");

        let mut seq = Sequence::from_def(0.into(), seq)?;
        assert_eq!(seq.next_val(), 1);
        assert_eq!(seq.next_val(), 2);
        assert_eq!(seq.next_val(), 3);

        let seq = SequenceDef::new("simple").with_min_value(10).with_max_value(12);
        let mut seq = Sequence::from_def(0.into(), seq)?;
        assert_eq!(seq.next_val(), 10);
        assert_eq!(seq.next_val(), 11);
        assert_eq!(seq.next_val(), 12);
        assert_eq!(seq.next_val(), 10);
        assert_eq!(seq.next_val(), 11);
        assert_eq!(seq.next_val(), 12);
        assert_eq!(seq.next_val(), 10);

        Ok(())
    }

    #[test]
    fn test_sequence_desc() -> ResultTest<()> {
        let seq = SequenceDef::new("simple").with_increment(-1);

        let mut seq = Sequence::from_def(0.into(), seq)?;
        assert_eq!(seq.next_val(), -1);
        assert_eq!(seq.next_val(), -2);
        assert_eq!(seq.next_val(), -3);

        let seq = SequenceDef::new("simple")
            .with_increment(-1)
            .with_min_value(10)
            .with_max_value(12);
        let mut seq = Sequence::from_def(0.into(), seq)?;
        assert_eq!(seq.next_val(), 12);
        assert_eq!(seq.next_val(), 11);
        assert_eq!(seq.next_val(), 10);
        assert_eq!(seq.next_val(), 12);

        Ok(())
    }

    #[test]
    fn test_read_prefetch() -> ResultTest<()> {
        let seq = SequenceDef::new("simple");

        let mut seq = Sequence::from_def(0.into(), seq)?;
        assert!(seq.need_store(), "The first call must have restarted PREFETCH_LOG");

        for i in 1..=PREFETCH_LOG - 1 {
            assert!(
                !seq.need_store(),
                "The PREFETCH_LOG must have kicked: {} -> {}",
                i,
                seq.cache
            );
        }
        assert!(
            seq.need_store(),
            "The PREFETCH_LOG must have been restarted:  {}",
            seq.cache
        );

        Ok(())
    }

    #[test]
    fn test_seq_for_table() -> ResultTest<()> {
        let (mut stdb, _, _) = make_test_db()?;
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        let seq_def = SequenceDef::new("simple").with_table(table_id, 0);
        let seq_id = stdb.create_sequence(seq_def, tx)?;

        let seq: Vec<_> = stdb.scan_sequences(tx)?.collect();
        assert_eq!(seq.len(), 1, "Not create the seq for table");

        let seq = stdb.catalog.get_sequence_mut(seq_id).unwrap();
        assert_eq!(seq.next_val(), 1);

        //
        // // On rollbacks, the sequence is still alive...
        //
        // //
        // // let seq_id = stdb.load_sequence(seq)?;
        //
        // let seq = stdb.catalog.get_sequence_mut(seq_id).unwrap();
        // assert_eq!(seq.next_val(), 2);

        //TODO: After rollback, new rows must show gaps...
        Ok(())
    }

    #[test]
    fn test_seq_remove() -> ResultTest<()> {
        let (mut stdb, _, _) = make_test_db()?;
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

        let seq_def = SequenceDef::new("simple").with_table(table_id, 0);
        let seq_id = stdb.create_sequence(seq_def, tx)?;

        let seq: Vec<_> = stdb.scan_sequences(tx)?.collect();
        assert_eq!(seq.len(), 1, "Not create the seq for table");

        stdb.drop_sequence(seq_id, tx)?;

        let seq: Vec<_> = stdb.scan_sequences(tx)?.collect();
        assert_eq!(seq.len(), 0, "Not removed the seq for table");

        Ok(())
    }
    // Must bootstrap correctly the sequences for schemas
    // and read them back on restart
    #[test]
    fn test_cache_after_shutdown() -> ResultTest<()> {
        let (tmp_dir, seq_table_id) = {
            let (mut stdb, tmp_dir, mlog) = make_test_db()?;

            let mut tx_ = stdb.begin_tx();
            let (tx, stdb) = tx_.get();

            let seq_id = stdb.catalog.seq_id();

            for i in 1..(PREFETCH_LOG as i64) {
                let next = stdb.next_sequence(seq_id)?;
                assert_eq!(next, i, "Initial seq wrong");
            }

            let table_id = stdb.create_table(tx, "MyTable", TupleDef::from_iter([("my_col", TypeDef::I32)]))?;

            let seq_def = SequenceDef::new("simple").with_table(table_id, 0);
            let seq_table_id = stdb.create_sequence(seq_def, tx)?;

            let commit_result = stdb.commit_tx(tx.clone())?;
            assert!(
                RelationalDB::persist_tx(&mlog, commit_result)?,
                "The Tx was not persisted to disk"
            );
            (tmp_dir, seq_table_id)
        };
        let mut stdb = make_test_db_reopen(&tmp_dir)?;
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        assert!(stdb.seqdb.was_recovered(), "Sled not was reloaded");

        let seq_id = stdb.catalog.seq_id();
        let next = stdb.next_sequence(seq_id)?;

        assert_eq!(next, PREFETCH_LOG as i64, "Seq after shutdown wrong");

        let seq: Vec<_> = stdb.scan_sequences(tx)?.collect();
        assert_eq!(seq.len(), 1, "Did not create the seq for table");
        let seq_table = stdb.catalog.get_sequence(seq_table_id);

        assert!(seq_table.is_some(), "Not reload seq for table");
        let seq_table = seq_table.unwrap();

        assert_eq!(seq_table.current, 1, "Seq for table after shutdown wrong");

        Ok(())
    }
}
