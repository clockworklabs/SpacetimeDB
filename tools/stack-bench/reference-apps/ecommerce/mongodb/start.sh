#!/usr/bin/env bash
set -e
[ -d server/node_modules ] || npm --prefix server ci --no-audit --no-fund
[ -d client/node_modules ] || npm --prefix client ci --no-audit --no-fund
npm --prefix client run build
exec npm run start
