# Swift Package Index Submission + Badge Verification Checklist

Use this checklist when releasing the standalone mirror repository for `sdks/swift` (for example: `spacetimedb-swift`).

## Release Inputs

- [ ] Set release variables:

```bash
export MIRROR_REPO="../spacetimedb-swift"
export SPI_OWNER="<owner>"
export SPI_REPO="<repo>"
export SPI_VERSION="0.1.0"
```

## 1. One-Time Mirror Readiness

- [ ] Mirror repository is public.
- [ ] Mirror repository root contains package files:
  - `Package.swift`
  - `Package.resolved`
  - `Sources/`
  - `Tests/`
  - `.spi.yml`
- [ ] `.spi.yml` includes `SpacetimeDB` as a documentation target.

Quick check:

```bash
ls -la "$MIRROR_REPO"
cat "$MIRROR_REPO/.spi.yml"
```

## 2. Monorepo Preflight

Run from monorepo root before mirroring:

- [ ] `swift test --package-path sdks/swift`
- [ ] `swift package --package-path sdks/swift resolve --force-resolved-versions`
- [ ] `tools/swift-benchmark-smoke.sh`
- [ ] `tools/swift-docc-smoke.sh`
- [ ] `tools/check-swift-demo-bindings.sh`
- [ ] `swift build --package-path demo/simple-module/client-swift`
- [ ] `swift build --package-path demo/ninja-game/client-swift`

## 3. Mirror Sync + Release Tag

- [ ] Dry-run mirror sync:

```bash
tools/swift-package-mirror.sh sync --mirror "$MIRROR_REPO" --dry-run
```

- [ ] Create release commit + tag + push:

```bash
tools/swift-package-mirror.sh release --mirror "$MIRROR_REPO" --version "$SPI_VERSION" --push
```

- [ ] Confirm the release tag exists locally and on origin:

```bash
git -C "$MIRROR_REPO" tag --list "v$SPI_VERSION"
git -C "$MIRROR_REPO" ls-remote --tags origin "refs/tags/v$SPI_VERSION"
```

## 4. Submit Package To SPI (Manual)

- [ ] Open <https://swiftpackageindex.com/add-a-package>.
- [ ] Submit mirror repository URL (`https://github.com/$SPI_OWNER/$SPI_REPO`).
- [ ] Confirm submission acceptance and monitor indexing/doc generation progress.

## 5. Verify SPI Badge APIs

Use SPI badge JSON endpoints to confirm indexing has completed:

```bash
SPI_API_BASE="https://swiftpackageindex.com/api/packages/$SPI_OWNER/$SPI_REPO"

curl -fsSL "$SPI_API_BASE/badge?type=swift-versions" -o /tmp/spi-swift-versions.json
curl -fsSL "$SPI_API_BASE/badge?type=platforms" -o /tmp/spi-platforms.json

grep -q '"isError":false' /tmp/spi-swift-versions.json
grep -q '"isError":false' /tmp/spi-platforms.json

grep -o '"message":"[^"]*"' /tmp/spi-swift-versions.json
grep -o '"message":"[^"]*"' /tmp/spi-platforms.json
```

- [ ] Both JSON payloads report `"isError":false`.
- [ ] Swift versions message is populated (not `pending`).
- [ ] Platforms message is populated (not `pending`).

## 6. Verify Shield Badge Endpoints

```bash
SPI_API_BASE="https://swiftpackageindex.com/api/packages/$SPI_OWNER/$SPI_REPO"
SWIFT_BADGE="https://img.shields.io/endpoint?url=$SPI_API_BASE/badge?type=swift-versions"
PLATFORMS_BADGE="https://img.shields.io/endpoint?url=$SPI_API_BASE/badge?type=platforms"

curl -fsSL "$SWIFT_BADGE" | head -n 1
curl -fsSL "$PLATFORMS_BADGE" | head -n 1
```

- [ ] Both responses are SVG payloads.

## 7. Mirror README Links + Badges

- [ ] Mirror README includes working package page link and badge markdown:

```markdown
[Swift Package Index](https://swiftpackageindex.com/<owner>/<repo>)

![Swift Versions](https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=swift-versions)
![Platforms](https://img.shields.io/endpoint?url=https://swiftpackageindex.com/api/packages/<owner>/<repo>/badge?type=platforms)
```

- [ ] Replace all `<owner>/<repo>` placeholders with mirror coordinates.

## 8. Final Sign-Off

- [ ] SPI package page loads in browser:
  - `https://swiftpackageindex.com/$SPI_OWNER/$SPI_REPO`
- [ ] SPI documentation page for `SpacetimeDB` target is available from package page.
- [ ] Release notes/backlog reflect submission completion.
