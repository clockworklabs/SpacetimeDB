# SpacetimeDB DST

Deterministic Simulation Testing framework for SpacetimeDB.

## Test

```sh
cargo test -p spacetimedb-dst
```

## Run

```sh
cargo run -p spacetimedb-dst -- run --seed 42 --max-interactions 1000
```

Options:

- `--seed <u64>` — RNG seed (defaults to wall-clock nanos)
- `--max-interactions <usize>` — interaction budget
