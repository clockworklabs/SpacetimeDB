#!/bin/bash
# sheet.sh out.png a.png b.png c.png d.png
o=$1; shift
ffmpeg -y -loglevel error -i $1 -i $2 -i $3 -i $4 -filter_complex "[0][1][2][3]xstack=inputs=4:layout=0_0|w0_0|0_h0|w0_h0,scale=1920:1080" $o
