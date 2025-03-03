#!/bin/bash
set -e

sanitize_docker_ref() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | sed -e 's/[^a-z0-9._-]/-/g' -e 's/^[.-]//g' -e 's/[.-]$//g'
}

# Shorten the first argument (commit sha) to 7 chars
SHORT_SHA=${1:0:7}
TAG="commit-$SHORT_SHA"

# Check if images for both amd64 and arm64 exist
if docker pull clockworklabs/spacetimedb:$TAG-amd64 --platform amd64 >/dev/null 2>&1 && docker pull clockworklabs/spacetimedb:$TAG-arm64 --platform arm64 >/dev/null 2>&1; then
  echo "Both images exist, preparing the merged manifest"
else
  echo "One or both images do not exist. Exiting"
  exit 0
fi

# Extract digests
AMD64_DIGEST=$(docker manifest inspect clockworklabs/spacetimedb:$TAG-amd64 | jq -r '.manifests[0].digest')
ARM64_DIGEST=$(docker manifest inspect clockworklabs/spacetimedb:$TAG-arm64 | jq -r '.manifests[0].digest')

FULL_TAG="$SHORT_SHA-full"

# Create a new manifest using extracted digests
docker manifest create clockworklabs/spacetimedb:$FULL_TAG \
  clockworklabs/spacetimedb@$AMD64_DIGEST \
  clockworklabs/spacetimedb@$ARM64_DIGEST

# Annotate the manifest with with proper platforms
docker manifest annotate clockworklabs/spacetimedb:$FULL_TAG \
  clockworklabs/spacetimedb@$ARM64_DIGEST --os linux --arch arm64
docker manifest annotate clockworklabs/spacetimedb:$FULL_TAG \
  clockworklabs/spacetimedb@$AMD64_DIGEST --os linux --arch amd64

# Push the manifest
docker manifest push clockworklabs/spacetimedb:$FULL_TAG

# re-tag the manifeast with a tag
ORIGINAL_VERSION=${GITHUB_REF#refs/*/}
VERSION=$(sanitize_docker_ref "$ORIGINAL_VERSION")
docker buildx imagetools create clockworklabs/spacetimedb:$FULL_TAG --tag clockworklabs/spacetimedb:$VERSION
