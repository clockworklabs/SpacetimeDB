#!/bin/bash
# Kiosk version: concatenate the slowed-down segments (render.js --slow N) with light
# film grain, no audio (the soundtrack is timed to the 1× cut). A 0.5 s fade-in from black
# matches the fade-out at the end so the loop point is seamless.
set -euo pipefail
cd "$(dirname "$0")"
DUR=$(node -e "console.log(require('./lib').DUR)")
SLOW=$(cat out-kiosk/slow.txt)
LEN=$(node -e "console.log($DUR * $SLOW)")
cd out-kiosk
ffmpeg -y -loglevel error -f concat -safe 0 -i list.txt \
  -vf "fade=t=in:st=0:d=0.5,noise=alls=3:allf=t,format=yuv420p" -c:v libx264 -preset slow -crf 19 -profile:v high \
  -an -t "$LEN" -movflags +faststart ../spacetimedb-showreel-kiosk.mp4
echo "wrote spacetimedb-showreel-kiosk.mp4 (${LEN}s, ${SLOW}x slower, no audio)"
