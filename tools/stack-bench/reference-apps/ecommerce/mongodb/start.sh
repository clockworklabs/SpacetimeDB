#!/usr/bin/env bash
set -e
npm --prefix server ci --no-audit --no-fund
npm --prefix client ci --no-audit --no-fund
npm --prefix client run build
exec npm run start
