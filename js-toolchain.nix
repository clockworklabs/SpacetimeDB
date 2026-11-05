# Node.js and pnpm for the dev shell, with versions taken from package.json,
# the way flake.nix takes the Rust toolchain from rust-toolchain.toml:
#
# - Node: the minimum version of the first `engines.node` range
#   (`^22.23.4 || ...` gives 22.23.4), which is the version CI installs.
# - pnpm: the version in `packageManager`.
#
# Both are official prebuilt releases. Nix needs a hash for every download, so
# the hashes live in the tables below, keyed by version. If package.json moves to
# a version that has no entry, evaluation fails and says where to find the hashes,
# so the shell never quietly keeps an old version.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  packageJson ? ./package.json,
}: let
  pkg = lib.importJSON packageJson;
  system = stdenv.hostPlatform.system;

  # sha256 of each `node-v<version>-<platform>.tar.xz`, copied from
  # https://nodejs.org/dist/v<version>/SHASUMS256.txt
  nodeHashes = {
    "22.23.4" = {
      x86_64-linux = "1805bcd35e6815559bd2eb97db438d3fdb0eb3a5e6a977b7c1badca76337e9ce";
      aarch64-linux = "6287aa46b6c375d816e63ae66d024fbbfd70404825aabdfdf898a970871d2f4b";
      x86_64-darwin = "f42abca7aaac61efc57d49fb42ea84fc53bbeba9a749cb03a291ce6e4e7a69c1";
      aarch64-darwin = "037650d0839f457747396f052073fb48333d31bf5abd3c98b74da20d89ebb1ad";
    };
  };

  # Integrity of each pnpm tarball, from `npm view pnpm@<version> dist.integrity`.
  pnpmHashes = {
    "10.16.0" = "sha512-gGbnsDQhe3AKmk27OgBQYdZBuhMKiZFSE6ELPKSRnBnAN77IBmr9xVm4ljX9uAaxbqZz8kaPuyiqv6E8U+P3aQ==";
  };

  nodePlatforms = {
    x86_64-linux = "linux-x64";
    aarch64-linux = "linux-arm64";
    x86_64-darwin = "darwin-x64";
    aarch64-darwin = "darwin-arm64";
  };

  # "^22.23.4 || ^24.22.1 || >=26.12.1" -> "22.23.4"
  nodeVersion = let
    range = pkg.engines.node or (throw "package.json has no engines.node");
    first = lib.trim (lib.head (lib.splitString "||" range));
    parsed = builtins.match "(\\^|~|>=|=)?v?([0-9]+\\.[0-9]+\\.[0-9]+)" first;
  in
    if parsed == null
    then throw "js-toolchain.nix: cannot take a minimum Node version from engines.node range `${first}` in package.json"
    else lib.elemAt parsed 1;

  pnpmVersion = let
    parsed = builtins.match "pnpm@([0-9]+\\.[0-9]+\\.[0-9]+)(\\+.*)?" (pkg.packageManager or "");
  in
    if parsed == null
    then throw "js-toolchain.nix: package.json packageManager must be `pnpm@<x.y.z>`, got `${pkg.packageManager or ""}`"
    else lib.head parsed;

  nodeSha256 =
    nodeHashes.${nodeVersion}.${system} or (throw ''
      js-toolchain.nix: no hash for Node ${nodeVersion} on ${system}.
      package.json engines.node asks for Node ${nodeVersion}. Add its hashes to nodeHashes
      from https://nodejs.org/dist/v${nodeVersion}/SHASUMS256.txt
      (the node-v${nodeVersion}-<platform>.tar.xz lines).
    '');

  pnpmHash =
    pnpmHashes.${pnpmVersion} or (throw ''
      js-toolchain.nix: no hash for pnpm ${pnpmVersion}.
      package.json packageManager asks for pnpm ${pnpmVersion}. Add it to pnpmHashes
      from `npm view pnpm@${pnpmVersion} dist.integrity`.
    '');

  nodejs = stdenv.mkDerivation {
    pname = "nodejs-bin";
    version = nodeVersion;

    src = fetchurl {
      url = "https://nodejs.org/dist/v${nodeVersion}/node-v${nodeVersion}-${nodePlatforms.${system}}.tar.xz";
      sha256 = nodeSha256;
    };

    # The Linux release binaries expect a standard FHS loader and libstdc++.
    nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [autoPatchelfHook];
    buildInputs = lib.optionals stdenv.hostPlatform.isLinux [stdenv.cc.cc.lib];

    dontConfigure = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -r bin include lib share $out/
      runHook postInstall
    '';

    # Point the `#!/usr/bin/env node` shebangs of npm, npx and corepack at this node.
    # patchShebangs resolves interpreters from HOST_PATH, not PATH.
    preFixup = ''
      export HOST_PATH=$out/bin''${HOST_PATH:+:$HOST_PATH}
    '';
  };

  pnpm = stdenv.mkDerivation {
    pname = "pnpm";
    version = pnpmVersion;

    src = fetchurl {
      url = "https://registry.npmjs.org/pnpm/-/pnpm-${pnpmVersion}.tgz";
      hash = pnpmHash;
    };

    nativeBuildInputs = [makeWrapper];

    dontConfigure = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall
      mkdir -p $out/lib/pnpm $out/bin
      cp -r . $out/lib/pnpm
      makeWrapper ${nodejs}/bin/node $out/bin/pnpm --add-flags $out/lib/pnpm/bin/pnpm.cjs
      makeWrapper ${nodejs}/bin/node $out/bin/pnpx --add-flags $out/lib/pnpm/bin/pnpx.cjs
      runHook postInstall
    '';
  };
in {
  inherit nodejs pnpm;
}
