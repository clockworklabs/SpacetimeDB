#!/bin/bash

set -euo pipefail

spacetime generate --out-dir ../client/Assets/Scripts/autogen --lang cs $@
