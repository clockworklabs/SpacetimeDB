#!/bin/env bash

# Before running, download d3-flamegraph-base.html
# from https://cdn.jsdelivr.net/npm/d3-flame-graph@4.1.3/dist/templates/d3-flamegraph-base.html
# and put it at /usr/share/d3-flame-graph/d3-flamegraph-base.html

set -e

while [[ $# -gt 0 ]]; do
    case $1 in
        -o|--output)
            OUTFILE="$2"
            shift
            shift
            ;;
        -d|--data)
            DATAFILE="$2"
            shift
            shift
            ;;
        -p|--pid)
            SPACETIME_PID="$2"
            shift
            shift
            ;;
        -t|--time)
            SLEEP_TIME="$2"
            shift
            shift
            ;;
        -h|--help)
            >&2 echo <<EOF
Run perf against an existing SpacetimeDB process, and produce a flamegraph.

Options:
-o FILE, --output FILE   Filename to save flamegraph.             Default: flamegraph.html
-d FILE, --output FILE   Filename to store perf data.             Default: perf.data
-p PID,  --pid PID       Process ID of the SpacetimeDB process.   Default: determined by ps
-t SEC,  --time SEC      Duration to sample for.                  Default: 10
-h,      --help          Print this message, then exit.
EOF
            exit 0
            ;;
        *)
            >&2 echo "Unknown parameter $1. Pass --help for valid options."
            exit 1
            ;;
    esac
done

if [[ -z "$OUTFILE" ]]; then
    OUTFILE=flamegraph.html
fi
if [[ -z "$DATAFILE" ]]; then
    DATAFILE=perf.data
fi
if [[ -z "$SPACETIME_PID" ]]; then
    SPACETIME_PID=$(ps -a | grep spacetime | awk '{print $1}')
fi
if ! [[ $SPACETIME_PID =~ ^[0-9]+$ ]]; then
    >&2 echo "Refusing to instrument suspicious-looking PID: $SPACETIME_PID"
    exit 1
fi
if [[ -z "$SLEEP_TIME" ]]; then
    SLEEP_TIME=10
fi
echo "Instrumenting PID $SPACETIME_PID for $SLEEP_TIME seconds."
echo "Writing data to $DATAFILE and flamegraph to $OUTFILE."

perf record -m 8192 --call-graph lbr --pid $SPACETIME_PID -o $DATAFILE sleep $SLEEP_TIME
perf script report flamegraph -i $DATAFILE -o $OUTFILE
