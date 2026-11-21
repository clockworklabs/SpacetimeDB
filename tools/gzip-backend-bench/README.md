# Isolated gzip backend comparison

This deliberately separate Cargo workspace prevents server/SDK test dependencies
from silently selecting a different flate2 backend. Select **exactly one** feature;
the binary rejects zero or multiple selections. These features control the only
flate2 dependency in this isolated graph, with its default features disabled.

## Reproduce

Run these sequentially, not concurrently with each other or a compiler workload:

```sh
cargo run --manifest-path tools/gzip-backend-bench/Cargo.toml --release \
  --no-default-features --features miniz --locked -- --large-initial-response

cargo run --manifest-path tools/gzip-backend-bench/Cargo.toml --release \
  --no-default-features --features zlib-rs --locked -- --large-initial-response

cargo run --manifest-path tools/gzip-backend-bench/Cargo.toml --release \
  --no-default-features --features zlib-ng --locked -- --large-initial-response
```

The large case needs multi-GiB transient RAM. Omit the flag for small/4 MiB cases.
Verify each feature graph, for example:

```sh
cargo tree --manifest-path tools/gzip-backend-bench/Cargo.toml \
  --no-default-features --features zlib-ng --invert flate2 --depth 0 \
  --format '{p} features={f}' --locked

cargo test --manifest-path tools/gzip-backend-bench/Cargo.toml --release \
  --no-default-features --features zlib-ng --locked
```

Repeat tree/test commands with `miniz` and `zlib-rs`. The verified feature sets
were `any_impl,miniz_oxide,rust_backend`,
`any_impl,any_zlib,libz-rs-sys,zlib-rs`, and
`any_impl,any_zlib,libz-ng-sys,zlib-ng`, respectively. No compat-mode backend or
other workspace dependencies participate. The native production graph can also
contain miniz features, but explicit zlib-rs takes precedence in the locked
flate2 implementation. Both native and browser production builds now select
pure-Rust zlib-rs; zlib-ng is retained only as an isolated comparison option.

## Workload and methodology

- flate2 **1.1.4**, miniz_oxide **0.8.9**, libz-rs-sys/zlib-rs **0.5.5**,
  libz-ng-sys **1.1.31** (comparison only, not a production dependency).
- Rust 1.97.1 release builds, opt-level 3, thin LTO, 16 codegen units, default
  allocator. Native zlib-ng uses its bundled library and CPU dispatch.
- Backend comparison below: gzip level **1** (`Compression::fast()`),
  single-threaded for every backend. The additional level sweep is separate.
- Same deterministic synthetic 64-byte rows as the previous snapshot experiment:
  three u64 fields (sequential ID, category modulo 16, xorshift value) and 40 zero
  bytes. Random cases fill all field bytes. This is not captured production data.
- Fixtures contain explicit, fixed current v2 BSATN update/snapshot envelopes
  (47/33 bytes), generated locally without linking server dependencies. This tool
  is not a protocol implementation or an end-to-end SDK benchmark. Its seed is
  12345.
- Warmup roundtrip, then **1,000 iterations** for tiny messages and **5** for
  each snapshot. Report medians of operation timings. Both encoding/decoding
  include fresh context construction, output allocation and finalization/checksum
  validation. Fixture construction, output destruction, equality checks, disk I/O,
  BSATN parsing and client-cache work are outside timing.
- Built/copied binaries first, waited for compiler workloads to finish, then ran
  miniz → zlib-rs → zlib-ng sequentially. Repeated in reverse order. Results below
  are the first pass; the repeat preserved the main large-message tradeoffs.

## Results

500 MiB row snapshot (524,288,033 BSATN bytes), compressed sizes in MiB:

| Backend | Gzip bytes | Gzip MiB | Encode ms | Decode ms |
| --- | ---: | ---: | ---: | ---: |
| miniz | 124,255,176 | 118.50 | 1,135.0 | 515.3 |
| zlib-rs | 136,427,904 | 130.11 | 635.3 | 317.4 |
| zlib-ng | 136,427,904 | 130.11 | 638.9 | 317.4 |

Reverse-order repeat: miniz encoded in 1,156.3 ms, zlib-rs in 631.8 ms, zlib-ng
in 641.6 ms. Payload sizes were unchanged. Thus zlib-ng was about **44% faster
to encode and 38% faster to decode than miniz**, but its output was **9.8% larger**.
It was not faster than zlib-rs on this large fixture; their compressed sizes were
identical.

4 MiB snapshot (4,194,337 raw bytes):

| Backend | Gzip bytes | Encode ms | Decode ms |
| --- | ---: | ---: | ---: |
| miniz | 995,275 | 9.080 | 4.139 |
| zlib-rs | 1,092,762 | 5.001 | 2.450 |
| zlib-ng | 1,092,762 | 5.527 | 2.696 |

Small messages:

| Case | Raw bytes | miniz gzip | zlib-rs gzip | zlib-ng gzip |
| --- | ---: | ---: | ---: | ---: |
| One row | 111 | 77 | 68 | 68 |
| Four rows | 303 | 139 | 119 | 119 |
| Four random rows | 303 | 326 | 326 | 326 |

Four-row encode medians were 37.5 / 2.65 / 1.71 us in the first pass and
38.5 / 3.06 / 1.52 us in the repeat (miniz / zlib-rs / zlib-ng). Context allocation
and the allocator strongly influence these tiny-message measurements. They are
not predictions of whole-server latency. This codec-only tool bypasses the
server's compression threshold.

Every timed decode verifies exact bytes outside timing. All three isolated
backend test runs passed fixture-size, roundtrip and corrupted-trailer checks.

## Zlib-rs level/size tradeoff

```sh
cargo run --manifest-path tools/gzip-backend-bench/Cargo.toml --release \
  --no-default-features --features zlib-rs --locked -- \
  --levels=1,2,3,4,6 --large-initial-response
```

The CSV includes a `level` column. Without `--levels`, the benchmark retains
level 1. The sweep uses the same fixtures, timings and five snapshot repetitions
as above; it does not construct a different 500 MiB input per level.

| Zlib-rs level | 500 MiB snapshot gzip bytes | MiB | Encode ms | Decode ms |
| --- | ---: | ---: | ---: | ---: |
| 1 | 136,427,904 | 130.11 | 638.2 | 322.7 |
| 2 | 118,676,384 | 113.18 | 1118.4 | 336.0 |
| 3 | 118,220,028 | 112.74 | 1561.3 | 333.1 |
| 4 | 109,353,920 | 104.29 | 1773.5 | 289.4 |
| 6 | 106,374,172 | 101.45 | 3313.2 | 280.7 |

A reverse-order 4/2/1 repeat gave encode medians of 1765.7 / 1106.1 / 639.5 ms,
with unchanged sizes. Level 2 saves **13%** relative to zlib-rs level 1 but takes
about **75% longer to encode**. Compared with the earlier miniz level-1 baseline
(118.50 MiB, 1135.0 ms), zlib-rs level 2 gives roughly the same encoding speed
with **4.5% less output**, not the speed gain of zlib-rs level 1.

Higher levels do not uniformly help tiny messages: the 111-byte fixture remains
68 gzip bytes at every tested level, while encoding rises from 2.42 us at level 1
to around 4.5–5.2 us. The four-row fixture is 119 / 116 / 110 / 112 / 107 bytes at
levels 1/2/3/4/6; encode times are 2.66 / 6.55 / 6.22 / 6.56 / 7.47 us. Random
four-row output remains 326 bytes at every level, with additional CPU at higher
levels. Sizes are not strictly monotonic with level on every fixture.

All three isolated backend tests now exercise levels 1, 2, 3, 4 and 6, including
roundtrips and corrupted-trailer rejection. Production remains **zlib-rs level 1**
on native and browser targets; no gzip-level runtime configuration was introduced.
Level 2 is a possible bandwidth/CPU compromise for large responses, not a free
feature toggle or a demonstrated better global default for small live updates.

Conclusion: select the pure-Rust zlib-rs backend for production and retain level 1
for speed. There is no measured setting that preserves zlib-rs level-1 speed and
also obtains the smaller outputs of its higher levels. Validate any policy change
with real payloads and production CPU/network constraints. These measurements do
not establish that gzip overtakes the earlier Zstd/Brotli results.
