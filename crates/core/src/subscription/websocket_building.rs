use bytestring::ByteString;
use core::mem;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, BsatnRowList, CompressableQueryUpdate, Compression, JsonFormat, QueryUpdate, RowOffset, RowSize,
    RowSizeHint, WebsocketFormat,
};
use spacetimedb_sats::bsatn::{self, ToBsatn};
use spacetimedb_sats::ser::serde::SerializeWrapper;
use spacetimedb_sats::Serialize;
use std::io;
use std::io::Write as _;

/// A list of rows being built.
pub trait RowListBuilder: Default {
    type FinishedList;

    /// Push a row to the list in a serialized format.
    fn push(&mut self, row: impl ToBsatn + Serialize);

    /// Finish the in flight list, throwing away the capability to mutate.
    fn finish(self) -> Self::FinishedList;
}

pub trait BuildableWebsocketFormat: WebsocketFormat {
    /// The builder for [`Self::List`].
    type ListBuilder: RowListBuilder<FinishedList = Self::List>;

    /// Encodes the `elems` to a list in the format and also returns the length of the list.
    fn encode_list<R: ToBsatn + Serialize>(elems: impl Iterator<Item = R>) -> (Self::List, u64) {
        let mut num_rows = 0;
        let mut list = Self::ListBuilder::default();
        for elem in elems {
            num_rows += 1;
            list.push(elem);
        }
        (list.finish(), num_rows)
    }

    /// Convert a `QueryUpdate` into `Self::QueryUpdate`.
    /// This allows some formats to e.g., compress the update.
    fn into_query_update(qu: QueryUpdate<Self>, compression: Compression) -> Self::QueryUpdate;
}

impl BuildableWebsocketFormat for JsonFormat {
    type ListBuilder = Self::List;

    fn into_query_update(qu: QueryUpdate<Self>, _: Compression) -> Self::QueryUpdate {
        qu
    }
}

impl RowListBuilder for Vec<ByteString> {
    type FinishedList = Self;
    fn push(&mut self, row: impl ToBsatn + Serialize) {
        let value = serde_json::to_string(&SerializeWrapper::new(row)).unwrap().into();
        self.push(value);
    }
    fn finish(self) -> Self::FinishedList {
        self
    }
}

/// A [`BsatnRowList`] that can be added to.
#[derive(Default)]
pub struct BsatnRowListBuilder {
    /// A size hint about `rows_data`
    /// intended to facilitate parallel decode purposes on large initial updates.
    size_hint: RowSizeHintBuilder,
    /// The flattened byte array for a list of rows.
    rows_data: Vec<u8>,
}

/// A [`RowSizeHint`] under construction.
pub enum RowSizeHintBuilder {
    /// We haven't seen any rows yet.
    Empty,
    /// Each row in `rows_data` is of the same fixed size as specified here
    /// but we don't know whether the size fits in `RowSize`
    /// and we don't know whether future rows will also have this size.
    FixedSizeDyn(usize),
    /// Each row in `rows_data` is of the same fixed size as specified here
    /// and we know that this will be the case for future rows as well.
    FixedSizeStatic(RowSize),
    /// The offsets into `rows_data` defining the boundaries of each row.
    /// Only stores the offset to the start of each row.
    /// The ends of each row is inferred from the start of the next row, or `rows_data.len()`.
    /// The behavior of this is identical to that of `PackedStr`.
    RowOffsets(Vec<RowOffset>),
}

impl Default for RowSizeHintBuilder {
    fn default() -> Self {
        Self::Empty
    }
}

impl RowListBuilder for BsatnRowListBuilder {
    type FinishedList = BsatnRowList;

    fn push(&mut self, row: impl ToBsatn + Serialize) {
        use RowSizeHintBuilder::*;

        // Record the length before. It will be the starting offset of `row`.
        let len_before = self.rows_data.len();
        // BSATN-encode the row directly to the buffer.
        row.to_bsatn_extend(&mut self.rows_data).unwrap();

        let encoded_len = || self.rows_data.len() - len_before;
        let push_row_offset = |mut offsets: Vec<_>| {
            offsets.push(len_before as u64);
            RowOffsets(offsets)
        };

        let hint = mem::replace(&mut self.size_hint, Empty);
        self.size_hint = match hint {
            // Static size that is unchanging.
            h @ FixedSizeStatic(_) => h,
            // Dynamic size that is unchanging.
            h @ FixedSizeDyn(size) if size == encoded_len() => h,
            // Size mismatch for the dynamic fixed size.
            // Now we must construct `RowOffsets` for all rows thus far.
            // We know that `size != 0` here, as this was excluded when we had `Empty`.
            FixedSizeDyn(size) => RowOffsets(collect_offsets_from_num_rows(1 + len_before / size, size)),
            // Once there's a size for each row, we'll just add to it.
            RowOffsets(offsets) => push_row_offset(offsets),
            // First time a row is seen. Use `encoded_len()` as the hint.
            // If we have a static layout, we'll always have a fixed size.
            // Otherwise, let's start out with a potentially fixed size.
            // In either case, if `encoded_len() == 0`, we have to store offsets,
            // as we cannot recover the number of elements otherwise.
            Empty => match row.static_bsatn_size() {
                Some(0) => push_row_offset(Vec::new()),
                Some(size) => FixedSizeStatic(size),
                None => match encoded_len() {
                    0 => push_row_offset(Vec::new()),
                    size => FixedSizeDyn(size),
                },
            },
        };
    }

    fn finish(self) -> Self::FinishedList {
        let Self { size_hint, rows_data } = self;
        let size_hint = match size_hint {
            RowSizeHintBuilder::Empty => RowSizeHint::RowOffsets([].into()),
            RowSizeHintBuilder::FixedSizeStatic(fs) => RowSizeHint::FixedSize(fs),
            RowSizeHintBuilder::FixedSizeDyn(fs) => match u16::try_from(fs) {
                Ok(fs) => RowSizeHint::FixedSize(fs),
                Err(_) => RowSizeHint::RowOffsets(collect_offsets_from_num_rows(rows_data.len() / fs, fs).into()),
            },
            RowSizeHintBuilder::RowOffsets(ro) => RowSizeHint::RowOffsets(ro.into()),
        };
        let rows_data = rows_data.into();
        BsatnRowList::new(size_hint, rows_data)
    }
}

fn collect_offsets_from_num_rows(num_rows: usize, size: usize) -> Vec<u64> {
    (0..num_rows).map(|i| i * size).map(|o| o as u64).collect()
}

impl BuildableWebsocketFormat for BsatnFormat {
    type ListBuilder = BsatnRowListBuilder;

    fn into_query_update(qu: QueryUpdate<Self>, compression: Compression) -> Self::QueryUpdate {
        let qu_len_would_have_been = bsatn::to_len(&qu).unwrap();

        match decide_compression(qu_len_would_have_been, compression) {
            Compression::None => CompressableQueryUpdate::Uncompressed(qu),
            Compression::Brotli => {
                let bytes = bsatn::to_vec(&qu).unwrap();
                let mut out = Vec::new();
                brotli_compress(&bytes, &mut out);
                CompressableQueryUpdate::Brotli(out.into())
            }
            Compression::Gzip => {
                let bytes = bsatn::to_vec(&qu).unwrap();
                let mut out = Vec::new();
                gzip_compress(&bytes, &mut out);
                CompressableQueryUpdate::Gzip(out.into())
            }
        }
    }
}

pub fn decide_compression(len: usize, compression: Compression) -> Compression {
    /// The threshold beyond which we start to compress messages.
    /// 1KiB was chosen without measurement.
    /// TODO(perf): measure!
    const COMPRESS_THRESHOLD: usize = 1024;

    if len > COMPRESS_THRESHOLD {
        compression
    } else {
        Compression::None
    }
}

pub fn brotli_compress(bytes: &[u8], out: &mut impl io::Write) {
    // We are optimizing for compression speed,
    // so we choose the lowest (fastest) level of compression.
    // Experiments on internal workloads have shown compression ratios between 7:1 and 10:1
    // for large `SubscriptionUpdate` messages at this level.
    const COMPRESSION_LEVEL: i32 = 1;

    let params = brotli::enc::BrotliEncoderParams {
        quality: COMPRESSION_LEVEL,
        ..<_>::default()
    };
    let reader = &mut &bytes[..];
    brotli::BrotliCompress(reader, out, &params).expect("should be able to BrotliCompress");
}

pub fn gzip_compress(bytes: &[u8], out: &mut impl io::Write) {
    let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().expect("should be able to gzip compress `bytes`");
}
