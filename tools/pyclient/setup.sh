#!/bin/bash

# TODO: this should probably be done with setup.py or its more modern equivalents + a Makefile (for protoc etc)
# Additionally resolution of the proto path could be cleaner.

set -euo pipefail
cd "$(dirname "$0")"

pip install argparse
pip install websocket-client

protoc  --python_out=`pwd` -I=../../crates/spacetimedb/protobuf ../../crates/spacetimedb/protobuf/WebSocket.proto
