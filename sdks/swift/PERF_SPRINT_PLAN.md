# Swift SDK Perf Sprint Plan

## Goal

Close the measured throughput gap in the Swift SDK hot paths (BSATN encode/decode, cache mutation, keynote TPS path) with repeatable evidence.

## Baseline Discipline

- Always benchmark release builds (`swift -c release`, `cargo --release`).
- Record exact command lines, host info, commit SHA, and server binary/version.
- Use the same contention/workload knobs when comparing SDKs.
- Run at least 3 samples and report min/avg/max.

## Priority Work Items

1. Encode fast paths (highest impact)
- Add bulk primitive array encode paths (`[UInt8]`, `[Int32]`, `[UInt32]`, `[Float]`, `[Int64]`, `[UInt64]`, `[Double]`) using contiguous writes.
- Hoist type checks outside per-element loops.
- Keep fallback generic/special-encodable paths for correctness.

2. Decode fast paths
- Maintain zero-copy/low-copy read paths while preserving compatibility helpers (`read`, `readBytes`, `readArray`, `readTaggedEnum`).
- Add typed bulk decode branches where safe and measurable.
- Avoid unsafe array reinterprets that can violate type/layout guarantees.

3. Primitive hook routing
- Ensure `encodeIfPresent`/`decodeIfPresent` and single-value containers use explicit primitive readers/writers.
- Avoid generic fallback for primitive Codable hooks.

4. Cache hot path
- Profile `TableCache` insert/delete and reduce per-row overhead where possible.
- Add targeted cache microbench coverage for realistic batch sizes.

5. Keynote TPS client path
- Reduce actor-hopping/synchronization overhead in hot send/ack loops.
- Keep workload knobs aligned with comparison methodology.

## Validation Matrix

- `swift test --package-path sdks/swift`
- `swift package --package-path sdks/swift benchmark --target SpacetimeDBBenchmarks --no-progress`
- Keynote TPS: 3-run sample (Swift + Rust clients), same server and knobs.

## Reporting Template

- Throughput table for:
  - BSATN read/write
  - Message encode/decode
  - Cache insert/delete
  - Round-trip reducer/procedure
  - Keynote TPS
- Include deltas vs previous baseline and vs comparison SDK claims.
- Explicitly call out methodology differences when claims are not directly comparable.
