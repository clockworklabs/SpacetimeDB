#!/usr/bin/env bash
: "${VITE_MODULE_NAME:?}"
: "${VITE_SPACETIMEDB_URI:?}"
: "${VITE_PORT:?}"
exec npm --prefix client run dev -- --host 0.0.0.0 --port "$VITE_PORT" --strictPort
