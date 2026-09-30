#!/bin/bash
# Build one release video: soundtrack → frames (parallel) → mux, under a memory watchdog.
# Usage: ./build-release.sh 2.5   → release-v2.5/spacetimedb-v2.5.mp4
set -euo pipefail
cd "$(dirname "$0")"
V=$1; DIR=release-v$V; MIN_FREE_GB=${MIN_FREE_GB:-6}
[ -f "$DIR/scenes.js" ] || { echo "no $DIR/scenes.js"; exit 1; }
# The soundtrack must not contain NaN (it would mux as silence).
AUDIO_OUT=$(node "$DIR/audio.js")
echo "$AUDIO_OUT"
if echo "$AUDIO_OUT" | grep -q "peak NaN"; then echo "soundtrack is NaN, check $DIR/audio.js"; exit 1; fi
node render.js --reel "$DIR" --samples 5 &
RP=$!
# Linux watchdog: stop the render if available memory drops too low (see README "Resource limits").
while kill -0 $RP 2>/dev/null; do
  if [ -r /proc/meminfo ]; then
    avail=$(awk '/MemAvailable/ {print int($2/1024/1024)}' /proc/meminfo)
    if [ "$avail" -lt "$MIN_FREE_GB" ]; then echo "watchdog: ${avail} GB free, stopping render"; pkill -P $RP || true; kill $RP; exit 1; fi
  fi
  sleep 2
done
wait $RP
./mux.sh "$DIR" "spacetimedb-v$V.mp4"
