@echo off
setlocal

set "STDB_PATH=%1"
set "SDK_PATH=%~dp0.."

cargo run --manifest-path %STDB_PATH%/crates/client-api-messages/Cargo.toml --example get_ws_schema | ^
cargo run --manifest-path %STDB_PATH%/crates/cli/Cargo.toml -- generate -l csharp --namespace SpacetimeDB.ClientApi ^
  --module-def ^
  -o %SDK_PATH%/src/SpacetimeDB/ClientApi/.output

move "%SDK_PATH%\src\SpacetimeDB\ClientApi\.output\Types\*" "%SDK_PATH%\src\SpacetimeDB\ClientApi"
rmdir /s /q "%SDK_PATH%\src\SpacetimeDB\ClientApi\.output"

endlocal
