#!/bin/bash

# Define files and their target names
files=(
  "../docker-compose.yml"
  "../docker-compose.yml-PerfConfig"
  "../docker-compose.yml-OriginalConfig"
  "../crates/standalone/Dockerfile"
  "../crates/standalone/Dockerfile-PerfConfig"
  "../crates/standalone/Cargo.toml"
  "../crates/standalone/Cargo.toml-OriginalConfig"
  "../crates/standalone/config.toml"
  "../crates/standalone/config.toml-OriginalConfig"
)

# Check for FirstTime setup state
if [[ -f "../docker-compose.yml-TracyConfig" && -f "../docker-compose.yml-PerfConfig" && -f "../crates/standalone/Cargo.toml-TracyConfig" && -f "../crates/standalone/config.toml-TracyConfig" && -f "../crates/standalone/Dockerfile-PerfConfig" ]]; then
  echo "First time setup, setting to perf configuration."
  # Perform the renames
  # Rename original files so we don't overwrite them
  mv ../docker-compose.yml ../docker-compose.yml-OriginalConfig
  mv ../crates/standalone/Dockerfile ../crates/standalone/Dockerfile-OriginalConfig
  # Rename our perf config files
  mv ../docker-compose.yml-PerfConfig ../docker-compose.yml
  mv ../crates/standalone/Dockerfile-PerfConfig ../crates/standalone/Dockerfile
  echo "BitCraft Testing Configuration successfully switched to perf."
  exit 0
fi

# Check if already in TracyConfig (renamed) state
if [[ -f "../docker-compose.yml-TracyConfig" && -f "../crates/standalone/Cargo.toml-TracyConfig" && -f "../crates/standalone/config.toml-TracyConfig" && -f "../docker-compose.yml-OriginalConfig" && -f "../crates/standalone/Dockerfile-OriginalConfig" && -f "../docker-compose.yml" && -f "../crates/standalone/Dockerfile" ]]; then
  echo "BitCraft Testing already in perf configuration."
  exit 0
fi

# Check if all original files exist
missing=()
for file in "${files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing+=("$file")
  fi
done

if [[ ${#missing[@]} -ne 0 ]]; then
  echo "Error: The following files are missing and configuration cannot be changed:"
  for m in "${missing[@]}"; do
    echo "  $m"
  done
  exit 1
fi

# Rename docker-compose.yml from Tracy config to perf config
mv ../docker-compose.yml ../docker-compose.yml-TracyConfig
mv ../docker-compose.yml-PerfConfig ../docker-compose.yml
# Rename Dockerfile from Tracy config to perf config
mv ../crates/standalone/Dockerfile ../crates/standalone/Dockerfile-OriginalConfig
mv ../crates/standalone/Dockerfile-PerfConfig ../crates/standalone/Dockerfile
# Rename Cargo.toml from Tracy config to perf config
mv ../crates/standalone/Cargo.toml ../crates/standalone/Cargo.toml-TracyConfig
mv ../crates/standalone/Cargo.toml-OriginalConfig ../crates/standalone/Cargo.toml
# Rename config.toml from Tracy config to perf config
mv ../crates/standalone/config.toml ../crates/standalone/config.toml-TracyConfig
mv ../crates/standalone/config.toml-OriginalConfig ../crates/standalone/config.toml

echo "BitCraft Testing Configuration successfully switched to perf."