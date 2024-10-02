use std::sync::Arc;

use bitflags::bitflags;
use spacetimedb_sats::buffer::{BufReader, BufWriter, DecodeError};
use thiserror::Error;

use crate::{
    error,
    varint::{decode_varint, encode_varint},
    Encode, Varchar,
};

// Re-export so we get a hyperlink in rustdocs by default
pub use spacetimedb_primitives::TableId;

/// A visitor useful to implement stateful [`super::Decoder`]s of [`Txdata`] payloads.
pub trait Visitor {
    type Error: From<DecodeError>;
    /// The type corresponding to one element in [`Ops::rowdata`].
    type Row;

    /// Called for each row in each [`Ops`] of a [`Mutations`]' `inserts`.
    ///
    /// The implementation is expected to determine the appropriate schema based
    /// on the supplied [`TableId`], and to decode the row data from the
    /// supplied [`BufReader`].
    ///
    /// The reader's position is assumed to be at the start of the next row
    /// after the method returns.
    fn visit_insert<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> Result<Self::Row, Self::Error>;

    /// Called for each row in each [`Ops`] of a [`Mutations`]' `deletes`.
    ///
    /// Similar to [`Self::visit_insert`], but allows the visitor to determine
    /// the start of the next section in a [`Mutations`] payload.
    fn visit_delete<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> Result<Self::Row, Self::Error>;

    /// Called to skip over rows from `reader` within a [`Mutations`] of a TX that should not be folded.
    ///
    /// Takes `&mut self` because schema lookups may need mutable access to the visitor
    /// in order to memoize the computed schema.
    /// This method should not store or use the row in any way.
    fn skip_row<'a, R: BufReader<'a>>(&mut self, table_id: TableId, reader: &mut R) -> Result<(), Self::Error>;

    /// Called for each [`TableId`] encountered in the `truncates` section of
    /// a [`Mutations`].
    ///
    /// The default implementation does nothing.
    fn visit_truncate(&mut self, _table_id: TableId) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called for each [`Txdata`] record in a [`crate::Commit`].
    ///
    /// The default implementation does nothing.
    fn visit_tx_start(&mut self, _offset: u64) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after each successful decode of a [`Txdata`] payload.
    ///
    /// The default implementation does nothing.
    fn visit_tx_end(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called for each [`Inputs`] encountered in a [`Txdata`] payload.
    ///
    /// The default implementation does nothing.
    fn visit_inputs(&mut self, _inputs: &Inputs) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called for each [`Outputs`] encountered in a [`Txdata`] payload.
    ///
    /// The default implementation does nothing.
    fn visit_outputs(&mut self, _outputs: &Outputs) -> Result<(), Self::Error> {
        Ok(())
    }
}

bitflags! {
    #[derive(Clone, Copy)]
    pub struct Flags: u8 {
        const HAVE_INPUTS    = 0b10000000;
        const HAVE_OUTPUTS   = 0b01000000;
        const HAVE_MUTATIONS = 0b00100000;
    }
}

/// The canonical payload format of a [`crate::Commitlog`].
///
/// This type may eventually be defined in the core datastore crate.
#[derive(Clone, Debug, PartialEq)]
pub struct Txdata<T> {
    pub inputs: Option<Inputs>,
    pub outputs: Option<Outputs>,
    pub mutations: Option<Mutations<T>>,
}

impl<T> Txdata<T> {
    /// `true` if `self` contains neither inputs, outputs nor mutations.
    pub fn is_empty(&self) -> bool {
        self.inputs.is_none()
            && self.outputs.is_none()
            && self.mutations.as_ref().map(Mutations::is_empty).unwrap_or(true)
    }
}

impl<T: Encode> Txdata<T> {
    pub const VERSION: u8 = 0;

    pub fn encode(&self, buf: &mut impl BufWriter) {
        let mut flags = Flags::empty();
        flags.set(Flags::HAVE_INPUTS, self.inputs.is_some());
        flags.set(Flags::HAVE_OUTPUTS, self.outputs.is_some());
        flags.set(Flags::HAVE_MUTATIONS, self.mutations.is_some());

        buf.put_u8(flags.bits());
        if let Some(inputs) = &self.inputs {
            inputs.encode(buf);
        }
        if let Some(outputs) = &self.outputs {
            outputs.encode(buf);
        }
        if let Some(mutations) = &self.mutations {
            mutations.encode(buf)
        }
    }

    /// Decode `Self` from the given buffer.
    pub fn decode<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<Self, V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        let flags = Flags::from_bits_retain(reader.get_u8()?);

        // If the flags indicate that the payload contains `Inputs`,
        // try to decode and visit them.
        let inputs = flags
            .contains(Flags::HAVE_INPUTS)
            .then(|| Inputs::decode(reader))
            .transpose()?;
        if let Some(inputs) = &inputs {
            visitor.visit_inputs(inputs)?;
        }

        // If the flags indicate that the payload contains `Outputs`,
        // try to decode and visit them.
        let outputs = flags
            .contains(Flags::HAVE_OUTPUTS)
            .then(|| Outputs::decode(reader))
            .transpose()?;
        if let Some(outputs) = &outputs {
            visitor.visit_outputs(outputs)?;
        }

        // If the flags indicate that the payload contains `Mutations`,
        // try to decode them.
        let mutations = flags
            .contains(Flags::HAVE_MUTATIONS)
            .then(|| Mutations::decode(visitor, reader))
            .transpose()?;

        Ok(Self {
            inputs,
            outputs,
            mutations,
        })
    }

    /// Variant of [`Self::decode`] which doesn't allocate `Self`.
    ///
    /// Useful for folding traversals where the visitor state suffices.
    /// Note that both [`Inputs`] and [`Outputs`] are still allocated to satisfy
    /// the [`Visitor`] trait, but [`Mutations`] aren't.
    pub fn consume<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<(), V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        let flags = Flags::from_bits_retain(reader.get_u8()?);

        // If the flags indicate that the payload contains `Inputs`,
        // try to decode and visit them.
        if flags.contains(Flags::HAVE_INPUTS) {
            let inputs = Inputs::decode(reader)?;
            visitor.visit_inputs(&inputs)?;
        }

        // If the flags indicate that the payload contains `Outputs`,
        // try to decode and visit them.
        if flags.contains(Flags::HAVE_OUTPUTS) {
            let outputs = Outputs::decode(reader)?;
            visitor.visit_outputs(&outputs)?;
        }

        // If the flags indicate that the payload contains `Mutations`,
        // try to consume them (i.e. decode but don't allocate).
        if flags.contains(Flags::HAVE_MUTATIONS) {
            Mutations::consume(visitor, reader)?;
        }

        Ok(())
    }

    pub fn skip<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<(), V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        let flags = Flags::from_bits_retain(reader.get_u8()?);

        // If the flags indicate that the payload contains `Inputs`,
        // try to decode them.
        if flags.contains(Flags::HAVE_INPUTS) {
            Inputs::decode(reader)?;
        }

        // If the flags indicate that the payload contains `Outputs`,
        // try to decode them.
        if flags.contains(Flags::HAVE_OUTPUTS) {
            Outputs::decode(reader)?;
        }

        // If the flags indicate that the payload contains `Mutations`,
        // try to consume them (i.e. decode but don't allocate).
        if flags.contains(Flags::HAVE_MUTATIONS) {
            Mutations::skip(visitor, reader)?;
        }

        Ok(())
    }
}

/// The inputs of a transaction, i.e. the name and arguments of a reducer call.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Inputs {
    pub reducer_name: Arc<Varchar<255>>,
    pub reducer_args: Arc<[u8]>,
    // TODO: `reducer_args` should be a `ProductValue`, which
    // requires `Decoder` to be able to resolve the argument
    // type by `reducer_name`.
    //
    // We can do that once modules are stored in the database
    // itself, or the database is otherwise able to load the
    // module info.
    //
    // At that point, we can also remove the length prefix of
    // `Inputs`.
}

impl Inputs {
    pub fn encode(&self, buf: &mut impl BufWriter) {
        let slen = self.reducer_name.len() as u8;
        let len = /* slen */ 1 + slen as usize + self.reducer_args.len();
        buf.put_u32(len as u32);
        buf.put_u8(slen);
        buf.put_slice(self.reducer_name.as_bytes());
        buf.put_slice(&self.reducer_args);
    }

    pub fn decode<'a, R: BufReader<'a>>(reader: &mut R) -> Result<Self, DecodeError> {
        let len = reader.get_u32()?;
        let slen = reader.get_u8()?;
        let reducer_name = {
            let bytes = reader.get_slice(slen as usize)?;
            Varchar::from_str(std::str::from_utf8(bytes)?)
                .expect("slice len cannot be > 255")
                .into()
        };
        let reducer_args = reader.get_slice(len as usize - /* slen */ 1 - slen as usize)?.into();

        Ok(Self {
            reducer_name,
            reducer_args,
        })
    }
}

/// The outputs of a transaction.
///
/// The only currently possible output of a transaction is a string
/// representation of an `Err` return value of a reducer.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Outputs {
    // TODO: We may want a `Cow`-backed variant of `Varchar` for this.
    pub reducer_output: Arc<Varchar<255>>,
}

impl Outputs {
    pub fn encode(&self, buf: &mut impl BufWriter) {
        let slen = self.reducer_output.len() as u8;
        buf.put_u8(slen);
        buf.put_slice(self.reducer_output.as_bytes());
    }

    pub fn decode<'a, R: BufReader<'a>>(reader: &mut R) -> Result<Self, DecodeError> {
        let slen = reader.get_u8()?;
        let reducer_output = {
            let bytes = reader.get_slice(slen as usize)?;
            Varchar::from_str(std::str::from_utf8(bytes)?).unwrap().into()
        };

        Ok(Self { reducer_output })
    }
}

/// Mutations of the data store performed by a transaction.
///
/// All operations are supposed to be **mutually exclusive**.
///
/// Even though not enforced, each kind of mutation should not contain duplicate
/// [`TableId`]s.
#[derive(Clone, Debug, PartialEq)]
pub struct Mutations<T> {
    /// Rows inserted.
    pub inserts: Box<[Ops<T>]>,
    /// Rows deleted.
    pub deletes: Box<[Ops<T>]>,
    /// Truncated tables.
    pub truncates: Box<[TableId]>,
}

impl<T> Mutations<T> {
    /// `true` if `self` contains no mutations at all.
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.deletes.is_empty() && self.truncates.is_empty()
    }
}

impl<T: Encode> Mutations<T> {
    /// Encode `self` in the canonical format to the given buffer.
    pub fn encode(&self, buf: &mut impl BufWriter) {
        encode_varint(self.inserts.len(), buf);
        for ops in self.inserts.iter() {
            ops.encode(buf);
        }
        encode_varint(self.deletes.len(), buf);
        for ops in self.deletes.iter() {
            ops.encode(buf);
        }
        encode_varint(self.truncates.len(), buf);
        for TableId(table_id) in self.truncates.iter() {
            buf.put_u32(*table_id);
        }
    }

    pub fn skip<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<(), V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        // Skip the 'insert' operations.
        // The row data of a single 'insert' operation is decoded by the visitor.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            for _ in 0..m {
                visitor.skip_row(table_id, reader)?;
            }
        }

        // Do the same for the `delete` operations.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            for _ in 0..m {
                visitor.skip_row(table_id, reader)?;
            }
        }

        // Skip the truncates. This does not require involvement from the visitor.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            reader.get_u32()?;
        }

        Ok(())
    }

    /// Decode `Self` from the given buffer.
    pub fn decode<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<Self, V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        // Extract, visit and collect the 'insert' operations.
        // The row data of a single 'insert' operation is extracted by the visitor.
        let n = decode_varint(reader)?;
        let mut inserts = Vec::with_capacity(n);
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            let mut rowdata = Vec::with_capacity(m);
            for _ in 0..m {
                let row = visitor.visit_insert(table_id, reader)?;
                rowdata.push(row);
            }
            inserts.push(Ops {
                table_id,
                rowdata: rowdata.into(),
            });
        }

        // Extract, visit and collect the 'delete' operations.
        // The row data of a single 'delete' operation is extracted by the visitor.
        let n = decode_varint(reader)?;
        let mut deletes = Vec::with_capacity(n);
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            let mut rowdata = Vec::with_capacity(m);
            for _ in 0..m {
                let row = visitor.visit_delete(table_id, reader)?;
                rowdata.push(row);
            }
            deletes.push(Ops {
                table_id,
                rowdata: rowdata.into(),
            });
        }

        // Extract, visit and collect the 'truncate' operations.
        // 'truncate' operations don't have row data.
        let n = decode_varint(reader)?;
        let mut truncates = Vec::with_capacity(n);
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            visitor.visit_truncate(table_id)?;
            truncates.push(table_id);
        }

        Ok(Self {
            inserts: inserts.into(),
            deletes: deletes.into(),
            truncates: truncates.into(),
        })
    }

    /// Variant of [`Self::decode`] which does not allocate `Self`.
    ///
    /// Useful for folding traversals where the visitor state suffices.
    pub fn consume<'a, V, R>(visitor: &mut V, reader: &mut R) -> Result<(), V::Error>
    where
        V: Visitor<Row = T>,
        R: BufReader<'a>,
    {
        // Extract and visit the 'insert' operations.
        // Any row data returned by the visitor is discarded.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            for _ in 0..m {
                visitor.visit_insert(table_id, reader)?;
            }
        }

        // Extract and visit the 'delete' operations.
        // Any row data returned by the visitor is discarded.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            let m = decode_varint(reader)?;
            for _ in 0..m {
                visitor.visit_delete(table_id, reader)?;
            }
        }

        // Extract and visit the 'truncate' operations.
        let n = decode_varint(reader)?;
        for _ in 0..n {
            let table_id = reader.get_u32().map(TableId)?;
            visitor.visit_truncate(table_id)?;
        }

        Ok(())
    }
}

impl<T: Encode> Encode for Txdata<T> {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        self.encode(writer)
    }
}

/// An operation (insert or delete) on a given table.
#[derive(Clone, Debug, PartialEq)]
pub struct Ops<T> {
    /// The table this operation applies to.
    pub table_id: TableId,
    /// The list of full rows inserted or deleted.
    pub rowdata: Arc<[T]>,
}

impl<T: Encode> Ops<T> {
    /// Encode `self` in the canonical format to the given buffer.
    pub fn encode(&self, buf: &mut impl BufWriter) {
        buf.put_u32(self.table_id.0);
        encode_varint(self.rowdata.len(), buf);
        for row in self.rowdata.iter() {
            Encode::encode_record(row, buf);
        }
    }
}

#[derive(Debug, Error)]
pub enum DecoderError<V> {
    #[error("unsupported version: {given} supported={supported}")]
    UnsupportedVersion { supported: u8, given: u8 },
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Visitor(V),
    #[error(transparent)]
    Traverse(#[from] error::Traversal),
}

/// A free standing implementation of [`crate::Decoder::skip_record`]
/// specifically for `Txdata<ProductValue>`.
pub fn skip_record_fn<'a, V, R>(visitor: &mut V, version: u8, reader: &mut R) -> Result<(), DecoderError<V::Error>>
where
    V: Visitor,
    V::Row: Encode,
    R: BufReader<'a>,
{
    if version > Txdata::<V::Row>::VERSION {
        return Err(DecoderError::UnsupportedVersion {
            supported: Txdata::<V::Row>::VERSION,
            given: version,
        });
    }
    Txdata::skip(visitor, reader).map_err(DecoderError::Visitor)?;

    Ok(())
}

/// A free standing implementation of [`crate::Decoder::decode_record`], which
/// drives the supplied [`Visitor`].
///
/// Simplifies decoder implementations operating on [`Txdata`], as the
/// implementation only needs to care about managing the [`Visitor`] (which may
/// be behind a lock).
pub fn decode_record_fn<'a, V, R>(
    visitor: &mut V,
    version: u8,
    tx_offset: u64,
    reader: &mut R,
) -> Result<Txdata<V::Row>, DecoderError<V::Error>>
where
    V: Visitor,
    V::Row: Encode,
    R: BufReader<'a>,
{
    process_record(visitor, version, tx_offset, reader, Txdata::decode)
}

/// Variant of [`decode_record_fn`] which expects, but doesn't allocate
/// [`Txdata`] records.
pub fn consume_record_fn<'a, V, R>(
    visitor: &mut V,
    version: u8,
    tx_offset: u64,
    reader: &mut R,
) -> Result<(), DecoderError<V::Error>>
where
    V: Visitor,
    V::Row: Encode,
    R: BufReader<'a>,
{
    process_record(visitor, version, tx_offset, reader, Txdata::consume)
}

fn process_record<'a, V, R, F, T>(
    visitor: &mut V,
    version: u8,
    tx_offset: u64,
    reader: &mut R,
    decode_txdata: F,
) -> Result<T, DecoderError<V::Error>>
where
    V: Visitor,
    V::Row: Encode,
    R: BufReader<'a>,
    F: FnOnce(&mut V, &mut R) -> Result<T, V::Error>,
{
    if version > Txdata::<V::Row>::VERSION {
        return Err(DecoderError::UnsupportedVersion {
            supported: Txdata::<V::Row>::VERSION,
            given: version,
        });
    }
    visitor.visit_tx_start(tx_offset).map_err(DecoderError::Visitor)?;
    let record = decode_txdata(visitor, reader).map_err(DecoderError::Visitor)?;
    visitor.visit_tx_end().map_err(DecoderError::Visitor)?;

    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use proptest::prelude::*;
    use spacetimedb_sats::{product, AlgebraicType, ProductType, ProductValue};

    fn gen_table_id() -> impl Strategy<Value = TableId> {
        any::<u32>().prop_map(TableId)
    }

    fn gen_ops(pv: ProductValue) -> impl Strategy<Value = Ops<ProductValue>> {
        (gen_table_id(), prop::collection::vec(Just(pv), 1..10)).prop_map(|(table_id, rowdata)| Ops {
            table_id,
            rowdata: rowdata.into(),
        })
    }

    fn gen_mutations(pv: ProductValue) -> impl Strategy<Value = Mutations<ProductValue>> {
        (
            prop::collection::vec(gen_ops(pv.clone()), 0..10),
            prop::collection::vec(gen_ops(pv.clone()), 0..10),
            prop::collection::vec(gen_table_id(), 0..10),
        )
            .prop_map(|(inserts, deletes, truncates)| Mutations {
                inserts: inserts.into(),
                deletes: deletes.into(),
                truncates: truncates.into(),
            })
    }

    fn gen_txdata(pv: ProductValue) -> impl Strategy<Value = Txdata<ProductValue>> {
        (
            prop::option::of(any::<Inputs>()),
            prop::option::of(any::<Outputs>()),
            prop::option::of(gen_mutations(pv)),
        )
            .prop_map(|(inputs, outputs, mutations)| Txdata {
                inputs,
                outputs,
                mutations,
            })
    }

    static SOME_PV: Lazy<ProductValue> = Lazy::new(|| product![42u64, "kermit", 4u32, 2u32, 18u32]);
    static SOME_PV_TY: Lazy<ProductType> = Lazy::new(|| {
        ProductType::from([
            ("id", AlgebraicType::U64),
            ("name", AlgebraicType::String),
            ("x", AlgebraicType::U32),
            ("y", AlgebraicType::U32),
            ("z", AlgebraicType::U32),
        ])
    });

    struct MockVisitor;

    impl Visitor for MockVisitor {
        type Error = DecodeError;
        type Row = ProductValue;

        fn visit_insert<'a, R: BufReader<'a>>(
            &mut self,
            _table_id: TableId,
            reader: &mut R,
        ) -> Result<Self::Row, Self::Error> {
            ProductValue::decode(&SOME_PV_TY, reader)
        }

        fn visit_delete<'a, R: BufReader<'a>>(
            &mut self,
            _table_id: TableId,
            reader: &mut R,
        ) -> Result<Self::Row, Self::Error> {
            ProductValue::decode(&SOME_PV_TY, reader)
        }

        fn skip_row<'a, R: BufReader<'a>>(&mut self, _table_id: TableId, reader: &mut R) -> Result<(), Self::Error> {
            ProductValue::decode(&SOME_PV_TY, reader)?;
            Ok(())
        }
    }

    proptest! {
        #[test]
        fn prop_inputs_roundtrip(inputs in any::<Inputs>()) {
            let mut buf = Vec::new();
            inputs.encode(&mut buf);
            assert_eq!(inputs, Inputs::decode(&mut buf.as_slice()).unwrap());
        }

        #[test]
        fn prop_outputs_roundtrip(outputs in any::<Outputs>()) {
            let mut buf = Vec::new();
            outputs.encode(&mut buf);
            assert_eq!(outputs, Outputs::decode(&mut buf.as_slice()).unwrap());
        }

        #[test]
        fn prop_mutations_roundtrip(muts in gen_mutations(SOME_PV.clone())) {
            let mut buf = Vec::new();
            muts.encode(&mut buf);
            assert_eq!(muts, Mutations::decode(&mut MockVisitor, &mut buf.as_slice()).unwrap());
        }

        #[test]
        fn prop_txdata_roundtrip(txdata in gen_txdata(SOME_PV.clone())) {
            let mut buf = Vec::new();
            txdata.encode(&mut buf);
            assert_eq!(txdata, Txdata::decode(&mut MockVisitor, &mut buf.as_slice()).unwrap());
        }
    }
}
