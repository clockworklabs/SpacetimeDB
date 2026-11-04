# `keynote-benchmarks` *Rust* Benchmarks

Implementations of various benchmarks used for the *Keynote* presentation.

## How to Run

1. Start a fresh local server, for example with `cargo spacetime start --in-memory`, which discards its data on exit.
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