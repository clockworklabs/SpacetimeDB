# Release assembly and verification

Stack Bench uses two deliberately different release states.

- A `candidate` has exact image digests, checksummed files, and digest-bound
  SPDX SBOMs. It is useful for inspecting and testing a proposed bundle, but it
  is unsigned and cannot be called qualified.
- A `qualified` release adds a bundled public key, a detached Sigstore bundle
  covering `release.json`, and registry signatures for every image. Verification
  must use a public key obtained outside the release bundle.

Schema v2 is the only accepted format. Pre-release schema v1 required candidate
bundles to contain placeholder signature files and could not verify a qualified
release. It has no compatibility reader because no v1 bundle was published.

## Build a candidate

Publish the first-party images, resolve every first- and third-party image to an
exact single-platform `linux/amd64` manifest reference, then generate one SPDX
SBOM for each exact reference. Do not use a multi-architecture index digest:
Docker Scout correctly reports the selected child-manifest digest, so an index
digest cannot satisfy the one-image/one-SBOM identity contract.

```sh
node release-bundle.mjs sbom registry.example/controller@sha256:DIGEST \
  --output bundle/sbom/controller.spdx.json
```

The command uses registry resolution, refuses mutable references and existing
output, and checks that Docker Scout's SPDX 2.3 document contains the requested
image digest. A successful tool exit without that digest binding is rejected.

Create a strict release specification with `state: "candidate"`,
`signing: null`, and `files` entries containing only `path` and `role`. Place
every input below the bundle root, then materialize immutable size and SHA-256
metadata:

```sh
node release-bundle.mjs assemble release-spec.json \
  --root bundle --output bundle/release.json
node release-manifest.mjs verify bundle/release.json --root bundle
```

Candidate verification reports `candidate-file-integrity`. It validates all
declared files and all four image-to-SBOM digest bindings. Candidate manifests
must use `signing: null` and cannot include a public signing key.

## Sign and qualify

Signing keys are external CI inputs. Never copy a private key, registry token,
or signing password into the source tree, image, bundle, Compose environment,
or command transcript. Sign each exact registry image with Cosign. The
authoritative image-signature evidence stays attached to the registry object
and is checked directly during verification; the release does not preserve a
redundant unverified export. Add the public half of the signing key as
`signing/cosign.pub` with the `public-key` role.

Change the specification to `state: "qualified"` and declare:

```json
{
  "signing": {
    "scheme": "cosign-public-key-v1",
    "publicKeyPath": "signing/cosign.pub",
    "manifestBundlePath": "signing/release-manifest.sigstore.json"
  }
}
```

Assemble `release.json` only after all other evidence exists, then sign that
exact file with a detached Cosign bundle:

```sh
cosign sign-blob --yes --key "$COSIGN_KEY" \
  --bundle bundle/signing/release-manifest.sigstore.json bundle/release.json
```

The detached bundle is intentionally not checksummed by `release.json`: a file
cannot contain the hash of its own signature. Cosign authenticates it instead.

Verify with the trusted public key copied to a path outside the downloaded
bundle:

```sh
node release-manifest.mjs verify bundle/release.json --root bundle \
  --trusted-key /operator/trust/stack-bench-cosign.pub
```

Qualified verification refuses an absent or bundle-local trust key, requires
it to equal the public key bound by the signed manifest, verifies the detached
manifest signature, and runs `cosign verify` against every exact registry image
reference. A failed or unavailable Cosign invocation is a failed release; there
is no downgrade to candidate verification. The controller image includes
checksum-pinned Cosign 3.1.3 so this command is available in the delivered
appliance rather than depending on an untracked host installation.

## Trust distribution

The release bundle cannot establish trust in its own key. Publish the expected
public key and its SHA-256 fingerprint through a separately controlled channel.
The operator must compare that fingerprint before verification. Key rotation
requires a new release and an explicit trust-distribution update.
