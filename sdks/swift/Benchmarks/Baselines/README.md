# Swift Benchmark Baselines

This directory defines the baseline capture format for `SpacetimeDBBenchmarks`.

## Profile Naming

Use a machine/profile baseline name so comparisons are meaningful across runs on the same hardware/toolchain.

Recommended pattern:

- `<platform>-<arch>-<os>-swift<version>`

Example:

- `macos-arm64-14.4-swift6.2`

## Capture Command

From repo root:

```bash
tools/swift-benchmark-baseline.sh <baseline-name> [benchmark-filter-regex]
```

Example:

```bash
tools/swift-benchmark-baseline.sh macos-arm64-14.4-swift6.2
```

Generated files:

- `sdks/swift/.benchmarkBaselines/SpacetimeDBBenchmarks/<baseline-name>/results.json`
  - raw `package-benchmark` baseline data (histograms + percentiles)
- `sdks/swift/Benchmarks/Baselines/captures/<baseline-name>/<timestamp>.baseline-results.json`
  - copied snapshot of the raw baseline result
- `sdks/swift/Benchmarks/Baselines/captures/<baseline-name>/<timestamp>.summary.json`
  - compact JSON summary (`jsonSmallerIsBetter` format)
- `sdks/swift/Benchmarks/Baselines/captures/<baseline-name>/<timestamp>.metadata.txt`
  - machine/toolchain/profile details + exact benchmark commands

`latest.*` aliases are also written in the same capture directory.

## Comparison Method

Compare two named baselines:

```bash
cd sdks/swift
swift package benchmark baseline compare <baseline-a> <baseline-b> --target SpacetimeDBBenchmarks --no-progress
```

Use baseline names with matching machine profile for regression checks.
