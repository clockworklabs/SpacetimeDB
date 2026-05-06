#!/bin/bash

set -euo pipefail

SPACETIMEDB_SERVER_URL="${SPACETIMEDB_SERVER_URL:-local}"

spacetime publish -s "$SPACETIMEDB_SERVER_URL" blackholio --delete-data -y
