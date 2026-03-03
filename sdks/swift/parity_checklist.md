# Protocol Parity Checklist (Kotlin vs Swift)

This document tracks the parity level of protocol handling between the Kotlin and Swift SDKs after the Refactor.

| Feature | Kotlin (PR 4471) | Swift (New) | Status |
| :--- | :--- | :--- | :--- |
| **Domain Wrappers** | Value classes for IDs | Struct wrappers for IDs | ✅ Parity |
| **RequestId** | `RequestId(u32)` | `RequestId(UInt32)` | ✅ Parity |
| **QuerySetId** | `QuerySetId(u32)` | `QuerySetId(UInt32)` | ✅ Parity |
| **RawIdentifier** | `RawIdentifier(String)` | `RawIdentifier(String)` | ✅ Parity |
| **TimeDurationMicros** | `TimeDurationMicros(u64)` | `TimeDurationMicros(UInt64)` | ✅ Parity |
| **Identity / ConnId** | Value classes | Struct wrappers | ✅ Parity |
| **Reader Helpers** | `readArray`, `readTaggedEnum` | `readArray`, `readTaggedEnum` | ✅ Parity |
| **Boilerplate Reduction** | Procedural loop reduction | Procedural loop reduction | ✅ Parity |
| **File Structure** | Split into Client/Server/Shared | Split into Client/Server/Shared | ✅ Parity |
| **Golden Tests** | Hex-payload parity | Hex-payload parity | ✅ Parity |
| **Benchmarks** | Baseline collection | Baseline collection | ✅ Parity |

## Key Differences Remaining
- **Memory Management**: Kotlin uses JVM garbage collection; Swift uses ARC.
- **Concurrency**: Kotlin uses Coroutines; Swift uses Structured Concurrency (Async/Await).
- **Serialization Hooks**: Kotlin uses internal property delegates; Swift uses `BSATNSpecialDecodable`.
