#!/bin/bash
set -eu -o pipefail

CROSSBUILD_MACOS_SDK="macosx12.3"

# Build macOS binaries
targets="aarch64-apple-darwin x86_64-apple-darwin"
for target in $targets; do
  rustup target add $target

  # From: https://stackoverflow.com/a/66875783/473672
  SDKROOT=$(xcrun -sdk $CROSSBUILD_MACOS_SDK --show-sdk-path) \
  MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk $CROSSBUILD_MACOS_SDK --show-sdk-platform-version) \
    cargo build --release "--target=$target"
done

# From: https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary#Update-the-Architecture-List-of-Custom-Makefiles
lipo -create \
  -output ../../target/spacetime-universal-apple-darwin-release \
  ../../target/aarch64-apple-darwin/release/spacetime \
  ../../target/x86_64-apple-darwin/release/spacetime
