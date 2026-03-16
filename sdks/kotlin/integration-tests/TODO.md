# Integration Tests TODO

- [ ] Integrate into `crates/smoketests` framework (like C#/TS SDKs)
  - Smoketests spin up a server, publish a module, generate bindings, and drive client tests
  - Needs Rust test code that invokes Gradle to build/run Kotlin tests
  - See `crates/smoketests/tests/smoketests/templates.rs` for C#/TS patterns
- [ ] Until then, this module runs standalone against a manually started server
  - Start server: `spacetimedb-cli dev --project-path integration-tests/spacetimedb`
  - Run tests: `./gradlew :integration-tests:test -PintegrationTests`
- [ ] Remove `sdks/kotlin/integration-tests/spacetimedb` from `Cargo.toml` workspace exclude once migrated to smoketests
