#!/usr/bin/env bash
# Thin launcher — Harbor's oracle entry point (Harbor requires solve.sh on Linux).
# Publishes the reference module under ./module. Backend readiness is gated by the
# task.toml healthcheck, so no wait is needed here.
set -euo pipefail
exec spacetime publish --server "${STDB_SERVER:-http://127.0.0.1:3000}" -y \
  -p "$(dirname "$0")/module" chat
