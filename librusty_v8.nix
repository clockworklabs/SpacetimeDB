# Fetches a pre-built v8 build from GitHub.
#
# rusty_v8 is a bad citizen: it attempts to download a binary within its build.rs.
# When running in the Nix sandbox, this download fails, so the crate cannot build.
# We instead download the archive ahead of time using a Nix-friendly fetcher function,
# and in our flake.nix we'll reference it in an appropriate env var so the crate build finds it.
#
# From https://github.com/msfjarvis/crane_rusty_v8/blob/4e076af4edb396d9d9398013d4393ec8da49c841/librusty_v8.nix
# modified for our desired version
{
  rust,
  stdenv,
  fetchurl,
}: let
  arch = rust.toRustTarget stdenv.hostPlatform;
  fetch_librusty_v8 = args:
    fetchurl {
      name = "librusty_v8-${args.version}";
      url = "https://github.com/denoland/rusty_v8/releases/download/v${args.version}/librusty_v8_release_${arch}.a.gz";
      sha256 = args.shas.${stdenv.hostPlatform.system};
      meta = {inherit (args) version;};
    };
in
  fetch_librusty_v8 {
    version = "140.2.0";
    shas = {
      x86_64-linux = "sha256-r3qrYDVaT4Z6udC6YuQG1BKqrsQc7IhuACDCTbr083U=";
      # I (pgoldman 2025-10-17) only use x86_64-linux, so I haven't filled in these hashes.
      # If you use one of these platforms, run the build and wait for it to fail,
      # copy the detected sha256 from the error message in here, then re-run.
      aarch64-linux = "0000000000000000000000000000000000000000000000000000";
      x86_64-darwin = "0000000000000000000000000000000000000000000000000000";
      aarch64-darwin = "sha256-eZ2l9ovI2divQake+Z4/Ofcl5QwJ+Y/ql2Dymisx1oA=";
    };
  }
