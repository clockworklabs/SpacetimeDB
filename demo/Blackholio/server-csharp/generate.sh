#!/bin/bash

set -euo pipefail

spacetime generate --out-dir ../client-unity/Assets/Scripts/autogen --lang cs --module-path ./ $@
spacetime generate --lang unrealcpp --uproject-dir ../client-unreal --module-path ./ --module-name client_unreal
