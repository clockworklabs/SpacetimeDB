#!/bin/bash

set -u

cd "$(readlink -f "$(dirname "$0")")"
# to repo root
cd ../..

FILE='docs/docs/00500-cli-reference/00100-cli-reference.md'
cat <<EOF > "$FILE"
---
title: CLI Reference
slug: /cli-reference
---

EOF
cargo run --features markdown-docs -p spacetimedb-cli >> "$FILE"
