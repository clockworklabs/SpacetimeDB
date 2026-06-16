# SpacetimeDB DST

Deterministic Simulation Testing framework for SpacetimeDB.

## Test

```sh
cargo test -p spacetimedb-dst
```

## Run

```sh
cargo run -p spacetimedb-dst -- run --seed 42 --tables 5
```

Options:

- `--seed <u64>` — RNG seed (defaults to wall-clock nanos)
- `--tables <usize>` — number of tables to generate (default 3)
- `--max-interactions <usize>` — interaction budget
