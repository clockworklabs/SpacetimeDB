#!/bin/bash

set -euo pipefail

spacetime generate --out-dir ../client-unity/Assets/Scripts/autogen --lang cs $@
spacetime generate --lang unrealcpp --uproject-dir ../client-unreal --project-path ./ --module-name client_unreal
