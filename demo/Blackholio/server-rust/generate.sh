#!/bin/bash

set -euo pipefail

spacetime generate --out-dir ../client-unity/Assets/Scripts/autogen --lang cs $@
