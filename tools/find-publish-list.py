import argparse
import toml
from pathlib import Path

def find_spacetimedb_dependencies(cargo_toml_path):
    with open(cargo_toml_path, 'r') as file:
        cargo_data = toml.load(file)

    deps = cargo_data.get('dependencies', {})
    return [dep for dep in deps if dep.startswith("spacetimedb-")]

def dep_to_crate_dir(dep_name):
    return dep_name.replace("spacetimedb-", "", 1)

def process_crate(crate_name, crates_dir, recursive=False, debug=False):
    cargo_toml_path = crates_dir / crate_name / "Cargo.toml"

    if not cargo_toml_path.is_file():
        if debug:
            print(f"Warning: Cargo.toml not found for crate '{crate_name}' at {cargo_toml_path}")
        return []

    if debug:
        print(f"\nChecking crate '{crate_name}'...")

    deps = find_spacetimedb_dependencies(cargo_toml_path)

    if debug:
        if deps:
            for name in deps:
                print(f"  {name}")
        else:
            print("  No spacetimedb-* dependencies found.")

    all_deps = list(deps)

    if recursive:
        for dep_name in deps:
            sub_crate = dep_to_crate_dir(dep_name)
            sub_deps = process_crate(sub_crate, crates_dir, recursive=True, debug=debug)
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

    crates_dir = Path("crates")
    all_crates = list(args.root)

    for crate in args.root:
        deps = process_crate(crate, crates_dir, recursive=args.recursive, debug=not args.quiet)
        all_crates.extend(dep_to_crate_dir(dep) for dep in deps)

    publish_order = reversed(all_crates)
    publish_order = ordered_dedup(publish_order)

    if not args.quiet:
        print("\nAll crates to publish, in order:")
    for crate in publish_order:
        print(crate)
