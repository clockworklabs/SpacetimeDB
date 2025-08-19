import argparse
import json
import subprocess
import sys
import toml
from pathlib import Path
from typing import Dict

def get_all_crate_metadata() -> Dict[str, dict]:
    result = subprocess.run(
        ['cargo', 'metadata', '--format-version=1', '--no-deps'],
        capture_output=True,
        text=True,
        check=True
    )
    metadata = json.loads(result.stdout)
    return {pkg['name']: pkg for pkg in metadata.get('packages', [])}

def find_spacetimedb_dependencies(crate_metadata, cargo_toml_path):
    with open(cargo_toml_path, 'r') as file:
        cargo_data = toml.load(file)

    deps = cargo_data.get('dependencies', {})
    return [dep for dep in deps if dep in crate_metadata]

def process_crate(crate_name, crate_metadata, recursive=False, debug=False):
    cargo_toml_path = Path(crate_metadata[crate_name]['manifest_path'])

    if debug:
        print(f"\nChecking crate '{crate_name}'...")

    deps = find_spacetimedb_dependencies(crate_metadata, cargo_toml_path)

    if debug:
        if deps:
            for name in deps:
                print(f"  {name}")
        else:
            print("  No spacetimedb-* dependencies found.")

    all_deps = list(deps)

    if recursive:
        for dep_name in deps:
            sub_deps = process_crate(dep_name, crate_metadata, recursive=True, debug=debug)
            all_deps.extend(sub_deps)

    return all_deps

def ordered_dedup(items):
    seen = set()
    result = []
    for item in items:
        if item not in seen:
            seen.add(item)
            result.append(item)
    return result

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Find spacetimedb-* dependencies for one or more crates.")
    parser.add_argument("root", nargs="+", help="One or more crate names to start with")
    parser.add_argument("--recursive", action="store_true", help="Recursively resolve dependencies")
    parser.add_argument("--quiet", action="store_true", help="Only print the final output")
    args = parser.parse_args()

    if not args.quiet:
        print("Loading crate metadata...", file=sys.stderr)
    crate_metadata = get_all_crate_metadata()

    all_crates = list(args.root)

    for crate in args.root:
        deps = process_crate(crate, crate_metadata, recursive=args.recursive, debug=not args.quiet)
        all_crates.extend(deps)

    # It takes a bit of reasoning to conclude that this is, in fact, going to be a legitimate
    # dependency-order of all of these crates. Because of how the list is constructed, once it's reversed,
    # every crate will be mentioned before any of the crates that use it. Because of that, it's safe to
    # deduplicate the list in a way that preserves the _first_ occurrence of every crate name, without
    # violating the "mentioned before it's used" property of the list.
    publish_order = reversed(all_crates)
    publish_order = ordered_dedup(publish_order)

    if not args.quiet:
        print("\nAll crates to publish, in order:")
    for crate in publish_order:
        print(crate)
