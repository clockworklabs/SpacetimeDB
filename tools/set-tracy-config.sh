#!/bin/bash

# Define files and their target names
files=(
  "../docker-compose.yml"
  "../docker-compose.yml-TracyConfig"
  "../docker-compose.yml-OriginalConfig"
  "../crates/standalone/Dockerfile"
  "../crates/standalone/Dockerfile-OriginalConfig"
  "../crates/standalone/Cargo.toml"
  "../crates/standalone/Cargo.toml-TracyConfig"
  "../crates/standalone/config.toml"
  "../crates/standalone/config.toml-TracyConfig"
)

# Check for FirstTime setup state
if [[ -f "../docker-compose.yml-TracyConfig" && -f "../crates/standalone/Cargo.toml-TracyConfig" && -f "../crates/standalone/config.toml-TracyConfig" && -f "../docker-compose.yml-PerfConfig" && -f "../crates/standalone/Dockerfile-PerfConfig" ]]; then
  echo "First time setup, setting to Tracy configuration."
  # Perform the renames
  # Rename original files so we don't overwrite them
  mv ../docker-compose.yml ../docker-compose.yml-OriginalConfig
  mv ../crates/standalone/Cargo.toml ../crates/standalone/Cargo.toml-OriginalConfig
  mv ../crates/standalone/config.toml ../crates/standalone/config.toml-OriginalConfig
  # Rename our perf config files
  mv ../docker-compose.yml-TracyConfig ../docker-compose.yml
  mv ../crates/standalone/Cargo.toml-TracyConfig ../crates/standalone/Cargo.toml
  mv ../crates/standalone/config.toml-TracyConfig ../crates/standalone/config.toml
  echo "BitCraft Testing Configuration successfully switched to Tracy."
  exit 0
fi

# Check if already in TracyConfig (renamed) state
if [[ -f "../docker-compose.yml-OriginalConfig" && -f "../crates/standalone/Cargo.toml-OriginalConfig" && -f "../crates/standalone/config.toml-OriginalConfig" && -f "../docker-compose.yml" && -f "../crates/standalone/Cargo.toml" && -f "../crates/standalone/config.toml" && -f "../docker-compose.yml-PerfConfig" && -f "../crates/standalone/Dockerfile-PerfConfig" ]]; then
  echo "BitCraft Testing already in Tracy configuration."
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

# Rename docker-compose.yml from perf config to Tracy config
mv ../docker-compose.yml ../docker-compose.yml-PerfConfig
mv ../docker-compose.yml-TracyConfig ../docker-compose.yml
# Rename Dockerfile from perf config to Tracy config (Orignal config)
mv ../crates/standalone/Dockerfile ../crates/standalone/Dockerfile-PerfConfig
mv ../crates/standalone/Dockerfile-OriginalConfig ../crates/standalone/Dockerfile
# Rename Cargo.toml from perf config to Tracy config
mv ../crates/standalone/Cargo.toml ../crates/standalone/Cargo.toml-OriginalConfig
mv ../crates/standalone/Cargo.toml-TracyConfig ../crates/standalone/Cargo.toml
# Rename config.toml from perf config to Tracy config
mv ../crates/standalone/config.toml ../crates/standalone/config.toml-OriginalConfig
mv ../crates/standalone/config.toml-TracyConfig ../crates/standalone/config.toml

echo "BitCraft Testing Configuration successfully switched to Tracy."