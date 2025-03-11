#!/bin/bash
set -e
set -u

sanitize_docker_ref() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | sed -e 's/[^a-z0-9._-]/-/g' -e 's/^[.-]//g' -e 's/[.-]$//g'
}

IMAGE_NAME="$1"
# Docker tag to use for platform-specific images
TAG="$2"
# Docker tag to use for the "universal" image
FULL_TAG="$3"

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

# Create a new manifest using extracted digests
docker manifest create "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$AMD64_DIGEST \
  "${IMAGE_NAME}"@$ARM64_DIGEST

# Annotate the manifest with with proper platforms
docker manifest annotate "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$ARM64_DIGEST --os linux --arch arm64
docker manifest annotate "${IMAGE_NAME}":$FULL_TAG \
  "${IMAGE_NAME}"@$AMD64_DIGEST --os linux --arch amd64

docker manifest push "${IMAGE_NAME}":$FULL_TAG

# if undefined, use the empty string
GITHUB_REF="${GITHUB_REF-}"
# re-tag the manifest with the GitHub ref
echo "GITHUB_REF is ${GITHUB_REF}"
if [[ "${GITHUB_REF}" == refs/tags/* ]]; then
  ORIGINAL_VERSION=${GITHUB_REF#refs/*/}
  VERSION=$(sanitize_docker_ref "$ORIGINAL_VERSION")
  echo "Tagging image with sanitized GITHUB_REF: $VERSION (original: $ORIGINAL_VERSION)"
  docker buildx imagetools create "${IMAGE_NAME}":$FULL_TAG --tag "${IMAGE_NAME}":$VERSION
fi

echo "Image merging and tagging completed successfully."
