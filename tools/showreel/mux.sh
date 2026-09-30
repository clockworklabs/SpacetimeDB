#!/bin/bash
# Concatenate rendered segments, add the soundtrack and light film grain.
# Usage: ./mux.sh                       -> showreel (out/ → spacetimedb-showreel.mp4)
#        ./mux.sh REEL_DIR OUT_NAME.mp4 -> another reel (REEL_DIR/out/ → REEL_DIR/OUT_NAME.mp4)
set -euo pipefail
cd "$(dirname "$0")"
REEL=${1:-.}
NAME=${2:-spacetimedb-showreel.mp4}
DUR=$(node -e "const p=require('path').resolve('$REEL');const s=require(p+'/scenes');console.log(s.DUR ?? require('./lib').DUR)")
cd "$REEL/out"
ffmpeg -y -loglevel error -f concat -safe 0 -i list.txt -i reel.wav \
  -vf "noise=alls=3:allf=t,format=yuv420p" -c:v libx264 -preset slow -crf 19 -profile:v high \
  -c:a aac -b:a 256k -t "$DUR" -movflags +faststart "../$NAME"
echo "wrote $REEL/$NAME"
