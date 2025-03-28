# spacetimedb-bench

> ⚠️ **Internal Crate** ⚠️
>
> This crate is intended for internal use only. It is **not** stable and may change without notice.

Benchmarking suite for SpacetimeDB using [Criterion](https://github.com/bheisler/criterion.rs) and [Callgrind](https://valgrind.org/docs/manual/cl-manual.html) (via [iai-callgrind](https://github.com/clockworklabs/iai-callgrind)). Provides comparisons between the underlying spacetime datastore, spacetime modules, and sqlite.

To run the criterion benchmarks:

```bash
cargo bench --bench generic --bench special
```

To enable the criterion benchmarks that do one million inserts or updates, set the RUN_ONE_MILLION environment variable:

```bash
RUN_ONE_MILLION=true cargo bench --bench generic --bench special
```

To run the callgrind benchmarks, you need valgrind installed.
The easiest way to get it is to use the docker image in this folder. 
There's a handy bash script:
```bash
bash callgrind-docker.sh 
```
Which will build the docker image and run the callgrind benchmarks inside of it.

You can also comment "benchmarks please" or "callgrind please" on a pull request in the SpacetimeDB repository to run the criterion/callgrind benchmarks on that PR. The results will be posted in a comment on the PR.

This is coordinated using the benchmarks Github Actions: see [`../../.github/workflows/benchmarks.yml`](../../.github/workflows/benchmarks.yml), and
[`../../.github/workflows/callgrind_benchmarks.yml`](../../.github/workflows/callgrind_benchmarks.yml). 
These also rely on the benchmarks-viewer application (https://github.com/clockworklabs/benchmarks-viewer).


## Caveats

The criterion benchmarks take a long time to run -- there are a lot of them. See below for information on running select groups of them.

The callgrind benchmarks only measure a select portion of the codebase. In particular, they do not collect instruction counts in any async code, due to callgrind limitations; they only time the synchronous code that runs a reducer / the client code that calls a reducer. This is most of the code that it takes to run a reducer, including all database modifications. The async code between client and reducer is mostly just handing off data through a couple of channels. Still, if you're interested in measuring that code, you should rely on the criterion benchmarks & `perf`.



## Criterion benchmarks
Timings for spacetime modules should be understood as *latencies to call a spacetime reducer without network delays*. They include serialization and deserialization times for any arguments passed. There are also separate benchmarks that measure these times on their own.

The complete structure (not yet fully implemented) is as follows:

```
# generic benchmarks:

[db]/[disk]/
    empty_transaction/
    insert_1/[schema]/[index_type]/[load]
    insert_bulk/[schema]/[index_type]/[load]/[count]
    insert_large_value/[count]
    iterate/[schema]/[count]/
    filter/[string, u64]/[index?]/[load]/[count]
    find_unique/u32/[load]/
    delete/[index_type]/[load]/[count]

# "load" refers to the number of rows in the table at the start of the benchmark.

# db: [stdb_raw, stdb_module, sqlite]
# disk: [disk, mem]
# schema: [person, location]
# index_type: [unique, non_unique, multi_index]

# "person" is a schema with 2 ints and a string, "location" is a schema with 3 ints.

serialize/
    bsatn/[schema]/[count]
    json/[schema]/[count]
    product_value/[schema]/[count]

deserialize/
    bsatn/[schema]/[count]
    json/[schema]/[count]

stdb_module/
    print_bulk/[count]
    large_arguments/64KiB/

db_game/
    circles/load=[num rows]
    ia_loop/load=[num rows]
```

Typically, you don't want to run all benchmarks at once, there are a lot of them and it will take many minutes.
You can pass regexes to the bench script to select what slice of benchmarks you'd like. For example,

```sh
cargo bench -- 'stdb_raw/.*/insert_bulk'
```
Will run all of the `insert_bulk` benchmarks against the raw spacetime backend.

Similarly, 
```sh
cargo bench -- 'mem/.*/unique'
```
Will run benchmarks involving unique primary keys against all databases, without writing to disc.

## Workload Benches

The workload benches within the `db_game` module are designed to simulate realistic workloads commonly found in games. 

Instead of testing for a small set of values, these benches generate a larger number of rows to more effectively stress the database engine. As consequence, they take more time when run inside `criterion`. 

This approach reduces interference caused by noise and provides the potential for better detection of display improvements or regressions in performance.

To run the workload benches directly outside `criterion`, you can use the following commands:

```bash
cargo test --release --package spacetimedb-testing --test standalone_integration_test test_calling_bench_db_ia_loop -- --exact --nocapture
cargo test --release --package spacetimedb-testing --test standalone_integration_test test_calling_bench_db_circles -- --exact --nocapture
```

## Pretty report
To generate a nicely formatted markdown report, you can use the "summarize" binary.
This is used on CI (see [`../../.github/workflows/benchmarks.yml`](../../.github/workflows/benchmarks.yml)).

To generate a report without comparisons, use:
```bash
cargo bench --bench generic --bench special -- --save-baseline current
cargo run --bin summarize markdown-report current
```

To compare to another branch, do:
```bash
git checkout master
cargo bench --bench generic --bench special -- --save-baseline base
git checkout high-octane-feature-branch
cargo bench --bench generic --bench special -- --save-baseline current
cargo run --bin summarize markdown-report current base
```

Of course, this will take about an hour, so it might be better to let the CI do it for you.

## Adding more

There are two ways to write benchmarks:

- Targeted, non-generic benchmarks (`benches/special.rs`)
- Generic over database backends (`benches/generic.rs`)

See the following sections for how to write these.

#### Targeted benchmarks
These are regular [Criterion.rs](https://github.com/bheisler/criterion.rs) benchmarks. Nothing fancy, do whatever you like with these. Put them in `benches/special.rs`.

#### Generic benchmarks
Generic benchmarks live in `benches/generic.rs`. These benchmarks involve two traits:

- [`BenchDatabase`](src/database.rs), which is implemented by different database backends.
- [`BenchTable`](src/schemas.rs), which is implemented by different benchmark tables.

To add a new generic benchmark, you'll need to:
- Add a relevant method to the `BenchDatabase` trait.
- Implement it for each `BenchDatabase` implementation.
    - [`SQLite`](src/sqlite.rs) will require you to write it as a SQL query and submit that to sqlite.
    - [`SpacetimeRaw`](src/spacetime_raw.rs) will require you to execute the query against the Spacetime database backend directly.
    - [`SpacetimeModule`](src/spacetime_module.rs) will require you to add a reducer to the [`benchmarks`](../../modules/benchmarks/src/lib.rs) crate, and then add logic to invoke your reducer with the correct arguments.
- Add a benchmark harness that actually invokes your method in `benches/generic.rs`.


## Callgrind benchmarks
These live in `benches/callgrind.rs` and can only be run on linux, although a docker image is provided above.
See that f


## Install tools

There are also some scripts that rely on external tools to extract data from the benchmarks.

### OSX + Linux

- [samply](https://github.com/mstange/samply/)

```bash
cargo install samply
```
Run *any* command to see perf data on Firefox:

```bash
# Note if the `cargo` command triggers compilation, it will also be captured in the profile.
# Therefore it is useful to run this after the artifact has already been cached.
samply record -r 10000000 cargo bench --bench=subscription --profile=profiling -- full-scan --exact --profile-time=30
```

### OSX Only

- [cargo-instrument](https://github.com/cmyr/cargo-instruments)

```bash
brew install cargo-instruments
```

```bash
# ./instruments.sh TEMPLATE BENCHMARK BENCHMARK_FILTER
./instruments.sh time generic stdb_raw/mem/insert_bulk/location/multi_index/load=0/count=100
```

Where `TEMPLATE` is one from 

```bash
cargo instruments --list-templates
```


### Linux only

- [cargo-flamegraph](https://github.com/flamegraph-rs/flamegraph)

```bash
cargo install flamegraph
```

You can generate flamegraphs using `flamegraph.sh` script:

```bash
# ./flamegraph.sh BENCH_EXECUTABLE FILTER SVG_PATH
./flamegraph.sh generic stdb_raw/mem/insert_bulk/location/multi_index/load=0/count=100 result.svg"
```