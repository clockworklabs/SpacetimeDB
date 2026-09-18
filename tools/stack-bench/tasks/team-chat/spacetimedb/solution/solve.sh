#!/usr/bin/env bash
# Thin launcher — Harbor's oracle entry point. Publishes the reference module.
# Backend readiness is gated by the task.toml healthcheck.
set -euo pipefail
exec spacetime publish --server "${STDB_SERVER:-http://127.0.0.1:3000}" -y \
  -p "$(dirname "$0")/module" teamchat
