#!/bin/bash
# Exhaust Test Launcher
#
# Runs the benchmark in Claude Code with OpenTelemetry enabled
# for exact per-request token tracking.
#
# Usage:
#   ./run.sh                          # defaults: level=1, backend=spacetime
#   ./run.sh --level 5 --backend postgres
#   ./run.sh --level 12 --backend spacetime
#
# Prerequisites:
#   - Claude Code CLI installed
#   - OpenTelemetry Collector running (see collector-config.yaml)
#   - SpacetimeDB running (spacetime start)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TELEMETRY_DIR="$SCRIPT_DIR/telemetry"

# Parse arguments
LEVEL=1
BACKEND="spacetime"
while [[ $# -gt 0 ]]; do
  case $1 in
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Create telemetry output directory
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RUN_DIR="$TELEMETRY_DIR/$BACKEND-level$LEVEL-$TIMESTAMP"
mkdir -p "$RUN_DIR"

echo "=== Exhaust Test ==="
echo "  Level:   $LEVEL"
echo "  Backend: $BACKEND"
echo "  Output:  $RUN_DIR"
echo ""

# Enable OpenTelemetry for token tracking
export CLAUDE_CODE_ENABLE_TELEMETRY=1
export OTEL_LOGS_EXPORTER=otlp
export OTEL_METRICS_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_LOGS_EXPORT_INTERVAL=1000
export OTEL_METRIC_EXPORT_INTERVAL=5000

# Save run metadata
cat > "$RUN_DIR/metadata.json" <<EOF
{
  "level": $LEVEL,
  "backend": "$BACKEND",
  "timestamp": "$TIMESTAMP",
  "startedAt": "$(date -Iseconds)"
}
EOF

echo "Starting Claude Code with telemetry enabled..."
echo "Make sure the OTel Collector is running: docker compose -f $SCRIPT_DIR/docker-compose.otel.yaml up -d"
echo ""

# Run Claude Code with the exhaust test prompt
# Claude Code will auto-discover the CLAUDE.md in the exhaust-test directory
cd "$SCRIPT_DIR"
claude --prompt "Run the exhaust test. Level: $LEVEL, Backend: $BACKEND. Follow the CLAUDE.md instructions."

# After session ends, parse telemetry
echo ""
echo "=== Session Complete ==="
echo "Parsing telemetry..."
node "$SCRIPT_DIR/parse-telemetry.mjs" "$RUN_DIR"
