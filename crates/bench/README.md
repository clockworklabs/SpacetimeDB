# spacetimedb-bench
Benchmarking suite for SpacetimeDB using Criterion. Provides comparisons between the underlying spacetime datastore, spacetime modules, and sqlite.

Timings for spacetime modules should be understood as *latencies to call a spacetime reducer without network delays*. They include serialization and deserialization times for any arguments passed. There are also separate benchmarks that measure these times on their own.

## Benchmark structure

The complete structure (not yet fully implemented) is as follows:

```
[db]/[disk]/
    empty_transaction/
    insert_1/[schema]/[index_type]/[load]
    insert_bulk/[schema]/[index_type]/[load]/[count]
    insert_large_value/[count]
    iterate/[schema]/[count]/
    filter/[string, u64]/[index?]/[load]/[count]
    find_unique/u32/[load]/
    update/[load]/[count]
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
```

Typically you don't want to run all benchmarks at once, there are a lot of them and it will take many minutes.
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

## Install tools

- [hyperfine](https://github.com/sharkdp/hyperfine)

Note: These are just some examples. Go the repo page for full install instructions.

```bash
# Ubuntu
wget https://github.com/sharkdp/hyperfine/releases/download/v1.15.0/hyperfine_1.15.0_amd64.deb
sudo dpkg -i hyperfine_1.15.0_amd64.deb
# macOs
brew install hyperfine
# Windows
conda install -c conda-forge hyperfine

# Any
cargo install hyperfine
```

- [critcmp](https://github.com/BurntSushi/critcmp)
```bash
cargo install critcmp
```

### OSX Only

- [cargo-instrument](https://github.com/cmyr/cargo-instruments)

```bash
brew install cargo-instruments
```

## Run

List the available benchmarks:

```bash
# From root
# cd SpacetimeDB/crates/bench
cargo run --  --help
```
Exist two engines to test: `spacetimedb` & `sqlite`.

## Benches with Criterion

Run normally with cargo:

```bash
#cargo bench -- NAME_OF_COMMAND
cargo bench -- insert
```

To compare results across benches, use `critcmp`:

```bash
# Get the list of baselines you can compare
critcmp --baselines
# Compare current with older
critcmp base new
```

## Hyperfine

You can run benchmarks using `hyperfine.sh` script, it already tests against both engines:

```bash
# ./hyperfine.sh NAME_OF_COMMAND
./hyperfine.sh insert
```

## Flamegraph

You can generate flamegraphs using `flamegraph.sh` script, it already do it for both engines:

```bash
# ./flamegraph.sh NAME_OF_COMMAND
./flamegraph.sh insert
# Generated files
open spacetime.svg
open sqlite.svg
```

## Instruments

You can run benchmarks using `instruments.sh` script. This check against only one engine:

```bash
# ./instruments.sh TEMPLATE ENGINE NAME_OF_COMMAND
./instruments.sh time sqlite insert
```
Where `TEMPLATE` is one from 

```bash
cargo instruments --list-templates
```
