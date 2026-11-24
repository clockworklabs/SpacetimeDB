@echo off
setlocal

set "STDB_PATH=%1"

cargo run --manifest-path "%STDB_PATH%/crates/client-api-messages/Cargo.toml" --example get_ws_schema_v2 | ^
cargo run --manifest-path "%STDB_PATH%/crates/codegen/Cargo.toml" --example regen-csharp-clientapi

endlocal
