---
title: Commitlog
slug: /reference/internals/commitlog
---

# Commitlog

The commitlog is the write-ahead log (WAL) used by SpacetimeDB to persist all committed transactions. As an in-memory database, SpacetimeDB relies entirely on this log for durability. Every committed transaction is written to the commitlog before it is considered durable, and the full state of any database can be reconstructed by replaying the log from the beginning.

This page describes the on-disk format, integrity model, and indexing strategy of the commitlog.

## Overview

The commitlog serves several purposes:

- **Durability.** It is the sole record of committed state. If the process terminates unexpectedly, the database is recovered by replaying the log.
- **Replication.** The log is streamed to replicas in the same binary format used on disk, requiring no re-encoding.
- **Historical reconstruction.** Because the log is never compacted, any past database state can be reconstructed by replaying a prefix of the log up to a given transaction offset.

## Terminology

| Term | Meaning |
|------|---------|
| **Commitlog** | The full sequence of committed transaction data, stored across one or more segment files. |
| **Commit** | A single entry in the commitlog. Contains one or more transaction records. |
| **Segment** | A single file within the commitlog. Segments are created when the current file approaches a configurable maximum size. |
| **Transaction offset** | A monotonically increasing, gapless sequence number assigned to each transaction record. Starts at zero. |
| **Epoch** | The replication leader term. Used alongside the transaction offset to uniquely identify a commit within a replication group. |

## Segments

The commitlog is stored as a sequence of files called segments. Each segment is named after the minimum transaction offset it contains, left-padded with zeros to 20 characters, with the file extension `.stdb.log`. For example:

```
00000000000000000000.stdb.log
00000000000000010000.stdb.log
00000000000000020000.stdb.log
```

A new segment is created when the encoded size of the next commit would exceed a configurable maximum segment size. The default maximum is 1 GiB (1,073,741,824 bytes).

Because segment boundaries are determined by commit size, segments may be slightly smaller or larger than the maximum. A commit whose encoded size exceeds the maximum is written to a dedicated segment.

Segments may be compressed using the [zstd seekable format](https://github.com/facebook/zstd/tree/dev/contrib/seekable_format). This compression format preserves random access, so the offset index remains valid after compression.

### Segment Header

Each segment begins with a 10-byte header:

```
Bytes 0-5:  Magic       "(ds)^2"      — represents (Δs)², a spacetime interval
Byte  6:    Format version  "0" or "1"
Byte  7:    Checksum algo   "0"       — CRC32C
Bytes 8-9:  Reserved        "00"
```

Format version 0 is the original format, which does not include an `epoch` field in commits. Format version 1 (the current default) adds the `epoch` field to each commit. Software must reject segments with a format version higher than what it supports. Because `epoch` has a default value of zero, software supporting version 1 can also read version 0 segments.

## Commit Format

Following the header, a segment contains zero or more commits. Each commit has the following binary layout:

```
┌──────────────────┬───────┬─────┬─────┬──────────────┬──────┐
│ min-tx-offset    │ epoch │  n  │ len │   records    │ crc  │
│     (u64)        │ (u64) │(u16)│(u32)│  (len bytes) │(u32) │
└──────────────────┴───────┴─────┴─────┴──────────────┴──────┘
```

All integers are encoded in little-endian byte order.

| Field | Size | Description |
|-------|------|-------------|
| `min-tx-offset` | 8 bytes | The transaction offset of the first record in this commit. Starts at zero and increments by `n` for each successive commit. |
| `epoch` | 8 bytes | The replication leader epoch (term). Zero indicates single-node mode. Present only in format version 1 and later; in version 0, the epoch is implicitly zero and the commit header is 8 bytes shorter. |
| `n` | 2 bytes | The number of transaction records in this commit. |
| `len` | 4 bytes | The byte length of the `records` payload. Allows skipping over the payload without decoding it. |
| `records` | `len` bytes | The encoded transaction data. See [Transaction Records](#transaction-records). |
| `crc` | 4 bytes | A CRC32C checksum computed over all preceding fields of this commit. |

The `min-tx-offset` of each commit must equal `min-tx-offset + n` of the preceding commit. This invariant holds across segment boundaries.

### Epoch

The `epoch` field supports distributed replication. Within a replication group, all nodes must eventually store identical commitlogs containing the same sequence of `(offset, txdata)` pairs. The replication protocol guarantees that commits are proposed by a single, majority-elected leader, and that followers accept commits only from the elected leader.

The tuple `(epoch, offset)` uniquely identifies a commit within a replication group. The triple `(epoch, offset, crc)` additionally verifies the integrity of the payload.

A zero epoch indicates single-node mode. A single-node database can be transitioned into a replicated database without changes to the commitlog, because the epoch has a default value of zero and the format is backwards-compatible.

The commitlog writer must reject any attempt to set the epoch to a value smaller than the current epoch.

## Transaction Records

The `records` payload of a commit contains `n` transaction records. The payload is defined and interpreted by the datastore (not the commitlog itself).

Each committed datastore transaction is encoded into a record called `txdata` with the following structure:

```
┌───────┬──────────┬─────────┬───────────┐
│ flags │ [inputs] │[outputs]│[mutations]│
│ (u8)  │          │         │           │
└───────┴──────────┴─────────┴───────────┘
```

The `flags` byte is a bitfield. The three most significant bits indicate which sections are present:

| Mask | Meaning |
|------|---------|
| `0x80` | Inputs present |
| `0x40` | Outputs present |
| `0x20` | Mutations present |
| `0x1f` | Reserved |

Each section is present only if its corresponding bit is set.

### Inputs

The inputs section records the reducer call that produced this transaction:

```
inputs = len(u32) slen(u8) slen(reducer-name) reducer-args
```

The section starts with a 4-byte (`u32`) length prefix covering the entire inputs payload (excluding the length prefix itself). Within this payload, the reducer name is encoded as a length-prefixed UTF-8 string with a 1-byte (`u8`) length prefix (maximum 255 bytes). The remaining bytes after the reducer name are the reducer arguments, encoded as a contiguous BSATN byte string.

The arguments can only be decoded using a runtime representation of the WASM module in effect at that transaction offset, as the argument types are determined by the reducer signature.

### Outputs

The outputs section consists of a length-prefixed UTF-8 string (1-byte length prefix, maximum 255 bytes) representing an error result from the reducer, if any.

### Mutations

The mutations section records the database state modifications that constitute the transaction:

```
mutations = inserts deletes truncates
```

Each sub-section uses varint-encoded counts (variable-length unsigned integers):

- **Inserts**: A varint count of table groups, then for each group: a `u32` table ID, a varint row count, and then that many rows encoded as BSATN.
- **Deletes**: Same structure as inserts. Each row is encoded in full (whole-row content, not pointers).
- **Truncates**: A varint count followed by that many `u32` table IDs whose contents were cleared.

Delete operations store the full row content rather than row pointers, because row IDs are not stable across restarts.

#### Ordering and Atomicity

The ordering of operations within the mutations section does not matter. SpacetimeDB treats all mutation operations of a committed transaction as having occurred instantly and atomically. This has two important implications:

1. Intermediate transaction state is not stored in or recoverable from the commitlog. While a transaction is running, the datastore maintains in-memory state for that transaction, but after commit, only the final set of mutations is persisted.

2. References to the same row across `inserts`, `deletes`, and `truncates` within a single transaction must be mutually exclusive. If a row appears in both an insert and a delete for the same table, the replay order would be ambiguous.

Although mutations within a single table are unordered, mutations across tables are stored in ascending order by table ID. This ordering supports schema migrations, where system table updates must be applied before changes to user tables.

All mutations within a transaction are written using the final schema of that transaction. If a schema change occurs mid-transaction (for example, adding a column), all row data in the commit reflects the post-migration schema.

## Prerequisites for Replay

A stated goal of the commitlog is that a database can be fully reconstructed from the log alone. This requires that:

1. System tables and their contents are stored in the log.
2. The module program data (WASM blob) is stored in the system tables and, by extension, in the log.

To bootstrap replay, the reader must have built-in knowledge of the `st_table` and `st_columns` system table schemas. The schemas of these two tables must remain stable across versions.

## Integrity

The commitlog is append-only. Commits are written in FIFO order as received from the transaction engine. The implementation may buffer commits in memory before writing to disk. Flushing and syncing (via `fsync`) is managed by a higher-level component, allowing users to trade durability for throughput.

A segment must be flushed and synced to disk before the next segment is created. On failure, the log rejects further writes.

Because not every commit is individually synced, partial writes may occur at the end of the log after an unexpected termination. Before resuming writes, the commitlog must verify that the last segment is intact, truncating it to the last valid commit if necessary.

Every other detectable inconsistency is considered a fatal error.

### Integrity Checking

A thorough consistency check involves traversing the commitlog from the beginning and:

1. Computing the CRC32C of each commit and verifying it against the stored value.
2. Verifying that `min-tx-offset` values are strictly sequential (`previous.min-tx-offset + previous.n == current.min-tx-offset`) across all commits and segment boundaries.
3. Verifying that the first offset in each segment matches that segment's file name.

Under some circumstances, it may be acceptable to seek to the last recorded offset via the offset index and check only the suffix of the final segment.

## Offset Index

To support random access into the commitlog, an offset index is maintained alongside each segment. The index maps transaction offsets to byte positions within the corresponding segment file.

### Index Files

Each segment has an associated index file with the same name but the file extension `.stdb.ofs`:

```
00000000000000000000.stdb.log
00000000000000000000.stdb.ofs
00000000000000010000.stdb.log
00000000000000010000.stdb.ofs
```

Index files are pre-allocated and accessed via memory mapping (`mmap`). The required space is determined from the segment's maximum size and the index interval configuration parameter.

Like segment files, index files are never compacted. When moving segments to cold storage, the associated index files may be discarded, because they can be rebuilt from the segment data.

### Index Entries

Each index entry is 16 bytes:

| Bytes | Field | Description |
|-------|-------|-------------|
| 0-7 | Transaction offset | The `min-tx-offset` of the indexed commit (u64, little-endian) |
| 8-15 | Byte offset | The position of the commit within the segment file (u64, little-endian) |

Entries are written back-to-back. An all-zero entry indicates unused, pre-allocated space (the zeroth commit is never indexed).

### Write Behavior

Index entries are written on a best-effort basis whenever a configurable number of bytes have been flushed to the segment (default: 4,096 bytes). The index is written asynchronously with respect to the database engine and does not block transaction processing.

Only the index file for the currently active segment is open for writing. When a segment is closed, its index file becomes immutable.

Errors writing to the index are not propagated. Segment writing continues normally regardless of index write failures.

### Read Behavior

Readers must not assume that the index is fully consistent with its segment. In particular, a byte offset in the index may point beyond the current end of the segment file if the index was updated before the segment was synced. Readers should handle this gracefully.

Finding the byte offset for a given transaction offset is performed via binary search over the index entries. Because the dominant access pattern is reading near the end of the log (for replication and snapshot recovery), implementations may benefit from probing sections near the end of the index first.

### Configuration

The offset index is controlled by two parameters:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `offset_index_interval_bytes` | 4,096 | An index entry is written whenever this many bytes have been flushed to the active segment. |
| `offset_index_require_segment_fsync` | false | If true, the segment must be synced to disk before an index entry is written. |

## Wire Format

The commitlog's wire format is identical to its on-disk format. No re-encoding is required on the producer side. The receiver is responsible for integrity checking.

A commitlog is typically sent as a contiguous stream. Producers may concatenate physical segments, which means receivers may encounter segment headers in the stream and should ignore them.

Producers must support sending a log stream starting from a particular transaction offset. In this case, the stream begins at the commit containing the requested offset (which may be greater than the commit's `min-tx-offset` but less than the next commit's `min-tx-offset`).

## Versioning

Because commitlogs are never compacted, implementations must remain backwards-compatible. Any amendment that would prevent an older implementation from reading a newer segment requires incrementing the `log-format-version` in the segment header. Implementations must abort upon encountering a format version higher than the latest they support.

The current format versions are:

| Version | Description |
|---------|-------------|
| 0 | Original format |
| 1 | Adds the `epoch` field to each commit |
