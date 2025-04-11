import argparse
import toml
from pathlib import Path

def find_spacetimedb_dependencies(cargo_toml_path):
    with open(cargo_toml_path, 'r') as file:
        cargo_data = toml.load(file)

    dependencies = []
    deps = cargo_data.get('dependencies', {})

    for dep in deps:
        if dep.startswith("spacetimedb-"):
            dependencies.append(dep)

    return dependencies

def dep_to_crate_dir(dep_name):
    return dep_name.replace("spacetimedb-", "", 1)

def process_crate(crate_name, crates_dir, recursive=False):
    cargo_toml_path = crates_dir / crate_name / "Cargo.toml"

    if not cargo_toml_path.is_file():
        print(f"Warning: Cargo.toml not found for crate '{crate_name}' at {cargo_toml_path}")
        return

    print(f"\nChecking crate '{crate_name}'...")
    deps = find_spacetimedb_dependencies(cargo_toml_path)

    if deps:
        for name in deps:
            print(f"  {name}")
    else:
        print("  No spacetimedb-* dependencies found.")

    if recursive:
        for dep_name in deps:
            sub_crate = dep_to_crate_dir(dep_name)
            process_crate(sub_crate, crates_dir, recursive=True)

def main():
    parser = argparse.ArgumentParser(description="Recursively find spacetimedb-* dependencies for a crate.")
    parser.add_argument("crate", help="Starting crate name (in crates/<crate>/Cargo.toml)")
    parser.add_argument("--recursive", action="store_true", help="Recursively resolve spacetimedb-* dependencies")
    args = parser.parse_args()

    crates_dir = Path("crates")
    process_crate(args.crate, crates_dir, recursive=args.recursive)

if __name__ == "__main__":
    main()
