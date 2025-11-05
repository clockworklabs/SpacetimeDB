{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    # crane is a framework for building Rust projects in Nix,
    # which we prefer over alternatives because it better handles multi-crate workspaces.
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    # rust-overlay provides more and more recent builds of the rust toolchain
    # than are available in nixpkgs.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };

        inherit (pkgs) lib;

        # Inject git commit in an env var around the build so that we can embed it in the binary
        # without calling into `git` during our build.
        # Note that `self.rev` is not set for builds with a dirty worktree, in which case we instead use `self.dirtyRev`.
        gitCommit = if (self ? rev) then self.rev else self.dirtyRev;

        librusty_v8 = if pkgs.stdenv.isDarwin then
          # Building on MacOS, we've seen errors building rusty_v8 with a local RUSTY_V8_ARCHIVE:
          # https://github.com/clockworklabs/SpacetimeDB/pull/3422#issuecomment-3416972711 .
          # For now, error on MacOS (darwin) targets.
          builtins.abort ''
          This flake doesn't work on MacOS due to some quirk of compiling rusty-v8 against a precompiled V8 archive.
          If you can get a build working on MacOS under Nix, please submit a PR to https://github.com/clockworklabs/SpacetimeDB/pulls.
          See https://github.com/clockworklabs/SpacetimeDB/pull/3422 for more details.
          ''
            # We fetch a precompiled v8 binary.
            # The rusty_v8 build.rs normally tries to download v8 artifacts during compilation,
            # but the Nix build sandbox doesn't give it network access.
            # Instead, download the archive in a Nix-friendly way with a recorded sha.
                      else (pkgs.callPackage ./librusty_v8.nix {});

        # The Rust toolchain that we actually build with.
        rustStable = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # An additional Rust toolchain we put in our devShell for rust-analyzer.
        rustNightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.rust-analyzer);

        version = (craneLib.crateNameFromCargoToml { inherit src; }).version;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustStable;

        # We don't use craneLib.cleanCargoSource here because we have a lot of non-Rust files in our repo.
        # I (pgoldman 2025-10-17) am too lazy to properly compose together source cleaners appropriately.
        src = lib.cleanSource ./.;

        # Arguments we'll pass to all of our derivations.
        commonArgs = {
          inherit src;
          strictDeps = true;
          # nativeBuildInputs are tools that are required to run during the build.
          # Usually this is stuff like programming language interpreters for build scripts.
          # In cross-compilation, these will be packages for the host machine's architecture.
          nativeBuildInputs = [
            pkgs.perl
            pkgs.python3
            pkgs.cmake
            pkgs.pkg-config
          ];
          # buildInputs are libraries that wind up in the target build.
          # In cross-compilation, these will be packages for the target machine's architecture.
          buildInputs = [
            pkgs.openssl
          ];
          # Add MacOS specific dependencies to either nativeBuildInputs or buildInputs with the following snippet:
          # ++ lib.optionals pkgs.stdenv.isDarwin [
          #   pkgs.whateverPackage
          # ];

          # Include our precompiled V8.
          RUSTY_V8_ARCHIVE = librusty_v8;
          SPACETIMEDB_NIX_BUILD_GIT_COMMIT = gitCommit;
        };

        # Build a separate derivation containing our dependencies,
        # which can be cached and shared between `-cli` and `-standalone`.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts version;
          # We disable tests since we'll run them all in the checks target
          doCheck = false;
        };

        makeSpacetimePackage = name: craneLib.buildPackage (individualCrateArgs // {
          pname = name;
          cargoExtraArgs = "-p ${name}";
        });

        spacetimedb-cli = makeSpacetimePackage "spacetimedb-cli";

        spacetimedb-standalone = makeSpacetimePackage "spacetimedb-standalone";

        # I've chosen not to package spacetimedb-update, since it won't work on Nix systems anyways.

        # Combine -standalone and -cli into a single derivation, with -cli named as spacetime.
        # It would be nice to use `symlinkJoin` here, but our re-exec machinery to have -cli call into -standalone
        # misbehaves when the two binaries are neighboring symlinks to real files in different directories.
        # So we just copy them.
        spacetime = pkgs.runCommand "spacetime-${version}" {} ''
          mkdir -p $out/bin

          cp ${spacetimedb-cli}/bin/spacetimedb-cli $out/bin/spacetime
          cp ${spacetimedb-standalone}/bin/spacetimedb-standalone $out/bin/spacetimedb-standalone
        '';
      in
        {
          checks = {
            inherit spacetimedb-cli spacetimedb-standalone;

            workspace-clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            workspace-fmt = craneLib.cargoFmt {
              inherit src;
            };

            workspace-test = craneLib.cargoTest (commonArgs // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
              # I (pgoldman 2025-10-17) have not figured out a sensible packaging of Unreal or of the .NET WASI SDK.
              # The SDK reauth tests attempt to create files in the home directory, which the nix sandbox disallows.
              cargoTestExtraArgs = "--workspace -- --skip unreal --skip csharp --skip reauth";
            });

            # TODO: Also run smoketests.
          };

          packages = {
            inherit spacetimedb-cli spacetimedb-standalone spacetime;
            default = spacetime;
          };

          devShells.default = craneLib.devShell {
            checks = self.checks.${system};

            inputsFrom = [ spacetimedb-standalone spacetimedb-cli ];

            # Required to make jemalloc_tikv_sys build in local development, otherwise you get:
            #   /nix/store/0zv32kh0zb4s1v4ld6mc99vmzydj9nm9-glibc-2.40-66-dev/include/features.h:422:4: warning: #warning _FORTIFY_SOURCE requires compiling with optimization (-O) [-Wcpp]
            #    422 | #  warning _FORTIFY_SOURCE requires compiling with optimization (-O)
            #        |    ^~~~~~~
            #  In file included from /nix/store/0zv32kh0zb4s1v4ld6mc99vmzydj9nm9-glibc-2.40-66-dev/include/bits/libc-header-start.h:33,
            #                   from /nix/store/0zv32kh0zb4s1v4ld6mc99vmzydj9nm9-glibc-2.40-66-dev/include/math.h:27,
            #                   from include/jemalloc/internal/jemalloc_internal_decls.h:4,
            #                   from include/jemalloc/internal/jemalloc_preamble.h:5,
            #                   from src/pac.c:1:
            CFLAGS = "-O";

            packages = [
              rustStable
              rustNightly
              cargoArtifacts
              pkgs.cargo-insta
            ];
          };
        }
    );
}
