#!/usr/bin/env bash
# Kill every foreground bench process and the tmux session.
# Leaves Postgres (systemd) and Supabase (Docker) running — those have their own lifecycles.

pkill -f sqlite-rpc-server     2>/dev/null
pkill -f postgres-rpc-server   2>/dev/null
pkill -f cockroach-rpc-server  2>/dev/null
pkill -f supabase-rpc-server   2>/dev/null
pkill -f bun-server            2>/dev/null
pkill -f "convex dev"          2>/dev/null
pkill -f "cockroach start-single-node" 2>/dev/null
tmux kill-session -t bench 2>/dev/null
echo "stopped."
