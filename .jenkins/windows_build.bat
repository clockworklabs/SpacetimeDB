@echo off
set branch=%1

rustup update

cd SpacetimeDB
git fetch -a origin
git checkout -f origin/%branch%
cargo build --release
