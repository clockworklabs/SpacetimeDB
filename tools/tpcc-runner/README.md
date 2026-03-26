# TPC-C Runner

`tpcc-runner` is the Rust-side harness for the SpacetimeDB TPC-C module in `modules/tpcc`.

It supports three subcommands:

- `load`: populate the module with the initial TPC-C dataset
- `driver`: run one benchmark driver with one logical terminal per SDK connection
- `coordinator`: synchronize multiple remote drivers and aggregate their summaries

## Local workflow

1. Publish or start the `modules/tpcc` module.
2. Load data:

```bash
cargo run -p tpcc-runner -- load --database tpcc --warehouses 1
```

3. Run a single local driver:

```bash
cargo run -p tpcc-runner -- driver --database tpcc --warehouses 1 --terminals 10 --warmup-secs 5 --measure-secs 30
```

The driver writes:

- `summary.json`
- `txn_events.ndjson`

under `tpcc-results/<run-id>/<driver-id>/` unless `--output-dir` is provided.

## Distributed workflow

Start the coordinator:

```bash
cargo run -p tpcc-runner -- coordinator --expected-drivers 2 --warmup-secs 5 --measure-secs 30
```

Start each remote driver with disjoint terminal ranges:

```bash
cargo run -p tpcc-runner -- driver --database tpcc --warehouses 2 --terminal-start 1 --terminals 10 --coordinator-url http://coordinator-host:7878
cargo run -p tpcc-runner -- driver --database tpcc --warehouses 2 --terminal-start 11 --terminals 10 --coordinator-url http://coordinator-host:7878
```

When all expected drivers register, the coordinator publishes a common schedule and writes an aggregated `summary.json` under `tpcc-results/coordinator/<run-id>/`.

## Config file

All subcommands accept `--config <path>`. The file is TOML with optional sections:

```toml
[connection]
uri = "http://127.0.0.1:3000"
database = "tpcc"
confirmed_reads = true
timeout_secs = 30

[load]
warehouses = 1
batch_size = 500
reset = true

[driver]
driver_id = "driver-a"
terminal_start = 1
terminals = 10
warehouses = 1
warmup_secs = 5
measure_secs = 30
delivery_wait_secs = 60
keying_time_scale = 1.0
think_time_scale = 1.0

[coordinator]
run_id = "tpcc-demo"
listen = "127.0.0.1:7878"
expected_drivers = 2
warmup_secs = 5
measure_secs = 30
output_dir = "tpcc-results/coordinator"
```

CLI flags override config-file values.

## Regenerating bindings

If the module signatures change, regenerate the Rust SDK bindings:

```bash
cargo build -p spacetimedb-standalone
cargo run -p spacetimedb-cli -- generate --lang rust --out-dir tools/tpcc-runner/src/module_bindings --module-path modules/tpcc --yes
```
