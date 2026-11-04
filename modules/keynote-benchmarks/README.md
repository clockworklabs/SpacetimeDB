# `keynote-benchmarks` *Rust* Benchmarks

Implementations of various benchmarks used for the *Keynote* presentation.

## How to Run

1. Create a new database, for example using [`run_standalone_temp.sh`](../../run_standalone_temp.sh).
2. Publish this module:
    ```bash
    # This will `DESTROY` the existing `keynote` database, so be careful! 
    spacetimedb-cli publish keynote -c -p crates/keynote-benchmarks
    ```
3. Run the benchmarks:
    ```bash
   spacetimedb-cli call keynote update_positions_by_collect
   spacetimedb-cli call keynote roundtrip
    ```
4. See the result:
    ```bash
    # After running the `publish` to see the results of `init` 
    # and any of the above commands:
    spacetimedb-cli logs keynote
    ```