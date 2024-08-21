#!/bin/bash

cd "$(dirname "$0")"

function fix_meta_files {
    # Directory containing the .cs and .cs.meta files
    DIR="$(pwd)/.."

    # Find all .cs and .cs.meta files
    CS_FILES=$(find "$DIR" -name "*.cs")
    META_FILES=$(find "$DIR" -name "*.cs.meta")

    # Convert lists to arrays
    CS_ARRAY=($CS_FILES)
    META_ARRAY=($META_FILES)

    # Create associative arrays to check existence
    declare -A CS_MAP
    declare -A META_MAP

    # Populate CS_MAP with .cs files (remove extensions for easier comparison)
    for cs in "${CS_ARRAY[@]}"; do
      base=${cs%.cs}
      CS_MAP["$base"]=1
    done

    # Check for .cs.meta files with no associated .cs file
    for meta in "${META_ARRAY[@]}"; do
      base=${meta%.cs.meta}
      if [[ -z ${CS_MAP["$base"]} ]]; then
        echo "Deleting orphaned meta file: $meta"
        rm "$meta"
      else
        META_MAP["$base"]=1
      fi
    done

    # Check for .cs files with no associated .cs.meta file
    for cs in "${CS_ARRAY[@]}"; do
      base=${cs%.cs}
      if [[ -z ${META_MAP["$base"]} ]]; then
        echo "Warning: Missing .cs.meta file for $cs - You MUST create this file in Unity!"
      fi
    done
}


STDB_CSHARP="../../spacetimedb-csharp-sdk"

if ! [ -d "../../spacetimedb-csharp-sdk" ] ; then
    echo "Please clone the spacetimedb-csharp-sdk as a directory sibiling of the UnitySDK repo."
    echo "git clone https://github.com/clockworklabs/spacetimedb-csharp-sdk.git"
    exit 1
fi

if ! which rsync > /dev/null ; then
    echo "This script requires rsync, which may be installed through brew on macOS:"
    echo ""
    echo "  brew install rsync"
    echo ""
    echo "or through your package manager in Linux."
fi

rsync -av $STDB_CSHARP/src/ ../Scripts --delete
fix_meta_files
