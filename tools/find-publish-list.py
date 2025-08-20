import argparse
import json
import subprocess
import sys
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

def find_spacetimedb_dependencies(crate_metadata, crate):
    deps = crate_metadata[crate].get('dependencies', [])
    # filter out dev-dependencies. otherwise, we get dependency cycles.
    deps = [ dep for dep in deps if dep['kind'] != 'dev' ]
    dep_names = [ dep['name'] for dep in deps ]
    # We use --no-deps to generate the metadata, so a dep will be in crate_metadata only if it is
    # one we create in this workspace.
    dep_names = [ dep for dep in dep_names if dep in crate_metadata ]
    return dep_names

def process_crate(crate_name, crate_metadata, recursive=False, debug=False):
    if debug:
        print(f"\nChecking crate '{crate_name}'...")

    deps = find_spacetimedb_dependencies(crate_metadata, crate_name)

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
    parser.add_argument("--directories", action="store_true", help="Print crate paths instead of names")
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
        if args.directories:
            print(crate_metadata[crate]['manifest_path'])
        else:
            print(crate)
