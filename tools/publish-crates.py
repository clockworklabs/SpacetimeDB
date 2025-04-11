import argparse
import toml
from pathlib import Path

def find_spacetimedb_dependencies(cargo_toml_path):
    with open(cargo_toml_path, 'r') as file:
        cargo_data = toml.load(file)

    dependencies = {}

    for section in ['dependencies', 'dev-dependencies', 'build-dependencies']:
        deps = cargo_data.get(section, {})
        for dep, val in deps.items():
            if dep.startswith("spacetimedb-"):
                dependencies[dep] = val

    return dependencies

def dep_to_crate_dir(dep_name):
    # Convert spacetimedb-foo -> foo
    return dep_name.replace("spacetimedb-", "", 1)

def process_crate(crate_name, crates_dir, visited, recursive=False):
    if crate_name in visited:
        return
    visited.add(crate_name)

    crate_path = crates_dir / crate_name / "Cargo.toml"

    if not crate_path.is_file():
        print(f"Warning: Cargo.toml not found for crate '{crate_name}' at {crate_path}")
        return

    print(f"\nChecking crate '{crate_name}'...")
    deps = find_spacetimedb_dependencies(crate_path)
    
    if deps:
        for name, val in deps.items():
            print(f"  {name}: {val}")
    else:
        print("  No spacetimedb-* dependencies found.")

    if recursive:
        for dep_name in deps.keys():
            sub_crate = dep_to_crate_dir(dep_name)
            process_crate(sub_crate, crates_dir, visited, recursive=True)

def main():
    parser = argparse.ArgumentParser(description="Recursively find spacetimedb-* dependencies for a crate.")
    parser.add_argument("crate", help="Starting crate name (located in crates/<crate>/Cargo.toml)")
    parser.add_argument("--recursive", action="store_true", help="Recursively resolve spacetimedb-* dependencies")
    args = parser.parse_args()

    crates_dir = Path("crates")
    visited = set()
    process_crate(args.crate, crates_dir, visited, recursive=args.recursive)

if __name__ == "__main__":
    main()
