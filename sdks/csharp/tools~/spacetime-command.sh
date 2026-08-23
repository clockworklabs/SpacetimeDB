# CI provides a prebuilt CLI; direct local invocations keep using Cargo.
prepare_spacetime() {
    local stdb_path="$1"

    if [[ -n "${SPACETIME_BIN:-}" ]]; then
        SPACETIME=("$SPACETIME_BIN")
    else
        cargo build --manifest-path "$stdb_path/crates/standalone/Cargo.toml"
        SPACETIME=(cargo run --manifest-path "$stdb_path/crates/cli/Cargo.toml" --)
    fi
}
