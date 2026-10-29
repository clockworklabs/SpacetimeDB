---
name: release
description: Build the StackBench images, assemble a release candidate, and sign a qualified release
metadata:
  author: clockworklabs
  role: operator
---

# Build and release

StackBench has two release states:

- A **candidate** has exact image digests, checksummed files, and one SPDX SBOM
  (software bill of materials) per image. It is unsigned.
- A **qualified** release adds a public key, a Sigstore signature on
  `release.json`, and a registry signature on every image. Only the release
  format `schema v2` is accepted.

A branch build is a local candidate. It runs provisional campaigns without any
of the signing steps.

## Build the images

From a clean clone, at the repository root. The build refuses a dirty tree.

```sh
docker build --platform linux/amd64 -t stack-bench-build:local tools/stack-bench/container
docker build --platform linux/amd64 -f tools/stack-bench/appliance/Controller.Dockerfile -t stack-bench-controller:local .
```

The controller image records the source revision and the checksums of the
native binaries it built. Setup pins the images by content ID, so rebuilding a
tag never changes a run that was already prepared.

## Assemble a candidate

1. Push the first-party images and resolve every image, first- and
   third-party, to an exact single-platform `linux/amd64` digest. Do not use a
   multi-architecture index digest.
2. Generate one SBOM per image digest:

   ```sh
   node dist/src/releases/release-bundle.js sbom registry.example/controller@sha256:DIGEST --output bundle/sbom/controller.spdx.json
   ```

3. Write `release-spec.json` with `state: "candidate"`, `signing: null`, and a
   `files` list of `path` and `role`. Put every file under the bundle root.
4. Assemble and verify:

   ```sh
   node dist/src/releases/release-bundle.js assemble release-spec.json --root bundle --output bundle/release.json
   node dist/src/releases/release-manifest.js verify bundle/release.json --root bundle
   ```

   Verification reports `candidate-file-integrity` when every file and every
   image-to-SBOM binding checks out.

## Sign a qualified release

Signing keys are CI inputs. Never copy a private key, registry token, or
password into the source tree, an image, the bundle, or a command transcript.

1. Sign each exact registry image with Cosign.
2. Add the public key as `signing/cosign.pub` with role `public-key`.
3. Change the spec to `state: "qualified"` and declare:

   ```json
   { "signing": { "scheme": "cosign-public-key-v1", "publicKeyPath": "signing/cosign.pub", "manifestBundlePath": "signing/release-manifest.sigstore.json" } }
   ```

4. Assemble `release.json` last, then sign that exact file:

   ```sh
   cosign sign-blob --yes --key "$COSIGN_KEY" --bundle bundle/signing/release-manifest.sigstore.json bundle/release.json
   ```

5. Verify with a copy of the public key stored outside the bundle:

   ```sh
   node dist/src/releases/release-manifest.js verify bundle/release.json --root bundle --trusted-key /operator/trust/stack-bench-cosign.pub
   ```

   A missing or bundle-local trust key, a signature mismatch, or a failed
   `cosign verify` on any image fails the release. There is no fallback to
   candidate verification.

## Publish the key

A bundle cannot vouch for its own key. Publish the public key and its SHA-256
fingerprint through a separate channel, and tell operators to compare the
fingerprint before verifying. Rotating the key means a new release and a new
fingerprint announcement.
