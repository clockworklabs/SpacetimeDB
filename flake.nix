{
  description = "A Nix-flake-based Rust development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      treefmt-nix,
      rust-overlay,
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forEachSupportedSystem =
        f:
        nixpkgs.lib.genAttrs supportedSystems (
          system:
          f (rec {
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };

            rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
            rustToolchain = rustToolchainFor pkgs;

            # NB: we don't need to overlay our custom toolchain for the *entire*
            # pkgs (which would require rebuidling anything else which uses rust).
            # Instead, we just want to update the scope that crane will use by appending
            # our specific toolchain there.
            craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchainFor;
          })
        );
    in
    {
      formatter = forEachSupportedSystem (
        { pkgs, ... }: (treefmt-nix.lib.evalModule pkgs ./treefmt.nix).config.build.wrapper
      );
      devShells = forEachSupportedSystem (
        { pkgs, rustToolchain, ... }:
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
            ];
          };
        }
      );
      packages = forEachSupportedSystem (
        {
          pkgs,
          rustToolchain,
          craneLib,
          ...
        }:
        let
          rustToolchainSettings = (pkgs.lib.importTOML ./rust-toolchain.toml).toolchain;
          workspaceManifest = (pkgs.lib.importTOML ./Cargo.toml).workspace.package;

          # Build a workspace member, specified by its Cargo.toml
          buildPackage =
            manifestFile: nativeBuildInputs: overrides:
            (
              let
                manifest = (pkgs.lib.importTOML manifestFile).package;
              in
              craneLib.buildPackage (
                {
                  # Only build the specified package
                  cargoExtraArgs = "-p " + manifest.name;

                  pname = manifest.name;
                  src = ./.;
                  version = workspaceManifest.version;
                  strictDeps = true;

                  inherit nativeBuildInputs;
                }
                // overrides
              )
            );
        in
        {
          spacetimedb-cli = buildPackage (./crates/cli/Cargo.toml) (with pkgs; [
            git
            perl
          ]) ({ });
          spacetimedb-standalone = buildPackage (./crates/standalone/Cargo.toml) (with pkgs; [ perl ]) ({ });
          spacetimedb-update = buildPackage (./crates/update/Cargo.toml) (with pkgs; [ pkg-config ]) ({
            env.OPENSSL_DIR = "${pkgs.openssl.dev}";
            env.OPENSSL_LIB_DIR = "${pkgs.lib.getLib pkgs.openssl}/lib";
            env.OPENSSL_NO_VENDOR = 1;
          });
        }
      );

      overlays.default = final: prev: {
        spacetimedb-cli = self.packages.spacetimedb-cli.${final.system};
        spacetimedb-standalone = self.packages.spacetimedb-standalone.${final.system};
        spacetimedb-update = self.packages.spacetimedb-update.${final.system};
      };

      nixosModules.spacetimedb =
        { pkgs, ... }:
        {
          nixpkgs.overlays = [ self.overlay ];
        };
    };
}
