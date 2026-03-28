# TPC-C Runner

`tpcc-runner` is the Rust-side harness for the SpacetimeDB TPC-C module in `modules/tpcc`.

It supports three subcommands:

- `load`: populate the module with the initial TPC-C dataset
- `driver`: run one benchmark driver with one logical terminal per SDK connection
- `coordinator`: synchronize multiple remote drivers and aggregate their summaries

The runner assumes the TPC-C module is published to one or more databases named
`<database-prefix>-<database-number>`, for example `tpcc-0`, `tpcc-1`, `tpcc-2`.
Warehouses are assigned to databases in contiguous ranges:

- database `0` owns warehouses `1..=warehouses_per_database`
- database `1` owns the next `warehouses_per_database`
- and so on

Without a coordinator, `--warehouses` is the total logical warehouse count in
the benchmark, and `--warehouse-start` plus `--warehouse-count` define the
warehouse slice owned by one driver. With a coordinator, the coordinator
assigns each driver its warehouse slice and the database topology, so those
warehouse flags are not needed on the driver command line. The driver always
uses exactly `10` terminals per owned warehouse.

For multi-database runs, the `uri` passed to the loader and driver is also
stored in the module and used for cross-database HTTP calls. In normal builds,
that URI must be a non-private, routable address reachable from the database
host. `127.0.0.1`, `localhost`, and RFC1918 private IPs are rejected by the
module HTTP egress policy.

For local single-machine development, you can opt into loopback HTTP by
building `spacetimedb-standalone` with:

```bash
cargo build --release -p spacetimedb-standalone \
  --features spacetimedb-standalone/allow_loopback_http_for_tests
```

With that feature enabled, multi-database localhost runs can use
`http://127.0.0.1:3000`. This is intended for local testing, not a normal
production configuration.

## Local workflow

1. Build the release binaries you need.

```bash
cargo build --release -p spacetimedb-cli -p spacetimedb-standalone -p tpcc-runner
```

2. Start a local SpacetimeDB server.

```bash
cargo run --release -p spacetimedb-cli -- start --listen-addr 127.0.0.1:3000
```

3. Publish the TPC-C module to one or more databases. For a single database:

```bash
cargo run -p spacetimedb-cli -- publish \
  --server http://127.0.0.1:3000 \
  --module-path modules/tpcc \
  -c=always \
  -y \
  tpcc-0
```

For two databases:

```bash
cargo run -p spacetimedb-cli -- publish \
  --server http://127.0.0.1:3000 \
  --module-path modules/tpcc \
  -c=always \
  -y \
  tpcc-0

cargo run -p spacetimedb-cli -- publish \
  --server http://127.0.0.1:3000 \
  --module-path modules/tpcc \
  -c=always \
  -y \
  tpcc-1
```

4. Load data. For one warehouse in one database:

```bash
cargo run --release -p tpcc-runner -- load \
  --uri http://127.0.0.1:3000 \
  --database-prefix tpcc \
  --num-databases 1 \
  --warehouses-per-database 1 \
  --reset true
```

For two databases with one warehouse each on the same machine:

```bash
cargo run --release -p tpcc-runner -- load \
  --uri http://127.0.0.1:3000 \
  --database-prefix tpcc \
  --num-databases 2 \
  --warehouses-per-database 1 \
  --reset true
```

To load databases in parallel, add `--load-parallelism <N>`. The loader runs
databases concurrently but still loads each individual database in the normal
table order. If you omit the flag, it defaults to `min(num_databases, 8)`.

For example, to load those two local databases in parallel:

```bash
cargo run --release -p tpcc-runner -- load \
  --uri http://127.0.0.1:3000 \
  --database-prefix tpcc \
  --num-databases 2 \
  --warehouses-per-database 1 \
  --load-parallelism 2 \
  --reset true
```

5. Run a single local driver against one warehouse:

```bash
cargo run --release -p tpcc-runner -- driver \
  --uri http://127.0.0.1:3000 \
  --database-prefix tpcc \
  --warehouses 1 \
  --warehouses-per-database 1 \
  --warmup-secs 5 \
  --measure-secs 30
```

If you want to load multiple databases on one machine and actually exercise all
loaded warehouses, set `--warehouses` to the total logical warehouse count. For
example, after loading two databases with one warehouse each, a single-driver
run would be:

```bash
cargo run --release -p tpcc-runner -- driver \
  --uri http://127.0.0.1:3000 \
  --database-prefix tpcc \
  --warehouses 2 \
  --warehouses-per-database 1 \
  --warmup-secs 5 \
  --measure-secs 30
```

Using `--warehouses 1` after loading two one-warehouse databases will only
benchmark warehouse `1`; warehouse `2` will remain unused.

The driver writes:

- `summary.json`
- `txn_events.ndjson`

under `tpcc-results/<run-id>/<driver-id>/` unless `--output-dir` is provided.

## Distributed workflow

To run multiple databases across machines, first publish `tpcc-0`, `tpcc-1`,
... and load them using a routable, non-private server URL, for example
`http://public-host:3000` or a public DNS name pointing at the SpacetimeDB
server. Build `tpcc-runner` in release mode on each driver machine before
running the commands below.

Start the coordinator:

```bash
cargo run -p tpcc-runner -- coordinator \
  --expected-drivers 2 \
  --warehouses 2 \
  --warehouses-per-database 1 \
  --warmup-secs 5 \
  --measure-secs 30
```

Start each remote driver. The coordinator assigns the warehouse slices. This
example assumes two databases with one warehouse each:

```bash
cargo run --release -p tpcc-runner -- driver \
  --uri http://public-server-host:3000 \
  --database-prefix tpcc \
  --coordinator-url http://coordinator-host:7878

cargo run --release -p tpcc-runner -- driver \
  --uri http://public-server-host:3000 \
  --database-prefix tpcc \
  --coordinator-url http://coordinator-host:7878
```

Those two drivers together cover warehouse `1` and warehouse `2`.

When all expected drivers register, the coordinator publishes a common schedule and writes an aggregated `summary.json` under `tpcc-results/coordinator/<run-id>/`.

## Config file

All subcommands accept `--config <path>`. The file is TOML with optional sections:

```toml
[connection]
uri = "http://127.0.0.1:3000"
database_prefix = "tpcc"
confirmed_reads = true
timeout_secs = 30

[load]
num_databases = 1
warehouses_per_database = 1
load_parallelism = 1
batch_size = 500
reset = true

[driver]
driver_id = "driver-a"
warehouses = 1
warehouses_per_database = 1
warehouse_start = 1
warehouse_count = 1
warmup_secs = 5
measure_secs = 30
delivery_wait_secs = 60
keying_time_scale = 1.0
think_time_scale = 1.0

[coordinator]
run_id = "tpcc-demo"
listen = "127.0.0.1:7878"
expected_drivers = 2
warehouses = 2
warehouses_per_database = 1
warmup_secs = 5
measure_secs = 30
output_dir = "tpcc-results/coordinator"
```

CLI flags override config-file values.

## Regenerating bindings

If the module signatures change, regenerate the Rust SDK bindings:

```bash
cargo run -p spacetimedb-cli -- generate --lang rust --out-dir tools/tpcc-runner/src/module_bindings --module-path modules/tpcc --yes
```
