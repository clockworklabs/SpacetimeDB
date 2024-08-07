@echo off
setlocal

if "%CL_HOME%"=="" (
  echo "Variable CL_HOME not set"
  exit /b 1
)

cd %CL_HOME%\SpacetimeDB\crates\client-api-messages
cargo run --example get_ws_schema > %CL_HOME%/schema.json

cd %CL_HOME%\SpacetimeDB\crates\cli
cargo run -- generate -l csharp -n SpacetimeDB.ClientApi ^
  --json-module %CL_HOME%\schema.json ^
  -o %CL_HOME%\spacetimedb-csharp-sdk\src\SpacetimeDB\ClientApi

cd %CL_HOME%\spacetimedb-csharp-sdk\src\SpacetimeDB\ClientApi
del /q _Globals

del %CL_HOME%\schema.json
