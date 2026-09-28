#!/bin/bash
# Concatenate rendered segments, add the soundtrack and light film grain.
set -euo pipefail
cd "$(dirname "$0")"
DUR=$(node -e "console.log(require('./lib').DUR)")
cd out
nice -n 19 ffmpeg -y -loglevel error -threads 4 -f concat -safe 0 -i list.txt -i reel.wav \
  -vf "noise=alls=3:allf=t,format=yuv420p" -c:v libx264 -preset slow -crf 19 -profile:v high \
  -c:a aac -b:a 256k -t "$DUR" -movflags +faststart ../spacetimedb-showreel.mp4
echo "wrote spacetimedb-showreel.mp4"
