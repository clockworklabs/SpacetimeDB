# Benchmarking suite for SpacetimeDB

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
# cd SpacetimeDB/crates/spacetimedb-bench
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