#!/bin/bash
set -e

sanitize_docker_ref() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | sed -e 's/[^a-z0-9._-]/-/g' -e 's/^[.-]//g' -e 's/[.-]$//g'
}

IMAGE_NAME="$1"
# Shorten the first argument (commit sha) to 7 chars
SHORT_SHA=${2:0:7}
TAG="commit-$SHORT_SHA"

# Check if images for both amd64 and arm64 exist
if docker pull "${IMAGE_NAME}":$TAG-amd64 --platform amd64 >/dev/null 2>&1 && docker pull "${IMAGE_NAME}":$TAG-arm64 --platform arm64 >/dev/null 2>&1; then
  echo "Both images exist, preparing the merged manifest"
else
  echo "One or both images do not exist. Exiting"
  exit 0
fi

# Extract digests
AMD64_DIGEST=$(docker manifest inspect "${IMAGE_NAME}":$TAG-amd64 | jq -r '.manifests[0].digest')
ARM64_DIGEST=$(docker manifest inspect "${IMAGE_NAME}":$TAG-arm64 | jq -r '.manifests[0].digest')

FULL_TAG="$SHORT_SHA-full"

# Create a new manifest using extracted digests
docker manifest create "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$AMD64_DIGEST \
  "${IMAGE_NAME}"@$ARM64_DIGEST

# Annotate the manifest with with proper platforms
docker manifest annotate "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$ARM64_DIGEST --os linux --arch arm64
docker manifest annotate "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$AMD64_DIGEST --os linux --arch amd64

# Push the manifest
docker manifest push "${IMAGE_NAME}":$FULL_TAG

# re-tag the manifeast with a tag
ORIGINAL_VERSION=${GITHUB_REF#refs/*/}
VERSION=$(sanitize_docker_ref "$ORIGINAL_VERSION")
docker buildx imagetools create "${IMAGE_NAME}":$FULL_TAG --tag "${IMAGE_NAME}":$VERSION
