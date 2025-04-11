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

def process_crate(crate_name, crates_dir, recursive=False):
    cargo_toml_path = crates_dir / crate_name / "Cargo.toml"

    if not cargo_toml_path.is_file():
        print(f"Warning: Cargo.toml not found for crate '{crate_name}' at {cargo_toml_path}")
        return []

    print(f"\nChecking crate '{crate_name}'...")
    deps = find_spacetimedb_dependencies(cargo_toml_path)

    if deps:
        for name in deps:
            print(f"  {name}")
    else:
        print("  No spacetimedb-* dependencies found.")

    all_deps = list(deps)

    if recursive:
        for dep_name in deps:
            sub_crate = dep_to_crate_dir(dep_name)
            sub_deps = process_crate(sub_crate, crates_dir, recursive=True)
            all_deps.extend(sub_deps)

    return all_deps

def dedupe_preserve_first(items):
    seen = set()
    result = []
    for item in items:
        if item not in seen:
            seen.add(item)
            result.append(item)
    return result

def main():
    parser = argparse.ArgumentParser(description="Recursively find spacetimedb-* dependencies for one or more crates.")
    parser.add_argument("crate", nargs="+", help="One or more crate names (in crates/<crate>/Cargo.toml)")
    parser.add_argument("--recursive", action="store_true", help="Recursively resolve spacetimedb-* dependencies")
    args = parser.parse_args()

    crates_dir = Path("crates")
    all_crates = list(args.crate)

    for crate in args.crate:
        deps = process_crate(crate, crates_dir, recursive=args.recursive)
        all_crates.extend(dep_to_crate_dir(dep) for dep in deps)

    # Reverse the list, then dedupe preserving first (i.e., keep last occurrence from original)
    all_crates = dedupe_preserve_first(reversed(all_crates))

    print("\nAll crates including dependencies (deduplicated, last occurrence kept):")
    for crate in all_crates:
        print(crate)

if __name__ == "__main__":
    main()
