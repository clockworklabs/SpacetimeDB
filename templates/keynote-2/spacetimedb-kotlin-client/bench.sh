#!/usr/bin/env bash
set -euo pipefail

# Kotlin SDK TPS Benchmark Runner
# Usage: ./bench.sh [--duration 10s] [--connections 10] [--server http://localhost:3000] [--module sim] [--runs 2]

DURATION="10s"
CONNECTIONS=10
SERVER="http://localhost:3000"
MODULE="sim"
RUNS=2

usage() {
    cat <<EOF
Kotlin SDK TPS Benchmark Runner

Usage: ./bench.sh [OPTIONS]

Options:
  --duration <time>       Benchmark duration per run (default: $DURATION)
  --connections <n>       Number of concurrent connections (default: $CONNECTIONS)
  --server <url>          SpacetimeDB server URL (default: $SERVER)
  --module <name>         Published module name (default: $MODULE)
  --runs <n>              Number of benchmark runs (default: $RUNS)
  -h, --help              Show this help

Prerequisites:
  1. Build server:  cargo build --release -p spacetimedb-cli -p spacetimedb-standalone
  2. Start server:  target/release/spacetimedb-cli start
  3. Publish module: target/release/spacetimedb-cli publish --server http://localhost:3000 \\
                       --module-path templates/keynote-2/rust_module --no-config -y sim

Examples:
  ./bench.sh                                     # defaults
  ./bench.sh --duration 30s --connections 20      # heavier load
  ./bench.sh --runs 5                             # more samples
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)     usage ;;
        --duration)    DURATION="$2"; shift 2 ;;
        --connections) CONNECTIONS="$2"; shift 2 ;;
        --server)      SERVER="$2"; shift 2 ;;
        --module)      MODULE="$2"; shift 2 ;;
        --runs)        RUNS="$2"; shift 2 ;;
        *) echo "Unknown option: $1 (use --help for usage)"; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/build/install/spacetimedb-kotlin-tps-bench/bin/spacetimedb-kotlin-tps-bench"

# Build if needed
if [[ ! -x "$BIN" ]]; then
    echo "Building..."
    "$SCRIPT_DIR/gradlew" installDist --no-daemon -q
fi

# Monitor function: samples CPU% and RSS every second
monitor() {
    local pattern="$1" outfile="$2"
    while true; do
        for pid in $(pgrep -f "$pattern" 2>/dev/null); do
            read cpu rss < <(ps -p "$pid" -o %cpu=,rss= 2>/dev/null) || continue
            echo "$cpu $rss"
        done >> "$outfile"
        sleep 1
    done
}

# Parse monitor output -> peak CPU, peak RSS (MB)
parse_peak() {
    local file="$1"
    if [[ ! -s "$file" ]]; then
        echo "0 0"
        return
    fi
    awk '{
        if ($1+0 > maxcpu) maxcpu=$1+0
        if ($2+0 > maxrss) maxrss=$2+0
    } END {
        printf "%.0f %d\n", maxcpu, maxrss/1024
    }' "$file"
}

# Format number with comma separators
fmt_num() {
    printf "%'d" "$1" 2>/dev/null || printf "%d" "$1"
}

echo ""
printf "%-14s %s\n" "Server:" "$SERVER"
printf "%-14s %s\n" "Module:" "$MODULE"
printf "%-14s %s\n" "Duration:" "$DURATION"
printf "%-14s %s\n" "Connections:" "$CONNECTIONS"
printf "%-14s %s\n" "Runs:" "$RUNS"
echo ""

# Seed
printf "Seeding... "
"$BIN" seed --server "$SERVER" --module "$MODULE" --quiet 2>/dev/null | grep -v "^\[SpacetimeDB" > /dev/null || true
echo "done"
echo ""

# Collect results
declare -a TPS_RESULTS CPU_RESULTS RSS_RESULTS

for i in $(seq 1 "$RUNS"); do
    tmpmon=$(mktemp)

    monitor "MainKt" "$tmpmon" &
    MON_PID=$!

    output=$("$BIN" bench \
        --server "$SERVER" \
        --module "$MODULE" \
        --duration "$DURATION" \
        --connections "$CONNECTIONS" \
        --quiet 2>/dev/null | grep -v "^\[SpacetimeDB") || true

    kill $MON_PID 2>/dev/null; wait $MON_PID 2>/dev/null || true

    tps=$(echo "$output" | grep "throughput" | grep -oP '[\d.]+(?= TPS)') || tps="0"
    tps_int=$(printf "%.0f" "$tps")

    read peak_cpu peak_rss < <(parse_peak "$tmpmon")
    rm -f "$tmpmon"

    TPS_RESULTS+=("$tps_int")
    CPU_RESULTS+=("$peak_cpu")
    RSS_RESULTS+=("$peak_rss")

    printf "Run %d:  %s TPS  |  CPU %s%%  |  RSS %s MB\n" \
        "$i" "$(fmt_num "$tps_int")" "$peak_cpu" "$(fmt_num "$peak_rss")"

    [[ $i -lt "$RUNS" ]] && sleep 2
done

# Summary table
echo ""
echo "┌───────┬────────────────┬───────────┬────────────┐"
echo "│  Run  │      TPS       │  Peak CPU │   Peak RSS │"
echo "├───────┼────────────────┼───────────┼────────────┤"
for i in $(seq 0 $((RUNS - 1))); do
    printf "│  %2d   │ %14s │    %5s%% │ %7s MB │\n" \
        $((i + 1)) "$(fmt_num "${TPS_RESULTS[$i]}")" "${CPU_RESULTS[$i]}" "$(fmt_num "${RSS_RESULTS[$i]}")"
done
echo "├───────┼────────────────┼───────────┼────────────┤"

sum_tps=0; sum_cpu=0; sum_rss=0
for i in $(seq 0 $((RUNS - 1))); do
    sum_tps=$((sum_tps + TPS_RESULTS[i]))
    sum_cpu=$((sum_cpu + CPU_RESULTS[i]))
    sum_rss=$((sum_rss + RSS_RESULTS[i]))
done
avg_tps=$((sum_tps / RUNS))
avg_cpu=$((sum_cpu / RUNS))
avg_rss=$((sum_rss / RUNS))

printf "│  avg  │ %14s │    %5s%% │ %7s MB │\n" \
    "$(fmt_num "$avg_tps")" "$avg_cpu" "$(fmt_num "$avg_rss")"
echo "└───────┴────────────────┴───────────┴────────────┘"
echo ""
