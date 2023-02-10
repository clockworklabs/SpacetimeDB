@echo off
set branch=%1

cd SpacetimeDB
git fetch -a origin
git checkout -f origin/%branch%
cargo build --release
