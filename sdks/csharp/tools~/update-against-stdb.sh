#!/usr/bin/env bash

# Do everything needed to update this repo from an upstream STDB clone.

set -ueox pipefail

STDB_PATH="$1"
SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"

"$SDK_PATH/tools~/write-nuget-config.sh" "$STDB_PATH"
"$SDK_PATH/tools~/gen-client-api.sh"
"$SDK_PATH/tools~/gen-quickstart.sh"
"$SDK_PATH/tools~/gen-regression-tests.sh"
dotnet nuget locals all --clear
dotnet pack "$STDB_PATH/crates/bindings-csharp"
rm -rf "$SDK_PATH/packages"
dotnet pack
dotnet test
pushd "$SDK_PATH"; git checkout -- 'packages/*.meta' 'packages/**/*.meta' packages/.gitignore; popd


