# WebSocket compression policy

Binary messages retain the existing one-byte compression tag. Compression is
applied only when the serialized body exceeds 1,024 bytes; the tag is not counted.
Text messages and the client-selected `None` mode are unchanged.

## Brotli

Brotli remains the default codec. The encoder uses quality 2. The resulting
standard Brotli stream is readable by existing clients, including streams
produced by the parallel encoder.

Bodies below 512 KiB use the synchronous encoder. At or above 512 KiB, eligible
jobs use two compression lanes; at or above 64 MiB they may use four. Counts are
capped by available parallelism. Parallel jobs share a process-wide CPU-slot
budget of **50% of available CPUs, rounded down**. The capacity is initialized
when first needed from the CPU quota/affinity visible to the encode worker;
restart the server after changing CPU limits to recalculate it.

Slots account for all threads participating in an MT job, including its
coordinator. Brotli reserves one per lane; Zstd reserves one per additional
worker plus its coordinator. A reservation can grant fewer threads than requested,
but needs at least two slots. Jobs never wait for capacity: when fewer than two
slots are free they compress synchronously with the same codec. On machines with
fewer than four available CPUs the half-CPU budget therefore disables MT.

For example, 32 available CPUs give 16 slots. These can admit eight two-lane
Brotli jobs, four four-lane Brotli jobs, five Zstd jobs with two additional workers
each, or mixtures within the same total. No cores are pinned or reserved by this
accounting. This is a limit on concurrent MT thread allocations, **not a cap on
total server compression CPU**: synchronous work still uses the existing executor.

Core-pinning builds stay synchronous: codec child threads inherit their caller's
affinity, and must not silently contend on one pinned CPU or use reserved CPUs.
This policy does not change the existing async/Rayon encode scheduling or
compress subscription results while holding database transaction locks.

The Brotli MT API needs a maximum-sized temporary output allocation. It is
released after compression rather than retained in each connection's reusable
buffer. Input bytes are shared with workers without copying the serialized body.
Parallel compression therefore still increases transient memory and CPU use.
Several large MT jobs can coexist under the CPU-slot budget, each with its own
temporary buffers; there is no separate byte-based memory budget. The slot limit
is not a guarantee of lower process memory or improved throughput.

### Measurement rationale and limitations

Exploratory release-build measurements used a synthetic 500 MiB BSATN snapshot
with sequential IDs, a repeated category, xorshift values and zero padding. The
quality-2 compression stage took about 2,067 / 1,052 / 532 ms using the same MT
API with 1 / 2 / 4 lanes. A separate fresh-pool size sweep first showed a two-lane
pipeline win around 128–256 KiB; the 512 KiB guard leaves margin. Four lanes can
increase aggregate CPU for medium messages, so they are reserved for much larger
payloads. These are single-host synthetic findings, not a promise of production
speedup. Quality 1 can use less aggregate CPU than quality 2.

Regression tests verify synchronous/parallel decoder compatibility, exact size
cutoffs, CPU caps, concurrent/partial reservations, unwind cleanup,
v2/coalesced-v3 payload equality, and serialization-buffer reclamation.
Production throughput, peak memory and real application payloads should also be
evaluated when changing this policy.

## Opt-in Zstandard

Servers supporting this extension accept the existing connection query parameter
with `compression=Zstd`. The Rust SDK requests it explicitly through
`.with_compression(spacetimedb_sdk::Compression::Zstd)`. Brotli remains the default;
no existing connection is automatically switched to Zstd. Other SDKs retain
their current codecs. Requesting Zstd from an older server fails negotiation;
use Brotli/gzip/None when connecting to servers without this extension.

Zstd-compressed binary frames use tag **3**, followed by one ordinary Zstd frame
with a content checksum and no dictionary. Tags 0 (None), 1 (Brotli), and 2 (gzip)
are unchanged. The body and its v1/v2/v3 BSATN schema are unchanged. In particular,
v1's inner query-update enum is not extended: Zstd operates on the outer frame.
V3 still coalesces serialized messages before compressing them together.

The encoder uses **fast-2 (level -2)**. Bodies below 4 MiB stay synchronous; bodies
at or above 4 MiB may use two additional workers, and at or above 64 MiB may use
four. Zstd counts workers in addition to the coordinator, unlike Brotli's total
lane count. Workers are capped at available parallelism minus one. Both codecs
share the same CPU-slot budget and core-pinning fallback. A partial reservation
can reduce Zstd's worker count, including to one additional worker. Thread counts
are a server CPU policy, not a client-controlled query parameter.

Exploratory fresh-context measurements first showed Zstd fast-2/level-1 MT wins
at around 4 MiB, not at the 512 KiB Brotli cutoff. At 2 MiB, the measured fast-2
pipeline was 2.925 ms synchronously vs 4.288 ms with two workers; at 4 MiB it was
6.712 vs 4.785 ms, and at 64 MiB 77.553 vs 47.938 ms. These synthetic, single-host
findings motivate separate codec cutoffs, not a universal speedup guarantee.

Native Rust SDK targets use libzstd to decode. The Rust browser target uses a
pure-Rust decoder so it does not require a native C library. Codec/frame tests
exercise both decoders against synchronous and multithreaded frames and check
truncation/checksum errors. A native SDK test passes actual server-serializer
frames through the SDK parser for all four codecs, including small uncompressed
and large compressed payloads. This tests serializer/parser interoperability,
not a complete live-network subscription benchmark. The browser build is checked
separately; that alone does not constitute browser runtime testing.

Adding a Rust enum variant is additive on the wire but can require downstream
Rust exhaustive matches to add a `Zstd` arm. No protocol defaults or persisted
data formats change. Dictionaries, LZ4, Arrow, planner changes, and the earlier
experimental SDK decompression changes are not part of this extension.
