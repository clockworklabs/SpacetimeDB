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
capped by available parallelism. One shared process-wide allowance limits the
number of parallel compression jobs to one. If it is occupied, compression
falls back to the same codec synchronously, without waiting for a permit.

Core-pinning builds stay synchronous: codec child threads inherit their caller's
affinity, and must not silently contend on one pinned CPU or use reserved CPUs.
This policy does not change the existing async/Rayon encode scheduling or
compress subscription results while holding database transaction locks.

The Brotli MT API needs a maximum-sized temporary output allocation. It is
released after compression rather than retained in each connection's reusable
buffer. Input bytes are shared with workers without copying the serialized body.
Parallel compression therefore still increases transient memory and CPU use;
the concurrency budget is not a guarantee of lower process memory or throughput.

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
cutoffs, CPU caps, nonblocking budget cleanup, v2/coalesced-v3 payload equality,
and serialization-buffer reclamation. Production throughput, peak memory and
real application payloads should also be evaluated when changing this policy.
