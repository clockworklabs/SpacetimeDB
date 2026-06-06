#!/bin/bash

set -euo pipefail

spacetime generate --lang typescript --out-dir ../client-ts/src/module_bindings --module-path . $@
