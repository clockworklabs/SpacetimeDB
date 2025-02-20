#!/usr/bin/env sh

set -u

if command -v curl >/dev/null 2>&1; then
  _downloader=curl
elif command -v wget >/dev/null 2>&1; then
  _downloader=wget
else
  echo "Error: you need to have either 'curl' or 'wget' installed and in your path"
  exit 1
fi

# First make sure all config is valid, we don't want to end up with
# a partial install.

# Detect the OS and arch
_oss="$(uname -s)"
_cpu="$(uname -m)"

case "$_oss" in
Linux) _oss=linux ;;
Darwin) _oss=darwin ;;
*) err "Error: unsupported operating system: $_oss" ;;
esac

case "$_cpu" in
arm64 | aarch64) _cpu=arm64 ;;
x86_64 | x86-64 | x64 | amd64) _cpu=amd64 ;;
*) err "Error: unsupported CPU architecture: $_cpu" ;;
esac

_arc="${_oss}-${_cpu}"

# Compute the download file extension type
case "$_oss" in
linux) _ext=linux ;;
darwin) _ext=macos ;;
*) echo "Invalid OSS: $_oss" && exit 1 ;;
esac

# Define the latest SpacetimeDB download url
printf "This script will install the spacetimedb-cli command line tool. platform=%s os=%s\n" "$_arc" "$_oss"
# echo "Our EULA for spacetimedb-cli can be found here: https://eula.spacetimedb.com"
# read -p "Press [enter] to agree to the EULA"
printf "Press [enter] to install the spacetimedb-cli binary. Use Ctrl-C now to exit.\n\n"
read -r ans

# We can now install the binary
_download_file="$(mktemp)"
rm -f "$_download_file"
_download_file="${_download_file}.tar.gz"
_extract_dir="$(mktemp -d)"
_url="https://github.com/clockworklabs/SpacetimeDB/releases/latest/download/spacetime.$_arc.tar.gz"
if [ "$_downloader" = curl ]; then
  echo "Downloading from https://install.spacetimedb.com..."
  curl -L -sSf --progress-bar "$_url" -o "$_download_file"
elif [ "$_downloader" = wget ]; then
  echo "Downloading from https://install.spacetimedb.com..."
  wget -O - "$_url" >"$_download_file"
fi

echo "Extracting..."
tar xf "$_download_file" -C "$_extract_dir"
rm -f "$_download_file"
_bin_file="$_extract_dir/spacetime"

# Note: We would like to install globally if we can, however some users may not have sudo access.
if [ "$_oss" = linux ]; then
  echo "Default: /usr/bin/spacetime"
  if [ ! -z ${USER:-} ]; then
    echo "If you do not have sudo access for your computer, we recommend using /home/$USER/bin/spacetime"
  fi
  printf "Press enter or provide the full install path you would like to use: "
  read -r _install_path

  if [ "$_install_path" = "" ]; then
    _install_path="/usr/bin/spacetime"
  fi
elif [ "$_oss" = darwin ]; then
  echo "Default: /usr/local/bin/spacetime"
  if [ ! -z "${USER:-}" ]; then
    echo "If you do not have sudo access for your computer, we recommend using $HOME/bin/spacetime"
  fi
  read -p "Press enter or provide the full install path you would like to use: " _install_path

  if [ "$_install_path" = "" ]; then
    _install_path="/usr/local/bin/spacetime"
  fi
fi

_install_dir="$(dirname $_install_path)"
if [ ! -d "$_install_dir" ]; then
  if ! mkdir -pv "$_install_dir"; then
    sudo mkdir -pv "$_install_dir"
    if ! [ -d "$_install_dir" ]; then
      echo "Fatal Error: Failed to create installation direction."
      exit 1
    fi
  fi
fi

chmod +x "$_bin_file"
if ! mv -v "$_bin_file" "$_install_path"; then
  sudo mv -v "$_bin_file" "$_install_path"
fi

printf "\nspacetime is installed into %s Note: we recommend making sure that this executable is in your PATH.\n" "$_install_path"
printf "The install process is complete, head over to our quickstart guide to get started!\n\n"
printf "\thttps://spacetimedb.com/docs/quick-start\n\n"
