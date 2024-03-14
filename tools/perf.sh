#!/bin/env bash

set -euo pipefail

# Function to print help information
print_help() {
    cat <<EOF
Run perf against an existing SpacetimeDB process, and produce a flamegraph.

Options:
-o FILE, --output FILE   Filename to save flamegraph.             Default: flamegraph.html
-d FILE, --output FILE   Filename to store perf data.             Default: perf.data
-p PID,  --pid PID       Process ID of the SpacetimeDB process.   Default: determined by ps
-t SEC,  --time SEC      Duration to sample for.                  Default: 10
-h,      --help          Print this message, then exit.
EOF
}

if ! [ -f /usr/share/d3-flame-graph/d3-flamegraph-base.html ] ; then
    read -p "Could not find d3-flamegraph-base.html, should we download it? (y/n): " answer

    case $answer in
        [Yy]* )
            url="https://cdn.jsdelivr.net/npm/d3-flame-graph@4.1.3/dist/templates/d3-flamegraph-base.html"
            target_dir="/usr/share/d3-flame-graph"
            target_file="${target_dir}/d3-flamegraph-base.html"
            sudo mkdir -p "$target_dir"
            echo "Downloading d3-flamegraph-base.html to ${target_file}..."
            sudo curl -o "$target_file" "$url"

            if [ $? -eq 0 ]; then
                echo "Download complete."
            else
                echo "Download failed. Please check your internet connection and the URL."
            fi
            ;;
        [Nn]* )
            echo "Download skipped."
            ;;
        * )
            echo "Invalid response. Please answer y or n."
            ;;
    esac
fi

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
            print_help
            exit 0
            ;;
        *)
            >&2 echo "Unknown parameter $1. Pass --help for valid options."
            exit 1
            ;;
    esac
done

set +u

if [[ -z "$OUTFILE" ]]; then
    OUTFILE=flamegraph.html
fi
if [[ -z "$DATAFILE" ]]; then
    DATAFILE=perf.data
fi
if [[ -z "$SPACETIME_PID" ]]; then
    # -f allows us to get the args
    # -e lets us see all users
    SPACETIMES="$(ps -a -e -f | grep '\<spacetime\>.*\<start\>' | grep -v '\<grep\>')"
    LINES="$(echo "$SPACETIMES" | wc -l)"
    if [[ $LINES < 1 ]] ; then
        >&2 echo "spacetime PID not found, is it running?"
        exit 1
    elif [[ $LINES > 1 ]] ; then
        >&2 echo "Multiple spacetime PIDs. Specify one with -z"
        >&2 echo "$SPACETIMES"
        exit 1
    fi

    SPACETIME_PID=$(echo "$SPACETIMES" | awk '{print $2}')
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

